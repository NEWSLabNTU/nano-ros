---
id: 308
title: "A model's `qos_overrides.*` configured QoS on a C++ image and silently nothing on a C or Rust one"
status: resolved
type: bug
severity: high
area: codegen, runtime
related: [issue-0303, issue-0306, rfc-0050]
---

## Filed retroactively (2026-07-28)

Found and fixed on 2026-07-27 (`5ffb1d2cf`) and recorded only in that commit
plus the phase-296 doc. It shipped, it was cross-language, and it is the
divergence the SystemModel exists to prevent — so it belongs in the issue
series.

## The defect

Phase 211.H (issue #52) lowered `qos_overrides.<topic>.<role>.<policy>`
parameters into a typed table and applied it. Only the **C++** emitter ever
consumed that table:

- `emit_cpp` emitted the static table + `set_qos_overrides` — correct.
- `emit_c` had no QoS code at all. Its only `qos_overrides` mentions were
  `Vec::new()` in test fixtures.
- The **Rust** path had no mechanism whatsoever: overrides existed on
  `NodeHandle` (the session-borrowing type the C++ FFI uses), while Rust
  components install through the EXECUTOR (`install_node_typed` →
  `ExecutorSink` → `Executor::node_mut`), which had no override support
  anywhere.

So one model produced three different systems: QoS configured on C++, ignored
on C and Rust. The `multi-node-workspace-cpp` template model carries such a
param, so this was reachable from a shipped template.

A related half was found in the same sweep and fixed in `bf860800d`:
`plan_from_model` built every `PlanNode` with `qos_overrides: Vec::new()`, so
even the C++ emitter received an empty table on the MODEL path — the
decomposition only ran on the retired launch path. Both halves were needed.

## Fix (2026-07-27, `5ffb1d2cf` + `bf860800d`)

- `plan_from_model` decomposes `qos_overrides.*` out of the resolved params.
- `emit_c` emits the table + `nros_cpp_node_set_qos_overrides` (C entries create
  nodes through the same FFI as C++ ones), sharing the C++ emitter's code
  lowering rather than respelling it.
- Rust gained the mechanism: `NodeRecord.qos_overrides` +
  `Executor::set_node_qos_overrides`, folded in the publisher and subscription
  preludes BEFORE `validate_against`, so an override the backend cannot honour
  errors loudly instead of being dropped. `nros::main!` bakes the table into
  `RuntimeCtx`; `nros::node!`'s register seam installs it.

## Why it was invisible

No test asserted an override's EFFECT on a running system in any language — the
phase-211.H fixture proves the fold (`table → entity`) using a hand-built table
via `NodeHandle`, never the bake path. `tests/qos_override_e2e.rs` now closes
that: a stock `rmw_zenoh_cpp` peer reports the advertised profile of an entry
baked from a committed model.

Two follow-on defects fell out of the same area and are filed separately:
issue 0303 (unmodelled policies dropped silently) and issue 0306 (the component
runtime discarded a node's own declared QoS).
