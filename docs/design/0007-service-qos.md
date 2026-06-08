---
rfc: 0007
title: "Service / action QoS — design"
status: Stable
since: 2026-05
last-reviewed: 2026-05
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# Service / action QoS — design

**Status:** design (2026-05-28). Closes the gap in
[`0008-service-qos-gap.md`](0008-service-qos-gap.md) (the `create_service_*` no-QoS path),
which blocks Phase 189.M3.3 (C/C++ service/action QoS + options parity). Seeds a
dedicated phase — this is a breaking RMW-trait change rippling to every backend
plus a C↔C++ vtable ABI bump.

## Upstream reference (rclcpp / rclc / rclrs)

All three give services a **dedicated default profile** —
`rmw_qos_profile_services_default` = **RELIABLE + VOLATILE + KEEP_LAST(10)** (not
the topic default) — and apply **one profile per service to both the request and
reply** endpoints (a service is two DDS topics; rmw uses the single
`rmw_qos_profile_t` for both). They differ only in the override surface:

| Client lib | Service-create shape | Override |
|---|---|---|
| **rclcpp** | `create_service<T>(name, cb, qos = ServicesQoS(), group)` | full `rclcpp::QoS` arg |
| **rclc** | `rclc_service_init_default(...)` / `rclc_service_init_best_effort(...)` | named presets + `rclc_service_init_with_options(..., const rmw_qos_profile_t* qos)` |
| **rclrs** | `create_service::<T,_>(name, cb)` + `ServiceOptions` | `QoSProfile` via the options / fluent name (`IntoPrimitiveOptions`); historically default-only ([ros2-rust#391](https://github.com/ros2-rust/ros2_rust/issues/391)) |

**Semantic reality:** request/reply needs **RELIABLE** — a dropped request hangs
the client — so the services default is RELIABLE and overriding to BEST_EFFORT is
a constrained-network trade (rclc exposes it as an explicit `_best_effort`
init, opt-in). VOLATILE is the norm (services aren't durable). Actions: the
service halves use the services profile; the **status** topic is
TRANSIENT_LOCAL + RELIABLE (`rcl_action_qos_profile_status_default`).

## nano-ros design

### 1. Core: thread one `QosSettings` per service through the trait

`Session::create_service_server` / `create_service_client`
(`nros-rmw/src/traits.rs:852,858`) gain a `qos: QosSettings` param — mirroring
`create_publisher` / `create_subscriber`. **One profile per service**, applied to
both request + reply endpoints (matches `rmw_qos_profile_t` semantics). This is
the breaking change; it ripples to every backend.

The default stays **`QosSettings::services_default()`** (already exists =
`QOS_PROFILE_SERVICES_DEFAULT` = RELIABLE+VOLATILE+KEEP_LAST(10) =
`rmw_qos_profile_services_default`), so stock-`rmw_cyclonedds_cpp` interop is
preserved when callers don't override.

### 2. Backend plumbing

- **Cyclone** (`nros-rmw-cyclonedds/src/service.cpp:728,912`): replace the
  hard-coded `NROS_RMW_QOS_PROFILE_SERVICES_DEFAULT` with the passed qos in
  `make_dds_qos`. **Keep the matched-reader gating** (the
  `dds_get_publication_matched_status` wait) — it is a VOLATILE-write
  safeguard, orthogonal to the profile values (a TRANSIENT_LOCAL service could
  relax it later, but that's a follow-up, not required).
- **C↔C++ vtable (ABI bump):** `nros-rmw-cffi/src/rust_adapter.rs:597,716`
  trampolines + the vtable `create_service_{server,client}` slots have **no qos
  arg**. Add `qos: nros_rmw_qos_t` (the struct already crosses the FFI for
  pub/sub — reuse it). This is the one ABI change; both sides bump together.
- **zenoh** (`nros-rmw-zenoh/src/shim/session.rs:398,441`): pass `qos` to
  `ZenohServiceServer::new` / `ZenohServiceClient::new` (currently `None`); keep
  the `services_default()` liveliness-discovery keyexpr.
- **mock** (`nros-node/src/mock.rs:179,186`): accept + ignore.

### 3. Validation — LANDED (193.5)

Mirrors pub/sub: `qos.validate_against(session.supported_qos_policies())` runs at
the service-create chokepoints — `Node::create_service_sized` /
`create_client_sized` (`node.rs`) and the typed-arena
`register_service_sized_on` / `register_service_client_raw_sized_on`
(`spin.rs`). A backend missing a required policy returns
`TransportError::IncompatibleQos` synchronously at create time rather than
silently ignoring the request — the M3 lesson (C++ silently discarding qos is the
bug we removed). The `services_default()`-only convenience + the param/lifecycle
internal services skip the check (their profile is always supported, so it is a
no-op). Tested: `traits::tests::services_default_validates_and_rejects_missing_policy`.

**Caveat (document, don't enforce).** `validate_against` checks policy
*presence*, not the reliability *value*: it cannot reject a BEST_EFFORT service.
RELIABLE is effectively required for request/reply correctness — a BEST_EFFORT
service is an opt-in trade (rclc-style) the caller takes knowingly, and the
default stays `services_default()` (RELIABLE+VOLATILE) so stock interop is
preserved. The non-default path is covered by the Cyclone `service_roundtrip`
test passing a RELIABLE+KEEP_LAST(5) profile end-to-end.

### 4. Client surfaces (mirror each binding's idiom)

- **Rust (rclrs mirror)** — extend the Phase 189 entity builder:
  `node.service(name).qos(q).build(cb)` + `node.client(name).qos(q)`; convenient
  `create_service` / `create_client` default to `services_default()`. (rclrs is
  moving to options/fluent QoS — the builder is our equivalent.) `nros-node`:
  `create_service_sized` (node.rs:342) gains a qos param / a `_with_qos`
  variant, passed to `session.create_service_server(&info, qos)`.
- **C++ (rclcpp mirror)** — `create_service(out, name, qos = QoS::services())`
  already takes the arg; **stop discarding `_qos`**
  (`nros-cpp/src/service.rs:36,99`) and thread it to
  `create_service_server(&svc_info, qos)`. Same for `create_action_server`.
- **C (rclc mirror)** — keep `nros_service_init` defaulting to services_default;
  add `nros_service_init_with_qos(..., const nros_qos_t* qos)` (+ optional
  `_best_effort` convenience), and the client mirrors. The L1 polling init gets
  the same. This also delivers M3.3's C parity.

### 5. Actions

`create_action_server(qos)` threads `qos` to the **three service** creates
(send_goal / cancel_goal / get_result — `executor/action.rs:105,117,125`); the
**feedback** topic keeps `QOS_PROFILE_DEFAULT` and **status** keeps
`QOS_PROFILE_ACTION_STATUS_DEFAULT` (TRANSIENT_LOCAL) — those are dedicated
profiles, not the service qos. Document that the action qos governs the
service planes only.

## Slices

1. **Core trait + backends** — add `qos` to `create_service_{server,client}`;
   plumb Cyclone (incl. the vtable ABI bump), zenoh, mock; default
   `services_default()`. Build every backend green. *The breaking core change.*
2. **nros-node + Rust builder** — `create_service`/`create_client` default +
   `node.service(name).qos().build(cb)` builder; thread to the trait. Action
   server qos → the three service creates.
3. **C++** — make `create_service(qos)` / `create_action_server(qos)` apply
   (stop discarding `_qos`); options struct (M3.3-cpp).
4. **C** — `nros_service_init_with_qos` / `_with_options` + client mirror
   (M3.3-c); `nros_action_server_init_with_qos`.
5. **Validation + tests** — `validate_against` on the service path; a
   roundtrip test per backend (zenoh + Cyclone) that a non-default (e.g.
   KEEP_LAST depth, or BEST_EFFORT opt-in) profile reaches the wire; document
   the RELIABLE-for-request/reply caveat.

## Risks / constraints

- **RELIABLE is effectively required** for request/reply correctness. BEST_EFFORT
  services are an opt-in trade (rclc-style); document, don't forbid.
- **Stock interop default must stay** RELIABLE+VOLATILE (the Cyclone
  `services_default`) — only *override* when the caller asks.
- **One ABI bump** (the cffi vtable service slots) — coordinate both sides in
  slice 1; bump the vtable version if one is tracked.
- The Cyclone **matched-reader gating** stays regardless of profile.

## Sources

- [ros2-rust #391 — QoS for Clients and Services](https://github.com/ros2-rust/ros2_rust/issues/391)
- [ros2_rust `rclrs/src/qos.rs`](https://github.com/ros2-rust/ros2_rust/blob/main/rclrs/src/qos.rs)
- [ROS 2 — About QoS Settings](https://docs.ros.org/en/rolling/Concepts/Intermediate/About-Quality-of-Service-Settings.html)
