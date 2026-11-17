import pytest
from agent09.platform import create_agent, list_agents, run_task, metrics, route, BUILTIN, MAX_CUSTOM, handle


class KV:
    def __init__(self): self.m = {}
    def get(self, k): return self.m.get(k)
    def put(self, k, v, ttl=None): self.m[k] = v


def env(route_reply, answer=None):
    def ai(_m, inp):
        s = inp["messages"][0]["content"]
        return {"response": route_reply if s.startswith("You are a router") else (answer or "done by " + s[:20])}
    return ai, KV()


def test_router_dispatches_and_metrics_count():
    ai, kv = env('{"agent":"coder","why":"code task"}')
    assert run_task(ai, kv, "write fizzbuzz")["agent"] == "coder"
    run_task(ai, kv, "again")
    assert metrics(kv)["coder"]["runs"] == 2


def test_bad_router_output_falls_back():
    ai, kv = env("garbage")
    assert run_task(ai, kv, "x")["routing"]["why"] == "router fallback"


def test_create_custom_agent_validate_and_route():
    ai, kv = env('{"agent":"chef","why":"food"}')
    assert "error" in create_agent(kv, {"name": "Bad Name", "description": "d", "instructions": "i"})
    create_agent(kv, {"name": "chef", "description": "Recipes", "instructions": "You are a chef."})
    assert len(list_agents(kv)) == 5 and run_task(ai, kv, "pasta?")["agent"] == "chef"
    assert create_agent(kv, {"name": "chef", "description": "d", "instructions": "i"})["error"] == "name taken"


def test_builtin_names_hallucinated_agent_and_prose_json():
    assert [a["name"] for a in BUILTIN] == ["researcher", "coder", "copywriter", "analyst"]
    ai, _ = env('{"agent":"wizard"}')
    assert route(ai, "x", BUILTIN)["why"] == "router fallback"
    ai, _ = env('Sure! {"agent":"analyst","why":"pricing"}')
    assert route(ai, "x", BUILTIN)["agent"] == "analyst"


def test_forced_agent_skips_router_and_unknown_errors():
    routed = []
    def ai(_m, i):
        if i["messages"][0]["content"].startswith("You are a router"): routed.append(1)
        return {"response": "ok"}
    r = run_task(ai, KV(), "x", "copywriter")
    assert r["agent"] == "copywriter" and not routed
    assert run_task(env("{}")[0], KV(), "x", "ghost")["error"] == "unknown agent"


def test_name_and_length_validation():
    kv = KV()
    assert "error" in create_agent(kv, {"name": "ab", "description": "d", "instructions": "i"})
    assert "error" in create_agent(kv, {"name": "okname", "description": "d" * 121, "instructions": "i"})
    assert "error" in create_agent(kv, {"name": "researcher", "description": "d", "instructions": "i"})


def test_custom_agent_cap():
    kv = KV()
    for i in range(MAX_CUSTOM):
        assert "error" not in create_agent(kv, {"name": f"agent-{i}", "description": "d", "instructions": "i"})
    assert create_agent(kv, {"name": "one-more", "description": "d", "instructions": "i"})["error"] == "agent limit reached"
    assert len(list_agents(kv)) == len(BUILTIN) + MAX_CUSTOM


def test_http_router():
    ai, kv = env('{"agent":"coder"}')
    assert len(handle(ai, kv, "GET", "/api/agents")[1]) == 4
    assert handle(ai, kv, "POST", "/api/run", {})[0] == 400
    assert handle(ai, kv, "POST", "/api/agents", {"name": "x"})[0] == 400
    assert handle(ai, kv, "POST", "/api/agents", {"name": "poet", "description": "d", "instructions": "i"})[0] == 201
    s, r = handle(ai, kv, "POST", "/api/run", {"task": "t", "agent": "ghost"})
    assert s == 400 and r["error"] == "unknown agent"
    assert handle(ai, kv, "POST", "/api/run", {"task": "t"})[0] == 200 and "coder" in handle(ai, kv, "GET", "/api/metrics")[1]
