"""Live eval suite: hits the deployed Worker (real Workers AI calls). Run: BASE=https://... python3 evals/run.py"""
import json, math, os, re, time, urllib.error, urllib.request
from datetime import datetime, timezone
from pathlib import Path

NAME = "agent-09-multi-agent-platform"
BASE = os.environ.get("BASE", f"https://{NAME}.braylo777.workers.dev")
GAP = float(os.environ.get("GAP_MS", "13000")) / 1000  # 5 req/min rate limit on the demo
here = Path(__file__).parent
lat, last = [], [0.0]


def call(method, path, body=None):
    w = last[0] + GAP - time.time()
    if w > 0:
        time.sleep(w)
    for _ in range(3):
        last[0] = time.time(); t0 = time.time()
        req = urllib.request.Request(BASE + path, None if method == "GET" else json.dumps(body).encode(), {"content-type": "application/json"}, method=method)
        try:
            r = urllib.request.urlopen(req, timeout=90); status = r.status; raw = r.read()
        except urllib.error.HTTPError as e:
            status = e.code; raw = e.read()
        ms = round((time.time() - t0) * 1000)
        try:
            j = json.loads(raw)
        except ValueError:
            j = {}
        if status == 429:
            time.sleep(61); continue
        lat.append(ms)
        return {"status": status, "ms": ms, "j": j}
    raise RuntimeError("rate limited 3x")


pct = lambda a, p: sorted(a)[max(0, math.ceil(p / 100 * len(a)) - 1)] if a else None
ratio = lambda n, d: {"passed": n, "total": d, "rate": n / d if d else None}
CASES = [
    ("Explain how photosynthesis works.", "researcher"), ("What causes inflation? Give the main factors.", "researcher"),
    ("Summarize the history of the Roman Empire in bullet points.", "researcher"), ("How do vaccines train the immune system?", "researcher"),
    ("What is the difference between a virus and a bacterium?", "researcher"),
    ("Write a Python function that dedupes a list while keeping order.", "coder"), ("Fix this JavaScript: const x = [1,2,3].map(n => n * 2", "coder"),
    ("Write a SQL query to find the top 5 customers by revenue.", "coder"), ("Implement binary search in Go.", "coder"),
    ("Write a bash one-liner to count lines in all .js files.", "coder"),
    ("Write an Instagram caption for a new coffee shop opening.", "copywriter"), ("Draft a cold email pitching window cleaning to offices.", "copywriter"),
    ("Write a catchy tagline for an AI receptionist product.", "copywriter"), ("Write three tweet ideas announcing our summer sale.", "copywriter"),
    ("Write a short landing page headline and subhead for a dog grooming service.", "copywriter"),
    ("Should I charge $99 or $149/month for an AI receptionist for dentists?", "analyst"), ("Compare leasing vs buying a delivery van for a small bakery.", "analyst"),
    ("What are the pros and cons of opening a second location next year?", "analyst"), ("Estimate the break-even point if fixed costs are $5k/month and margin is $25/unit.", "analyst"),
    ("Should a 10-person agency hire a full-time salesperson or use freelancers?", "analyst"),
]
cases, conf, correct = [], {}, 0
for i, (task, expect) in enumerate(CASES):
    r = call("POST", "/api/run", {"task": task})
    got = r["j"].get("agent")
    ok = got == expect
    correct += ok
    conf.setdefault(expect, {})
    conf[expect][str(got) if got is not None else "null"] = conf[expect].get(str(got) if got is not None else "null", 0) + 1
    why = (r["j"].get("routing") or {}).get("why")
    cases.append({"task": task, "expected": expect, "got": got, "why": why, "fallback": why == "router fallback", "correct": ok, "http": r["status"], "ms": r["ms"]})
    print(i + 1, expect, got, f"{r['ms']}ms", flush=True)
metrics = {"routing_accuracy": {**ratio(correct, len(CASES)), "definition": "router chose the labelled agent (4 built-in agents, 5 tasks each)"}, "confusion": conf,
           "router_fallbacks": sum(1 for c in cases if c["fallback"])}
out = {"project": NAME, "endpoint": BASE, "model": "@cf/meta/llama-3.3-70b-instruct-fp8-fast", "run_at": datetime.now(timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z"),
       "latency_ms": {"n": len(lat), "p50": pct(lat, 50), "p95": pct(lat, 95), "note": "client-side wall clock per HTTP request, box to Cloudflare edge"}, "metrics": metrics, "cases": cases}
(here / "results.json").write_text(json.dumps(out, indent=2) + "\n")
print(json.dumps({"metrics": metrics, "latency_ms": out["latency_ms"]}, indent=2))
