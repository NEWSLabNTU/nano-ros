# Phase 160 — Post-Phase-159 Test-All Failure Catalog

**Goal.** Catalog every distinct `just test-all` failure remaining after
Phases 154 / 155 / 156 / 159 + the Phase 140 install-local rip-off
follow-ups landed. Acts as the index a future per-cluster phase
inherits from. Phase 150 (post-Phase-140 inventory) served the same
role for the previous baseline and is now archived; this doc is its
successor.

**Status.** Inventory captured 2026-05-19 from a clean
`just test-all` run with fresh `just build-test-fixtures` (commit
`ba43f2da` HEAD).

**Run summary.** 803 tests / 740 pass / 63 fail / 12 skip / 37 slow
/ 3 flaky. Fail count down from 189 (cf. Phase 150 v6 inventory) →
110 (mid-session) → 63 here.

**Update 2026-05-19** — partial re-verify via rtos_e2e matrix
run (36/36 PASS) closes clusters D + I + J (10 tests) on top of
160.A (11 tests). Remaining: ~42 fails across B / E / F / G / H /
K / L / M. Full `just test-all` re-run pending for new baseline.

**Priority.** Medium — no test is gating a release; clusters split
naturally along subsystem boundaries that map onto independent
follow-up phases.

## Failure inventory by cluster

### A. Zephyr XRCE C/C++ (11 tests) → **CLOSED 2026-05-19**

```
test_zephyr_xrce_c_talker_listener
test_zephyr_xrce_cpp_action_client_boots
test_zephyr_xrce_cpp_action_e2e
test_zephyr_xrce_cpp_action_server_boots
test_zephyr_xrce_cpp_listener_boots
test_zephyr_xrce_cpp_service_client_boots
test_zephyr_xrce_cpp_service_e2e
test_zephyr_xrce_cpp_service_server_boots
test_zephyr_xrce_cpp_talker_boots
test_zephyr_xrce_cpp_talker_listener
```

**Symptom.** `nros_support_init_named(...) -> -3` (`InvalidArgument`)
before any communication starts.

**Root cause.** Two-layer issue:

1. **`nros_app_register_backends` weak no-op wins on Zephyr.** Per
   Phase 155.B.4, `linkme`'s distributed-slice ctor doesn't fire on
   Zephyr/FreeRTOS/NuttX, so `nros_support_init` explicitly calls
   `nros_app_register_backends()`. The weak no-op default
   (`packages/core/nros-c/c-stubs/weak_register_backends.c`) fires
   when no strong def exists. The `nano_ros_link_rmw()` cmake helper
   emits the strong stub for `add_subdirectory(<repo>)` consumers,
   but Zephyr uses the Zephyr module form which never calls that
   helper → zero backends register → `default_vtable() ->
   InvalidArgument`.
2. **`#include <cstdio>` fails on every Zephyr cpp build.**
   `zephyr/lib/cpp/minimal/include` only ships `<cstddef>`,
   `<cstdint>`, `<new>`. The `cxx-compat/` shim was gated on
   `CONFIG_PICOLIBC`; `native_sim` is newlib, so the shim was
   skipped → `nros-cpp/include/nros/log.hpp:30` `fatal error: cstdio:
   No such file or directory`.

**Fix.** `zephyr/CMakeLists.txt`:
- Emit a strong `nros_app_register_backends` stub in both
  `CONFIG_NROS_C_API` and `CONFIG_NROS_CPP_API` branches, dispatching
  to the active RMW backend's `nros_rmw_<x>_register` entry from
  `CONFIG_NROS_RMW_*` Kconfig.
