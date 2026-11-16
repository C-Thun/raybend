# `.agents/` — agent-facing material

Everything in this directory exists for the agent, not for the application: it is
committed (except the runtime folders ignored by `framework/.gitignore`) and
reviewed by humans, but it is not part of the shipped product.

| Directory | What it is |
| --- | --- |
| `.agents/memory/` | Long-lived memory: decisions, constraints, finished work, deferred ideas. Rules: `.agents/memory/README.md` |
| `.agents/implementations/` | One record per change (what changed, why, evidence) |
| `.agents/prompts/` | The operator's own words, verbatim (optional; often local-only) |
| `.agents/framework/` | Tool state for `/ask-init`: `state.toml`, snapshots, plans, reports |

The entry point every session reads is `AGENTS.md` in the repository root.
Memory is not loaded automatically; it is read on demand from the index there.
