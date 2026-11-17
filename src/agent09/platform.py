"""Agent platform: a registry of agents (built-in + user-created), an LLM router that dispatches each task
to the best agent, and per-agent usage metrics."""
import json, math, re, time
from datetime import datetime, timezone

MODEL = "@cf/meta/llama-3.3-70b-instruct-fp8-fast"
BUILTIN = [
    {"name": "researcher", "description": "Explains topics and facts with clear bullet points", "instructions": "You are a precise researcher. Answer with concise, factual bullet points and flag uncertainty."},
    {"name": "coder", "description": "Writes and explains code", "instructions": "You are a senior software engineer. Reply with a short explanation and a minimal, correct code sample."},
    {"name": "copywriter", "description": "Writes marketing copy, emails and social posts", "instructions": "You are a punchy copywriter. Write persuasive, plain-English copy. Keep it short."},
    {"name": "analyst", "description": "Business analysis, pricing, pros/cons and decisions", "instructions": "You are a pragmatic business analyst. Give a structured recommendation with numbers where possible."},
]
MAX_CUSTOM = 30
dumps = lambda o: json.dumps(o, separators=(",", ":"), ensure_ascii=False)
iso = lambda ms=None: datetime.fromtimestamp((time.time() * 1000 if ms is None else ms) / 1000, timezone.utc).isoformat(timespec="milliseconds").replace("+00:00", "Z")
_u16 = lambda s: len(s.encode("utf-16-le")) // 2


def text(r):
    v = r.get("response") if isinstance(r, dict) else None
    return v if isinstance(v, str) else dumps(v if v is not None else r)


def parse_json(s, fallback=None):
    m = re.search(r"[\[{][\s\S]*[\]}]", str(s))
    try:
        return json.loads(m.group(0)) if m else fallback
    except ValueError:
        return fallback


def cap(s, n=500):
    return isinstance(s, str) and bool(s.strip()) and len(s) <= n


def _ask(ai, system, user, mx):
    return text(ai(MODEL, {"messages": [{"role": "system", "content": system}, {"role": "user", "content": user}], "max_tokens": mx}))


def _kv_json(kv, key, default):
    raw = kv.get(key)
    return json.loads(raw) if raw else default


def list_agents(kv):
    return [*({**a, "builtin": True} for a in BUILTIN), *_kv_json(kv, "agents", [])]


def create_agent(kv, a, now=None):
    name = a.get("name")
    if not (isinstance(name, str) and re.fullmatch(r"[a-z][a-z0-9-]{2,24}", name)):
        return {"error": "name: 3-25 chars, lowercase letters, digits, dashes"}
    d, ins = a.get("description"), a.get("instructions")
    if not (isinstance(d, str) and d and _u16(d) <= 120 and isinstance(ins, str) and ins and _u16(ins) <= 600):
        return {"error": "description (<=120) and instructions (<=600) required"}
    custom = _kv_json(kv, "agents", [])
    if any(x["name"] == name for x in [*BUILTIN, *custom]):
        return {"error": "name taken"}
    if len(custom) >= MAX_CUSTOM:
        return {"error": "agent limit reached"}
    agent = {"name": name, "description": d, "instructions": ins, "created": iso(now)}
    kv.put("agents", dumps([*custom, agent]))
    return agent


def route(ai, task, agents):
    system = "You are a router. Pick the single best agent for the task. Agents:\n" + "\n".join(f"- {a['name']}: {a['description']}" for a in agents) + '\nReply ONLY JSON: {"agent": name, "why": short reason}'
    r = parse_json(_ask(ai, system, task, 80), {})
    if isinstance(r, dict) and any(a["name"] == r.get("agent") for a in agents):
        return r
    return {"agent": agents[0]["name"], "why": "router fallback"}


def run_task(ai, kv, task, forced=None, now=lambda: time.time() * 1000):
    agents = list_agents(kv)
    t0 = now()
    routing = {"agent": forced, "why": "chosen by user"} if forced else route(ai, task, agents)
    agent = next((a for a in agents if a["name"] == routing["agent"]), None)
    if not agent:
        return {"error": "unknown agent"}
    answer = _ask(ai, agent["instructions"], task, 450)
    ms = round(now() - t0)
    m = _kv_json(kv, "metrics", {})
    s = m.get(agent["name"]) or {"runs": 0, "total_ms": 0}
    m[agent["name"]] = {"runs": s["runs"] + 1, "total_ms": s["total_ms"] + ms, "last": iso(now())}
    kv.put("metrics", dumps(m))
    return {"agent": agent["name"], "routing": routing, "answer": answer, "ms": ms}


def metrics(kv):
    m = _kv_json(kv, "metrics", {})
    return {k: {"runs": v["runs"], "avg_ms": math.floor(v["total_ms"] / v["runs"] + 0.5), "last": v["last"]} for k, v in m.items()}


def handle(ai, kv, method, path, body=None, **kw):
    """Pure router: (status, json); status 0 = serve the page."""
    if method == "GET" and path == "/api/agents":
        return 200, list_agents(kv)
    if method == "GET" and path == "/api/metrics":
        return 200, metrics(kv)
    if method == "POST" and path in ("/api/agents", "/api/run"):
        b = body if isinstance(body, dict) else {}
        if path == "/api/agents":
            r = create_agent(kv, b)
            return (400, r) if "error" in r else (201, r)
        if not cap(b.get("task")):
            return 400, {"error": "task required (<=500 chars)"}
        try:
            r = run_task(ai, kv, b["task"], b.get("agent"), **kw)
            return (400, r) if "error" in r else (200, r)
        except Exception as e:
            return 502, {"error": "Agent error: " + str(e)}
    return 0, None
