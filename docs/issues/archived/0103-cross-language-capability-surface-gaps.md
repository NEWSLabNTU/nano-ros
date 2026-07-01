---
id: 103
title: "C++ lifecycle has no idiomatic wrapper class (drops to extern \"C\") — the last cross-language capability gap"
status: open
type: enhancement
area: core
related: [rfc-0019, rfc-0015, phase-269, phase-270]
---

## Re-audit (2026-07-01) — 2 of 3 hard gaps were already CLOSED; only C++ lifecycle remains

The original 2026-06-26 audit below was **stale/inaccurate** for two of its three hard gaps
(verified against the current tree; also see phase-269 which closed the *entry-codegen*
projections of these capabilities):

- **Hard gap 1 (multi-type params C/C++) — CLOSED.** C `parameter.h` has full typed
  declare/get/set for bool/int/double/string **plus arrays**
  (`nros_param_declare_integer` … `nros_param_get_double_array`, landed Phase 91.C). C++
  `nros::ParameterServer<Cap>` (`parameter.hpp`) is a typed template with
  `declare_parameter<T>`/`get_parameter<T>`/`set_parameter<T>` + `nros::Seq<T,N>` arrays (Phase
  117.9). The "string-only" matrix row was already wrong when the audit was written. *Minor
  residual:* the phase-269 #116 **component live-read** shim (`ctx.parameter::<T>` analog over
  the executor handle) covers int/double/string only — no bool/array — but the param-server API
  itself is complete.
- **Hard gap 3 (RT tiers C/C++) — CLOSED.** Both C (`nros_generated.h`:
  `nros_executor_create_sched_context` / `..._bind_handle_to_sched_context` + `enum
  nros_sched_priority_t`) and C++ (`sched_context.hpp`: `create_sched_context` / `bind_*` +
  `enum class Priority { Critical, Normal, BestEffort }`) have the full create/manage/bind +
  priority-bucket surface (Phase 110.B). The audit **cited the wrong path**
  (`nros-c/include/nano_ros/*.h` — the headers live under `nros-c/include/nros/`), which is why
  it reported "C has no scheduling API." phase-269 #119 layered entry-level tier codegen on top.
- **Semantic "declarative vs manual wiring" — mostly resolved.** phase-269 #116/#117 added
  entry-codegen that auto-wires param-services + lifecycle-services + autostart for C/C++
  entries, so the manual `nros_executor_register_*` asymmetry is gone on the declarative entry
  path.

