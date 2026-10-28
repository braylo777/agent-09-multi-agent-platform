import { json, limited, body, cap } from "./core.js";
import { listAgents, createAgent, runTask, metrics } from "./platform.js";
import page from "./page.html";

export default {
  async fetch(req, env) {
    const { pathname: p } = new URL(req.url);
    if (req.method === "GET" && p === "/api/agents") return json(await listAgents(env));
    if (req.method === "GET" && p === "/api/metrics") return json(await metrics(env));
    if (req.method === "POST" && (p === "/api/agents" || p === "/api/run")) {
      if (await limited(req, env)) return json({ error: "Rate limit: try again in a minute." }, 429);
      const b = await body(req);
      if (p === "/api/agents") { const r = await createAgent(env, b); return r.error ? json(r, 400) : json(r, 201); }
      if (!cap(b.task)) return json({ error: "task required (<=500 chars)" }, 400);
      try { const r = await runTask(env, b.task, b.agent); return r.error ? json(r, 400) : json(r); } catch (e) { return json({ error: "Agent error: " + e.message }, 502); }
    }
    return new Response(page, { headers: { "content-type": "text/html;charset=utf-8" } });
  },
};
