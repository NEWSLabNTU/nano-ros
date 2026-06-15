---
id: 67
title: Rust typed CycloneDDS publisher creation fails (PublisherCreationFailed) — native rust cyclone talker/listener
status: resolved
type: bug
area: rmw
related: [phase-248, phase-249, issue-0057, issue-0068]
resolved_in: "2026-06-15 — re-expose nros/rmw-cyclonedds descriptor-hook marker + fix example/board manifests"
---

> **RESOLVED (2026-06-15).** Root cause: phase-248 C5c removed the `nros/rmw-cyclonedds`
> feature, which was the **only** activator of `nros-node/__cyclonedds-link` →
> `cfg(rmw_cyclonedds_present)`. With it gone, `cyclonedds_register::register_type::<M>`
> compiled to a no-op for every rust app → the Int32 descriptor was never built via
> the runtime seam → the C++ `dds_create_topic` had no descriptor → `PublisherCreationFailed`.
> (phase-248 correctly removed the concrete-backend *dep*, but orphaned the functional
> *cfg marker* the typed-descriptor hook needs — they are distinct.)
>
> **Fix:** re-expose a **marker-only** `rmw-cyclonedds = ["nros-node/__cyclonedds-link"]`
> on `nros` (NO concrete-RMW dep — the backend stays app-owned via
> `dep:nros-rmw-cyclonedds-sys`, so phase-248 agnosticism holds), and point the
> affected manifests' `rmw-cyclonedds` feature at it (`["nros/rmw-cyclonedds",
> "dep:nros-rmw-cyclonedds-sys"]`): 12 native rust examples + 2 boards
> (fvp-aemv8r-smp, s32z270dc2-r52). `custom-msg` reverted — its hand-written
> `SensorReading` doesn't impl `nros_serdes::schema::Message` (the bound
> `rmw_cyclonedds_present` adds), and typed cyclone is a non-goal for it (its cyclone
> variant is never built as a fixture). nros-board-native has no `nros` dep to forward
> through; an app on it enables `nros/rmw-cyclonedds` on its own `nros` dep.
>
> **Validated locally:** the rust cyclone talker now publishes (C listener receives
> 5/5); the 4 `native_api` cyclone talker↔listener tests (C+Cpp) PASS; all 12 typed
> cyclone examples compile under the new `M: Message` bound; the default (zenoh) build
> + `nros --features rmw-cyclonedds` check are clean.
>
> The 5th failure originally lumped here — `test_cyclonedds_action_nano_server_ros2_client`
> ("Goal was rejected") — was MIS-ATTRIBUTED: it is the **C** action server + ROS 2
> path (static `descriptors.cpp`, not the rust seam), a separate regression split out
> to [issue 0068](0068-cyclonedds-ros2-action-goal-rejected.md).

## Symptom

A native **Rust** CycloneDDS binary panics at `create_publisher::<M>()`:

```
thread 'main' panicked at src/main.rs:82:14:
Failed to create publisher: Transport(PublisherCreationFailed)
```

Reproduced standalone (domain 91):

```
ROS_DOMAIN_ID=91 examples/native/rust/talker/target-cyclonedds/nros-fast-release/talker
  → Node created: talker
  → PublisherCreationFailed   (core dumped)
```

Surfaced by the #57 local lane validation (2026-06-15). The 5 real failures it
left, all this one root cause:

- `native_api::test_native_cyclonedds_rust_talker_to_listener::{C,Cpp}` — "Expected
  at least 2 CycloneDDS samples from Rust talker, got 0" (the rust talker dies at
  publisher creation, so the C/C++ listener gets nothing).
- `native_api::test_native_cyclonedds_talker_to_rust_listener::{C,Cpp}` — the rust
  cyclone *listener* side (same typed-creation path for the subscription/return).
- `cyclonedds_ros2_interop::test_cyclonedds_action_nano_server_ros2_client` — the
  nano action server's typed cyclone pubs.

## Not fixture-absent, not the C path

- The **C** cyclone path is fine: `c_talker → c_listener` on a shared domain
  delivers 4/4 (C++ generated `descriptors.cpp` table).
- The rust binary is fully wired: `nm` shows `nros_rmw_cyclonedds_register`,
  `nros_rmw_cyclonedds_sys::register`, `install_descriptor_registrar`,
  `cyclonedds_type_descriptor_registrar`, the `TypeRegistry` symbols, and
  `__FORCE_LINK_PLATFORM_CFFI` — the backend + descriptor registrar ARE linked.
- The backend registers (the node is created OK, so the vtable resolved — not a
  `NoBackend`), and `install_descriptor_registrar()` is called from BOTH
  `nros-rmw-cyclonedds-sys::register()` (lib.rs:90) and the backend-init macro
  (lib.rs:106). So the registrar is installed by publisher-creation time.

So the failure is **inside** the rust typed publisher-creation flow
(`register::<M>()` building the descriptor → the cyclone writer create), not the
registration trigger and not a missing fixture.

## Likely origin

phase-248 (C2 — descriptor registrar moved into the generic `nros_rmw` seam) /
phase-249 (registration-trigger rework, linkme deletion) churn around the typed
descriptor seam (`nros-node/src/cyclonedds_register.rs`,
`nros-rmw-cyclonedds/src/type_registry.rs`,
`nros-rmw/src/type_descriptor.rs`). The same `PublisherCreationFailed` was seen
in #53 in the *cffi multi-RMW* bridge (where typed cyclone is genuinely
unsupported — no `nros/rmw-cyclonedds`), but here it is the **native rust cyclone
fixture built WITH `nros/rmw-cyclonedds`**, where the typed path is supposed to work.

## Scope / impact

CI-invisible: the host-integration light lane does not build the cyclone extras
(`build-fixture-extras`), so these tests `skip!` on CI — that is why #57's lane is
green and this stayed hidden. It bites any native **Rust** CycloneDDS publisher
(talker, service/action server feedback pubs). The C/C++ cyclone path and the
zenoh/xrce rust paths are unaffected.

## Direction (not started)

1. Add `RUST_BACKTRACE=1` + trace which step returns `PublisherCreationFailed`:
   `TypeRegistry::register::<M>` (descriptor build) vs the writer create in the
   cyclone backend's typed publisher path.
2. Compare the descriptor the rust seam builds for `std_msgs/Int32` against the
   working C++ `descriptors.cpp` entry.
3. Bisect across the phase-248 C2 + phase-249 P4b commits (the registrar-seam +
   linkme-deletion window) on this exact standalone repro.
