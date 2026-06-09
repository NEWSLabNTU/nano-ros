---
rfc: 0037
title: "Rust and C user-API surfaces"
status: Draft
since: 2026-06
last-reviewed: 2026-06
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# RFC-0037 — Rust and C user-API surfaces

## Summary

The C++ user API is frozen by RFC-0018 (Stable). The **Rust** (`nros-node` /
`nros-core`) and **C** (`nros-c`) user APIs have no equivalent surface RFC —
RFC-0019/0020 govern the C *thin-wrapper discipline* (not the surface), and
RFC-0022 describes a *planned* Rust entity-tier refactor (not the current
surface). This RFC **records the current Rust and C user surfaces** as the
reference, so consumers and reviewers have a citable contract. It is `Draft`
deliberately: the Rust surface should not flip to `Stable` until RFC-0022's
entity-tier builder model lands (or is dropped), because that refactor would
revise it; the C surface is closer to freezable.

## Motivation / problem

- Examples, tests, and the C/C++ shims all depend on these surfaces, but a
  reviewer has no single doc to check a change against — only the code.
- Without a recorded surface, accidental breaking changes (renamed `create_*`,
  changed error variants) pass review unnoticed.

## Design

### Rust surface (`nros-node`, `nros-core`)

**Entry / lifecycle.** `Executor::open(&ExecutorConfig)` /
`Executor::open_multi(&[SessionSpec])` → `Executor`; `create_node(name) ->
NodeCtx<'_>`. Domain id is in the config / baked (RFC-0036). No `Arc<Node>` —
`NodeCtx<'_>` borrows `&mut Executor` (RFC-0022).

**Entities** (mirroring rclrs `create_*`):
- `create_publisher_on::<M>(...) -> Publisher<M>` — `Publisher::publish(&M)`.
- `create_subscription*` — callback `FnMut(&M)` (owned today; borrowed view is
  RFC-0033 § borrowed / phase-229.6).
- `register_service::<Svc, F>` / `register_service_client_raw`.
- action client/server via `Promise` handles.
- `register_timer::<F>`.

**Spin** (RFC-0021 blocking rule — helpers take `&mut Executor`):
`spin_once(timeout) -> SpinOnceResult`, `spin(timeout) -> !`,
`spin_blocking(SpinOptions)`. Service/action waits: `promise.wait(&mut executor,
timeout)`.

**Message contracts** (`nros-core`): trait `RosMessage: Serialize + Deserialize`,
`RosService`, `RosAction`; the executor takes `M: MessageForRmw`. Value types:
`Time`, `Duration`, `MessageInfo`, `GoalStatus`, `LifecycleState`.

**Error:** `NodeError` (enum: `Max*Reached`, `SerializationFailed`,
`DeserializationFailed`, `BufferTooSmall`, `TransportError`, `NotConnected`,
`*NameTooLong`). Distinct from `nros-core`'s `NanoRosError`/`RclReturnCode`
(RFC-0036) — `NodeError` is the node-builder layer.

### C surface (`nros-c`)

~120 `nros_*` functions (cbindgen → `nros_generated.h` + hand-written
`visibility.h`/`platform.h`/`types.h`), mirroring rclc:

- init: `nros_executor_init`, `nros_node_init`, `nros_support_init`.
- pub/sub: `nros_publisher_init[_with_qos|_with_options]`, `nros_publish_raw`,
  `nros_subscription_init[_with_qos|_polling]`.
- services/clients: `nros_service_init[_polling]`, `nros_service_send_reply_raw`,
  `nros_client_init`, `nros_client_send_request_raw`, `nros_client_call`.
- actions: `nros_action_{server,client}_init`, `nros_action_send_goal`,
  `nros_action_get_result`.
- executor: `nros_executor_register_{subscription,timer}`,
  `nros_executor_spin[_some]`.
- guard/lifecycle/params: `nros_guard_condition_*`, `nros_lifecycle_*`,
  `nros_param_*`.

Entities are opaque structs (`nros_node_t`, `nros_publisher_t`, …). Error:
`nros_ret_t` enum. The C layer is a **thin wrapper** — no logic re-impl
(RFC-0019); it delegates to `nros-node`.

### Freeze policy

- **C surface** — eligible to flip to `Stable` once the RFC-0019 opaque-entity
  refactor settles; at that point this RFC lists the frozen function set and an
  add-only rule (new `nros_*` fns append; signatures of existing fns are stable).
- **Rust surface** — stays `Draft` until RFC-0022 resolves. When it does, freeze
  the resulting `create_*` / `spin*` / `NodeError` shape here.
- Both: breaking a recorded signature requires updating this RFC in the same PR.

## Alternatives considered

- **One combined API RFC (Rust + C + C++).** Rejected — C++ already has the
  Stable RFC-0018; merging would muddy its status. Keep per-language.
- **Freeze the Rust surface now.** Rejected — RFC-0022's entity-tier builder
  would immediately supersede it; recording-as-Draft avoids a churned Stable doc.

## Open questions

1. Does RFC-0022 land as specced (fork/clone tiers), get trimmed, or get
   dropped? The Rust freeze waits on this.
2. Should `NodeError` and `NanoRosError`/`RclReturnCode` be unified, or is the
   two-layer split (builder vs core) intentional? Proposed: keep split; document
   here.

## Changelog

- 2026-06 — created (Draft). Recorded the current Rust (`nros-node`/`nros-core`)
  and C (`nros-c`) user surfaces and the freeze policy; C++ remains RFC-0018.
