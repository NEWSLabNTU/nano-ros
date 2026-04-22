# Phase 89: `just test-all` triage — fix failures, relax per-platform parallelism

**Goal**: Close out the ~25 distinct failures / timeouts surfaced by a
full `just test-all` run, and restore the per-platform parallelism
that CLAUDE.md already advertises but the current `.config/nextest.toml`
has collapsed into a single `qemu-serial` group (hence "test X runs
sequentially with every other RTOS test" observable in CI logs).

**Status**: Not Started
**Priority**: High — test signal is currently noisy enough that
regressions are easy to miss (cf. the pre-existing
`test_native_service_communication::lang_1_Language__C` failure that
persisted under `native_api`'s fail-fast-free group for weeks before
being noticed during the Phase 84.G3 post-verification).
**Depends on**: Nothing external. Several categories below may be
absorbed by in-flight Phase 77 / Phase 85 work (explicitly noted per
item).

## Overview

A recent `just test-all` against `main` produced the following
outcome buckets. Total failures: **25 distinct tests** (some retried
3× under the `qemu-serial` override), **2 flakes**.

```
TIMEOUT (60s hard ceiling):                              6
FAIL after 3 retries:                                   14
FAIL (no retry because filter bypassed qemu-serial):     5
FLAKY 2/3:                                               2
```

The failing tests cluster into seven themes. All `lang_3_Lang__Cpp`
cross-RTOS failures share a family trait — the C++ arena and lifecycle
FFI paths aren't being exercised well on every RTOS — but each
platform needs its own investigation.

Parallelism: `.config/nextest.toml` currently assigns every
RTOS/QEMU/bridge-networked binary (`emulator`, `esp32_emulator`,
`zephyr`, `freertos_qemu`, `nuttx_qemu`, `threadx_riscv64_qemu`,
`threadx_linux`, `rtos_e2e`) to a **single** `qemu-serial` group with
`max-threads = 1`. CLAUDE.md still describes this as "per-platform
groups (e.g. `qemu-freertos`). Platforms run in parallel; tests
within a platform are serial." The code diverged.

Root cause of the collapse (per the comment block in the config):
"Host resource pressure (one QEMU instance per test plus a zenohd
router)". But the per-platform port table in `nros_tests::platform`
already prevents port collisions, and the original per-platform
grouping existed precisely so different platforms could run in
parallel while each platform's own QEMU instance stays serial. The
current blanket serialisation makes a full-matrix RTOS run take
roughly N× longer than needed, where N is the number of
platforms (~7).

## Work Items

- [ ] 89.1 — Re-split `qemu-serial` into per-platform `max-threads=1` groups
- [ ] 89.2 — Category A: C/C++ service-RPC failures (3 tests)
- [ ] 89.3 — Category B: C++-on-RTOS `lang_3` failures (5 tests)
- [ ] 89.4 — Category C: ESP32 QEMU suite (4 tests)
- [ ] 89.5 — Category D: QEMU RTIC suite (4 tests)
- [ ] 89.6 — Category E: `nano2nano` RTIC/TLS timeouts (4 tests)
- [ ] 89.7 — Category F: Standalone failures — `qemu_serial_pubsub`, `large_publish`, `dds` (3 tests)
- [ ] 89.8 — Category G: Flake reduction for `rtos_action_e2e` (2/3 flakes)

### 89.1 — Restore per-platform nextest groups

**Problem**: `.config/nextest.toml` lines 20–33 replaced 7 per-platform
groups with one `qemu-serial` group. Comment suggests this was a
simplification, but the per-platform zenohd-port table already makes
cross-platform concurrency safe; the collapse only hurts wall-clock
test time. CLAUDE.md and the book both describe a per-platform model
that the config no longer implements.

**Action**:

1. Replace the single `[test-groups.qemu-serial] max-threads = 1`
   block with one block per platform:
   - `qemu-baremetal` — `binary(emulator)` + `binary(large_msg)`
     (large_msg uses the baremetal port 7450)
   - `qemu-freertos` — `binary(freertos_qemu)`
   - `qemu-nuttx` — `binary(nuttx_qemu)`
   - `qemu-threadx-riscv` — `binary(threadx_riscv64_qemu)`
   - `qemu-esp32` — `binary(esp32_emulator)`
   - `qemu-zephyr` — `binary(zephyr)` (TAP / NSOS depending on build)
   - `threadx-linux` — `binary(threadx_linux)`
   - Each platform's case in `rtos_e2e` also needs per-platform
     routing. `rtos_e2e` is parametrised across all 4 RTOSes, so
     either split the binary into per-platform binaries or use
     nextest `test(...)` filtersets to assign each parametrised
     case to its platform group. The `test(...)` approach avoids
     the test-binary split.

