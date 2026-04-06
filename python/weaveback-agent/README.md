# weaveback-agent

Typed Python bindings and agent orchestration helpers for weaveback.

This package is intentionally thin:

- Rust owns tracing, context gathering, and verified source edits.
- Python owns typed planning, Pydantic models, and agent-loop composition.
- Edits are expected to go through a typed `ChangePlan`, not arbitrary file writes.
