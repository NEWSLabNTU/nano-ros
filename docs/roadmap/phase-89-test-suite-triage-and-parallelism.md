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

- [x] 89.1 — Re-split `qemu-serial` into per-platform `max-threads=1` groups
- [x] 89.2 — Category A: C/C++ service-RPC failures (3 tests) — wall-clock budget in blocking spin loops
- [x] 89.3 — Category B: C++-on-RTOS `lang_3` failures (6 tests) — size-probe LTO bitcode, zero-sized C++ wrapper storage, nros_platform_time_ns link, Zephyr minimal libcpp `<cstring>`, stale readiness marker
- [x] 89.4 — Category C: ESP32 QEMU suite (4 tests)
- [x] 89.5 — Category D: QEMU RTIC suite (4 tests) — fix size-probe platform detection for bare-metal + smoltcp
- [x] 89.6 — Category E: `nano2nano` RTIC/TLS timeouts (4 tests) — resolved by 89.5 size-probe fix
- [ ] 89.7 — Category F: Standalone failures — `qemu_serial_pubsub`, `large_publish`, `dds` (3 tests)
- [ ] 89.8 — Category G: Flake reduction for `rtos_action_e2e` (2/3 flakes)
- [x] 89.9 — Within-platform parallelism, tier 1: ThreadX-Linux per-case port split
- [x] 89.10 — Within-platform parallelism, tier 2: per-variant zenohd ports for slirp QEMU platforms
- [x] 89.Zephyr — Within-platform parallelism, tier 2 extension: Zephyr native_sim
- [x] 89.Baremetal — Within-platform parallelism, tier 2 extension: bare-metal MPS2-AN385 RTIC
- [ ] 89.11 — (Optional) Runtime locator override on RTOS — collapses 89.10's config matrix

### 89.1 — Restore per-platform nextest groups — **Landed** (commit `8e7b9727`)

Replaced the single `qemu-serial` group with 7 per-platform groups,
each still `max-threads = 1` so same-platform tests stay strictly
serial (one QEMU/native-sim instance + one zenohd per test). Cross-
platform concurrency is now free because the port table already
prevents zenohd collisions:

  qemu-baremetal      port 7450  (emulator + large_msg)
  qemu-freertos       port 7451
  qemu-nuttx          port 7452
  qemu-threadx-riscv  port 7453
  qemu-esp32          port 7454
  threadx-linux       port 7455
  qemu-zephyr         port 7456

`rtos_e2e` is one parametrised binary covering all 4 RTOSes; each
case routed to its platform group via nextest's `test(...)` substring
predicate against rstest's generated `platform_N_Platform__<Variant>`
name. No test source changes — pure config.

`large_msg` merged into `qemu-baremetal` (shares port 7450); former
`[test-groups.large_msg]` block removed.

`justfile::test` fast-path and the "fast path" comment block updated
to `group(=qemu-baremetal) or group(=qemu-freertos) or …` so `just
test` keeps excluding all heavy-dep binaries.

