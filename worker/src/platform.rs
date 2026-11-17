//! Agent platform: a registry of agents (built-in + user-created), an LLM router that dispatches each task
//! to the best agent, and per-agent usage metrics.
use regex::Regex;
use serde_json::{json, Map, Value};

pub const MODEL: &str = "@cf/meta/llama-3.3-70b-instruct-fp8-fast";
pub const MAX_CUSTOM: usize = 30;

#[allow(async_fn_in_trait)]
pub trait Ai {
    async fn run(&self, model: &str, input: Value) -> Result<Value, String>;
}
#[allow(async_fn_in_trait)]
pub trait Kv {
    async fn get(&self, key: &str) -> Option<String>;
    async fn put(&self, key: &str, val: &str);
}

pub fn builtin() -> Vec<Value> {
    vec![
        json!({"name":"researcher","description":"Explains topics and facts with clear bullet points","instructions":"You are a precise researcher. Answer with concise, factual bullet points and flag uncertainty."}),
        json!({"name":"coder","description":"Writes and explains code","instructions":"You are a senior software engineer. Reply with a short explanation and a minimal, correct code sample."}),
        json!({"name":"copywriter","description":"Writes marketing copy, emails and social posts","instructions":"You are a punchy copywriter. Write persuasive, plain-English copy. Keep it short."}),
        json!({"name":"analyst","description":"Business analysis, pricing, pros/cons and decisions","instructions":"You are a pragmatic business analyst. Give a structured recommendation with numbers where possible."}),
    ]
}

pub fn text(r: &Value) -> String {
    match r.get("response") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => r.to_string(),
        Some(v) => v.to_string(),
    }
}
pub fn parse_json(s: &str) -> Option<Value> {
    Regex::new(r"[\[{][\s\S]*[\]}]").unwrap().find(s).and_then(|m| serde_json::from_str(m.as_str()).ok())
}
pub fn cap(s: Option<&str>, n: usize) -> bool {
    matches!(s, Some(s) if !s.trim().is_empty() && s.chars().count() <= n)
}
pub fn iso_ms(ms: i64) -> String {
    let (days, rem) = (ms.div_euclid(86_400_000), ms.rem_euclid(86_400_000));
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z", if m <= 2 { y + 1 } else { y }, m, d, rem / 3_600_000, rem % 3_600_000 / 60_000, rem % 60_000 / 1000, rem % 1000)
}
fn u16len(s: &str) -> usize {
    s.encode_utf16().count()
}

async fn ask<A: Ai>(ai: &A, system: &str, user: &str, max: u32) -> Result<String, String> {
    Ok(text(&ai.run(MODEL, json!({"messages":[{"role":"system","content":system},{"role":"user","content":user}],"max_tokens":max})).await?))
}
async fn kv_json<K: Kv>(kv: &K, key: &str, default: Value) -> Value {
    kv.get(key).await.and_then(|r| serde_json::from_str(&r).ok()).unwrap_or(default)
}

pub async fn list_agents<K: Kv>(kv: &K) -> Vec<Value> {
    let mut v: Vec<Value> = builtin().into_iter().map(|mut a| { a["builtin"] = json!(true); a }).collect();
    v.extend(kv_json(kv, "agents", json!([])).await.as_array().cloned().unwrap_or_default());
    v
}

pub async fn create_agent<K: Kv>(kv: &K, a: &Value, now: i64) -> Value {
    let name = a.get("name").and_then(|v| v.as_str());
    if !name.map_or(false, |n| Regex::new(r"^[a-z][a-z0-9-]{2,24}$").unwrap().is_match(n)) {
        return json!({"error": "name: 3-25 chars, lowercase letters, digits, dashes"});
    }
    let (d, ins) = (a.get("description").and_then(|v| v.as_str()), a.get("instructions").and_then(|v| v.as_str()));
    match (d, ins) {
        (Some(d), Some(i)) if !d.is_empty() && u16len(d) <= 120 && !i.is_empty() && u16len(i) <= 600 => {}
        _ => return json!({"error": "description (<=120) and instructions (<=600) required"}),
    }
    let name = name.unwrap();
    let custom = kv_json(kv, "agents", json!([])).await.as_array().cloned().unwrap_or_default();
    if builtin().iter().chain(custom.iter()).any(|x| x["name"] == name) {
        return json!({"error": "name taken"});
    }
    if custom.len() >= MAX_CUSTOM {
        return json!({"error": "agent limit reached"});
    }
    let agent = json!({"name": name, "description": d.unwrap(), "instructions": ins.unwrap(), "created": iso_ms(now)});
    let mut all = custom;
    all.push(agent.clone());
    kv.put("agents", &Value::Array(all).to_string()).await;
    agent
}