2. Retry semantics stay at `retries = 2`, `slow-timeout = 120s /
   terminate-after = 3` per group — identical to today, just
   distributed.

3. **Acceptance**: `just test-all` wall-clock on a populated workspace
   drops by ≥40 % (rough estimate: 7 platforms serialised → 2–3 cores
   running in parallel). Per-platform tests must stay strictly
   ordered within their group (no TAP/port-table contention).

4. **Files**: `.config/nextest.toml` (sole edit).

**Risk**: Low. If cross-platform contention shows up (it shouldn't,
given the port table), the diff is two-line revertible.

### 89.2 — Category A: C/C++ service-RPC failures

Three tests:

| Test | Duration | Symptom |
|---|---|---|
| `native_api::test_native_service_communication::lang_1_Language__C` | 6.1 s | `Call [1]: Timeout`, subsequent calls `error -8` |
| `native_api::test_native_service_communication::lang_2_Language__Cpp` | 5.4 s | same signature — 0 / 4 calls succeed |
| `services::test_service_multiple_sequential_calls` | 23.6 s | N sequential `call()` invocations; at least one fails |

**Observed**: C and C++ native service clients make a `call()` that
reaches the server (the test starts the server) but the reply never
arrives within the client's blocking timeout. Error `-8` is
`NROS_RET_TIMEOUT`. The first call times out at the 5 s blocking
timeout; subsequent calls also time out because the in-flight flag
(Phase 84.D3) is still set on the client.

**Suspect**: the blocking `call_raw` path in `Client::call` on the C
side goes through `zpico_get` which Phase 77 already flagged as the
root of every blocking-action timeout ("Phase 77 WIP: blocking
zpico_get in send_goal returns Timeout immediately on native"). The
native C/C++ `Client::call` likely hits the same code path.

**Action**:

1. Confirm the `call_raw` → `zpico_get` chain is the blocker by
   running the same test with `RUST_LOG=trace` and noting where the
   call stalls. If it is `zpico_get`, Phase 77 owns this (adds
   `zpico_get_start`/`zpico_get_check` polled by the executor).
   Close this item as "blocked on Phase 77".
2. If not `zpico_get`, instrument the server-side queryable callback
   (`nros_rmw_zenoh::shim::service::ZenohServiceServer::poll`) to
   see whether the request is received at all. A missed request
   means liveliness token discovery isn't completing before the
   client sends.
3. **Files**:
   - `packages/core/nros-c/src/service.rs`
   - `packages/core/nros-cpp/include/nros/client.hpp`
   - `packages/zpico/nros-rmw-zenoh/src/shim/service.rs` (server side)

### 89.3 — Category B: C++-on-RTOS `lang_3_Lang__Cpp` failures

Five tests:

| Test | Duration |
|---|---|
| `rtos_e2e::test_rtos_action_e2e::platform_1_Platform__Freertos::lang_3_Lang__Cpp` | 45.5 s |
| `rtos_e2e::test_rtos_pubsub_e2e::platform_4_Platform__ThreadxRiscv64::lang_3_Lang__Cpp` | 15.8 s |
| `rtos_e2e::test_rtos_service_e2e::platform_1_Platform__Freertos::lang_3_Lang__Cpp` | 49.8 s |
| `rtos_e2e::test_rtos_service_e2e::platform_3_Platform__ThreadxLinux::lang_3_Lang__Cpp` | 41.2 s |
| `zephyr::test_zephyr_cpp_action_server_to_client_e2e` | 15.3 s |
| `native_api::test_cpp_action_communication` | 22.5 s |

**Observed**: C++ E2E tests across FreeRTOS, ThreadX-RV64, ThreadX-Linux,
Zephyr, and native all fail. Rust counterparts (`lang_1_Lang__Rust`)
on the same platforms pass (with one NuttX-Rust flake, see 89.8). The
C `lang_2_Lang__C` cases generally pass.

**Suspect**: The C++ API's blocking patterns are more conservative
than Rust/C — `ActionClient::send_goal` + `get_result` still block
through `zpico_get` on some paths that the async variants sidestepped.
Phase 77 explicitly noted:
> "The C++ action client hangs during `create_action_server` on
>  FreeRTOS QEMU (zenoh-pico deadlock when declaring 5 entities)"

The FreeRTOS + ThreadX Linux service failures are likely the same
deadlock class. The ThreadX-RV64 pubsub failure is different —
probably the post-Phase-85.10 ABI size probe succeeded at build time
but some C++-side cbindgen / storage-size assertion now trips.

**Action**:

1. Tag each failure with its exact stall point (install log probe at
   `create_action_server`, `create_service_client`, `call`, etc.).
2. For the FreeRTOS / ThreadX-Linux deadlocks: wait for Phase 77.x
   (async action client) and then rerun — most should flip to green
   automatically when blocking `zpico_get` disappears.
3. For the ThreadX-RV64 pubsub C++ failure: run the Phase 87 probe
   output against the ThreadX-RV64 target and verify
   `NROS_CPP_*_STORAGE_SIZE` macros match the compiled layout. If
   they don't, either fix the probe for RV64 or extend Phase 87's
   layout-mirror checks (87.11) to RV64.
4. **Files**:
   - `examples/qemu-arm-freertos/cpp/zenoh/action-client/`
   - `examples/threadx-linux/cpp/zenoh/service-client/`
   - `examples/qemu-riscv64-threadx/cpp/zenoh/talker/`
   - `examples/zephyr/cpp/zenoh/action-client/`
   - `packages/core/nros-cpp/build.rs` (probe)

### 89.4 — Category C: ESP32 QEMU suite

Four tests, all fail in `esp32_emulator`:

| Test | Duration |
|---|---|
| `test_esp32_qemu_talker_boots` | 1.3 s |
| `test_esp32_talker_listener_e2e` | 2.4 s |
| `test_esp32_to_native` | 6.4 s |
| `test_native_to_esp32` | 1.5 s |

Durations < 7 s on every retry suggest a **fast-fail** path — the
binary either isn't being built, Espressif QEMU isn't on PATH, or
the ESP32 image is panicking during boot. Not a real pub/sub bug
(those would take longer to time out).

**Action**:

1. Run `just doctor esp32` and `just esp32 doctor` — expect one of:
   (a) "qemu-system-riscv32 missing" → a user env issue, not a code
       bug; add a skip-with-diagnostic in the test if so.
   (b) "zenoh-pico RISC-V stale" — rebuild via `just esp32 setup`.
   (c) Everything OK, actual ESP32 boot failure — diff against
       last-known-green commit and bisect.
2. If (c): run `timeout 5 ./scripts/esp32/launch-esp32c3.sh
   <binary>` manually and collect the serial log. The fast-fail
   suggests an assertion panic during early init, not a pub/sub
   timeout.
3. **Files**: `packages/testing/nros-tests/tests/esp32_emulator.rs`,
   `packages/boards/nros-esp32-qemu/`,
   `examples/qemu-esp32-baremetal/`.

### 89.5 — Category D: QEMU RTIC suite

Four tests in `emulator`:

| Test | Duration |
|---|---|
| `test_qemu_rtic_action_e2e` | 16.5 s |
| `test_qemu_rtic_mixed_priority_pubsub_e2e` | 16.0 s |
| `test_qemu_rtic_pubsub_e2e` | 16.1 s |
| `test_qemu_rtic_service_e2e` | 15.9 s |

**Observed**: Identical ~16 s timeout across all four RTIC tests.
`emulator` binary hosts bare-metal MPS2-AN385 RTIC examples — they
run on the QEMU bare-metal platform with slirp user-mode networking.

**Suspect**: either the RTIC binaries aren't being built (the RTIC
suite predates Phase 82's service-client refactor and may still use
pre-Phase-82 API), or the zenohd port-7450 instance started by the
test framework isn't reachable from slirp after a recent change.

