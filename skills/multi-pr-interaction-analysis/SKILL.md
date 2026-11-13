---
name: multi-pr-interaction-analysis
description: Analyze a related pair of local PR revisions sharing declared runtime resources.
---

Use `osmogrep predict --left <sha-a> --right <sha-b>` (or `--prs <a>,<b>` after
fetching the corresponding refs). Start with one pair. The engine checks a common
base and shared resource changes before asking Git to form a combined tree. It
does not check out that tree, update branches, or run custom merge drivers.

Keep conflicts visible. Record both PR heads, common base, parent order and the
combined commit. Each single change passing does not prove their combination safe.
Only explicit `capacity_covers_concurrency` invariants currently yield automatic
causal hypotheses. Other interaction classes require investigation by the agent.

The current VM API snapshots a checkout. Combined-revision verification therefore
needs that exact commit in a clean, authorized checkout; do not silently verify a
different tree or expand the service's allowed repository folder.
