---
id: 157
title: "Zephyr+CycloneDDS C/C++ service e2e: client never receives a reply (boots + participant fine); Rust service flakes under group load"
status: open
type: bug
area: zephyr
related: [issue-0155]
---

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
