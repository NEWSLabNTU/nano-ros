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
- [~] **193.2 — nros-node + Rust builder.** *Service server DONE:* threaded
      `qos` through `register_service_sized_on` (typed arena core) + a NodeCtx
      `node.service(name).qos(q).build::<Svc,_>(cb)` builder (Phase 189 pattern)
      + convenient `node_mut(id).create_service::<Svc,_>(name, cb)` (default
      `services_default()`). Test `tests::test_service_builder_qos`. *Remaining
      (193.2b):* `node.client(name).qos(q)` + the raw-service `register_service_raw_*`
      qos param + action-server qos → the three service creates
      (`executor/action.rs`). The session-borrowing `Node::create_service_with_qos`
      can also follow.
- [ ] **193.3 — C++.** Make `create_service(qos)` / `create_action_server(qos)`
      apply (stop discarding `_qos`) + `ServiceOptions` (Phase 189.M3.3-cpp).
- [ ] **193.4 — C.** `nros_service_init_with_qos` / `_with_options` (+ optional
      `_best_effort`) + client mirror + `nros_action_server_init_with_qos`
      (Phase 189.M3.3-c).
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
