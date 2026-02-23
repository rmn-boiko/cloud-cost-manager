# Cloud Cost Manager UI

Simple React one-pager (no build step).

## Run

1. Start the API:

```bash
cargo run -p cloud-cost-api -- --bind 127.0.0.1:8080 --auth none
```

2. Open `ui/index.html` in a browser.

Serve with npm (Vite):

```bash
cd ui
npm install
npm run dev
```

Then visit http://127.0.0.1:5173

Optional: override API base URL via env:

```
API_BASE=http://127.0.0.1:8080
```
