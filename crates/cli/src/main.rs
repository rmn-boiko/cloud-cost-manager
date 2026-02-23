use anyhow::Result;
use clap::Parser;
use chrono::Utc;
use cloud_cost_aws::{AwsCostProvider, StaticCredentials};
use cloud_cost_core::generate_report;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "cloud-cost-manager")]
#[command(about = "Multi-account AWS cost summary", long_about = None)]
struct Args {
    /// Comma-separated list of AWS shared config profiles
    #[arg(long, value_delimiter = ',')]
    profiles: Vec<String>,

    /// Override AWS region (Cost Explorer is us-east-1 by default)
    #[arg(long, default_value = "us-east-1")]
    region: String,

    /// Load AWS credentials from a JSON file (overrides profiles)
    #[arg(long)]
    accounts_file: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct AccountsFileEntry {
    access_key_id: String,
    secret_access_key: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let (provider, accounts) = if let Some(path) = args.accounts_file {
        let contents = fs::read_to_string(&path)?;
        let entries: Vec<AccountsFileEntry> = serde_json::from_str(&contents)?;
        let mut creds_map = HashMap::new();
        let mut labels = Vec::with_capacity(entries.len());
        for (idx, entry) in entries.into_iter().enumerate() {
            let label = format!("credential-{}", idx + 1);
            labels.push(label.clone());
            creds_map.insert(
                label,
                StaticCredentials {
                    access_key_id: entry.access_key_id,
                    secret_access_key: entry.secret_access_key,
                    session_token: None,
                },
            );
        }
        (AwsCostProvider::with_static_credentials(args.region, creds_map), labels)
    } else {
        let profiles = if args.profiles.is_empty() {
            vec!["default".to_string()]
        } else {
            args.profiles
        };
        (AwsCostProvider::new(args.region), profiles)
    };

    let today = Utc::now().date_naive();
    let report = generate_report(&provider, &accounts, today).await?;

    println!("Cloud Cost Manager\n");

    println!(
        "Month-to-date window: {} to {} (exclusive)",
        report.month_start, report.month_end_exclusive
    );
    println!(
        "Previous month window: {} to {} (exclusive)\n",
        report.prev_start, report.prev_end_exclusive
    );

    println!("Breakdown by account:");
    for s in &report.summaries {
        println!(
            "- {} ({}) via profile {}: ${:.2}",
            s.account_name, s.account_id, s.account_ref, s.total
        );
    }

    println!("\nTotal across all accounts: ${:.2}", report.total_all);

    println!("\nTop 5 services across all accounts:");
    for (svc, amt) in &report.top_services {
        println!("- {}: ${:.2}", svc, amt);
    }

    println!("\nMonth-to-month comparison:");
    println!("- Current MTD: ${:.2}", report.total_all);
    println!("- Previous month same point: ${:.2}", report.prev_total);
    println!("- Change: ${:.2} ({:.2}%)", report.delta, report.delta_pct);

    Ok(())
}
