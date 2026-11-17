// Parity helper: same scripted scenario as the legacy JS harness; prints JSON (timestamps masked).
use agent_09_multi_agent_platform::platform::*;
use futures::executor::block_on;
use serde_json::{json, Value};
use std::{cell::RefCell, collections::HashMap};

#[derive(Default)]
struct Map(RefCell<HashMap<String, String>>);
impl Kv for Map {
    async fn get(&self, k: &str) -> Option<String> { self.0.borrow().get(k).cloned() }
    async fn put(&self, k: &str, v: &str) { self.0.borrow_mut().insert(k.into(), v.into()); }
}
struct F(RefCell<String>);
impl Ai for F {
    async fn run(&self, _m: &str, i: Value) -> Result<Value, String> {
        let s = i["messages"][0]["content"].as_str().unwrap();
        Ok(json!({"response": if s.starts_with("You are a router") { self.0.borrow().clone() } else { format!("ans:{}|{}", s.chars().take(15).collect::<String>(), i["messages"][1]["content"].as_str().unwrap()) }}))
    }
}
fn mask(v: &mut Value) {
    match v {
        Value::Object(o) => for (k, x) in o.iter_mut() { if k == "ms" || k == "created" || k == "last" { *x = json!("<t>"); } else { mask(x); } },
        Value::Array(a) => a.iter_mut().for_each(mask),
        _ => {}
    }
}
fn main() {
    let (kv, ai) = (Map::default(), F(RefCell::new(String::new())));
    let now = || 1_700_000_000_000i64;
    let mut out: Vec<Value> = vec![];
    for (r, t, f) in [(r#"{"agent":"coder","why":"code"}"#, "fizz", None), (r#"Sure {"agent":"analyst","why":"p"}"#, "price?", None), ("garbage", "x", None), (r#"{"agent":"wizard"}"#, "y", None), ("[1,2]", "z", None), (r#"{"agent":"copywriter"}"#, "é", None), ("{}", "q", Some("researcher")), ("{}", "q", Some("ghost"))] {
        *ai.0.borrow_mut() = r.to_string();
        out.push(block_on(run_task(&ai, &kv, t, f, &now)).unwrap());
    }
    let agents = vec![json!({"name":"chef","description":"Recipes","instructions":"You are a chef."}), json!({"name":"chef","description":"d","instructions":"i"}), json!({"name":"Bad","description":"d","instructions":"i"}), json!({"name":"ab","description":"d","instructions":"i"}), json!({"name":"okname","description":"d".repeat(121),"instructions":"i"}), json!({"name":"okname","description":"d","instructions":"i".repeat(601)}), json!({"name":"okname","description":"","instructions":"i"}), json!({"name":"researcher","description":"d","instructions":"i"}), json!({"name":"x-y-z","description":"é","instructions":"i"})];
    for a in agents { out.push(block_on(create_agent(&kv, &a, 0))); }
    *ai.0.borrow_mut() = r#"{"agent":"chef"}"#.into();
    out.push(block_on(run_task(&ai, &kv, "pasta", None, &now)).unwrap());
    out.push(Value::Array(block_on(list_agents(&kv))));
    out.push(block_on(metrics(&kv)));
    for o in out.iter_mut() { mask(o); }
    println!("{}", serde_json::to_string(&out).unwrap());
}