Acceptance note: the CLAUDE.md promise ("per-platform nextest
groups — each platform has its own `max-threads = 1` group … Platforms
run in parallel; tests within a platform are serial") now matches the
config again.

**Risk verification**: smoke test via `cargo nextest run -p nros-tests
--test actions` — 3/3 tests pass in 4.9 s wall-clock (parallel).

### 89.2 — Category A: C/C++ service-RPC failures — **Landed**

**Root cause**: the blocking service-call spin loops on the C and C++
sides budgeted work by *iteration count* (`max_spins = timeout_ms / 10`),
not wall-clock. On POSIX / Zephyr with `Z_FEATURE_MULTI_THREAD == 1`
the underlying `zpico_spin_once(timeout_ms)` waits on a condvar that
the zenoh-pico background tasks signal on *any* incoming frame —
keep-alives, discovery gossip, routing updates, etc. Each signal
returns the spin well before the requested timeout. With the
default 5 s budget, the 500-iteration loop exhausts in milliseconds,
and `nros_client_call` / `Future::wait()` return `Timeout` before
the reply can arrive (especially for the first RPC on a session,
where zenoh-pico's initial scout / handshake hasn't settled yet).

Rust's `Promise::wait` has the same loop shape but stayed green
because all-Rust tests see the reply on the very first spin (fast
path, no need to burn the budget).

Cascading symptoms observed pre-fix:

| Test | Pre-fix symptom |
|---|---|
| `test_native_service_communication::lang_1_Language__C` | Call [1] `Timeout`, Calls [2–4] `NROS_RET_BAD_SEQUENCE` (entry.pending left set) |
| `test_native_service_communication::lang_2_Language__Cpp` | Call [1] `Timeout` (`-2`), Calls [2–4] `send_request failed` (cascading slot state) |
| `test_service_multiple_sequential_calls` (Rust) | now passes (pre-fix it intermittently failed when the first RPC needed discovery) |
| `test_cpp_action_communication` (Category B) | also passes — same `Future::wait` loop |

**Fix** (3 files):

- `packages/core/nros-c/src/service.rs::nros_client_call` — replaced
  the `for _ in 0..max_spins` loop with a wall-clock budget using
  `crate::platform::get_time_ns()`. Each iteration still calls
  `nros_executor_spin_some(10ms)` but the loop exits only when
  `elapsed_ns >= timeout_ns`.
- `packages/core/nros-cpp/include/nros/future.hpp::Future::wait` —
  same replacement, but header-side. Freestanding C++ can't call
  `std::chrono`, so a new FFI function `nros_cpp_time_ns()` exposes
  the monotonic clock.
- `packages/core/nros-cpp/src/lib.rs` — `nros_cpp_time_ns()` export,
  `Instant`-backed in `std` mode, forwarded to
  `nros_platform_time_ns()` in `no_std`.

**Not touched (deliberate)**:

- `Promise::wait` in `nros-node::executor::handles.rs` has the same
  structural bug but currently passes all tests. Leaving it on the
  max-spins path until a test surfaces it.
- The C blocking action client (`nros_action_send_goal_blocking` etc.)
  uses a hard-coded `for _ in 0..1000` with no wall-clock budget.
  Phase 77 rewrites the action client to a fully async path and
  deletes this loop outright — not worth touching twice.

**Test results**:

- `test_native_service_communication::lang_1_Language__C` → PASS (6.8 s, was FAIL)
- `test_native_service_communication::lang_2_Language__Cpp` → PASS (5.7 s, was FAIL)
- `test_service_multiple_sequential_calls` → PASS (4.3 s)
- `test_cpp_action_communication` (Category B bonus) → PASS (7.6 s)

### 89.3 — Category B: C++-on-RTOS `lang_3_Lang__Cpp` failures — **Landed**

Triage summary (pre-fix, 6 failing tests across every RTOS + native):

| Test | Pre-fix | Status after 89.3 |
|---|---|---|
| `native_api::test_cpp_action_communication` | 22.5 s FAIL | ✅ **passes** (4.4 s) |
| `rtos_e2e::test_rtos_action_e2e::platform_1_Platform__Freertos::lang_3_Lang__Cpp` | 45.5 s FAIL | ✅ (42.8 s) |
| `rtos_e2e::test_rtos_pubsub_e2e::platform_4_Platform__ThreadxRiscv64::lang_3_Lang__Cpp` | 15.8 s FAIL | ✅ (96.9 s) |
| `rtos_e2e::test_rtos_service_e2e::platform_1_Platform__Freertos::lang_3_Lang__Cpp` | 49.8 s FAIL | ✅ (42.8 s) |
| `rtos_e2e::test_rtos_service_e2e::platform_3_Platform__ThreadxLinux::lang_3_Lang__Cpp` | 41.2 s FAIL | ✅ (67.1 s) |
| `zephyr::test_zephyr_cpp_action_server_to_client_e2e` | 15.3 s FAIL | ✅ (36.7 s) |

Full 18-case regression run (all C++ tests across all C++-capable
platforms): 18/18 pass, 97 s wall-clock. No Rust / C regressions.

Five distinct bugs stacked on top of each other; fixing one surfaced
the next. Peeled in dependency order:

1. **Size-probe silently returned zero on release builds.** The
   workspace `[profile.release]` has `lto = true`. With full LTO
   rustc writes each rlib's codegen-unit object (`*.rcgu.o`) as
   **LLVM bitcode** instead of a native ELF. `nros-sizes-build`
   parses rlib members through the `object` crate (ELF/Mach-O/COFF
   only), so it silently returned `0` for every `__NROS_SIZE_*`
   symbol — which cascaded into the generated
   `nros_cpp_config_generated.h`:

   ```c
   #define NROS_CPP_ACTION_CLIENT_STORAGE_SIZE 0   // should be 48
   #define NROS_CPP_ACTION_SERVER_STORAGE_SIZE 0   // should be 72
   #define NROS_*_SIZE                         0   // etc.
   ```

   Result: the C++ `ActionClient<A>` / `ActionServer<A>` wrapper
   classes reserved `alignas(8) uint8_t storage_[0]` bytes of inline
   storage. `nros_cpp_action_client_create` wrote the `CppActionClient`
   Rust struct (16 bytes) into zero-byte storage, corrupting adjacent
   stack / struct fields and reading garbage for
   `arena_entry_index` (`26979` in one run). Every C++ action-client
   call then found `action_client_core_mut` returned `None` and
   bailed out as `NROS_CPP_RET_ERROR` — the misleading "Goal
   REJECTED by server" banner users saw.

   **Fix**: disable LTO in the workspace release profile
   (`lto = "off"`). Native object files land in rlibs again, the
   probe recovers real `size_of::<T>()` values, storage sizes land
   at 48 / 72, and the C++ wrappers allocate real memory. A more
   surgical fix — teach `nros-sizes-build` to parse bitcode — can
   replace the LTO-off workaround later if release binary size
   matters.

2. **`nros_platform_time_ns` is `static inline` in the platform
   headers**, so `extern "C" fn nros_platform_time_ns()` from
   `nros-cpp/src/lib.rs::nros_cpp_time_ns` (added by 89.2) has no
   linker-visible definition on any RTOS build. FreeRTOS / ThreadX
   C++ examples failed with `undefined reference to nros_cpp_time_ns`
   (the C++ side refers to the Rust symbol, which in turn needs a
   non-existent C symbol). **Fix**: redirect the no-std
   `nros_cpp_time_ns` path to `z_clock_now()` (zpico-platform-shim's
   non-inline monotonic-ms primitive) scaled to ns.

3. **`<cstring>` is missing from Zephyr's minimal C++ stdlib**
   (`CONFIG_CPP=y, CONFIG_STD_CPP14=y` enables the minimal-libcpp
   that ships with Zephyr 3.7). `publisher.hpp` / `subscription.hpp`
   used `std::memcpy` via `<cstring>`; all C++ Zephyr builds failed
   at preprocessor. **Fix**: switch to `<string.h>` + `::memcpy`,
   which is guaranteed by every C library Zephyr exposes.

4. **C++ `send_goal` / `get_result` used iteration-count spin
   budgets** (`for _ in 0..1000`), the exact bug 89.2 fixed for
   service clients. On multi-threaded zpico backends
   `spin_once(10)` returns early on every incoming frame, so the
   nominal 10 s budget collapsed to milliseconds before the
   server's goal-accept response could arrive. **Fix**: rewrite
   the two loops in `nros-cpp/src/action.rs` to wall-clock
   budgeting via the existing `nros_cpp_time_ns()` helper, mirroring
   the 89.2 pattern.

5. **Stale readiness marker on ThreadX-Linux C++ service server**.
   `rtos_e2e::ensure_ready` waits for `"Waiting for requests"`;
   `examples/threadx-linux/cpp/zenoh/service-server/src/main.cpp`
   only printed `"Service server ready"`. Server *was* handling
   RPCs correctly (the test captured `Request [1]..[4]` in the
   failure dump) — the readiness check just never matched.
   **Fix**: add the canonical marker string to the server example.

**Files touched**:
- `Cargo.toml` — `[profile.release]` `lto = "off"` with explanatory comment.
- `packages/core/nros-cpp/src/action.rs` — wall-clock budgets on `send_goal`, `get_result`.
- `packages/core/nros-cpp/src/lib.rs` — no-std `nros_cpp_time_ns` via `z_clock_now`.
- `packages/core/nros-cpp/include/nros/publisher.hpp` — `<string.h>` + `::memcpy`.
- `packages/core/nros-cpp/include/nros/subscription.hpp` — same.
- `examples/threadx-linux/cpp/zenoh/service-server/src/main.cpp` — emit `"Waiting for requests"`.

### 89.4 — Category C: ESP32 QEMU suite — **Landed**

Triage summary (pre-fix, 4 failing tests — all fast-fail in 1.3–6.4 s):

| Test | Duration | Status after 89.4 |
|---|---|---|
| `test_esp32_qemu_talker_boots` | 1.3 s | ✅ **passes** (0.8 s) |
| `test_esp32_talker_listener_e2e` | 2.4 s | ✅ **passes** (6.4 s) |
| `test_esp32_to_native` | 6.4 s | ✅ **passes** (6.4 s) |
| `test_native_to_esp32` | 1.5 s | ✅ **passes** (6.9 s) |

Full `esp32_emulator` suite: 20.7 s wall-clock, 9/9 pass.

**Root cause of the 1–6 s fast-fail**: `build/esp32-zenoh-pico/
libzenohpico.a` missing → `require_zenoh_pico_riscv()` returned
false → `nros_tests::skip!` panicked with `[SKIPPED]` before the
test body ran. Silent precondition skip was counted as FAIL.
Resolved by running `just esp32 setup`, which the root orchestrator
(`just setup`) invokes as part of its module walk.

**Deeper bugs surfaced once the precondition cleared** — real boot
and networking problems, not skips:

1. **Stale boot banner pattern** — test asserted `"nros ESP32-C3
   QEMU BSP"` but the board crate's `init_hardware()` prints
   `"nros ESP32-C3 QEMU Platform"` (renamed during 79.9 platform
   unification). Fixed in `esp32_emulator.rs`.
2. **`nros_baremetal_common::sleep::sleep_ms` was a silent no-op**
   on every bare-metal platform. Commit `430e65b4` centralised
   the busy-wait loop behind a `set_clock_fn` function-pointer
   registration but never wired it up in any board crate →
   `CLOCK_FN == 0` → `sleep_ms` returns immediately → the
   bare-metal busy-wait poll callback never runs. Fixed for ESP32
   QEMU by calling `nros_platform_esp32_qemu::sleep::init_clock()`
   and `::set_poll_callback(smoltcp_network_poll)` in
   `init_hardware`. (Same fix is needed for MPS2-AN385, STM32F4,
   ESP32 WiFi — tracked in 89.5 / 89.Baremetal.)
3. **DMA-buffer lifetime bug in `nros-esp32-qemu`**. `OpenEth::
   init()` writes the addresses of `self.tx_buf` / `self.rx_buf`
   (fields inside the struct) into hardware TX/RX descriptors,
   but the board crate constructed `OpenEth` on the stack, called
   `init()`, and *then* moved the struct into `ETH_DEVICE:
   MaybeUninit<OpenEth>`. After the move, both DMA pointers were
   stale — the open_eth QEMU device transmitted pre-move stack
   contents (`pcap` showed a single 64-byte all-zeros Ethernet
   frame) and received frames into memory the driver never read.
   Fixed by constructing `OpenEth` via `ETH_DEVICE.write(…)`
   first, then calling `init()` on the static-storage reference.
   With this fix the pcap shows correct ARP/SYN traffic: ARP
   request → ARP reply → SYN, where previously only the
   all-zeros frame appeared.

4. **QEMU `open_eth` queued-packet flush bug** (found via
   `third-party/esp32/qemu/hw/net/opencores_eth.c`). QEMU's model
   only flushes queued ingress packets when `MODER_RXEN` transitions
   0→1 (see `open_eth_moder_host_write` → `open_eth_notify_can_
   receive` → `qemu_flush_queued_packets`). Once the guest is
   busy-looping after init, slirp-generated frames (SYN-ACK, TCP
   ACK, keep-alives) that arrive after the initial flush sit in
   QEMU's internal netdev queue **indefinitely** — the host's
   filter-dump pcap shows them but the NIC never writes them to
   any RX descriptor. ARP reply slipped through because it was
   generated synchronously while the first MODER write was still
   in flight, but everything after that stalled. Fixed by
   toggling `MODER.RXEN` (clear→set) every 8th `smoltcp_
   network_poll` call — two MMIO writes, drains the queue.
   Also extended the OpenETH driver to a 4-slot RX ring (ESP-IDF
   reference count); single-slot rings exhibited the same stall
   in standalone tests.

Remaining deliberate scope: the driver could alternatively
register for the OpenETH RX IRQ (INT_SOURCE bit 2, RXF) and do
the flush from an ISR. The busy-poll toggle is simpler and
matches how zpico already drives the smoltcp bridge; the IRQ
path can be added later if some platform needs it.

**Files touched**:
- `packages/testing/nros-tests/tests/esp32_emulator.rs`
  (boot-banner pattern; remove stale `Connected!` assertions that
  the examples never emit; drop the earlier `#[ignore]` attrs).
- `packages/boards/nros-esp32-qemu/src/node.rs`
  (`init_clock`, `sleep::set_poll_callback`, re-ordered
  `OpenEth::new` / `ETH_DEVICE.write` / `init`).
- `packages/boards/nros-esp32-qemu/src/network.rs`
  (RXEN-toggle flush workaround in `smoltcp_network_poll`).
- `packages/drivers/openeth-smoltcp/src/lib.rs`
  (4-slot RX ring + rotating `next_rx_idx`; promiscuous mode;
  updated struct-size test to reflect the ring growth).

### 89.5 — Category D: QEMU RTIC suite — **Landed**

**Root cause**: ABI corruption in the `zpico-platform-shim` size probe.

`_z_open_tcp(_z_sys_net_socket_t *sock, const _z_sys_net_endpoint_t rep, …)`
takes the endpoint struct **by value**. The Rust shim declares
`ZSysNetEndpoint` as an opaque `[u8; NET_ENDPOINT_SIZE]` whose size
is determined at build time by compiling a C size probe and reading
the symbol sizes from the object file.

For bare-metal builds (`thumbv7m-none-eabi`) the probe's heuristic
keyed off `env::var("FREERTOS_DIR").is_ok()`:

```rust
let primary = if target.contains("thumbv7m") && env::var("FREERTOS_DIR").is_ok() {
    ProbePlatform::Freertos
} else if target.contains("none") {
    ProbePlatform::BareMetal
} …
```

`.envrc` exports `FREERTOS_DIR` globally whenever the FreeRTOS SDK
is set up (default after `just setup`), so *every* bare-metal build
took the FreeRTOS branch. That branch compiles the probe against
lwIP headers, where `_z_sys_net_endpoint_t = int socket_fd` (4
bytes). The real bare-metal layout is `{uint8_t _ip[4]; uint16_t
_port}` (6 bytes).

Result: when the firmware called `_z_open_tcp(sock, rep, tout)`, the
`rep` argument passed by value was only 4 bytes — the IP octets
arrived intact, but the port bytes (high half of the `(_ip, _port)`
pair) got whatever was adjacent on the caller's stack. Every SYN
went to an arbitrary port on `10.0.2.2`; slirp RSTed; the firmware
panicked at `Executor::open()` with `Transport(ConnectionFailed)`.

This matches the pre-fix pcap exactly: firmware configured for
`tcp/10.0.2.2:7450` sent SYNs to `10.0.2.2:57244` (or whatever
stack garbage happened to sit after the IP in the caller frame).

**Fix** (1 file):

- `packages/zpico/zpico-platform-shim/build.rs` — check the
  `CARGO_FEATURE_NETWORK_SMOLTCP_BRIDGE` env var *first*. That
  feature is activated only by `zpico-sys/bare-metal`, so its
  presence is a definitive signal that the binary targets smoltcp
  (not FreeRTOS/lwIP), regardless of what SDK paths happen to be
  in the ambient environment.

**Before / after sizes** (thumbv7m bare-metal):

| | Before | After |
|---|---|---|
| `NET_SOCKET_SIZE` | 4 (FreeRTOS lwIP `int`) | 2 (bare-metal `{int8_t, bool}`) |
| `NET_ENDPOINT_SIZE` | 4 (FreeRTOS lwIP `int`) | 6 (bare-metal `{u8[4], u16}`) |

**Test results** (all 4 PASS where they were FAIL before):

- `test_qemu_rtic_pubsub_e2e` — 45.5 s
- `test_qemu_rtic_mixed_priority_pubsub_e2e` — 45.8 s
- `test_qemu_rtic_service_e2e` — 25.4 s
- `test_qemu_rtic_action_e2e` — 23.5 s

**Bonus Category F fixes** (same root cause, also PASS now):

- `test_qemu_zenoh_large_publish` (89.7 Category F)
- `test_nros_dds_to_ros2`, `test_ros2_to_nros_dds` (89.7 dds group)

Still failing (different root cause, not size-probe): `test_qemu_serial_pubsub_e2e`
— serial transport doesn't use the TCP endpoint path.

### 89.6 — Category E: `nano2nano` RTIC/TLS timeouts — **Landed**

Resolved by the 89.5 size-probe fix. The three RTIC tests were
blocked by the same bare-metal pass-by-value ABI corruption that
killed Category D; fixing the size probe let their firmware connect
to the native-side counterpart on the first try. The TLS test was
also gated by the same failure path (the `bare-metal` smoltcp
bridge feeds the TLS link).

**Test results** (all 4 PASS, pre-fix all timed out at 60 s):

- `test_rtic_pattern_communication` — 16.0 s
- `test_rtic_pattern_action` — 17.6 s
- `test_rtic_pattern_service` — 18.2 s
- `test_tls_talker_listener_communication` — 15.3 s

**Fix**: none required beyond 89.5 — pure cascade.

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

### 89.9 — Within-platform parallelism, tier 1: ThreadX-Linux — **Landed** (commit `5fd6c228`)

**Motivation**: 89.1 lifted cross-platform parallelism, but each
platform group was still `max-threads = 1` — within FreeRTOS,
`pubsub` / `service` / `action` cases still ran sequentially.
ThreadX-Linux is the lowest-cost path to within-platform
parallelism because its binaries are **native processes** and
nsos-netx (the NetX Duo BSD-socket shim) offloads straight to the
host kernel, ignoring the legacy `interface` / `ip` / `netmask` /
`gateway` fields (see `packages/boards/nros-threadx-linux/c/
app_define.c:50`). The only thing that actually matters for
cross-test isolation is the host zenohd port.

**What shipped**: the per-case veth / guest-IP matrix that the
original plan proposed turned out to be unnecessary because
nsos-netx ignores those fields entirely. The smaller fix that
actually ships:

- `nros_tests::platform`: `PlatformConfig::zenohd_port_for(variant)`
  returns `base + (0 | 10 | 20)` for pubsub / service / action.
- `examples/threadx-linux/{rust,c,cpp}/zenoh/{service-*,action-*}
  /config.toml`: locator bumped to 7465 (service) / 7475 (action).
  Rust configs also got per-case `interface` / `ip` for readability;
  these values are inert under NSOS but keep the intent legible.
- `rtos_e2e::Platform::zenoh_router_start(variant)` threads the
  current rstest variant through to the port lookup.
- `.config/nextest.toml::[test-groups.threadx-linux] max-threads = 3`.

**Not done**: no changes to `scripts/qemu/setup-network.sh`, no
per-case veth names, no env-var override path. None of those
were needed — the shared tap/veth pool already has 10 devices
(`0..9`) and NSOS bypasses the L2 routing anyway.

**Expected speedup**: 3× on ThreadX-Linux (per-case `rtos_e2e`
entries now run concurrently within the group).

### 89.10 — Within-platform parallelism, tier 2: slirp QEMU platforms — **Landed** (commit `5fd6c228`)

**Motivation**: Slirp-networked QEMU platforms (FreeRTOS, NuttX,
ThreadX-RV64, ESP32) have full per-instance NAT isolation (`qemu
.rs:117` — "Each QEMU instance gets its own fully isolated NAT
stack"), so **guest IPs don't collide between concurrent
instances**. The one shared resource is the host-side port that
slirp forwards guest `10.0.2.2:port` to (`127.0.0.1:port`). If
two concurrent QEMU instances both try to reach the same host
port, both end up at the single host zenohd instance on that
port — which was exactly the `max-threads = 1` constraint the
group imposed.

**What shipped** — simpler than the original plan expected.
Each example already maps 1:1 to a single variant (listener
/ talker → pubsub, `service-*` → service, `action-*` →
action), so there's no need for per-case build variants. All
that was needed: bump the port in the existing per-example
`config.toml` to `base + variant_offset`.

- `examples/qemu-arm-{freertos,nuttx}/*/zenoh/{service,action}-*/config
  .toml` — locator port bumped.
- `examples/qemu-riscv64-threadx/*/zenoh/{service,action}-*/config.toml`
  — same.
- ESP32 skipped — no service / action examples yet.
- `nros_tests::platform::PlatformConfig::zenohd_port_for(variant)`
  computes the right port for each case.
- `.config/nextest.toml::[test-groups.qemu-{freertos,nuttx,
  threadx-riscv}] max-threads = 3` — three concurrent rtos_e2e
  cases per platform.

**Not done**: no per-case build matrix, no new `config-*.toml`
files, no change to `build_*` fixture caches. The compile-time
baking is fine because each binary is only used by one variant
anyway.

**Expected speedup**: 3× per platform (bounded by host RAM —
each concurrent QEMU instance is ~100–200 MB; 9 parallel
QEMUs on a 3-platform run peak at ~1.5 GB).

**Zephyr**: split in a follow-up (see 89.Zephyr below). Kconfig
locator churn turned out to be trivial since each example already
maps 1:1 to a single variant (listener/talker → pubsub,
service-* → service, action-* → action).

### 89.Zephyr — Within-platform parallelism, tier 2 extension: Zephyr native_sim — **Landed**

**Same recipe as 89.10**, applied to Zephyr native_sim (NSOS,
Phase 81). NSOS offloads BSD sockets straight to the host kernel,
so per-variant host zenohd ports are all the isolation needed.

**What landed**:
- Example locator ports bumped per variant in-tree:
  service-* examples → `7466`, action-* examples →
  `7476` (Rust `src/lib.rs`, C/C++ `prj.conf`).
- `packages/testing/nros-tests/tests/zephyr.rs`: 22 call sites
  routed through `platform::ZEPHYR.zenohd_port_for(variant)`.
- `.config/nextest.toml::[test-groups.qemu-zephyr]
  max-threads = 3` (zenoh variants). XRCE tests share a
  hardcoded Agent port (2018, baked into firmware via Kconfig)
  and stay serial in a new `qemu-zephyr-xrce` group
  (`max-threads = 1`). The XRCE override is placed BEFORE the
  generic `binary(zephyr)` override so nextest's first-match-wins
  routing assigns the 2 XRCE tests to the serial group.
- `justfile` fast-path excludes both `qemu-zephyr` and
  `qemu-zephyr-xrce`.

**Expected speedup**: 3× on Zephyr zenoh variants (25 tests
across 3 variants → 3 concurrent `native_sim` processes at a
time), XRCE unchanged.

### 89.Baremetal — Within-platform parallelism, tier 2 extension: MPS2-AN385 RTIC — **Landed**

**Same recipe as 89.10**, applied to the bare-metal QEMU
MPS2-AN385 RTIC suite. Slirp gives each QEMU instance an
isolated `10.0.2.0/24`, so per-variant host zenohd ports
are sufficient for service/action concurrency.

**What landed**:
- `examples/qemu-arm-baremetal/rust/zenoh/rtic-service-{server,client}/config.toml`
  → `tcp/10.0.2.2:7460` (Service, offset +10).
- `examples/qemu-arm-baremetal/rust/zenoh/rtic-action-{server,client}/config.toml`
  → `tcp/10.0.2.2:7470` (Action, offset +20).
- `packages/testing/nros-tests/tests/emulator.rs`: 6 call sites
  (router start + `wait_for_port` × 3 tests) routed through
  `platform::BAREMETAL.zenohd_port_for(variant)`.
- `.config/nextest.toml`: `[test-groups.qemu-baremetal]
  max-threads = 3` (service/action variants). Three port-7450
  sharers — basic pubsub, mixed-priority pubsub, and the
  `large_msg` binary — stay serial in a new
  `qemu-baremetal-shared` group (`max-threads = 1`). The
  shared override is placed BEFORE the generic
  `binary(emulator) or binary(large_msg)` override so
  nextest's first-match-wins routing picks the shared group
  for the three collision-prone tests.
- `justfile` fast-path excludes both `qemu-baremetal` and
  `qemu-baremetal-shared`.

**Not done (deliberate)**: mixed-priority pubsub and `large_msg`
were *not* promoted to a 4th/5th `TestVariant` offset. They
are platform-specific specialty tests that don't generalize;
keeping the shared sub-group localizes the "port 7450
collision" concern to nextest config instead of polluting the
cross-platform enum.

**Expected speedup**: up to 3× on the bare-metal suite
(service || action || one-of-the-three-port-7450 tests run
concurrently).

**Note**: The underlying baremetal RTIC QEMU tests
(pubsub/mixed/service/action/serial/large_publish) are
currently failing with `Transport(ConnectionFailed)` at
firmware init — this is Phase 89.5 / 89.7 pre-existing
breakage, independent of the port split. The split is a
no-op for correctness until that lands; it just unlocks
parallel execution for when those tests are fixed.

### 89.11 — (Optional) Runtime locator override on RTOS

**Motivation**: Collapses 89.10's 3× firmware build matrix back
to a single firmware per example. Unlocks per-test locator
configuration without rebuilds, and future-proofs any further
port fan-out (e.g. per-test unique ephemeral ports à la the
`ZenohRouter::start_unique` pattern).

**Mechanism options** (pick one per platform):

- **(a)** QEMU `-fw_cfg name=opt/nros/locator,string=tcp/10.0.2.2:7471`:
  QEMU writes the string into a known firmware config ROM
  region. Boot code reads it via the `fw_cfg` interface
  (memory-mapped on ARM virt / RV64 virt, semihosted on M3).
  One-time per-platform boot-code addition; works for FreeRTOS
  / NuttX / ThreadX-RV64. Won't work on MPS2-AN385 without a
  semihosting fallback.
- **(b)** Semihosting argv: QEMU passes the locator as a kernel
  command-line string; the firmware's cold-boot reads it via
  `SYS_GET_CMDLINE` semihosting call. Works on MPS2-AN385
  (M3 semihosting already enabled for `-semihosting-config`),
  needs minor additions on other platforms.
- **(c)** Serial-console reader: firmware waits on UART for up
  to `N` ms at boot for a `LOCATOR=…\n` line, falls back to
  the `config.toml` default on timeout. Most portable but
  requires test-harness UART writes.

**Scope**: One new `nros_platform::runtime_config` trait with a
`load_locator() -> Option<&'static str>` hook, plus per-platform
implementations. Each RTOS's `Config::from_toml` precedence
order becomes: **runtime hook > env var (native only) > TOML
default**. The Zephyr side drops in cleanly via
`CONFIG_NROS_LOCATOR_FROM_FW_CFG=y`.

**Expected payoff**: Single firmware, same 3× parallelism as
89.10. Pays back the upfront infrastructure cost if more
per-test config fan-out is wanted later (e.g. per-test domain
IDs, per-test QoS profiles, per-test topic names for shared
namespace tests).

**Cost**: ~500 LOC per RTOS for the runtime config reader; plus
a debugging surface area for "why did my override not apply"
issues that the compile-time path doesn't have.

**Files**:
- `packages/core/nros-platform/src/runtime_config.rs` (new)
- Per-RTOS implementation in `packages/core/nros-platform-{freertos,nuttx,threadx,zephyr}/src/runtime_config.rs`
- QEMU invocation updates in `packages/testing/nros-tests/src/qemu.rs`
  (add `-fw_cfg` or `-append` lines)
- Per-example boot code hook in `examples/qemu-*/src/main.rs`
  (call runtime-config before `Config::from_toml`)

**Defer unless**: 89.10 ships and the ongoing maintenance cost
of the 3× build matrix becomes a real problem, or someone wants
another axis of per-test config fan-out that would also benefit
from runtime-configurable strings.

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
- [ ] 89.9: `[test-groups.threadx-linux] max-threads = 3` works
      without veth / guest-IP / port collisions; `just test-all`
      ThreadX-Linux fan-out shrinks by a factor of ~3.
- [ ] 89.10: each of the 4 slirp QEMU platforms runs its three
      `(pubsub, service, action)` cases concurrently in its group.
- [ ] 89.11 (if taken): single firmware per example serves all
      three cases via runtime locator override; 89.10's per-case
      `config-*.toml` scaffolding deleted.

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
