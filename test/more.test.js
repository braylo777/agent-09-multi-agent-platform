import { test } from "node:test";
import assert from "node:assert/strict";
import { createAgent, listAgents, runTask, route, BUILTIN, MAX_CUSTOM } from "../src/platform.js";

const kv = () => { const m = new Map(); return { get: async (k) => (m.has(k) ? JSON.parse(m.get(k)) : null), put: async (k, v) => m.set(k, v) }; };
const env = (route) => ({ KV: kv(), AI: { run: async (_m, inp) => ({ response: inp.messages[0].content.startsWith("You are a router") ? route : "ok" }) } });

test("four built-in agents", () => assert.deepEqual(BUILTIN.map((a) => a.name), ["researcher", "coder", "copywriter", "analyst"]));
test("router picks a hallucinated agent -> fallback", async () => assert.equal((await route(env('{"agent":"wizard"}'), "x", BUILTIN)).why, "router fallback"));
test("router JSON wrapped in prose is accepted", async () => assert.equal((await route(env('Sure! {"agent":"analyst","why":"pricing"}'), "x", BUILTIN)).agent, "analyst"));
test("forced agent skips the router", async () => {
  let routed = false; const e = { KV: kv(), AI: { run: async (_m, i) => { if (i.messages[0].content.startsWith("You are a router")) routed = true; return { response: "ok" }; } } };
  const r = await runTask(e, "x", "copywriter"); assert.equal(r.agent, "copywriter"); assert.equal(routed, false);
});
test("forcing an unknown agent errors", async () => assert.equal((await runTask(env("{}"), "x", "ghost")).error, "unknown agent"));
test("name and length validation", async () => {
  const e = env("{}");
  assert.ok((await createAgent(e, { name: "ab", description: "d", instructions: "i" })).error);
  assert.ok((await createAgent(e, { name: "okname", description: "d".repeat(121), instructions: "i" })).error);
  assert.ok((await createAgent(e, { name: "researcher", description: "d", instructions: "i" })).error);
});
test("custom agent cap is enforced", async () => {
  const e = env("{}"); for (let i = 0; i < MAX_CUSTOM; i++) assert.ok(!(await createAgent(e, { name: "agent-" + i, description: "d", instructions: "i" })).error);
  assert.equal((await createAgent(e, { name: "one-more", description: "d", instructions: "i" })).error, "agent limit reached");
  assert.equal((await listAgents(e)).length, BUILTIN.length + MAX_CUSTOM);
});