**Still open (the real remaining gap): hard gap 2 — C++ lifecycle has no idiomatic wrapper
class.** No `nros::LifecycleNode` in `nros-cpp` (`rclcpp_compat.hpp:32` lists it as "Phase
209.H (deferred)"). phase-269 #117 added only entry-level `[lifecycle]` autostart codegen, not a
user-facing class; a C++ managed node authoring transition behavior still drops to the
`extern "C"` `nros_cpp_lifecycle_*` / C-ABI functions (and the C++ shim lacks the
`register_on_configure/activate/...` callbacks the C side has). Fix direction unchanged: add a
thin `nros::LifecycleNode` wrapper over the complete C state machine (mechanical).

**Planned as [phase-270](../roadmap/phase-270-cpp-lifecycle-node-wrapper.md)** — an rclcpp-shape
`nros::LifecycleNode` base class (inherit + override `on_*` returning `CallbackReturn`) over new
no_std `nros_cpp_lifecycle_{get_state,register_on_*}` FFI shims; freestanding-safe (non-pure
virtuals, no RTTI/exceptions). Resolves this issue when its W3 e2e is green. Everything below is the
original (partly-stale) audit, kept for provenance.


## Summary

A 2026-06-26 audit of the three language API surfaces (`nros-node`/`nros` for Rust,
`nros-c/include/nano_ros/*.h` for C, `nros-cpp/include/nros/*.hpp` for C++) found the core
entity APIs (pub/sub/timer/service/action/QoS/bridge/logging) fully present in all three, but
several advanced capabilities are Rust-complete and missing or partial in C/C++.

## Capability × language matrix (gaps only)

| Capability | Rust | C | C++ |
| --- | --- | --- | --- |
| Parameters — multi-type (bool/int/double/string/array) | ✅ | ⚠️ string-only | ⚠️ string-only |
| Lifecycle (REP-2002) API | ✅ | ✅ | ❌ no wrapper (`extern "C"` to C only) |
| RT tiers / callback-group priority | ✅ `SchedContext` (High/Normal/Low) + OS dispatcher | ❌ none | ⚠️ `SubscriptionOptions::sched_context` is affinity-only, no priority |
| Param services registration | ✅ declarative (`param-services` feature) | ⚠️ manual `nros_executor_register_parameter_services()` | ❌ (call C) |
| Lifecycle services registration | ✅ declarative (`lifecycle-services` feature) | ⚠️ manual `nros_executor_register_lifecycle_services()` | ❌ (call C) |

(All others — publisher, subscription, timer, service client/server, action client/server,
QoS + overrides, safety/CRC, multi-host/bridge, logging — are present in all three.)

## Hard gaps

1. **Multi-type parameters (C, C++).** The C param server stores strings only
   (`nros_param_{declare,get,set}_string`); C++ `ParameterServer<Cap>` is likewise
   string-focused. A node needing typed params (int/double/bool/array) must be Rust-authored.
   Rust has the full `ParameterVariant` set.
2. **C++ lifecycle has no native API.** C exposes `nros_lifecycle_*` + `nros_make_node_a_
   lifecycle_node` (REP-2002 state machine + services); C++ ships no wrapper class — a C++
   managed node must drop to the C functions via `extern "C"`. Asymmetric with how C++ wraps
   every other capability in a class.
3. **RT tiers absent in C, affinity-only in C++.** Rust has `SchedContext` priority buckets +
   `register_os_priority_dispatcher`. C has no scheduling API. C++ can bind an entity to a
   numeric `sched_context` id but cannot create/manage contexts or express priority — so
   priority-based scheduling (RFC-0015 tiers) is effectively Rust-only at the API level even
   though the orchestration IR resolves tiers language-agnostically.

## Semantic inconsistencies (not missing, but divergent)

- **Declarative vs manual wiring.** Rust auto-wires param-services + lifecycle-services via
  cargo features; C/C++ require explicit `nros_executor_register_*` calls. Same capability,
  different ergonomics — easy to forget the manual call in C/C++.
- **Logging handle shape.** Rust `node.logger()` → `Logger` object; C++ `node.get_logger()`
  → opaque handle. Minor, but the C++ handle is less transparent.

## Fix direction

Decide per capability whether it's a real gap to close or an accepted asymmetry:
- **Multi-type params:** add typed C-ABI param entry points (`nros_param_{get,set}_{int,double,
  bool}` + array) and a C++ typed `ParameterServer`, or document string-only as the embedded
  contract (note: issue #80 already tracks param persistence; coordinate).
- **C++ lifecycle:** add a thin `nros::LifecycleNode` wrapper over the existing C state machine
  (mechanical — the C side is complete).
- **RT tiers in C/C++:** expose `SchedContext` create/bind + priority through the CFFI so C/C++
  nodes can participate in tiers, or explicitly scope tiers as Rust-only and say so in RFC-0015.
- **Declarative services:** give C/C++ a one-call/auto path for param + lifecycle service
  registration to match Rust's feature-gated declarative wiring.

This is an enhancement (parity), not a correctness bug — sequence after the #98/#101 boot-config
unification and #102 coverage work.

## Evidence

2026-06-26 capability audit; per-language API file:line citations in that audit
(`nros-c/include/nano_ros/*.h`, `nros-cpp/include/nros/{node,parameter,...}.hpp`,
`nros-node/src/executor/sched_context.rs`, `.../parameter_services.rs`, `.../lifecycle.rs`).
