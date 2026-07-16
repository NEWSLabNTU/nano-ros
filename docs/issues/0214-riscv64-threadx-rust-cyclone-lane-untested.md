---
id: 214
title: "riscv64-threadx rust cyclone lane: no test consumer, domain-0 deploy bake, shared firmware MAC"
status: open
type: tech-debt
area: testing
related: [issue-0205, issue-0195, issue-0190]
---

## Summary (carved out of #205, 2026-07-16)

The `examples/qemu-riscv64-threadx/rust/*` **CycloneDDS** images build and boot
(post-#195/#205: descriptors register via the `.init_array` ctors, the talker
publishes in QEMU), but the lane cannot pass an end-to-end pair and nothing
would notice:

1. **No test consumer.** `build_threadx_rv64_rust_example_rmw` (the cyclone
   resolver in `nros-tests/fixtures/binaries/mod.rs`) is defined but called by
   nothing; only the zenoh lane runs via `rtos_e2e` ThreadxRiscv64 Rust. The
   #181 silent-lane class.
2. **Domain mismatch.** The rust examples' deploy blocks bake `domain_id = 0`
   while the C cyclone pair (and `test_threadx_riscv64_cyclonedds_two_qemu_pubsub`)
   run domain **62** — a rust↔C pair can never discover. Verified: rust talker
   publishes 42 samples, C listener (domain 62) receives 0.
3. **Shared firmware MAC.** The deploy overlay carries no `mac` field, so a
   rust↔rust two-QEMU pair boots both guests with the board-default NetX MAC —
   identity/ARP collapse on the shared L2 link, 0 delivery (the #190-class
   hazard). The C pair differentiates via `-DNROS_APP_NET_MAC_LAST`.

## Direction

Fix the deploy `domain_id` (62, matching the C pair convention), add a MAC
differentiation path (deploy `mac` field or per-example bake), then wire a
rust↔rust (or rust↔C) two-QEMU cyclone pubsub e2e consuming the existing
resolver — mirroring `test_threadx_riscv64_cyclonedds_two_qemu_pubsub`.
