---
id: 1676
title: "A Rust entry bakes `system_monitors.rs` and never installs it, so a
  contracted Rust image monitors nothing — and so cannot report to `/diagnostics`"
status: open
type: bug
area: [codegen, cli, diagnostics]
severity: medium
found: 2026-10-03
related: [issue-1635, issue-1604, issue-0514, phase-462, rfc-0052]
---

## What happens

`codegen_system` writes `<bake>/system_monitors.rs` for a contracted Rust
image: the `NROS_MONITORS` / `NROS_AGE_MONITORS` tables and an installer,

```rust
pub fn nros_install_monitors(executor: &mut ::nros_node::executor::Executor<'_>) {
    executor.set_monitor_table(NROS_MONITORS);
    executor.set_age_table(NROS_AGE_MONITORS);
}
```

(`packages/cli/nros-cli-core/src/orchestration/model_ingest.rs`). Nothing calls
it:

```
git grep -n 'nros_install_monitors' -- packages examples ':!*.golden'
# only the generator itself
```

`nros::main!` builds the executor and never reaches the baked installer, so a
Rust image's `min_rate_hz` / `max_age_ms` / `max_latency_ms` promises are
checked by nothing on target — not even the detection-time log line (issue
0514's floor), because the tables are empty. phase-462 W1's status line
recorded this as a follow-up ("Rust entries bake system_monitors.rs but
nros::main! never installs it"); no issue tracked it.

Issue 1635 made a contracted C and C++ image publish its violations on
`/diagnostics` by arming a reporter where the monitor tables are installed
(`nros_cpp_install_monitors`), with the executor feeding it at detection
(`Executor::set_violation_sink`). The Rust road has the sink but no install,
so it gets nothing from that fix.

## What a fix needs

* `nros::main!` (and the tiered setup it renders) calls the baked
  `nros_install_monitors` on each executor before entity creation, as the C/C++
  packs call `nros_cpp_install_monitors` — a publisher attaches its counter
  cell at create time.
* The same `/diagnostics` reporter: a `nros`-level twin of `nros-cpp`'s
  `src/diag.rs` (create the publisher, install it with `set_violation_sink`),
  ideally ONE implementation the C ABI then calls, so the three languages
  report through one piece of code.
* Acceptance as 1635's: a contracted Rust image observed by a `/diagnostics`
  subscriber, with a compliant control.
