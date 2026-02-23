use anyhow::Result;
use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use axum::response::Response;
use http::header::{ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN};
use chrono::Utc;
use clap::{Parser, ValueEnum};
use cloud_cost_aws::{AssumeRoleConfig, AwsCostProvider, StaticCredentials};
use cloud_cost_core::generate_report;
use serde::Deserialize;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug)]
#[command(name = "cloud-cost-api")]
#[command(about = "REST API for multi-account AWS cost summary", long_about = None)]
struct Args {
    /// Bind address (host:port)
    #[arg(long, default_value = "127.0.0.1:8080")]
    bind: String,

    /// Override AWS region (Cost Explorer is us-east-1 by default)
    #[arg(long, default_value = "us-east-1")]
    region: String,

    /// Comma-separated list of AWS shared config profiles
    #[arg(long, value_delimiter = ',')]
    profiles: Vec<String>,

    /// Load AWS credentials from a JSON file (overrides profiles)
    #[arg(long)]
    accounts_file: Option<PathBuf>,

    /// Load role ARNs from a JSON file (overrides profiles/accounts)
    #[arg(long)]
    assume_roles_file: Option<PathBuf>,

    /// Base profile for STS AssumeRole calls
    #[arg(long)]
    base_profile: Option<String>,

    /// Authentication mode
    #[arg(long, value_enum, default_value_t = AuthMode::None)]
    auth: AuthMode,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum AuthMode {
    None,
    Iam,
}

#[derive(Clone)]
struct AppState {
    provider: AwsCostProvider,
    accounts: Vec<String>,
    auth: AuthMode,
}

#[derive(Debug, Deserialize)]
struct AccountsFileEntry {
    access_key_id: String,
    secret_access_key: String,
}

#[derive(Debug, Deserialize)]
struct AssumeRoleEntry {
    account_ref: String,
    role_arn: String,
    external_id: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new("info"))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();
    let (provider, accounts) = if let Some(path) = args.assume_roles_file.clone() {
        let contents = std::fs::read_to_string(&path)?;
        let entries: Vec<AssumeRoleEntry> = serde_json::from_str(&contents)?;
        let mut roles = HashMap::new();
        let mut account_refs = Vec::with_capacity(entries.len());
        for entry in entries {
            account_refs.push(entry.account_ref.clone());
            roles.insert(
                entry.account_ref,
                AssumeRoleConfig {
                    role_arn: entry.role_arn,
                    external_id: entry.external_id,
                },
            );
        }
        (
            AwsCostProvider::with_assume_roles(args.region, args.base_profile, roles),
            account_refs,
        )
    } else if let Some(path) = args.accounts_file.clone() {
        let contents = std::fs::read_to_string(&path)?;
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

    let state = Arc::new(AppState {
        provider,
        accounts,
        auth: args.auth,
    });

    let app = Router::new()
        .route("/health", get(health).options(options_handler))
        .route("/report/aws", get(report_aws).options(options_handler))
        .with_state(state);

    let addr: SocketAddr = args.bind.parse()?;
    tracing::info!("listening on {}", addr);
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;
    Ok(())
}

async fn health() -> impl IntoResponse {
    with_cors(StatusCode::OK.into_response())
}

async fn report_aws(State(state): State<Arc<AppState>>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(status) = authorize(state.auth, &headers) {
        return with_cors(status.into_response());
    }

    let today = Utc::now().date_naive();
    match generate_report(&state.provider, &state.accounts, today).await {
        Ok(report) => with_cors(Json(report).into_response()),
        Err(err) => {
            tracing::error!(error = %err, "report failed");
            with_cors(StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
    }
}

fn authorize(mode: AuthMode, headers: &HeaderMap) -> Result<(), StatusCode> {
    match mode {
        AuthMode::None => Ok(()),
        AuthMode::Iam => {
            if headers.get("x-amzn-iam-arn").is_some() {
                Ok(())
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
    }
}

// Simple permissive CORS for local UI usage
fn with_cors(mut res: Response) -> Response {
    let headers = res.headers_mut();
    headers.insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    headers.insert(
        ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, OPTIONS"),
    );
    headers.insert(
        ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type, x-amzn-iam-arn, authorization"),
    );
    res
}

async fn options_handler() -> impl IntoResponse {
    with_cors(StatusCode::NO_CONTENT.into_response())
}
