---
rfc: 0008
title: "Service / action QoS gap (the `create_service_*` no-QoS path)"
status: Superseded
since: 2026-05
last-reviewed: 2026-06
implements-tracked-by: [phase-189]
supersedes: []
superseded-by: 0007
---

# Service / action QoS gap (the `create_service_*` no-QoS path)

> **Superseded by [RFC-0007](0007-service-qos.md) (2026-06).** Phase 189.M3.3
> (service/action QoS + options parity) landed (COMPLETE 2026-05-29). The gap is
> resolved: pub/sub-style QoS is threaded where it applies, and the service/result
> plane QoS is **fixed at the RMW layer by design** (e.g. Cyclone RELIABLE+VOLATILE),
> with feedback/status planes honoring QoS. This doc is retained as historical
> gap-analysis; the live design is RFC-0007.

**Status:** Superseded by RFC-0007. Surfaced by Phase 189.M3; resolved by
Phase 189.M3.3 (done 2026-05-29).

## The gap

The `Session` trait (`packages/core/nros-rmw/src/traits.rs`) threads
`QosSettings` through the **pub/sub** create methods but **not** the service
ones:

```rust
fn create_publisher(&mut self, topic: &TopicInfo, qos: QosSettings) -> ...;     // qos ✓
fn create_subscription(&mut self, topic: &TopicInfo, qos: QosSettings) -> ...;    // qos ✓
fn create_service(&mut self, service: &ServiceInfo) -> ...;              // NO qos
fn create_client(&mut self, service: &ServiceInfo) -> ...;             // NO qos
```

So **service QoS is fixed at the RMW layer**, not caller-selectable:

- **Cyclone** hard-codes service QoS = **RELIABLE + VOLATILE** (`src/qos.cpp`;
  see the Phase 117 / 171.0.a notes in the root `CLAUDE.md` — a request written
  before the client writer matches the server's request reader is silently
  dropped under VOLATILE, which is why `service.cpp` gates the first write on
  `publication_matched`).
- Actions are built on services + topics; the **service halves** (goal / result
  / cancel) inherit this no-QoS path, while the feedback/status **topic** halves
  do flow QoS through `create_publisher`/`create_subscription`.

## Where it bites

- **C++** `Node::create_service(qos)` / `create_action_server(qos)` accept a
  `QoS` argument but **discard it** — it never reaches the backend (the
  `create_service` FFI has nowhere to put it). The API shape promises a
  knob that does nothing.
- **C** service/action init take **no QoS at all** (`nros_service_init`,
  `nros_action_server_init`, …) — consistent with the core, but a parity gap vs
  C++'s (ineffective) QoS arg.
- **Phase 189.M3.3** (C/C++ service/action named-options parity) is therefore
  blocked: exposing `_with_qos` / `_with_options` on the C service/action
  surface — and making C++'s existing `qos` arg actually apply — requires the
  core to carry the setting first.

## What a fix needs (a dedicated slice, before M3.3)

1. **Thread `QosSettings` into the service create path.** Either add a `qos`
   param to `create_service` / `create_client` (mirrors pub/sub),
   or carry it on `ServiceInfo` (`ServiceInfo<'a>` already bundles the
   service-name/type metadata — a QoS field is the lower-churn option, but a
   param is the more honest mirror of the pub/sub signature).
2. **Plumb it through every backend's `create_service`** — Cyclone
   (`packages/dds/nros-rmw-cyclonedds`), zenoh-pico, XRCE — replacing the
   hard-coded RELIABLE+VOLATILE with the caller's profile (keeping that as the
   *default*, since it is the stock-`rmw_cyclonedds_cpp`-interop requirement).
   Backends that can't honour a profile keep their fixed QoS + should surface
   `RET_UNSUPPORTED` on mismatch rather than silently ignoring.
3. **Surface it on the executor service-create** (`register_service_raw_sized`
   et al.) + the C/C++ FFI (`nros_cpp_service_server_create`,
   `nros_service_init_with_qos`).
4. **Then M3.3** can add the C `_with_qos` / `_with_options` variants and make
   C++'s `create_service(qos)` actually apply.

## Interim contract

Until the above lands: service/action QoS is **whatever the active RMW fixes it
to** (Cyclone = RELIABLE+VOLATILE). Callers should not rely on a passed `QoS`
having any effect on the service/result/cancel planes; the
feedback/status **topic** planes do honour QoS via the pub/sub path. Document
this in any service example that takes a QoS argument.
