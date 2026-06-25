---
id: 103
title: "Cross-language capability surface uneven — multi-type params, C++ lifecycle, RT tiers missing in C/C++"
status: open
type: enhancement
area: core
related: [rfc-0019, rfc-0015]
---

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
