const DEFAULT_API_BASE = "http://127.0.0.1:8080";

export function getApiBase() {
  return import.meta.env.API_BASE || DEFAULT_API_BASE;
}

export async function fetchAwsReport() {
  const base = getApiBase().replace(/\/$/, "");
  const res = await fetch(`${base}/report/aws`);
  if (!res.ok) {
    throw new Error(`Request failed: ${res.status}`);
  }
  return res.json();
}
