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

---

## Root cause (investigated 2026-06-13) — executor does not deliver received samples on the native_sim embedded executor build

The XRCE cluster (9/13) was reproduced and root-caused. **The RCA directions above
are disproven** — it is NOT agent bring-up, NOT an NSOS networking mismatch, NOT a
CDR/transport regression, and NOT a per-rmw bug. It is a single **executor delivery
gap** shared by all three rmw paths on native_sim.

### What was eliminated (each with direct evidence — strace + cross-tests)

- **Port mismatch (baked vs harness):** the fixture build *does* bake the per-test
  port via `-DCONFIG_NROS_XRCE_AGENT_PORT` / `-DCONFIG_NROS_ZENOH_LOCATOR`. Inspected
  `build-*/zephyr/.config`: baked ports match the harness (C pubsub 2118, C service
  2128, zenoh rust service 7466, action 7476, …). ✔ correct.
- **Stale fixtures (issue 0032 class):** reproduced with **freshly rebuilt** native_sim
  fixtures. Still fails.
- **233.6 CDR-header stripping (`0a88eab79`/`46f6632df`):** the headerless WRITE_DATA
  (no 4-byte CDR encapsulation header) works **host→agent→host** (native C XRCE
  talker/listener passes). The agent + FastDDS handle headerless fine. Not the cause.
- **Stream config (`STREAM_HISTORY`, MTU, buffer):** bumping native_sim
  `CONFIG_NROS_XRCE_STREAM_HISTORY` 4→16 (host value) did **not** help.
- **Talker / agent / DDS bridging:** all correct. Agent on the right port publishes to
  DDS (1186 RTPS sends); reliable input stream arrives **contiguous, no seq gap**
  (seq 0–3 STATUS, 4–11 DATA).

### Decisive isolation (cross-platform talker/listener against one agent)

| talker | listener | result |
|---|---|---|
| native_sim | **host** | host listener receives 6 ✔ — **native_sim talker is fine** |
| host | **native_sim** | native_sim listener receives **0** ✗ — **native_sim listener is broken** |

The break is the **native_sim listener (subscriber) delivery path**, independent of the
talker, agent, transport, and rmw.

### Exactly where it breaks (verified by instrumentation)

Instrumented `xrce_topic_callback` / `xrce_subscriber_has_data` in
`packages/xrce/nros-rmw-xrce/src/subscriber.c` and re-ran host-talker → native_sim-listener:

1. `recvfrom` on the listener **receives** every `0x09` DATA submessage (values 0,1,2…).
2. `xrce_topic_callback` **fires 7×**, matches the datareader slot, and **buffers** into
   the subscriber ring (`slot->count` grows). Receive side is fully working.
3. **`xrce_subscriber_has_data` / `xrce_subscriber_process_raw_in_place` are called 0
   times** — the executor **never drains the ring** → the user callback never fires →
   app prints `Waiting for messages…` and 0 received.

So: **data is received and buffered but never delivered to the subscription callback.**

### The exact defect (the `spin_once` that never returns)

Instrumenting further: the executor's readiness scan does run on `std`, but on the
native_sim build `spin_once` is **entered once and never returns** — it blocks inside
`xrce_session_drive_io` (`packages/xrce/nros-rmw-xrce/src/session.c`). That function
paces the spin by looping `uxr_run_session_time` until a relative deadline elapses:

```c
uint64_t start_ms = nros_platform_time_now_ms();   // ← wall clock
for (;;) {
    uint64_t now_ms = nros_platform_time_now_ms();  // ← wall clock
    int remaining = t - (int)(now_ms - start_ms);
    if (remaining <= 0) break;
    uxr_run_session_time(&st->session, remaining);
    ...
}
```

`nros_platform_time_now_ms()` is the **wall-clock / epoch** service. On every RTC-less
platform it is a stub returning `0` (`nros-platform-{zephyr,freertos,threadx}/src/platform.c`;
the header documents "`0` if the platform has no real-time clock"). With `now_ms`
permanently `0`, `remaining` is always `t` and the loop **spins forever** inside
`uxr_run_session_time` — it keeps receiving + buffering inbound (so `topic_cb` fires)
but never returns, so `spin_once` never reaches the readiness scan / dispatch and the
buffered samples are never delivered. The native host build passes because POSIX
`nros_platform_time_now_ms` returns real epoch ms.

Confirmed with a probe: `drive_io t=100 start_ms=0`, then `iter=0..3 now_ms=0 elapsed=0
remaining=100` — `now_ms` never advances.

### Fix (resolves all 9 XRCE cases)

`xrce_session_drive_io` must use the **monotonic** clock `nros_platform_clock_ms()`
(`k_uptime_get()` on Zephyr — works without an RTC, never decreases) for its relative
deadline deltas, not the wall-clock `nros_platform_time_now_ms()`. This is the same
contract the XRCE platform shim already documents for `uxr_millis` (`platform_aliases.c`)
and that zenoh-pico already uses for `z_clock_*`. One-symbol swap at the three call
sites in `session.c`.

**Verified after the fix** (rebuild fixtures, then run): `test_zephyr_xrce_c_talker_listener`,
`test_zephyr_xrce_c_service_e2e`, `test_zephyr_xrce_c_action_e2e` all **pass**. The C++/Rust
XRCE cases share the identical `session.c` drive path.

### Remaining (separate root cause — still open)

The **3 zenoh** + **1 cyclone** native_sim failures are **not** this clock bug — zenoh-pico's
timeout path (`z_clock_*`) already uses the monotonic clock, and cyclone has its own
discovery path. They need their own triage (likely native_sim NSOS connect/discovery,
as the original RCA directions noted). This issue stays **open** for those 4; the XRCE
cluster (9/13) is resolved.

**Reproduce (minimal):**
```sh
just zephyr build-one c/talker xrce && just zephyr build-one c/listener xrce
cargo nextest run -p nros-tests --test zephyr \
  test_zephyr_xrce_c_talker_listener --no-capture --test-threads=1
```
