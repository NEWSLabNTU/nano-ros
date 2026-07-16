---
id: 214
title: "riscv64-threadx rust cyclone lane: no test consumer, domain-0 deploy bake, shared firmware MAC"
status: resolved
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

## RESOLVED — 2026-07-16

All three gaps closed; the rust cyclone two-QEMU pair now delivers end-to-end:

1. **Test consumer:** new `test_threadx_riscv64_cyclonedds_two_qemu_rust_pubsub`
   (threadx_riscv64_qemu.rs) — the rust sibling of the C pair test, consuming
   the previously-uncalled `build_threadx_rv64_rust_example_rmw` resolver.
   PASS in ~7.7 s on fresh fixtures; the C pair test stays green.
2. **Identity:** the REAL wire identity on this path is the cmake-generated
   `NROS_APP_CONFIG` (subnet 10.0.2.x — applied by startup.c BEFORE the
   kernel), not the Rust `Config` (which only drives the Executor
   domain/locator). Both rust images used to boot 10.0.2.40/:56. The
   second-node examples (listener / service-client / action-client) now set
   `NROS_APP_NET_IP_LAST=41` + `NROS_APP_NET_MAC_LAST=0x57` as cache vars in
   their CMakeLists preamble (before `add_subdirectory`), mirroring the C
   fixtures' `-D` flags. Verified in the generated TU (.40 vs .41).
3. **Domain:** `Config::default()` now bakes `option_env!("NROS_DOMAIN_ID")`
   (was hardcoded 0 — the env bake previously lived only in the retired
   `from_toml`); `nros_threadx_rv64_rust_cyclone_app` sets it via corrosion
   env (arg or the configure's `-DNROS_DOMAIN_ID`), so the rust images join
   the fixture's domain 62. The zenoh path is unaffected (its deploy overlay
   explicitly overrides after `default()`).

Also: `run_app_thread` (the cyclone boot path) now prints an
`[app] MAC .. IP .. domain ..` banner — this path previously echoed no
identity, which is exactly what made the collapse invisible.
