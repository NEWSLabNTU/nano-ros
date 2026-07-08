---
id: 155
title: "Zephyr+CycloneDDS lane: images boot, net ready, then silence — no publish, no error (all 8 phase_118 tests)"
status: resolved
type: bug
area: zephyr
related: [issue-0152, phase-282, issue-0157]
resolved_in: (this commit) — four stacked causes
---

## Summary

All 8 `phase_118_collapse` zephyr-cyclonedds tests fail on freshly rebuilt
images (build-{rs,c,cpp}-*-cyclonedds wiped + `NROS_ZEPHYR_FIXTURE_FILTER=
cyclonedds just zephyr build-fixtures`): the image boots, prints
`nros_net_wait: Network ready (NSOS)` — then NOTHING. No publish log, no
session-open error, no crash. Rust boot tests fail in 3-4 s (no output in
window); pubsub/service e2e in ~50 s. The C lane also shows a
`cyclone: Failed to start RTPS` info line in earlier runs.

Not triaged to a cause yet. Candidates, in test order:

1. **Stash-baseline first** (env_machine_test_debt rule): does the lane pass
   with the phase-282 tx_express / QoS-struct commits stashed and images
   rebuilt from the pre-diff tree? `NrosRmwQos.tx_express` was carved from
   `_reserved0` (layout-stable by design) and the cyclone cffi ignores it,
   but the embedded-cyclone lane is the one consumer that was NOT
   re-validated after the append (native cyclone lanes are green — bridge
   tests 3/3 — so a regression would be zephyr-embedded-specific).
2. Embedded-cyclone heap rule (`ddsrt_malloc` — cyclonedds-known-limitations)
   and the `Failed to start RTPS` line — config/heap drift in the vendored
   cyclonedds fork or the zephyr conf after recent zephyr-workspace churn.
3. When was this lane last green? Its images sat unbuilt through the resync
   (the staleness gate listed them stale against orchestration-ir), so the
   breakage window is wide — bisect via the fixture build log dates.

## Resolution (2026-07-08) — four stacked causes, none of them tx_express

1. **West-update patch reversion (C/C++ lanes).** The zephyr TREE patches
   (nsos getsockname/getifaddrs/mcjoin/recvmsg/ipproto) had been reverted by
   a `west update` during the env-resync — cyclone's sockwaitset self-pipe
   needs NSOS `getsockname`, so `rtps_init` died ("can't allocate sock
   waitset for thread recv") and the session never started. Re-running
   `scripts/zephyr/*-patch.sh` (idempotent) + pristine rebuilds fixed the
   C and C++ pubsub lanes. **Operational rule: after ANY west update /
   workspace re-provision, re-run the zephyr patch suite.**

2. **Pure-Rust Zephyr images never registered a backend (since 248/249).**
   The zephyr module emits a strong `nros_app_register_backends` stub for
   the Kconfig-selected RMW, but only the C/C++ `nros_cpp_init` path called
   it — `zephyr_component_main!` reached `Executor::open` with no backend
   and got `Transport(ConnectionFailed)`. The macro now calls the hook
   before open (mirroring the C++ init path), and `zephyr/CMakeLists.txt`
   emits the stub for RUST-API-only images too (it lived in the C/C++
   branches only).

3. **Silent-return masking.** `zephyr_component_main!` swallowed open/
   register failures with a bare `return`, idling the image with zero
   output — undiagnosable until made to panic (repo fail-loud rule). Both
   error arms now panic with the error.

4. **Phase-271 heap sizing (zenoh/xrce leaves, found en route).**
   `Executor::open` now leaks a ~75 KB default backing; the leaf examples'
   64 KB `CONFIG_HEAP_MEM_POOL_SIZE` OOM'd at boot. Bumped to 128 KB in the
   12 zenoh/xrce leaf confs (cyclone confs already carried 4 MB).

Verified: rust-cyclone talker publishes; phase_118 boots + all three pubsub
e2e + rust service (solo) green. Residual, split to issue 0157: the C and
C++ zephyr-cyclone SERVICE e2e never deliver a reply (server+client boot
and create participants fine — distinct, older defect), and the rust
service e2e flakes under parallel group load.

## Repro

```
NROS_ZEPHYR_FIXTURE_FILTER=cyclonedds just zephyr build-fixtures
cargo nextest run -p nros-tests --test phase_118_collapse
```
