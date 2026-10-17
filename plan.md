# Plan: 本場・脳 honba-nou

## M0 — LLM abstraction (W1–2)
- [ ] LlmProvider::complete
- [ ] OpenAI / Anthropic / Ollama

## M1 — Prompting (W3–4)
- [ ] Context builder
- [ ] Token tracking

## M2 — Decisions & tools (W5–6)
- [ ] OpenAI function calling
- [ ] Confidence fallback to Hold

## M3 — Trader loop (W7–9)
- [ ] Async cycle: data → LLM → risk → execute
- [ ] IST-aware cadence

## M4 — Multi-trader (W10–11)
- [ ] TraderManager, rate limiting, per-trader metrics