**Action**:

1. Build one RTIC binary manually:
   ```bash
   cd examples/qemu-arm-baremetal/rust/zenoh/rtic-talker
   cargo build --release --target thumbv7m-none-eabi
   ```
   If this fails, Category D is a build-time API breakage.
2. If build is OK, run:
   ```bash
   ./scripts/qemu/launch-mps2-an385.sh --binary <rtic-talker> --network slirp
   ```
   alongside `build/zenohd/zenohd --listen tcp/127.0.0.1:7450
   --no-multicast-scouting` and capture the zenohd connection log.
3. **Files**:
   - `examples/qemu-arm-baremetal/rust/zenoh/rtic-*`
   - `packages/testing/nros-tests/tests/emulator.rs`

### 89.6 — Category E: `nano2nano` RTIC/TLS timeouts

Four tests, all time out at exactly 60 s:

| Test | Duration |
|---|---|
| `test_rtic_pattern_action` | 60.0 s |
| `test_rtic_pattern_communication` | 60.0 s |
| `test_rtic_pattern_service` | 60.0 s |
| `test_tls_talker_listener_communication` | 60.0 s |

**Observed**: The 60 s hard-ceiling hit means the test is blocked on
some synchronisation point (usually `wait_for_output_pattern`) that
never fires. This usually means the binary booted but never emitted
the expected marker string — or didn't boot at all.

**Suspect**:
- RTIC: linked to Category D. If RTIC binaries don't work, their
  native-side `nano2nano` counterpart will never see a peer.
- TLS: Phase 84 migrated `ZENOH_TLS_*` env vars to `NROS_TLS_*` but
  this test may still set the old names.

