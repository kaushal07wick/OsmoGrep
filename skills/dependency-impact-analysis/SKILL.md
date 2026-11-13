---
name: dependency-impact-analysis
description: Query OsmoGrep's committed-revision code graph and resource-linked regression history.
---

Index explicitly with `osmogrep index --commit <sha>`, then query
`osmogrep context impact --commit <sha> --symbol <name>` or the agent's
`query_code_graph` tool. Keep neighborhoods bounded and request an exact revision.
Use `osmogrep context history <resource>` for earlier outcomes.

Rust/Python extraction currently covers top-level definitions. Calls matched by
name are static candidates. Manifest relationships are declarations; timestamped
runtime observations describe an exact deployment. Neither is experimental proof.
Graph failure should end this query and leave the normal coding workflow usable.
