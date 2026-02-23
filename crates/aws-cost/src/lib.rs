use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_costexplorer::Client as CeClient;
use aws_sdk_costexplorer::types::{DateInterval, Granularity};
use aws_sdk_iam::Client as IamClient;
use aws_sdk_organizations::Client as OrgClient;
use aws_sdk_sts::Client as StsClient;
use chrono::NaiveDate;
use std::collections::HashMap;

use cloud_cost_core::{AccountSummary, CostProvider};

#[derive(Debug, Clone)]
pub struct StaticCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
}

impl StaticCredentials {
    fn into_provider(self) -> Credentials {
        Credentials::new(
            self.access_key_id,
            self.secret_access_key,
            self.session_token,
            None,
            "accounts.json",
        )
    }
}

#[derive(Debug, Clone)]
pub struct AssumeRoleConfig {
    pub role_arn: String,
    pub external_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AwsCostProvider {
    pub region: String,
    pub static_credentials: Option<HashMap<String, StaticCredentials>>,
    pub assume_roles: Option<HashMap<String, AssumeRoleConfig>>,
    pub base_profile: Option<String>,
}

impl AwsCostProvider {
    pub fn new(region: impl Into<String>) -> Self {
        Self {
            region: region.into(),
            static_credentials: None,
            assume_roles: None,
            base_profile: None,
        }
    }

    pub fn with_static_credentials(
        region: impl Into<String>,
        static_credentials: HashMap<String, StaticCredentials>,
    ) -> Self {
        Self {
            region: region.into(),
            static_credentials: Some(static_credentials),
            assume_roles: None,
            base_profile: None,
        }
    }

    pub fn with_assume_roles(
        region: impl Into<String>,
        base_profile: Option<String>,
        assume_roles: HashMap<String, AssumeRoleConfig>,
    ) -> Self {
        Self {
            region: region.into(),
            static_credentials: None,
            assume_roles: Some(assume_roles),
            base_profile,
        }
    }
}

#[async_trait]
impl CostProvider for AwsCostProvider {
    async fn fetch_account_summary(
        &self,
        account_ref: &str,
        start: NaiveDate,
        end_exclusive: NaiveDate,
    ) -> Result<AccountSummary> {
        let config = self.load_config(account_ref).await?;

        let sts = StsClient::new(&config);
        let ce = CeClient::new(&config);
        let iam = IamClient::new(&config);
        let org = OrgClient::new(&config);

        let account_id = sts
            .get_caller_identity()
            .send()
            .await
            .context("GetCallerIdentity failed")?
            .account
            .ok_or_else(|| anyhow!("Missing account id"))?;

        let account_name = resolve_account_name(&account_id, &org, &iam).await;

        let (total, services) = get_costs_by_service(&ce, start, end_exclusive).await?;

        Ok(AccountSummary {
            account_ref: account_ref.to_string(),
            account_id,
            account_name,
            total,
            services,
        })
    }

    async fn total_cost(
        &self,
        account_ref: &str,
        start: NaiveDate,
        end_exclusive: NaiveDate,
    ) -> Result<f64> {
        let config = self.load_config(account_ref).await?;

        let ce = CeClient::new(&config);
        let (total, _services) = get_costs_by_service(&ce, start, end_exclusive).await?;
        Ok(total)
    }
}

impl AwsCostProvider {
    async fn load_config(&self, account_ref: &str) -> Result<aws_config::SdkConfig> {
        if let Some(creds) = &self.static_credentials {
            let entry = creds
                .get(account_ref)
                .ok_or_else(|| anyhow!("Unknown account reference: {account_ref}"))?
                .clone();
            let config = aws_config::defaults(BehaviorVersion::latest())
                .region(Region::new(self.region.clone()))
                .credentials_provider(entry.into_provider())
                .load()
                .await;
            Ok(config)
        } else if let Some(roles) = &self.assume_roles {
            let role = roles
                .get(account_ref)
                .ok_or_else(|| anyhow!("Unknown account reference: {account_ref}"))?
                .clone();
            let mut base = aws_config::defaults(BehaviorVersion::latest());
            if let Some(profile) = &self.base_profile {
                base = base.profile_name(profile);
            }
            let base_config = base.region(Region::new(self.region.clone())).load().await;
            let sts = StsClient::new(&base_config);
            let mut assume = sts
                .assume_role()
                .role_arn(role.role_arn)
                .role_session_name(format!("cloud-cost-manager-{}", account_ref));
            if let Some(external_id) = role.external_id {
                assume = assume.external_id(external_id);
            }
            let resp = assume.send().await.context("AssumeRole failed")?;
            let creds = resp
                .credentials()
                .ok_or_else(|| anyhow!("Missing credentials from AssumeRole"))?;
            let creds = Credentials::new(
                creds.access_key_id().to_string(),
                creds.secret_access_key().to_string(),
                Some(creds.session_token().to_string()),
                None,
                "assume-role",
            );
            let config = aws_config::defaults(BehaviorVersion::latest())
                .region(Region::new(self.region.clone()))
                .credentials_provider(creds)
                .load()
                .await;
            Ok(config)
        } else {
            let config = aws_config::defaults(BehaviorVersion::latest())
                .profile_name(account_ref)
                .region(Region::new(self.region.clone()))
                .load()
                .await;
            Ok(config)
        }
    }
}

async fn resolve_account_name(account_id: &str, org: &OrgClient, iam: &IamClient) -> String {
    if let Ok(resp) = org.describe_account().account_id(account_id).send().await
        && let Some(acct) = resp.account()
        && let Some(name) = acct.name()
    {
        return name.to_string();
    }

    if let Ok(resp) = iam.list_account_aliases().send().await
        && let Some(alias) = resp.account_aliases().first()
    {
        return alias.to_string();
    }

    account_id.to_string()
}

async fn get_costs_by_service(
    ce: &CeClient,
    start: NaiveDate,
    end_exclusive: NaiveDate,
) -> Result<(f64, HashMap<String, f64>)> {
    let time_period = DateInterval::builder()
        .start(start.format("%Y-%m-%d").to_string())
        .end(end_exclusive.format("%Y-%m-%d").to_string())
        .build()?;

    let resp = ce
        .get_cost_and_usage()
        .time_period(time_period)
        .granularity(Granularity::Monthly)
        .metrics("UnblendedCost")
        .group_by(
            aws_sdk_costexplorer::types::GroupDefinition::builder()
                .key("SERVICE")
                .r#type(aws_sdk_costexplorer::types::GroupDefinitionType::Dimension)
                .build(),
        )
        .send()
        .await
        .context("GetCostAndUsage failed")?;

    let mut total = 0.0_f64;
    let mut services: HashMap<String, f64> = HashMap::new();

    for result in resp.results_by_time() {
        for g in result.groups() {
            let svc = g.keys().first().map(|s| s.as_str()).unwrap_or("Unknown");
            let amt = if let Some(metrics) = g.metrics()
                && let Some(unblended) = metrics.get("UnblendedCost")
                && let Some(amount) = unblended.amount()
            {
                amount.parse::<f64>().unwrap_or(0.0)
            } else {
                0.0
            };
            *services.entry(svc.to_string()).or_insert(0.0) += amt;
            total += amt;
        }
    }

    Ok((total, services))
}
