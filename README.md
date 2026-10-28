# agent-09-multi-agent-platform

A small **multi-agent platform**: an agent registry, an LLM router that dispatches tasks, user-defined agents, and usage metrics. It runs serverless on Cloudflare Workers with the free Workers AI tier and KV. Project 9 of a 10-part agentic AI portfolio.

**Live demo:** https://agent-09-multi-agent-platform.braylo777.workers.dev

## What it is
Four built-in agents (`researcher`, `coder`, `copywriter`, `analyst`), plus any agents you create through the API. Send a task: the router picks the best agent and explains why, or you can force a specific one. Per-agent run counts and average latency are tracked at `/api/metrics`.

## How it works
```
POST /api/agents ─▶ registry (KV)  ◀── built-ins
POST /api/run ─▶ ROUTER LLM {agent, why} ─▶ selected agent (its instructions) ─▶ answer
                                                 └─▶ metrics (KV): runs, avg_ms, last
```
- `src/platform.js`: registry, validation (slug names, size limits, max 30 custom agents), router with fallback, executor and metrics.
- New agents are pure configuration, with no deploys: this is the core idea behind agent platforms.
- Guardrails: 5 requests/min per IP, 500-char task cap, bounded custom-agent prompts.

## Run it
```bash
npm install
npm test            # offline unit tests (Node 20+), LLM is mocked
npx wrangler dev    # local; Workers AI runs via your Cloudflare login
npx wrangler deploy # free tier, no paid API keys
```
`wrangler.toml` needs your KV namespace id first: `npx wrangler kv namespace create KV`.

## Stack
Cloudflare Workers · Workers AI (`@cf/meta/llama-3.3-70b-instruct-fp8-fast`, Workers KV) · vanilla JS · node:test

## License
MIT
