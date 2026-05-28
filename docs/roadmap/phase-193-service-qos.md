# Phase 193 — Service / action QoS

**Goal.** Make service / action QoS caller-selectable, mirroring rclcpp / rclc /
rclrs. Today `Session::create_service_{server,client}` take no `QosSettings` —
service QoS is fixed at the RMW layer (Cyclone = RELIABLE+VOLATILE), the C++
`create_service(qos)` arg is discarded, and C has no service QoS at all. Thread
one `QosSettings` per service through the trait + every backend, defaulting to
`services_default()` so stock-`rmw_cyclonedds_cpp` interop is preserved.

**Status.** 193.1 DONE (2026-05-28) — trait + Rust backends + all callers
threaded, behaviour-preserving (default `services_default()`); 7 crates build,
nros-node 146 + cffi tests green. 193.1b (vtable ABI + Cyclone) next. Design:
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
- [ ] **193.1b — C↔C++ vtable ABI bump + Cyclone honours qos.** Add
      `qos: nros_rmw_qos_t` to the cffi vtable `create_service_{server,client}`
      slots + trampolines; Cyclone `service.cpp` uses it instead of the
      hard-coded `NROS_RMW_QOS_PROFILE_SERVICES_DEFAULT` (keep the matched-reader
      gating). Until this lands, the cffi boundary uses `services_default()`
      (behaviour-preserving). **Files:** `nros-rmw-cffi/**`,
      `nros-rmw-cyclonedds/src/{service,qos}.cpp`.
- [ ] **193.2 — nros-node + Rust builder.** `create_service`/`create_client`
      default + `node.service(name).qos(q).build(cb)` / `node.client(name).qos(q)`
      builder (Phase 189 pattern); action-server qos → the three service creates.
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
