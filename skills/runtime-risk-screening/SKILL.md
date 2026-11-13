---
name: runtime-risk-screening
description: Screen an OsmoGrep change for runtime risk before model reasoning or VM execution.
---

Run `osmogrep predict --base <base> --commit <head> --mode screen` in the intended
repository. Use fetched local references; this command does not authenticate or
fetch PRs. A documentation skip must use zero model calls and zero experiments.

Treat high risk as a routing decision, not a confirmed finding. Configuration,
dependencies, retries, timeouts and small diffs can be risky. If a change has no
explicit runtime mapping, report that limit instead of inventing a resource.
Use the report's exact revisions when requesting deeper analysis.