**Action**:

1. Grep the four test bodies for the readiness markers being awaited
   (`wait_for_output_pattern(...)` arg). For each marker, confirm the
   example binary logs it at startup.
2. For TLS: inspect `test_tls_talker_listener_communication` env setup
   and verify it uses `NROS_LOCATOR` + `ZENOH_TLS_ROOT_CA_CERTIFICATE`
   (TLS env vars weren't renamed in 84.E3 — double-check).
3. **Files**:
   - `packages/testing/nros-tests/tests/nano2nano.rs`
   - (same upstream as Category D for the RTIC half).

### 89.7 — Category F: Standalone failures

| Test | Category | Duration |
|---|---|---|
| `emulator::test_qemu_serial_pubsub_e2e` | serial-over-PTY | 7.8 s |
| `large_msg::test_qemu_zenoh_large_publish` | fragmented message reception | 10.2 s |
| `dds_api::test_dds_talker_listener_communication` | Cyclone DDS backend | 60.0 s (timeout) |

**Observed**:

- `qemu_serial_pubsub_e2e` — short runtime suggests early failure;
  probably `socat` PTY pair setup or `zpico-serial` framing.
- `large_publish` — 10 s is one `wait_for_output_pattern` cycle; the
  listener likely isn't receiving the reassembled fragmented
  message. Phase 80 (unified network interface) touched this surface
  recently.
- `dds_api` — 60 s timeout suggests Cyclone DDS isn't discovering
  the peer. `dds-rs` setup might be missing; this is a
  nice-to-have backend, not a blocker.

**Action**: Each sub-item is a separate ~1-day investigation. Start
with serial (smallest scope), then large_publish (owned by
Phase 80), then DDS (punt to Phase 71).

### 89.8 — Category G: Flake reduction

| Test | Outcome |
|---|---|
| `rtos_e2e::test_rtos_action_e2e::platform_2_Platform__Nuttx::lang_1_Lang__Rust` | 2/3 passed → nominally green, counted as flake |

Probably a timing race in goal → feedback delivery on NuttX. Not a
hard blocker — passes on retry — but worth fixing once Category B
is done (the two failure modes likely share infrastructure).

**Action**: After 89.3 lands, re-run NuttX action suite 10× to see
whether the flake reproduces independently. If yes, add a
per-platform `wait_for_output_pattern` poll interval bump on NuttX
matching the pattern used for Zephyr native_sim.

## Acceptance Criteria

- [ ] `.config/nextest.toml` has 7 per-platform groups; CLAUDE.md's
      "Per-platform nextest groups" line matches reality again.
- [ ] `just test-all` wall-clock improves ≥ 40 % on a populated
      workspace (rough before/after comparison, not a hard SLO).
- [ ] Category A resolves: either to "blocked on Phase 77" (closed
      as `[x] via Phase 77.x`) or to actual fixes in `nros-c` /
      `nros-cpp` service client.
- [ ] Category B: each of the 5 C++-on-RTOS failures has a
      dispositon — either fixed, or an explicit `#[ignore = "reason
      + owner phase"]` with a backing tracking note in Phase 77 /
      Phase 85.
- [ ] Category C: either the 4 ESP32 tests pass, or their env
      preconditions are tightened so they fail loudly at the
      fixture level instead of timing out at 1-6 s.
- [ ] Category D: the RTIC suite either passes or has a clear
      "migration-to-Phase-82-deferred" ignore reason.
- [ ] Category E: `nano2nano` RTIC/TLS no longer hit the 60 s hard
      timeout (either pass or fail-fast-at-setup).
- [ ] Category F: `qemu_serial_pubsub_e2e` and `large_publish` pass;
      DDS failure has an explicit deferral note.
- [ ] Category G: no flakes in 5 consecutive `just test-all` runs
      on the same workspace.

## Notes & Caveats

- The existing `[[profile.default.overrides]]` filter uses
  `filter = "binary(rtos_e2e)"`. For rtos_e2e's parametrised cases to
  route per-platform, either split `rtos_e2e.rs` into one test file
  per platform (big change, touches `nros-tests` layout), or use
  nextest's `test(...)` predicate alongside `binary(...)` in the
  override filter (simpler). Pick whichever reads cleaner — the book's
  test-infra section calls out that `binary()` + `test()` chaining is
  supported.
- Phase 85 (test-suite consolidation) has a broader mandate around
  test-time reduction; 89.1 is the pragmatic sub-slice that pays
  for itself immediately and shouldn't wait.
- Don't `#[ignore]` tests without a matching phase entry — silent
  skips hide real regressions (cf. the
  `test_native_service_communication::lang_1_Language__C` case that
  slipped under the radar before Phase 84.G3's verification pass).