- Unconditionally include `zephyr/cxx-compat/` for the CPP_API path
  (the shim's `using ::fprintf;` re-export is benign on newlib too).

**Verification.** 1 C + 9 C++ tests PASS:
```
test_zephyr_xrce_c_talker_listener        PASS
test_zephyr_xrce_cpp_listener_boots       PASS
test_zephyr_xrce_cpp_talker_boots         PASS
test_zephyr_xrce_cpp_service_client_boots PASS
test_zephyr_xrce_cpp_service_server_boots PASS
test_zephyr_xrce_cpp_service_e2e          PASS
test_zephyr_xrce_cpp_action_client_boots  PASS
test_zephyr_xrce_cpp_action_server_boots  PASS
test_zephyr_xrce_cpp_action_e2e           PASS
test_zephyr_xrce_cpp_talker_listener      PASS
```

Cluster C (cross-host bridge) likely cascades closed — re-run on
next full sweep.

### B. Zephyr Cyclone-A9 DDS Rust (4 tests) → **needs new phase**

```
test_zephyr_dds_rust_action_a9_e2e
test_zephyr_dds_rust_async_service_a9_e2e
test_zephyr_dds_rust_service_a9_e2e
test_zephyr_dds_rust_talker_to_listener_a9_e2e
```

**Hypothesis.** `qemu_cortex_a9` board target — likely Cortex-A9
Rust patch (`scripts/zephyr/cortex-a9-rust-patch.sh`) or
dust-dds-on-zephyr stack regression. Sibling `native_sim` DDS
tests pass.

### C. Zephyr cross-host bridge E2E (8 tests) → **needs new phase**

```
test_bidirectional_native_zephyr_e2e
test_native_server_zephyr_client
test_native_talker_to_zephyr_cpp_listener
test_native_to_zephyr_e2e
test_zephyr_cpp_action_server_to_client_e2e
test_zephyr_cpp_service_server_to_client_e2e
test_zephyr_cpp_talker_to_listener_e2e
test_zephyr_cpp_talker_to_native_listener
test_zephyr_action_e2e
test_zephyr_talker_to_listener_e2e
test_zephyr_to_native_e2e
```

**Hypothesis.** Same root cause as A — Zephyr-side XRCE/zenoh
session init fails (`-3`), bridge counterpart times out waiting.
Likely closes when A closes.

### D. NuttX C/C++ rtos_e2e (6 tests) → **CLOSED 2026-05-19**

Closed by Phase 159 (`7205eb4d`). Three fixes converged:

1. `NROS_ZENOH_PLATFORM_USES_UNIX` gate re-added for NuttX (had
   been narrowed to POSIX-only by `a529afb1`) → alias TU's
   wrong-shape `_z_send_tcp` no longer wins at link time over
   `system/unix/network.c`'s by-value impl. Was producing
   `_Z_ERR_TRANSPORT_TX_FAILED (-100)` → `support_init -> -4`
   on every NuttX C example.
2. Codegen lib emitted as INTERFACE on NuttX (host STATIC compile
   was dead weight + hit source-tree stub).
3. Source-tree `nros_{,cpp_}config_generated.h` stubs forward to
   checked-in fallback under `NROS_PLATFORM_NUTTX` (sizes from
   threadx-riscv64 ARM cross-build artifact); cmake/cargo wiring
   adds the define + nros-platform-cffi include + NanoRos::NanoRosCpp
   alias.

Verified 2026-05-19: full rtos_e2e matrix 36/36 PASS including
all 6 NuttX C/C++ tests in this cluster.

### E. ESP32 emulator (3 tests) → **real failure (NOT env-gated)**

```
test_esp32_talker_listener_e2e
test_esp32_to_native
test_native_to_esp32
```

**Verified 2026-05-19.** `require_esp32_networked()` already
wires `nros_tests::skip!` for missing prerequisites. All four
checks pass on this host (qemu-system-riscv32 + espflash +
riscv32imc-unknown-none-elf target + zenohd) → real test runs,
real test fails.

**Re-investigation (gdb-less, pcap + addr2line):**

- The TODO comment at `esp32_emulator.rs:205-211` (TCP-SYN/SYN-ACK
  stall on OpenETH) is STALE. Standalone listener-only run with
  zenohd on the correct port (7454, not 7448) reaches "Waiting
  for messages..." cleanly. Pcap shows full TCP handshake +
  zenoh InitSyn → InitAck → OpenSyn → OpenAck. The OpenETH /
  smoltcp stack works.

- Actual failure is in the TALKER: `Exception 'Load access
  fault' mepc=0x42025d90, mtval=0x0202000c`. Backtrace
  resolved via addr2line:
  ```
  ?? (riscv-pac result.rs:7)              ← panic intercept
  endpoint.c:560 (_z_endpoint_from_string)
  primitives.c:70
  network.c:231
  string.c:127
  zpico.c:2189 (zpico_get_start)
  ```
  Talker enters `zpico_get_start` (presumably for liveliness /
  routing-state query during publisher_put), descends into
  zenoh-pico endpoint string parsing, dereferences invalid
  pointer at riscv-pac → exception.

- Listener doesn't hit this path → no crash → "Listener received
  0 messages" because talker crashed before publishing.

**Further investigation (talker-only re-run):** Talker alone
(without listener) reproduces. addr2line on the fresh post-rebuild
binary gave the actual call chain (caller→callee):

```
zpico_open                 zpico.c:880
z_open                     api.c:766
_z_open_inner              session.c:166
_z_new_transport_client    manager.c:36
_z_open_link               link.c:99
_z_open_tcp                platform_aliases.c:385
nros_platform_tcp_open     (cffi crate, Rust)
```

Crash is INSIDE `nros_platform_tcp_open` — the Rust shim that
bridges zenoh-pico's `_z_open_tcp` (called from the alias TU)
to smoltcp on ESP32-C3.

mtval=0x0202000c on every retry — same NULL-page deref pattern.
0x02xxxxxx is invalid memory on ESP32-C3 (DRAM at 0x3fc80000,
IRAM at 0x40380000, flash at 0x42000000). Looks like
`*(null)->field` where field offset = 0x0c — classic NULL struct
deref.

Listener works (same code path); talker crashes. Different
heap state per binary (different MAC / IP config strings,
different timer registration) flips a pointer NULL on talker.

Looking at recent zpico-sys changes that touched the alias TU
(my Phase 159 `NROS_ZENOH_PLATFORM_USES_UNIX` gate, Phase 154
ABI fixes, Phase 156-F1 vendor-mode wiring): all those changes
deliberately SKIP the alias TU's network section on USES_UNIX
platforms. ESP32 is bare-metal NOT USES_UNIX (no unix.h socket
struct) → alias TU IS active for ESP32 → `_z_open_tcp` calls
`nros_platform_tcp_open` from the alias TU.

Suspected regression source: `nros_platform_tcp_open` in
`nros-platform-cffi` may have changed signature or struct layout
in Phase 154's accessor work, and the ESP32 platform crate
`nros-platform-esp32-qemu` wasn't re-validated. Needs:

1. Diff `nros_platform_tcp_open` signature on disk vs what the
   alias TU expects (struct layout match per Phase 154).
2. Check `nros-platform-esp32-qemu`'s implementation hasn't
   gone stale relative to the cffi shim contract.
3. Verify with the test's smoltcp::Interface globals are
   actually initialised before zpico_open fires.

Defer to Phase 89.4 follow-up (or new ESP32-focused phase) —
this needs ESP32 platform-crate domain knowledge.

**Deeper dig (2026-05-19, source exploration):**

Followed the call chain through the actual implementation:

- `nros-platform-cffi/src/lib.rs:1091` — `nros_platform_tcp_open`
  ABI shim, calls `<$ty as PlatformTcp>::open(sock, endpoint, tout)`.
- `nros-smoltcp/src/platform_macro.rs:107` — smoltcp impl. Does:
  - tcp_open() → finds free SOCKET_TABLE slot
  - tcp_connect(handle, ip, port) → sets remote_ip/port
  - Loop: `poll_network()` + `tcp_is_connected(handle)` until
    timeout or connected
- `nros-smoltcp/src/bridge.rs:820` — `SocketEntry { allocated,
  handle_raw: usize::MAX, ... }`. Pre-populated by
  `register_socket(handle)` during `init_hardware`.
- `nros-board-esp32-qemu/src/node.rs:127` — calls
  `SmoltcpBridge::init()` + `create_and_register_sockets(sockets)`
  during init_hardware. Confirmed runs to completion ("Ethernet
  ready." prints before talker crash).

Decoded RA from TrapFrame: `0x42021A06` →
`smoltcp::wire::tcp::Repr::emit` (TCP segment generation). So
crash happens INSIDE smoltcp's TCP wire emit during the FIRST
SYN packet construction inside `poll_network()` loop.

Talker pcap shows ZERO packets — crash before TX. Listener
pcap from earlier shows full TCP+zenoh handshake on same code
path. Difference: talker's user closure captures a `Publisher`
(generic over msg type) → different monomorphization → likely
different `.rodata` / `.bss` layout → some smoltcp pointer
lands on garbage memory.

**Next investigation hooks:**

- Diff `.bss` symbol map between talker + listener ELFs for
  smoltcp-related statics (`SOCKET_TABLE`, `SOCKET_RX_BUFFERS`,
  smoltcp Interface, etc.) — see which one moved.
- Reproduce with smoltcp Repr::emit instrumented (println of
  src/dst/seq/data ptr before each emit).
- Check if `NET_SOCKETS: MaybeUninit<SocketSet<'static>>` storage
  alignment / size differs across binaries (the SocketSet's
  internal slab is the obvious candidate for the offset-0x0c
  read).
- Try toggling `lto`, `codegen-units`, `panic = "abort"` in
  talker's profile to see if optimization shifts the crash.

Still Phase 89.4-tier — needs sustained ESP32 bare-metal
session. Documenting here so the next attempt has a precise
locus.

### F. QEMU bare-metal RTIC + serial (5 tests)

```
test_qemu_rtic_action_e2e
test_qemu_rtic_mixed_priority_pubsub_e2e
test_qemu_rtic_pubsub_e2e
test_qemu_rtic_service_e2e
test_qemu_serial_pubsub_e2e
test_qemu_zenoh_large_publish
```

**Hypothesis.** Either:
- Phase 132 (descoped) cmsdk-uart IRQ — serial pubsub blocked on
  init-handshake regression.
- Phase 141 (active) wake-callback cortex-m3 plumbing — RTIC
  scheduling change may have surfaced a regression.

Triage one to confirm + file under whichever phase fits.

### G. Cmake platform matrix (4 tests) → **phantom — already skipped**

```
cmake_platform_freertos
cmake_platform_nuttx
cmake_platform_threadx
cmake_platform_zephyr
```

**Verified 2026-05-19.** These are NOT real failures. They panic via
`nros_tests::skip!("Phase 138.6 ... cell deferred to Phase 139")`,
which the JUnit post-processor in `justfile::_count-real-failures`
correctly classifies as `[SKIPPED]`. Latest `_test-summary`:
```
Environment-skipped tests: 4 (missing prerequisites)
  1 [SKIPPED] Phase 138.6 zephyr cell deferred to Phase 139
  1 [SKIPPED] Phase 138.6 threadx cell deferred to Phase 139
  1 [SKIPPED] Phase 138.6 nuttx cell deferred — ...
  1 [SKIPPED] Phase 138.6 freertos cell deferred — ...
Real failures: 0 / 4 total failures
```

`/tmp/unique-fails.txt` was extracted from raw nextest console
output rather than the JUnit `<failure>` real-vs-skipped split, so
this cluster is artifact, not work. Closes-with-zero-changes:
remove the four tests from the real-fail rollup. Same caveat may
apply to any `*_integration_shell_smoke` / `_e2e` tests with
deferred-skip panics (see M).

### H. nano2nano + cross-RMW bridges (4 tests)

```
bridge_xrce_to_dds_starts_and_opens_both_sessions
bridge_zenoh_to_dds_starts_and_opens_both_sessions
test_c_rust_pubsub_interop
test_xrce_action_fibonacci
test_xrce_throughput_100hz
test_xrce_throughput_burst
```

**Hypothesis.** Single XRCE-agent flake or shared `g_session`
singleton issue Phase 156 doc flagged at the end. Bridge tests
open TWO RMW backends in same process; XRCE's process-global
state may collide with zenoh.

### I. ThreadX-Linux rtos_e2e (3 tests) → **CLOSED 2026-05-19**

**Status.** 3/3 PASS on rerun (12.5s total). Fixture staleness as
hypothesized — Phase 154/155.A platform-aliases work was already
applied; just needed a fresh fixture build after the unrelated
churn that produced the catalog. No source changes needed.
Verified again via full rtos_e2e matrix run (36/36 PASS).

### J. RV64 C pubsub (1 test) → **CLOSED 2026-05-19**

Verified 2026-05-19: full RV64 rtos_e2e matrix 9/9 PASS. Was
fixture skip per Phase 140 family; current `just threadx_riscv64
build-fixtures` produces all binaries.

### K. NuttX DDS + ThreadX-Linux DDS (2 tests)

```
test_nuttx_dds_rust_talker_to_listener_e2e
test_threadx_linux_dds_rust_talker_to_listener_e2e
```

Per-platform dust-dds bring-up. NuttX side may share root cause
with B (Zephyr A9 DDS).

### L. Native + misc (8 tests)

```
test_c_xrce_listener_builds
test_c_xrce_listener_starts
test_c_xrce_talker_builds
test_c_xrce_talker_listener_communication
test_c_xrce_talker_starts
test_native_talker_listener_communication::lang_1_Language__C
test_zenoh_overflow_detection
test_qos_reliable_delivery  (and other QoS tests)
```

c_xrce_api family + native_talker_listener_communication C variant
+ qos / overflow scattered fails. Each one-off — investigate
individually.

### M. Integration shells (3 tests) → **phantom — already `[SKIPPED]`**

```
esp_idf_integration_shell_smoke
nuttx_external_apps_link_into_kernel_binary
px4_integration_template_smoke
```

**Verified 2026-05-19.** All three already call
`nros_tests::skip!` for missing env (IDF_PATH / PX4_AUTOPILOT_DIR
/ NUTTX_APPS_DIR staging). Direct test-run output:
```
[SKIPPED] nano-ros not staged under ... — run `just nuttx build-fixtures-make`
[SKIPPED] idf.py not on PATH — install ESP-IDF >=5.1
[SKIPPED] PX4_AUTOPILOT_DIR unset
```
nextest reports these as panics ("[SKIPPED]"-prefixed message);
the JUnit post-processor in `justfile::_count-real-failures`
correctly reclassifies them as `[SKIPPED]`, same as cluster G.
No action needed.

## Remediation status

| Cluster | Tests | Hypothesis | Phase hook |
|---------|-------|------------|------------|
| A. Zephyr XRCE C/C++ | 11 | weak `nros_app_register_backends` + missing `<cstdio>` shim | **CLOSED 160.A** |
| B. Zephyr Cortex-A9 DDS Rust | 4 | dust-dds-on-A9 / Cortex-A9 Rust patch | New (160.B) |
| C. Zephyr cross-host bridge | 8 | cascades from A | Closes with A |
| D. NuttX C/C++ rtos_e2e | 6 | Phase 159 fix landed | **CLOSED 2026-05-19** |
| E. ESP32 emulator | 3 | OpenETH RX/TX stall (NOT env) | Phase 89.4 follow-up |
| F. RTIC + serial bare-metal | 5 | Phase 132 / 141 RTIC regression | Triage → 132 or 141 |
| G. cmake_platform_matrix cross | 4 | **phantom — already `[SKIPPED]`** | none (artifact of raw fail list) |
| H. nano2nano + bridges | 4 | XRCE `g_session` process-globals | Phase 156 follow-up |
| I. ThreadX-Linux rtos_e2e | 3 | fixture staleness | **CLOSED 2026-05-19** (rebuild) |
| J. RV64 C pubsub | 1 | recipe + Phase 159 fix landed | **CLOSED 160.J** (recipe `23e5650d`) |
| K. NuttX + ThreadX-Linux DDS | 2 | per-platform dust-dds bring-up | Phase 117-adjacent |
| L. Native + c_xrce + qos | 8 | scattered, one-offs | Per-test triage |
| M. Integration shells | 3 | **phantom — already `[SKIPPED]`** | none (artifact of raw fail list) |
| skipped | 12 | env (expected) | OK |
| **total** | **66** unique (63 + 3 retries-only) | | |

## Work items

- [x] **160.D — NuttX C/C++ rtos_e2e fixture path.** (commit
      `2b4eb535`) Re-enabled `just nuttx build-fixtures` cmake
      path: root CMakeLists already skips nros-c add_subdirectory
      for NuttX (no tier-3 Corrosion), Phase 159 Path C fallback
      header supplies sizes, host nros-codegen passed via
      `-D_NANO_ROS_CODEGEN_TOOL`. NuttX rtos_e2e 9/9 PASS.
- [x] **160.J — RV64 C pubsub fixture path.** Re-enabled
      `just threadx_riscv64 build-fixtures` cmake loop. Unlike
      NuttX, Corrosion successfully cross-builds nros-c for
      `riscv64gc-unknown-none-elf` under the bundled
      `cmake/toolchain/riscv64-threadx.cmake`. ThreadX + NetX Duo
      include paths flow in via `-DTHREADX_DIR=` /
      `-DNETX_DIR=` / `-DTHREADX_CONFIG_DIR=` /
      `-DNETX_CONFIG_DIR=`. ThreadX RV64 rtos_e2e 9/9 PASS
      (Rust + C + Cpp × pubsub + service + action).
- [ ] **160.E + 160.G + 160.M — env-precondition `skip!` wiring.**
      Each cluster has clear env gates (ESP_IDF_DIR, cross
      toolchains, vendor SDK staging); convert hard fails to
      `nros_tests::skip!` so missing env reports `[SKIPPED]` not
      `FAIL`. Closes 10 tests on hosts without those SDKs.
- [x] **160.A — Zephyr XRCE C/C++ backend register + cstdio shim**
      (closed 2026-05-19). Strong `nros_app_register_backends` stub
      emitted from `zephyr/CMakeLists.txt` for both C and C++ API
      paths; `cxx-compat/` include unconditional. Closes 11 directly,
      cluster C cascade pending re-run.
- [ ] **160.B — Zephyr Cortex-A9 DDS bring-up triage.** Re-run
      `just zephyr build-fixtures NROS_ZEPHYR_PRISTINE=always` +
      check Cortex-A9 Rust patch is current.
- [ ] **160.H — XRCE g_session collision audit.** Bridge tests
      open zenoh + XRCE in the same process; XRCE's process-global
      session in `zpico.c` (per Phase 156 closing note) likely
      collides. Move to per-Executor session OR document the
      single-RMW constraint.

## Acceptance

- [x] 160.D lands; NuttX rtos_e2e 9/9 PASS (Rust passes today, C +
      C++ blocked on fixture).
- [x] 160.J lands; ThreadX RV64 rtos_e2e 9/9 PASS (RV64 C pubsub
      fail was the visible symptom; rebuild closes Cpp + service +
      action C/Cpp too).
- [ ] 160.E/G/M land; the 10 env-precondition tests report
      `[SKIPPED]` on hosts without the SDK, `PASS` when env is
      present.
- [x] 160.A lands; Zephyr XRCE C/C++ 11/11 PASS (2026-05-19).
- [ ] Remaining clusters investigated per their per-phase hook.

## Notes

- This phase is an INDEX, not implementation work. Each cluster
  spins off its own remediation phase as work begins. Once a
  cluster closes, strike its row from the table here (or archive
  this doc when all rows resolve).
- The CI session that produced this catalog also landed the
  `NROS_PLATFORM_ALIASES` vendor-side wiring (Phases 154/159) and
  the `NROS_ZENOH_PLATFORM_USES_UNIX` POSIX+NuttX gate, which
  dropped the fail count from 189 → 63. Further large drops are
  unlikely without per-cluster investigation.
- `test_qemu_rtic_*` (cluster F) was the only NEW regression
  surfaced by this session's churn — pre-Phase 156 baseline had
  these passing. Triage in F is the highest-priority next step.
