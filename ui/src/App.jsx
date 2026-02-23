import React, { useEffect, useState } from "react";
import { fetchAwsReport, getApiBase } from "./api.js";

function formatUsd(value) {
  if (typeof value !== "number") return "-";
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
    maximumFractionDigits: 2,
  }).format(value);
}

export default function App() {
  const [report, setReport] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await fetchAwsReport();
      setReport(data);
    } catch (err) {
      setError(err.message || "Failed to load report");
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
  }, []);

  return (
    <div>
      <div className="header">
        <div>
          <div className="title">Cloud Cost Manager</div>
          <div className="subtitle">Multi-account AWS cost snapshot</div>
        </div>
        <button className="button" onClick={load} disabled={loading}>
          {loading ? "Loading..." : "Refresh"}
        </button>
      </div>

      {error && <div className="error">{error}</div>}

      <div className="panel" style={{ marginTop: 16 }}>
        {!report && !loading && <div>No data yet.</div>}
        {loading && <div>Loading report...</div>}
        {report && (
          <>
            <div className="grid">
              <div className="metric">
                <div className="label">Total (MTD)</div>
                <div className="value">{formatUsd(report.total_all)}</div>
              </div>
              <div className="metric">
                <div className="label">Previous Month (Same Point)</div>
                <div className="value">{formatUsd(report.prev_total)}</div>
              </div>
              <div className="metric">
                <div className="label">Delta</div>
                <div className="value">{formatUsd(report.delta)}</div>
              </div>
              <div className="metric">
                <div className="label">Delta %</div>
                <div className="value">{report.delta_pct.toFixed(2)}%</div>
              </div>
            </div>

            <div style={{ marginTop: 20 }}>
              <div className="badge">Top Services</div>
              <table className="table">
                <thead>
                  <tr>
                    <th>Service</th>
                    <th>Cost</th>
                  </tr>
                </thead>
                <tbody>
                  {report.top_services.map(([name, value]) => (
                    <tr key={name}>
                      <td>{name}</td>
                      <td>{formatUsd(value)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>

            <div style={{ marginTop: 20 }}>
              <div className="badge">Accounts</div>
              <table className="table">
                <thead>
                  <tr>
                    <th>Account</th>
                    <th>Profile/Ref</th>
                    <th>Cost</th>
                  </tr>
                </thead>
                <tbody>
                  {report.summaries.map((s) => (
                    <tr key={s.account_id}>
                      <td>
                        {s.account_name} ({s.account_id})
                      </td>
                      <td>{s.account_ref}</td>
                      <td>{formatUsd(s.total)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </>
        )}
      </div>

      <div className="footer">API: {getApiBase()}/report/aws</div>
    </div>
  );
}
