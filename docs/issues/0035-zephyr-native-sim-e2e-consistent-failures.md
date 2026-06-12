---
id: 35
title: zephyr native_sim e2e fail consistently (XRCE-heavy) — not load flakes
status: open
type: bug
area: zephyr
related: [issue-0032, issue-0034]
---

A clean idle `just test-all` (2026-06-12) — fresh fixtures (`build-test-fixtures`
green-stamped on all 8 platforms, **0 `is stale`**), all known build blockers
fixed ([0024](archived/0024-esp32-dram-overflow-size-class-buffers.md) esp32
`.bss`, [0032](archived/0032-zephyr-fixture-false-stale-dir-mtime.md) zephyr
false-stale, [0033](archived/0033-zenoh-service-seq-i64-store-32bit-break.md)
zenoh 32-bit) — still leaves ~15 zephyr `native_sim` e2e failures. **These are
NOT load flakes.** Running the zephyr e2e suite **alone, idle, with 3 retries**:

```
34 tests run: 21 passed, 13 failed   (every failure exhausted TRY 3)
```

13 reliable failures (XRCE-heavy — 9 of 13):

| test | rmw |
|---|---|
| `test_zephyr_xrce_c_talker_listener`        | xrce |
| `test_zephyr_xrce_cpp_talker_listener`      | xrce |
| `test_zephyr_xrce_rust_talker_listener`     | xrce |
| `test_zephyr_xrce_c_service_e2e`            | xrce |
| `test_zephyr_xrce_cpp_service_e2e`          | xrce |
| `test_zephyr_xrce_rust_service_e2e`         | xrce |
| `test_zephyr_xrce_c_action_e2e`             | xrce |
| `test_zephyr_xrce_cpp_action_e2e`           | xrce |
| `test_zephyr_xrce_rust_action_e2e`          | xrce |
| `test_zephyr_dds_rs_action_e2e`             | cyclonedds |
| `test_zephyr_action_e2e`                    | zenoh |
| `test_zephyr_rust_service_e2e`              | zenoh |
| `test_zephyr_talker_to_listener_e2e`        | zenoh |

Error shapes (from the full-run junit):

```
Zephyr service E2E failed — client did not connect to zenohd.
C talker published but listener didn't receive (timing issue?)
cpp/xrce service E2E failed (client OK=0, server requests=0).
XRCE communication failed: …
```

**Observations / RCA directions** (not yet root-caused):

- **XRCE concentration** (9/13) points at the native_sim XRCE path — likely the
  micro-XRCE agent the e2e needs is not started / not reachable by the harness
  (`client OK=0, server requests=0` = no agent transport), or a native_sim NSOS
  networking mismatch (native_sim uses NSOS, not eth_posix/zeth TAP — see agent
  memory). Confirm the harness brings up the agent for native_sim XRCE.
- **zenoh cases** report "client did not connect to zenohd" though the harness
  logs "Starting zenohd router…" — a connect/timing or NSOS-bind issue on
  native_sim, not a missing router.
- **Cyclone** (`dds_rs_action`) — native_sim Cyclone discovery is finicky
  (distinct `--seed` per process, unicast Peers, mutex-pool sizing — see the
  archived native_sim/zephyr-cyclone notes).
- **Regression vs pre-existing — UNDETERMINED.** The recently-pulled 237.x action
  + RFC-0040/0041 work touched `packages/core/nros-node`/xrce/dds; this set was
  not bisected against a pre-237 baseline (would need an old checkout + targeted
  fixture rebuild, ~40 min). The fixtures here are freshly built from current
  `main`, so the failures are real for current code regardless.

Distinct from [issue 0034](0034-host-integration-31-preexisting-test-failures.md)
(host-integration lane = native; **compile-in-test** timeout class). This is the
**zephyr native_sim e2e runtime** tail. Each rmw path (XRCE agent bring-up, zenoh
native_sim connect, Cyclone discovery) needs its owner's triage; start with the
XRCE-agent bring-up since it accounts for 9/13.
