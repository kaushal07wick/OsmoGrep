---
name: regression-hypothesis-verification
description: Turn an OsmoGrep prediction with an independent oracle into a bounded differential VM experiment.
---

Read the hypothesis, source and independent contract first. Create an Experiment
JSON with `path`, `source`, `command`, `oracle`, and `oracle_sources`. The path must
be a new root-level `osmogrep_repro_*` file. The test must execute the affected
implementation and must not derive its assertion from a proposed repair.

Submit through `osmogrep predict --base <base> --commit <head> --verify
--experiment <json> --sandbox <configured-bridge>`. Alternatively use
`--generate-experiment` for one budgeted call through the existing model provider.
The stdio MCP `verify_runtime_hypothesis` tool uses the same limits and requires
the server to have been started with an authorized sandbox bridge.

Preserve identical test bytes, command and environment across base and candidate.
Keep original-suite results separate. Infrastructure failures, missing integrity
attestations, stale revisions and unvalidated oracles cannot confirm a bug. Store
exported evidence and close the VM session even after failure. A rejected allowed
folder is a blocker; do not substitute host execution or change the allowlist.

The bridge normalization contract and current external limits are documented in
`docs/prre.md`. No live hosted worker or GitHub delivery is implied by local tests.