pub async fn route<A: Ai>(ai: &A, task: &str, agents: &[Value]) -> Result<Value, String> {
    let list: Vec<String> = agents.iter().map(|a| format!("- {}: {}", a["name"].as_str().unwrap_or(""), a["description"].as_str().unwrap_or(""))).collect();
    let system = format!("You are a router. Pick the single best agent for the task. Agents:\n{}\nReply ONLY JSON: {{\"agent\": name, \"why\": short reason}}", list.join("\n"));
    let r = parse_json(&ask(ai, &system, task, 80).await?).unwrap_or(json!({}));
    if agents.iter().any(|a| r.get("agent").map_or(false, |x| *x == a["name"])) {
        return Ok(r);
    }
    Ok(json!({"agent": agents[0]["name"], "why": "router fallback"}))
}

pub async fn run_task<A: Ai, K: Kv>(ai: &A, kv: &K, task: &str, forced: Option<&str>, now: &dyn Fn() -> i64) -> Result<Value, String> {
    let agents = list_agents(kv).await;
    let t0 = now();
    let routing = match forced.filter(|f| !f.is_empty()) {
        Some(f) => json!({"agent": f, "why": "chosen by user"}),
        None => route(ai, task, &agents).await?,
    };
    let Some(agent) = agents.iter().find(|a| a["name"] == routing["agent"]) else { return Ok(json!({"error": "unknown agent"})) };
    let answer = ask(ai, agent["instructions"].as_str().unwrap_or(""), task, 450).await?;
    let ms = now() - t0;
    let mut m = kv_json(kv, "metrics", json!({})).await;
    let name = agent["name"].as_str().unwrap();
    let (runs, total) = m.get(name).map_or((0, 0), |s| (s["runs"].as_i64().unwrap_or(0), s["total_ms"].as_i64().unwrap_or(0)));
    m[name] = json!({"runs": runs + 1, "total_ms": total + ms, "last": iso_ms(now())});
    kv.put("metrics", &m.to_string()).await;
    Ok(json!({"agent": name, "routing": routing, "answer": answer, "ms": ms}))
}

pub async fn metrics<K: Kv>(kv: &K) -> Value {
    let m = kv_json(kv, "metrics", json!({})).await;
    let mut o = Map::new();
    for (k, v) in m.as_object().cloned().unwrap_or_default() {
        let (runs, total) = (v["runs"].as_f64().unwrap_or(0.0), v["total_ms"].as_f64().unwrap_or(0.0));
        o.insert(k, json!({"runs": v["runs"], "avg_ms": (total / runs).round() as i64, "last": v["last"]}));
    }
    Value::Object(o)
}

