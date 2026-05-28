# Phase 193 — Service / action QoS

**Goal.** Make service / action QoS caller-selectable, mirroring rclcpp / rclc /
rclrs. Today `Session::create_service_{server,client}` take no `QosSettings` —
service QoS is fixed at the RMW layer (Cyclone = RELIABLE+VOLATILE), the C++
`create_service(qos)` arg is discarded, and C has no service QoS at all. Thread
one `QosSettings` per service through the trait + every backend, defaulting to
`services_default()` so stock-`rmw_cyclonedds_cpp` interop is preserved.

**Status.** 193.1 DONE (2026-05-28) — trait + Rust backends + all callers
threaded, behaviour-preserving (default `services_default()`). 193.1b DONE — the C↔C++
vtable ABI bump + Cyclone honours the profile (service_roundtrip green). The
core is now end-to-end caller→Cyclone-wire. 193.2–193.5 (bindings + validation)
remain. Design:
[`docs/design/service-qos.md`](../design/service-qos.md); gap:
[`docs/design/service-qos-gap.md`](../design/service-qos-gap.md).

**Priority.** P2 — unblocks Phase 189.M3.3 (C/C++ service/action QoS + options
parity); no MVP capability depends on it (services already work at the fixed
default).

**Depends on.** Phase 189 (entity builder — the Rust service-QoS surface extends
it). Touches the RMW trait + every `Session` backend + a C↔C++ vtable slot.

## Overview

Upstream all default services to `rmw_qos_profile_services_default`
(RELIABLE + VOLATILE + KEEP_LAST(10)), apply **one profile per service** to both
request + reply, and differ only in the override surface (rclcpp full `QoS` arg /
rclc `_default`/`_best_effort` presets + `_with_options` / rclrs `ServiceOptions`
+ fluent name). See the design doc for the reference table + rationale.

## Architecture

One `QosSettings` per service, threaded `Session::create_service_*` → backend,
exactly like `create_publisher`/`create_subscriber`. Default
`services_default()`. RELIABLE is effectively mandatory for request/reply;
BEST_EFFORT is an opt-in trade. Actions: the qos governs the three service planes
(send_goal / cancel_goal / get_result); feedback/status keep their dedicated
profiles.

## Work items

- [x] **193.1 — Core trait + Rust backends (the breaking change). DONE.**
      Add `qos: QosSettings` to `create_service_server` / `create_service_client`
      on the `Session` trait; update every `impl Session` (mock = ignore, zenoh
      shim = apply, cffi adapter = accept) and every caller (node.rs / action.rs
      pass `services_default()` — they don't expose qos yet, that's 193.2). Build
      every backend green; **behaviour-preserving** (default == the old fixed
      profile). **Files:** `nros-rmw/src/traits.rs`,
      `nros-rmw-zenoh/src/shim/session.rs`, `nros-node/src/mock.rs`,
      `nros-rmw-cffi/src/rust_adapter.rs`, `nros-node/src/executor/{node,action}.rs`.
- [x] **193.1b — C↔C++ vtable ABI bump + Cyclone honours qos. DONE.**
      `qos: *const NrosRmwQos` added to the cffi vtable
      `create_service_{server,client}` slots (`rmw_vtable.h` + `lib.rs`),
      `CffiSession` (builds `NrosRmwQos::from(qos)`), the rust_adapter
      trampolines (`qos_from_cffi`), and all Rust + C++ stubs/callers. Cyclone
      `service.cpp` now uses `qos != nullptr ? *qos : SERVICES_DEFAULT` for both
      the request reader + reply writer (matched-reader gating kept). Verified:
      cffi builds + 13 test binaries pass; Cyclone RMW + tests build [100%];
      `service_roundtrip` runs green (`OK 7+11=18`). Non-default service QoS now
      reaches the DDS wire.
- [x] **193.2 — nros-node + Rust builder.** Threaded `qos` through
      `register_service_sized_on` (typed arena core) + a NodeCtx
      `node.service(name).qos(q).build::<Svc,_>(cb)` builder (Phase 189 pattern)
      + convenient `node_mut(id).create_service::<Svc,_>(name, cb)` (default
      `services_default()`). Test `tests::test_service_builder_qos`.
- [x] **193.2b — Rust client + Node session-path QoS. DONE.** Node
      `create_service_with_qos` / `create_client_with_qos` (rclcpp-style qos
      overload; `create_service_sized`/`create_client_sized` now take `qos`,
      convenience defaults to `services_default()`). Test
      `tests::test_node_service_client_with_qos`; 148 nros-node tests pass.
      *Deferred (193.2c):* the raw-fn-ptr `register_service_raw_*` /
      `register_service_client_raw_*` qos param + action-server qos → the three
      service creates — both ripple into the C / generator / C++ paths, so they
      land with 193.3 (C++) / 193.4 (C) where those surfaces gain qos.
- [~] **193.3 — C++.** *Service server + client DONE:* the FFI
      `nros_cpp_service_{server,client}_create` now apply the caller's QoS
      (`qos.to_qos_settings()` → `session.create_service_{server,client}`,
      stopping the discard) — so C++ `create_service(name, qos)` reaches the
      backend. *193.3b DONE:* `create_action_server(qos)` now applies —
      `nros_cpp_action_server_create` stores the qos on `CppActionServer`, and
      `_register` passes `qos.to_qos_settings()` to
      `register_action_server_raw_sized_on` → the three goal/cancel/result
      service servers (the paired `CppActionServerLayout` mirror in
      `nros/src/sizes.rs` updated for the layout assert). *Remaining:* the
      `ServiceOptions` / `ActionServerOptions` named-options struct (Phase
      189.M3.3-cpp — ergonomic; the `qos` arg already works).
- [~] **193.4 — C.** *Service server DONE:* `register_service_raw_sized{,_on}`
      gained a `qos` param (193.2c, behaviour-preserving default
      `services_default()`); `nros_service_t` carries a `qos` field
      (defaults to the services profile) read by `nros_executor_register_service`;
      new `nros_service_init_with_qos(..., const nros_qos_t* qos)` sets it. The
      generator emits the qos arg (codegen `ab2c4eb`). nros-c builds, header has
      `nros_service_init_with_qos`. *193.4b DONE:* the client mirror
      (`nros_client_init_with_qos`: `nros_client_t.qos` field +
      `nros_executor_register_client` reads it → `register_service_client_raw_*`)
      + `nros_action_server_init_with_qos` (`nros_action_server_t.qos` →
      `register_action_server_raw_sized_*` → the three service servers; raw
      client/action register cores threaded with qos in nros-node, generator
      emits the action qos arg, codegen `4212eb0`). nros-c header exposes both
      new symbols.
- [ ] **193.5 — Validation + tests.** `validate_against` on the service path;
      a per-backend roundtrip test that a non-default profile reaches the wire;
      document the RELIABLE-for-request/reply caveat.

## Acceptance

A caller can select service/action QoS in Rust / C / C++; the chosen profile
reaches the backend (verified per backend); the default stays
`services_default()` (stock interop preserved); no silent-discard. Phase 189.M3.3
unblocked.

## Notes

- RELIABLE effectively required for request/reply; BEST_EFFORT opt-in (rclc-style).
- One ABI bump (193.1b, the cffi vtable service slots) — bump the vtable version
  if tracked.
- The Cyclone matched-reader gating stays regardless of profile.
