// Live eval suite: hits the deployed Worker (real Workers AI calls). Run: node evals/run.mjs
const NAME = "agent-09-multi-agent-platform";
const BASE = process.env.BASE || "https://"+NAME+".braylo777.workers.dev";
const GAP = Number(process.env.GAP_MS || 13000); // 5 req/min rate limit on the demo
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let last = 0; const lat = [];
async function call(method, path, body, { timed = true } = {}) {
  const w = last + GAP - Date.now(); if (w > 0) await sleep(w);
  for (let i = 0; i < 3; i++) {
    last = Date.now(); const t0 = Date.now();
    const r = await fetch(BASE + path, method === "GET" ? {} : { method, headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
    const ms = Date.now() - t0; const j = await r.json().catch(() => ({}));
    if (r.status === 429) { await sleep(61000); continue; }
    if (timed) lat.push(ms); return { status: r.status, ms, j };
  }
  throw new Error("rate limited 3x");
}
const pct = (a, p) => { if (!a.length) return null; const s = [...a].sort((x, y) => x - y); return s[Math.max(0, Math.ceil((p / 100) * s.length) - 1)]; };
const ratio = (n, d) => ({ passed: n, total: d, rate: d ? n / d : null });
async function save(metrics, cases) {
  const fs = await import("node:fs");
  const out = { project: NAME, endpoint: BASE, model: "@cf/meta/llama-3.3-70b-instruct-fp8-fast", run_at: new Date().toISOString(),
    latency_ms: { n: lat.length, p50: pct(lat, 50), p95: pct(lat, 95), note: "client-side wall clock per HTTP request, box to Cloudflare edge" }, metrics, cases };
  fs.mkdirSync(new URL(".", import.meta.url), { recursive: true });
  fs.writeFileSync(new URL("./results.json", import.meta.url), JSON.stringify(out, null, 2) + "\n");
  console.log(JSON.stringify({ metrics, latency_ms: out.latency_ms }, null, 2));
}
const CASES = [
  ["Explain how photosynthesis works.", "researcher"], ["What causes inflation? Give the main factors.", "researcher"],
  ["Summarize the history of the Roman Empire in bullet points.", "researcher"], ["How do vaccines train the immune system?", "researcher"],
  ["What is the difference between a virus and a bacterium?", "researcher"],
  ["Write a Python function that dedupes a list while keeping order.", "coder"], ["Fix this JavaScript: const x = [1,2,3].map(n => n * 2", "coder"],
  ["Write a SQL query to find the top 5 customers by revenue.", "coder"], ["Implement binary search in Go.", "coder"],
  ["Write a bash one-liner to count lines in all .js files.", "coder"],
  ["Write an Instagram caption for a new coffee shop opening.", "copywriter"], ["Draft a cold email pitching window cleaning to offices.", "copywriter"],
  ["Write a catchy tagline for an AI receptionist product.", "copywriter"], ["Write three tweet ideas announcing our summer sale.", "copywriter"],
  ["Write a short landing page headline and subhead for a dog grooming service.", "copywriter"],
  ["Should I charge $99 or $149/month for an AI receptionist for dentists?", "analyst"], ["Compare leasing vs buying a delivery van for a small bakery.", "analyst"],
  ["What are the pros and cons of opening a second location next year?", "analyst"], ["Estimate the break-even point if fixed costs are $5k/month and margin is $25/unit.", "analyst"],
  ["Should a 10-person agency hire a full-time salesperson or use freelancers?", "analyst"],
];
const cases = []; let correct = 0; const conf = {};
for (const [i, [task, expect]] of CASES.entries()) {
  const r = await call("POST", "/api/run", { task }); const got = r.j.agent ?? null; const ok = got === expect; if (ok) correct++;
  conf[expect] = conf[expect] || {}; conf[expect][got] = (conf[expect][got] || 0) + 1;
  cases.push({ task, expected: expect, got, why: r.j.routing?.why ?? null, fallback: r.j.routing?.why === "router fallback", correct: ok, http: r.status, ms: r.ms });
  console.error(i + 1, expect, got, r.ms + "ms");
}
await save({ routing_accuracy: { ...ratio(correct, CASES.length), definition: "router chose the labelled agent (4 built-in agents, 5 tasks each)" }, confusion: conf,
  router_fallbacks: cases.filter((c) => c.fallback).length }, cases);
process.exit(0);
