import { test } from "node:test";
import assert from "node:assert/strict";
import { createAgent, listAgents, runTask, metrics } from "../src/platform.js";

const kv = () => { const m = new Map(); return { get: async (k) => (m.has(k) ? JSON.parse(m.get(k)) : null), put: async (k, v) => m.set(k, v) }; };
const env = (route) => ({ KV: kv(), AI: { run: async (_m, inp) => ({ response: inp.messages[0].content.startsWith("You are a router") ? route : "done by " + inp.messages[0].content.slice(0, 20) }) } });

test("router dispatches and metrics count", async () => {
  const e = env('{"agent":"coder","why":"code task"}');
  const r = await runTask(e, "write fizzbuzz"); assert.equal(r.agent, "coder");
  await runTask(e, "again"); assert.equal((await metrics(e)).coder.runs, 2);
});
test("bad router output falls back", async () => assert.equal((await runTask(env("garbage"), "x")).routing.why, "router fallback"));
test("create custom agent, validate, and route to it", async () => {
  const e = env('{"agent":"chef","why":"food"}');
  assert.ok((await createAgent(e, { name: "Bad Name", description: "d", instructions: "i" })).error);
  await createAgent(e, { name: "chef", description: "Recipes", instructions: "You are a chef." });
  assert.equal((await listAgents(e)).length, 5); assert.equal((await runTask(e, "pasta?")).agent, "chef");
  assert.equal((await createAgent(e, { name: "chef", description: "d", instructions: "i" })).error, "name taken");
});
