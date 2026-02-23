# Cloud Cost Manager

Workspace with:
- `cloud-cost-core` (in `crates/core-traits`): provider trait + shared reporting logic
- `cloud-cost-aws` (in `crates/aws-cost`): AWS Cost Explorer implementation
- `cloud-cost-cli` (in `crates/cli`): CLI entrypoint
- `cloud-cost-api` (in `crates/api`): REST API
- `ui` (in `ui/`): simple React one-pager

## Requirements
- AWS credentials in your shared config/credentials files
- Cost Explorer enabled in each account
- Permissions: `ce:GetCostAndUsage`, `sts:GetCallerIdentity`, `iam:ListAccountAliases` (optional), `organizations:DescribeAccount` (optional)

## Build

```bash
cargo build -p cloud-cost-cli --release
cargo build -p cloud-cost-api --release
```

## CLI Run

Default profile only:

```bash
cargo run -p cloud-cost-cli -- --region us-east-1
```

Multiple profiles:

```bash
cargo run -p cloud-cost-cli -- --profiles prod,staging,dev
```

Load credentials from `accounts.json`:

```bash
cargo run -p cloud-cost-cli -- --accounts-file accounts.json
```

## API Run (local)

```bash
cargo run -p cloud-cost-api -- --bind 127.0.0.1:8080
```

Endpoints:
- `GET /health`
- `GET /report/aws`

### API auth modes

- `--auth none`: no auth (local development)
- `--auth iam`: requires `x-amzn-iam-arn` header (when running behind API Gateway IAM auth)

### Assume-role file (for API)

Example `assume-roles.json`:

```json
[
  {
    "account_ref": "prod",
    "role_arn": "arn:aws:iam::123456789012:role/CostExplorerReadRole",
    "external_id": "my-external-id"
  },
  {
    "account_ref": "staging",
    "role_arn": "arn:aws:iam::210987654321:role/CostExplorerReadRole",
    "external_id": null
  }
]
```

Run with assume-role:

```bash
cargo run -p cloud-cost-api -- --assume-roles-file assume-roles.json --base-profile default
```

## UI (local)

Serve it with npm (Vite):

```bash
cd ui
npm install
npm run dev
```

## accounts.json

```json
[
  {
    "access_key_id": "AKIAEXAMPLEKEY1",
    "secret_access_key": "exampleSecretKeyValue1"
  },
  {
    "access_key_id": "AKIAEXAMPLEKEY2",
    "secret_access_key": "exampleSecretKeyValue2"
  }
]
```
