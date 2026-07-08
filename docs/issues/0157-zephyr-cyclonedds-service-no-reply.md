---
id: 157
title: "Zephyr+CycloneDDS C/C++ service e2e: client never receives a reply (boots + participant fine); Rust service flakes under group load"
status: open
type: bug
area: zephyr
related: [issue-0155]
---

## Progress (2026-07-08 debug pass) — two fixes landed, one narrowed remainder

**FIXED: ROS-vs-DDS type-name form mismatch (killed C AND C++ service
create).** The hand-written zephyr C/C++ components pass the ROS user-level
type `example_interfaces/srv/AddTwoInts` to `nros_cpp_service_*`; the
cyclone descriptor registry stores the DDS-mangled
`example_interfaces::srv::dds_::AddTwoInts_{Request,Response}_` and the
lookup was exact-match → `descriptors_for_service` returned false →
`service_*_create` returned UNSUPPORTED (-5) → the carrier swallowed it (a
bare `return`) and the image idled silently. `ros_form_to_dds` in
`service.cpp` now converts `a/b/C` → `a::b::dds_::C_` before lookup (both
create paths share it); the typed-C carrier template prints nonzero
`run_components` codes instead of silently returning. Result: **C++ service
e2e green**, C server reaches "Waiting for service requests", rust service
green (solo).

**Group-load flakes (rust pubsub + rust service):** both PASS solo and fail
intermittently in the parallel `zephyr-native-cyclonedds` group (max-threads
4) — the 177.39 class; its 15 s budget may need revisiting or the group may
need max-threads 2. Recorded, not chased.

**REMAINING (the one deterministic red): C service pair never SEDP-matches.**
With fully fresh images: C client prints "Sending request", server sits at
"Waiting" — the client's request WRITER never matches the server's request
READER (`request_writer_matched` polls false forever, gdb-verified), so the
buffered request never flushes. Topics + types are IDENTICAL on both sides
(gdb: `rq/add_two_intsRequest` / `rr/add_two_intsReply`, converted DDS
types), same domain 0, and the C++ pair (same server architecture) matches
fine — so it's specific to the C client's endpoint shape. Next probes:
dump the WRITER QoS the C client creates (`nros_c_qos_default()` route)
vs the C++ client's and the server reader's — an RxO-incompatible field
(durability/reliability) would produce exactly silent non-matching;
cyclone tracing is unavailable on zephyr (ddsrt getenv stubbed) — consider
a config-header trace enable, or replicate the pair natively (host cyclone)
to use CYCLONEDDS_URI tracing.

## Summary

Residual of #155 (whose four boot/registration causes are fixed): with
freshly patched trees and rebuilt images,

- `test_zephyr_{c,cpp}_cyclonedds_service_e2e` fail DETERMINISTICALLY, solo
  or grouped: server prints its readiness, client boots, both create
  participants (`dds_create_participant` returns ok) — but the client never
  prints `Result:` within its 20 s budget.
- `test_zephyr_rust_cyclonedds_service_e2e` passes SOLO but failed twice in
  full-family group runs (4-7 s) — the 177.39 under-load discovery flake may
  be back with today's higher parallel load (its fix was a 15 s call budget).

Pubsub e2e all green on the same images, so transport + discovery work; the
break is service/request-reply specific. Starting points: the C service
client's call/timeout budget vs native_sim discovery (~15 s per 177.39);
whether the C client uses a blocking call with a shorter internal timeout;
reader/writer matching for the reply topic (cyclone request/reply uses
correlation via sample identity — check the C shim's reply matching).

## Repro

```
cargo nextest run -p nros-tests --test phase_118_collapse test_zephyr_c_cyclonedds_service_e2e --no-capture
```