/// Pure router: (status, json); status 0 = serve the page.
pub async fn handle<A: Ai, K: Kv>(ai: &A, kv: &K, method: &str, path: &str, body: &Value, now: &dyn Fn() -> i64) -> (u16, Value) {
    if method == "GET" && path == "/api/agents" {
        return (200, Value::Array(list_agents(kv).await));
    }
    if method == "GET" && path == "/api/metrics" {
        return (200, metrics(kv).await);
    }
    if method == "POST" && (path == "/api/agents" || path == "/api/run") {
        if path == "/api/agents" {
            let r = create_agent(kv, body, now()).await;
            return if r.get("error").is_some() { (400, r) } else { (201, r) };
        }
        let task = body.get("task").and_then(|v| v.as_str());
        if !cap(task, 500) {
            return (400, json!({"error": "task required (<=500 chars)"}));
        }
        return match run_task(ai, kv, task.unwrap(), body.get("agent").and_then(|v| v.as_str()), now).await {
            Ok(r) if r.get("error").is_some() => (400, r),
            Ok(r) => (200, r),
            Err(e) => (502, json!({"error": format!("Agent error: {e}")})),
        };
    }
    (0, Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use std::{cell::{Cell, RefCell}, collections::HashMap};

    #[derive(Default)]
    struct Map_(RefCell<HashMap<String, String>>);
    impl Kv for Map_ {
        async fn get(&self, k: &str) -> Option<String> { self.0.borrow().get(k).cloned() }
        async fn put(&self, k: &str, v: &str) { self.0.borrow_mut().insert(k.into(), v.into()); }
    }
    struct Fixed(&'static str, Cell<u32>);
    impl Ai for Fixed {
        async fn run(&self, _m: &str, i: Value) -> Result<Value, String> {
            let s = i["messages"][0]["content"].as_str().unwrap();
            if s.starts_with("You are a router") { self.1.set(self.1.get() + 1); return Ok(json!({"response": self.0})); }
            Ok(json!({"response": format!("done by {}", s.chars().take(20).collect::<String>())}))
        }
    }
    fn ai(r: &'static str) -> Fixed { Fixed(r, Cell::new(0)) }
    fn clk() -> impl Fn() -> i64 { let c = Cell::new(1_700_000_000_000i64); move || { c.set(c.get() + 5); c.get() } }
    fn mk(kv: &Map_, v: Value) -> Value { block_on(create_agent(kv, &v, 0)) }

    #[test]
    fn router_dispatches_metrics_and_fallbacks() {
        let (a, kv) = (ai(r#"{"agent":"coder","why":"code task"}"#), Map_::default());
        assert_eq!(block_on(run_task(&a, &kv, "write fizzbuzz", None, &clk())).unwrap()["agent"], "coder");
        block_on(run_task(&a, &kv, "again", None, &clk())).unwrap();
        assert_eq!(block_on(metrics(&kv))["coder"]["runs"], 2);
        let g = ai("garbage");
        assert_eq!(block_on(run_task(&g, &Map_::default(), "x", None, &clk())).unwrap()["routing"]["why"], "router fallback");
        let b = builtin();
        assert_eq!(block_on(route(&ai(r#"{"agent":"wizard"}"#), "x", &b)).unwrap()["why"], "router fallback");
        assert_eq!(block_on(route(&ai(r#"Sure! {"agent":"analyst","why":"pricing"}"#), "x", &b)).unwrap()["agent"], "analyst");
        assert_eq!(b.iter().map(|a| a["name"].as_str().unwrap()).collect::<Vec<_>>(), ["researcher", "coder", "copywriter", "analyst"]);
    }
    #[test]
    fn forced_agent_custom_agent_and_validation() {
        let (a, kv) = (ai("{}"), Map_::default());
        assert_eq!(block_on(run_task(&a, &kv, "x", Some("copywriter"), &clk())).unwrap()["agent"], "copywriter");
        assert_eq!(a.1.get(), 0);
        assert_eq!(block_on(run_task(&a, &kv, "x", Some("ghost"), &clk())).unwrap()["error"], "unknown agent");
        assert!(mk(&kv, json!({"name":"Bad Name","description":"d","instructions":"i"})).get("error").is_some());
        assert!(mk(&kv, json!({"name":"ab","description":"d","instructions":"i"})).get("error").is_some());
        assert!(mk(&kv, json!({"name":"okname","description":"d".repeat(121),"instructions":"i"})).get("error").is_some());
        assert!(mk(&kv, json!({"name":"researcher","description":"d","instructions":"i"})).get("error").is_some());
        mk(&kv, json!({"name":"chef","description":"Recipes","instructions":"You are a chef."}));
        assert_eq!(block_on(list_agents(&kv)).len(), 5);
        assert_eq!(mk(&kv, json!({"name":"chef","description":"d","instructions":"i"}))["error"], "name taken");
        let c = ai(r#"{"agent":"chef","why":"food"}"#);
        assert_eq!(block_on(run_task(&c, &kv, "pasta?", None, &clk())).unwrap()["agent"], "chef");
    }
    #[test]
    fn custom_agent_cap_and_http() {
        let kv = Map_::default();
        for i in 0..MAX_CUSTOM {
            assert!(mk(&kv, json!({"name": format!("agent-{i}"), "description":"d","instructions":"i"})).get("error").is_none());
        }
        assert_eq!(mk(&kv, json!({"name":"one-more","description":"d","instructions":"i"}))["error"], "agent limit reached");
        assert_eq!(block_on(list_agents(&kv)).len(), 4 + MAX_CUSTOM);
        let (a, kv) = (ai(r#"{"agent":"coder"}"#), Map_::default());
        let go = |m: &str, p: &str, b: Value| block_on(handle(&a, &kv, m, p, &b, &clk()));
        assert_eq!(go("GET", "/api/agents", json!({})).1.as_array().unwrap().len(), 4);
        assert_eq!(go("POST", "/api/run", json!({})).0, 400);
        assert_eq!(go("POST", "/api/agents", json!({"name":"x"})).0, 400);
        assert_eq!(go("POST", "/api/agents", json!({"name":"poet","description":"d","instructions":"i"})).0, 201);
        assert_eq!(go("POST", "/api/run", json!({"task":"t","agent":"ghost"})).1["error"], "unknown agent");
        assert_eq!(go("POST", "/api/run", json!({"task":"t"})).0, 200);
        assert!(go("GET", "/api/metrics", json!({})).1.get("coder").is_some());
    }
}
