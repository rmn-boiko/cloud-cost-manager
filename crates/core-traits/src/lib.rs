use anyhow::{Result, anyhow};
use async_trait::async_trait;
use chrono::{Datelike, Duration, NaiveDate};
use futures::future::try_join_all;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub struct AccountSummary {
    pub account_ref: String,
    pub account_id: String,
    pub account_name: String,
    pub total: f64,
    pub services: HashMap<String, f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub month_start: NaiveDate,
    pub month_end_exclusive: NaiveDate,
    pub prev_start: NaiveDate,
    pub prev_end_exclusive: NaiveDate,
    pub summaries: Vec<AccountSummary>,
    pub total_all: f64,
    pub services_total: HashMap<String, f64>,
    pub top_services: Vec<(String, f64)>,
    pub prev_total: f64,
    pub delta: f64,
    pub delta_pct: f64,
}

#[async_trait]
pub trait CostProvider: Send + Sync {
    async fn fetch_account_summary(
        &self,
        account_ref: &str,
        start: NaiveDate,
        end_exclusive: NaiveDate,
    ) -> Result<AccountSummary>;

    async fn total_cost(
        &self,
        account_ref: &str,
        start: NaiveDate,
        end_exclusive: NaiveDate,
    ) -> Result<f64>;
}

pub async fn generate_report<P: CostProvider>(
    provider: &P,
    accounts: &[String],
    today: NaiveDate,
) -> Result<Report> {
    let (month_start, month_end_exclusive) = month_to_date(today);
    let (prev_start, prev_end_exclusive) = previous_month_same_point(today)?;

    let summaries = try_join_all(accounts.iter().map(|account_ref| async move {
        provider
            .fetch_account_summary(account_ref, month_start, month_end_exclusive)
            .await
    }))
    .await?;

    let mut total_all = 0.0_f64;
    let mut services_total: HashMap<String, f64> = HashMap::new();

    for s in &summaries {
        total_all += s.total;
        for (svc, amt) in &s.services {
            *services_total.entry(svc.clone()).or_insert(0.0) += *amt;
        }
    }

    let mut top_services: Vec<(String, f64)> = services_total
        .iter()
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    top_services.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    top_services.truncate(5);

    let prev_total =
        total_for_all_accounts(provider, accounts, prev_start, prev_end_exclusive).await?;

    let delta = total_all - prev_total;
    let delta_pct = if prev_total.abs() < f64::EPSILON {
        0.0
    } else {
        (delta / prev_total) * 100.0
    };

    Ok(Report {
        month_start,
        month_end_exclusive,
        prev_start,
        prev_end_exclusive,
        summaries,
        total_all,
        services_total,
        top_services,
        prev_total,
        delta,
        delta_pct,
    })
}

async fn total_for_all_accounts<P: CostProvider>(
    provider: &P,
    accounts: &[String],
    start: NaiveDate,
    end_exclusive: NaiveDate,
) -> Result<f64> {
    let totals = try_join_all(accounts.iter().map(|account_ref| async move {
        provider.total_cost(account_ref, start, end_exclusive).await
    }))
    .await?;

    Ok(totals.into_iter().sum())
}

fn month_to_date(today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let start = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap();
    let end_exclusive = today + Duration::days(1);
    (start, end_exclusive)
}

fn previous_month_same_point(today: NaiveDate) -> Result<(NaiveDate, NaiveDate)> {
    let first_of_this_month = NaiveDate::from_ymd_opt(today.year(), today.month(), 1)
        .ok_or_else(|| anyhow!("Invalid current month date"))?;
    let last_of_prev_month = first_of_this_month - Duration::days(1);
    let prev_start =
        NaiveDate::from_ymd_opt(last_of_prev_month.year(), last_of_prev_month.month(), 1)
            .ok_or_else(|| anyhow!("Invalid previous month date"))?;

    let day = today.day();
    let prev_end_exclusive = prev_start + Duration::days(day as i64);

    Ok((prev_start, prev_end_exclusive))
}
