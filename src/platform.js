import { ask, parseJSON } from "./core.js";

// Agent platform: a registry of agents (built-in + user-created), an LLM router that
// dispatches each task to the best agent, and per-agent usage metrics.
export const BUILTIN = [
  { name: "researcher", description: "Explains topics and facts with clear bullet points", instructions: "You are a precise researcher. Answer with concise, factual bullet points and flag uncertainty." },
  { name: "coder", description: "Writes and explains code", instructions: "You are a senior software engineer. Reply with a short explanation and a minimal, correct code sample." },
  { name: "copywriter", description: "Writes marketing copy, emails and social posts", instructions: "You are a punchy copywriter. Write persuasive, plain-English copy. Keep it short." },
  { name: "analyst", description: "Business analysis, pricing, pros/cons and decisions", instructions: "You are a pragmatic business analyst. Give a structured recommendation with numbers where possible." },
];
export const MAX_CUSTOM = 30;
const slug = (s) => /^[a-z][a-z0-9-]{2,24}$/.test(s || "");

export async function listAgents(env) { return [...BUILTIN.map((a) => ({ ...a, builtin: true })), ...((await env.KV.get("agents", "json")) || [])]; }
export async function createAgent(env, a) {
  if (!slug(a.name)) return { error: "name: 3-25 chars, lowercase letters, digits, dashes" };
  if (!a.description || a.description.length > 120 || !a.instructions || a.instructions.length > 600) return { error: "description (<=120) and instructions (<=600) required" };
  const custom = (await env.KV.get("agents", "json")) || [];
  if ([...BUILTIN, ...custom].some((x) => x.name === a.name)) return { error: "name taken" };
  if (custom.length >= MAX_CUSTOM) return { error: "agent limit reached" };
  const agent = { name: a.name, description: a.description, instructions: a.instructions, created: new Date().toISOString() };
  await env.KV.put("agents", JSON.stringify([...custom, agent])); return agent;
}
export async function route(env, task, agents) {
  const r = parseJSON(await ask(env, `You are a router. Pick the single best agent for the task. Agents:\n${agents.map((a) => `- ${a.name}: ${a.description}`).join("\n")}\nReply ONLY JSON: {"agent": name, "why": short reason}`, task, { max_tokens: 80 }), {});
  return agents.find((a) => a.name === r.agent) ? r : { agent: agents[0].name, why: "router fallback" };
}
export async function runTask(env, task, forced) {
  const agents = await listAgents(env), t0 = Date.now();
  const routing = forced ? { agent: forced, why: "chosen by user" } : await route(env, task, agents);
  const agent = agents.find((a) => a.name === routing.agent);
  if (!agent) return { error: "unknown agent" };
  const answer = await ask(env, agent.instructions, task, { max_tokens: 450 });
  const ms = Date.now() - t0;
  const m = (await env.KV.get("metrics", "json")) || {};
  const s = m[agent.name] || { runs: 0, total_ms: 0 };
  m[agent.name] = { runs: s.runs + 1, total_ms: s.total_ms + ms, last: new Date().toISOString() };
  await env.KV.put("metrics", JSON.stringify(m));
  return { agent: agent.name, routing, answer, ms };
}
export async function metrics(env) {
  const m = (await env.KV.get("metrics", "json")) || {};
  return Object.fromEntries(Object.entries(m).map(([k, v]) => [k, { runs: v.runs, avg_ms: Math.round(v.total_ms / v.runs), last: v.last }]));
}
