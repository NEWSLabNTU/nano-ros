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

### B. Zephyr Cyclone-A9 DDS Rust (4 tests) → **3/4 CLOSED 2026-05-19, 1 deferred**

```
test_zephyr_dds_rust_action_a9_e2e             ✓ FIXED (160.B)
test_zephyr_dds_rust_service_a9_e2e            ✓ FIXED (160.B)
test_zephyr_dds_rust_talker_to_listener_a9_e2e ✓ FIXED (160.B)
test_zephyr_dds_rust_async_service_a9_e2e      ✗ Embassy spin_async
```

**Root cause (3 closed).** Two missing setup steps for the
`qemu_cortex_a9` target:

1. `armv7a-none-eabi` rust target not installed → cargo errored
   `error[E0463]: can't find crate for 'core'` on every nros-rmw-dds
   dep.
2. `packages/zpico/zpico-zephyr/src/zpico_zephyr.c` accessed
   `ipv4->unicast[i].is_used` directly — the prior "drop the
   `.ipv4` sub-struct" attempt (commit `defbb260`) was directionally
   wrong for Zephyr 3.7's `net_if.h`. The pin wraps each IPv4
   unicast entry in `struct net_if_addr_ipv4` with the
   `net_if_addr` carrying `is_used`/`addr_state` under
   `.ipv4` — restored.

**Fix.** `just zephyr setup` now explicitly:
- `rustup target add armv7a-none-eabi` (was relying on per-user
  prior install).
- `scripts/zephyr/cortex-a9-rust-patch.sh` runs on EVERY setup
  path (was only on the "already-present" branch). `just zephyr
  doctor` adds the rust-target check.

Plus `zpico_zephyr.c` field-access restored to
`unicast[i].ipv4.is_used` / `.addr_state`.

**Open.** `test_zephyr_dds_rust_async_service_a9_e2e` builds +
boots, reaches `Async service client ready: /add_two_ints` and
`Calling service: 5 + 3 = ?`, then hangs — Embassy `spin_async()`
doesn't drive the DDS RTPS request/reply path to completion. Sync
variant PASSES. Async-specific blocker, file as separate Embassy
follow-up.

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

**Hypothesis (refuted 2026-05-19).** Tested cascade after 160.A;
all 11 still fail with a DIFFERENT signature than A:

```
[err] nros_cpp_talker: nros::init(...) -> -100
socket_family_from_nsos_mid: socket family 6 not supported
```

`-100` is `NROS_RET_TRANSPORT_TX_FAILED` (Phase 155.B `_z_send_tcp`
maps to this). Plus an NSOS POSIX-socket-shim warning about an
unknown socket family. These tests use **zenoh** RMW (not XRCE
like cluster A), and the failure is on the TX path post-handshake,
not the support-init path A hit. Needs its own investigation
(likely zenoh-pico bare-metal `_z_send_tcp` regression on
Zephyr/NSOS, separate from NuttX's Phase 159 fix). Track as
**160.C** (new follow-up).

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

### E. ESP32 emulator (3 tests) → **CLOSED 2026-05-19**

**Resolution.** Same root-cause family as cluster D (Phase 159):
silent ABI mismatch between alias TU (`platform_aliases.c`) and
vendor zenoh-pico TX (`link/unicast/tcp.c`). On ESP32-C3 (RV32imc
baremetal) the alias TU declared the network `_z_sys_net_*_t`
typedefs as **16/32-byte opaques** (`nros_zenoh_generic_platform.h`),
while the vendor link layer compiled against `bare-metal/platform.h`'s
**6-byte endpoint** (`{uint8_t _ip[4]; uint16_t _port;}`) and
**2-byte socket** (`{int8_t _handle; bool _connected;}`). RV32 ELF
ABI: ≤8-byte structs pass inline in argument registers, >8-byte
structs pass via hidden pointer. Mismatch → vendor sent the IP
inline in `a1` (`0x0202000A` = `10.0.2.2` LE), alias TU tail-
called `nros_platform_tcp_open` treating `a1` as an endpoint
pointer → `lhu a2, 2(a1)` (smoltcp `Repr::emit` reading the high
half of `_ip`) faulted with `mtval=0x0202000C`.

Earlier hypothesis (smoltcp Interface globals / `bss` layout drift
/ Phase 132/141 regression / fb6b778b bisect target) was wrong —
the bug pre-dated all those commits but was masked by stale
incremental builds. Fresh full clean reproduced reliably.

Fix landed in two parts, both in `packages/zpico/zpico-sys/`:

1. **`c/zpico/platform_aliases.c`** — replaced the cross-platform
   16/32-byte opaque typedefs in the network section with the
   exact `bare-metal/platform.h` shape (`{int8_t _handle/_fd;
   bool _connected;}` socket, `{uint8_t _ip[4]; uint16_t _port;}`
   endpoint). Replaced the historical
   `#ifndef NROS_ZENOH_PLATFORM_USES_UNIX` skip-on-POSIX/NuttX
   gate with `#ifdef NROS_ZP_ALIAS_BARE_METAL_NET` —
   emit-only-on-baremetal, since every other platform's vendor
   `system/<rtos>/network.c` already provides `_z_open_tcp` /
   `_z_send_tcp` / etc. with the matching per-platform shape and
   the alias TU's wrappers compete with `--allow-multiple-definition`
   only to introduce silent ABI mismatches.

2. **`build.rs`** — moved `probe_net_type_sizes` to run BEFORE the
   alias-TU compile (it used to run after, so the
   `net_type_sizes.txt` file didn't exist when the alias build
   read it for the first post-clean run). For bare-metal builds
   the resolved sizes flow as `-DNROS_ZP_VENDOR_NET_SOCKET_SIZE=N`
   / `-DNROS_ZP_VENDOR_NET_ENDPOINT_SIZE=N` into the alias TU
   compile.

3. **Drift guard** — `_Static_assert(sizeof(nros_zp_alias_*_t)
   == NROS_ZP_VENDOR_NET_*_SIZE, "ABI drift")` inside
   `platform_aliases.c`. If a vendor header bump ever changes
   the `_z_sys_net_*_t` shape (TLS pointer added, port widened,
   etc.) the assert fires at compile time instead of producing
   another silent runtime fault. Verified by intentionally
   adding `+1` to the expected size — `error: static assertion
   failed: "alias TU's nros_zp_alias_socket_t drifted from
   vendor _z_sys_net_socket_t — pass-by-value ABI will corrupt"`
   immediately at `riscv64-unknown-elf-gcc`.

**Validation matrix.**

- `test_esp32_talker_listener_e2e` PASS (~31s)
- `test_esp32_to_native` PASS
- `test_native_to_esp32` PASS
- `test_qemu_rtic_pubsub_e2e` (cortex-m3 baremetal, same alias-TU
  path) PASS
- `test_qemu_lan9118_driver` PASS
- NuttX rtos_e2e Rust/C/Cpp (sequential) PASS — confirming the
  POSIX/NuttX gate inversion didn't reintroduce Phase 159's
  symptom
- FreeRTOS + ThreadX-Linux + ThreadX-RV64 rtos_e2e × Rust/C/Cpp
  (9 tests, sequential) all PASS — confirming the hosted-RTOS
  paths still consume vendor `network.c` exclusively, not the
  alias TU

#### Historical investigation (left for archaeology)


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

**Update 2026-05-19 (deeper bisect):**

- Reproduced with FRESH listener rebuild — earlier "listener
  works" observation was stale binary (5+ days old cache).
  Both talker AND listener crash identically with mtval=
  0x0202000c when rebuilt against current main.
- RA decode: `0x42021A06` → `smoltcp::wire::tcp::Repr::emit`.
  TrapFrame a5 = 0x02020000 (invalid ptr), faulting at
  offset 0xc into nonexistent memory.
- `.bss` symbol map looks normal: SOCKET_TABLE at 3fc8a190,
  TCP_RX/TX buffers at 3fcb57a4+, NET_SOCKETS at 3fcc1828.
  All in valid DRAM (0x3FC80000-0x3FCDFFFF).
- Bisect target: `fb6b778b` ("fix(bare-metal): resolve
  smoltcp_clock_now_ms + zpico.c link failures") is the most
  recent direct ESP32 platform-touching commit (5 days back).
  Earlier listener-alone success used a binary built BEFORE
  this commit; after the commit landed + we rebuild, both
  crash. Bisect blocked here this session — submodule
  rollback denied. Reproducer for next session:
  ```
  pkill -9 zenohd qemu-system 2>/dev/null
  build/zenohd/zenohd --listen tcp/0.0.0.0:7454 \
    --no-multicast-scouting > /tmp/zd.log 2>&1 &
  sleep 2
  qemu-system-riscv32 -M esp32c3 -icount 3 -nographic \
    -drive file=build/esp32-qemu/esp32-qemu-listener.bin,if=mtd,format=raw \
    -nic user,model=open_eth,id=net0 > /tmp/log 2>&1 &
  sleep 30; tail -30 /tmp/log
  ```
  Bisect window: git rev-list fb6b778b..HEAD -- packages/drivers/nros-smoltcp packages/zpico/zpico-sys packages/platforms/nros-platform-esp32-qemu packages/boards/nros-board-esp32-qemu — find the commit that flipped working → broken.

### F. QEMU bare-metal RTIC + serial (6 tests) → **5/6 CLOSED 2026-05-19** by cluster-E ABI fix

**Resolution (ETH path).** All five `test_qemu_rtic_*_e2e` +
`test_qemu_zenoh_large_publish` tests close out from the cluster
E fix in this same revision. The cortex-m3 mps2-an385 path
suffered the same alias-TU ↔ vendor zenoh-pico `_z_sys_net_*_t`
ABI mismatch as ESP32-C3 (both are bare-metal consumers of
`zenoh_generic_platform.h` and the alias TU's network section).
The earlier Phase 160.F "TCP handshake works, z_open fails fast"
triage (commit `2c5ed554`) correctly localized the failure to
zenoh-pico's `_z_open_tcp` → `nros_platform_tcp_open` call but
mis-attributed cause to "no `net_poll` task during
`Executor::open`". The actual root cause was the same
inline-vs-hidden-pointer struct ABI bug — cortex-m3 passes 6-byte
endpoint inline in `r0`/`r1`, but the alias TU was reading the
16-byte hidden-pointer form, so the first `lhu`/`ldrh` against
the "endpoint pointer" dereferenced the IP value (`0x0202000A` on
ESP32 LE; equivalent garbage on cortex-m3 LE).

Verified `cargo nextest run --test emulator`:

```
PASS test_qemu_rtic_pubsub_e2e
PASS test_qemu_rtic_action_e2e
PASS test_qemu_rtic_service_e2e
PASS test_qemu_rtic_mixed_priority_pubsub_e2e
PASS test_qemu_zenoh_large_publish (via large_msg crate)
```

**Remaining.** `test_qemu_serial_pubsub_e2e` still fails
(`published=0, received=0`) — this is the Phase 132.3 deferred
CMSDK UART known regression, NOT zenoh ABI. Tracks separately;
defer to Phase 132.3 follow-up.

#### Historical triage (left for archaeology)

**triaged 2026-05-19**

```
test_qemu_rtic_action_e2e             ETH — open -> Transport(ConnectionFailed)
test_qemu_rtic_mixed_priority_pubsub_e2e  ETH — same as above
test_qemu_rtic_pubsub_e2e             ETH — same as above
test_qemu_rtic_service_e2e            ETH — same as above
test_qemu_zenoh_large_publish         ETH — same as above
test_qemu_serial_pubsub_e2e           SERIAL — Phase 132.3 known-deferred
```

**Triage 2026-05-19 (Phase 160.F probe).**

- All five fixtures build cleanly: `cargo build --release` on
  `examples/qemu-arm-baremetal/rust/zenoh/talker-rtic/` +
  siblings finishes without warnings or link errors.
- `test_qemu_rtic_pubsub_e2e` end-to-end run under
  `qemu-system-arm -machine mps2-an385` reaches:
    - smoltcp ethernet init success
      (`IP: 10.0.2.10`, `MAC: 02:00:00:00:00:00`,
      `Ethernet ready.`)
    - then RTIC talker panics at `src/main.rs:68:57`:
      `called Result::unwrap() on an Err value:
      Transport(ConnectionFailed)`
    - listener also receives `Published=0 / Received=0`.
- Line 68 is `let mut executor = Executor::open(&exec_config)`.
  Failure is at zenoh-session TCP open against
  `tcp/10.0.2.2:7450` — host-side `zenohd` IS up on that port
  (test waits for `wait_for_port` to confirm).
- Boundary is therefore inside zenoh-pico's
  `_z_open_tcp` → `nros_platform_socket_*` path. smoltcp can
  ARP / RX / TX (Ethernet line ready) but the actual TCP
  handshake to the slirp gateway never completes from inside
  the firmware.
- Phase 132 (cmsdk-uart) is archived, never landed → not the
  cause.
- Phase 141 (wake-callback cortex-m3) added a
  `cycles_to_ns` helper + free-fn aliases on
  `nros-platform-mps2-an385` (commit `c262d145c`) — no socket
  path touched.
- The most recent zpico-side changes that touch the path are
  Phase 156's `a529afb15` (remove outer
  `NROS_ZENOH_PLATFORM_USES_UNIX` gate around the runtime
  aliases in `platform_aliases.c`) + Phase 159's `81910e006`
  (extend the inner network-section gate to NuttX). Both
  intentionally leave the bare-metal alias section active, so
  this is suspicious but not yet confirmed as the regression
  point. A worktree-based bisect against pre-Phase-154
  baseline aborted on missing rosidl-generated bindings
  (`examples/.../talker-rtic/generated/builtin_interfaces/
  Cargo.toml` absent in the historical tree — codegen pipeline
  has moved).

**Deeper probe (2026-05-19, session 2).** Direct FFI probes
injected into `examples/qemu-arm-baremetal/rust/zenoh/talker-rtic/
src/main.rs` (reverted before commit; see git history for the
diff) measured the canonical layers individually with semihosting
`println!`s:

| Probe call                              | Return | Wall time |
|-----------------------------------------|--------|-----------|
| `nros_platform_tcp_create_endpoint`     | `0`    | ~0 ms     |
| `nros_platform_tcp_open` (3 s timeout)  | `0`    | **1 ms**  |
| `nros_platform_tcp_send` (4-byte fake)  | `4/4`  | ~0 ms     |
| `nros_platform_tcp_read` (after 100 ms) | `0`    | —         |
| `zpico_init_with_config`                | `0`    | ~0 ms     |
| `zpico_open`                            | `-3`   | **<1 ms** |

Notable:
1. **smoltcp + slirp completes the TCP three-way handshake in
   ~1 ms** when called directly via the `nros_platform_tcp_*`
   canonical surface. The QEMU user-mode network slirp NATs to
   host port 7450 and `127.0.0.1:7450` (zenohd) responds inside
   the same QEMU mainloop tick.
2. **smoltcp accepts 4/4 bytes via `nros_platform_tcp_send`**;
   the tx-staging plumbing is alive on the canonical path.
3. **`zpico_open` returns `ZPICO_ERR_SESSION` (`-3`) in under a
   millisecond** — far faster than any TCP-connect timeout
   (`CONNECT_TIMEOUT_MS = 30000`) could expire. The fault is
   therefore *not* TCP connect, *not* alias-TU symbol resolution
   (`nm` confirms `_z_open_tcp` and `nros_platform_tcp_open`
   both resolve in `qemu-rtic-talker`), and *not* zenoh-pico
   config validation (which already returned `0` for
   `zpico_init_with_config`).
4. The failure must live inside `z_open` *after* `_z_open_tcp`
   succeeds — i.e. zenoh-pico's INIT / OPEN handshake. Likely
   suspects: a) the staged 4-byte send did not actually reach
   zenohd (smoltcp poll never serviced the tx queue because no
   `net_poll` task runs during `Executor::open`), b) the receive
   side returns 0 bytes so zenoh-pico's `_z_read_exact_tcp`
   either spin-fails or returns a length mismatch, c) the
   `_z_sys_net_socket_t` opaque storage that zenoh-pico passes
   by value between `_z_open_tcp` and `_z_send_tcp` carries
   stale bytes the smoltcp impl mis-reads (alias TU uses 32-byte
   opaque, smoltcp reads first 2 bytes as `{handle: i8,
   connected: bool}` — should be safe, but worth instrumenting).

The smoking gun on the in-firmware path: **no `net_poll` task is
spawned until `Executor::open` returns**. Zenoh-pico's `z_open`
issues `_z_send_tcp` then `_z_read_exact_tcp` synchronously; the
read path calls `nros_platform_tcp_read` which (per
`platform_macro.rs`) calls `SmoltcpBridge::poll_network()`
internally — but the *write* path goes straight to
`socket.send_slice` via the next pre-write `poll`, then needs a
*second* poll iteration to actually push the bytes to the wire.
With zenohd's INIT-ACK reply gated on the inbound INIT, the
firmware blocks in `_z_read_exact_tcp`'s polling loop, the loop
times out internally (zenoh-pico's `Z_TRANSPORT_LEASE` is
shorter than smoltcp's connect timeout), and `z_open` returns
`-1` early. This matches the <1 ms failure (zenoh-pico's own
short retry budget, not smoltcp's 30 s).

**Next steps** (split into Phase 160.F.x subphases for the
follow-up session):

- **160.F.1** — instrument zenoh-pico's `_z_open_link` /
  `_z_open_tcp` callers in `third-party/zenoh-pico/src/` with
  `Z_DEBUG` prints so each step of the INIT/OPEN handshake
  surfaces a label + return code via the alias TU's stderr
  (semihosting on bare-metal). Specifically wrap the
  `_z_send_n` / `_z_read_exact` calls inside
  `_z_transport_handshake_init` so we can tell whether the
  send returned the expected length and whether the receive
  loop hit zero-byte completion or read what it expected.
- **160.F.2** — exercise the *direct* `nros_platform_tcp_*`
  probe inside a release build of `talker-rtic` (the
  diagnostic probes from this session are in the git log; cherry-
  pick them as a temporary fixture). Wire it to write a real
  zenoh INIT frame (constructed from `z_transport_message_t`
  defaults) and read back the INIT-ACK, *with* a
  `SmoltcpBridge::poll_network` call interleaved on a 10 ms
  cadence (mimicking what `net_poll` does post-`Executor::open`).
  If that handshake succeeds, root cause is confirmed as
  "no poll task driving smoltcp during `Executor::open`"; the
  fix is to either start polling earlier (call
  `enable_wfi_idle` + a busy-poll inside `Executor::open` for
  bare-metal targets) or restructure RTIC examples to spawn
  `net_poll` *before* `Executor::open`.
- **160.F.3** — root-cause patch + cluster-wide regression
  guard test (host-side `cargo test` that asserts the alias
  TU exports `_z_open_tcp` symbol on bare-metal builds + a
  QEMU smoke that asserts `zpico_open` returns `0` within 2 s
  with zenohd reachable, not `-3`).

**Serial test (1 test).** Tracked under Phase 132.3 (descoped from
132). zenoh-pico bare-metal `connect_serial` Init/InitAck
regression post-Phase 128. Stays under Phase 132 follow-up.

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

### H. nano2nano + cross-RMW bridges (6 tests) → **partially closed 2026-05-19**

```
bridge_xrce_to_dds_starts_and_opens_both_sessions      → phantom (fixture skip)
bridge_zenoh_to_dds_starts_and_opens_both_sessions     → phantom (fixture skip)
test_c_rust_pubsub_interop                             → PASS
test_xrce_action_fibonacci                             → PASS
test_xrce_throughput_100hz                             → FAIL (real)
test_xrce_throughput_burst                             → FAIL (real)
```

**Status (2026-05-19).**

- Both `bridge_*` tests panic with `[SKIPPED] <lang> bridge binary
  not prebuilt` — same JUnit reclassification pattern as clusters
  G / M. Not real failures, just need fixture build before run
  (`cmake -B build -S . && cmake --build build` in the bridge
  example dirs, or `cargo build --release` for the Rust bridge).
- `test_xrce_action_fibonacci` + `test_c_rust_pubsub_interop`
  PASS without changes — no bridge-related regression.
- Remaining real failures: `test_xrce_throughput_{100hz, burst}`
  — receiver gets 1/3 expected messages in burst mode, fewer
  than expected at 100Hz steady. Likely XRCE-side
  send-buffer or agent rate-limit issue, NOT bridge or RMW-
  selection bug. Track as new XRCE follow-up (160.H.1) — not
  blocked on cluster-E ABI work.

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

### L. Native + misc (8 tests) → **CLOSED 2026-05-19 (8/8)**

```
test_c_xrce_listener_builds                                      ✓ FIXED (160.L)
test_c_xrce_listener_starts                                      ✓ FIXED (160.L)
test_c_xrce_talker_builds                                        ✓ FIXED (160.L)
test_c_xrce_talker_listener_communication                        ✓ FIXED (160.L)
test_c_xrce_talker_starts                                        ✓ FIXED (160.L)
test_native_talker_listener_communication::lang_1_Language__C    ✓ FIXED (160.L.1)
test_native_talker_listener_communication::lang_2_Language__Cpp  ✓ FIXED (160.L.1)
test_qos_reliable_delivery (and other QoS tests)                 ✓ verified PASS
test_zenoh_overflow_detection                                    ✓ FIXED (160.L.2)
```

**c_xrce_api family (5/5 PASS).** Root cause: Phase 154 dropped
`staticlib` from `nros-rmw-xrce-cffi`'s `[lib].crate-type` to
fix the no_std cross-compile panic_handler issue, but the comment
claimed "Corrosion's `--crate-type=staticlib` still works" — it
does not. `corrosion_import_crate(... CRATE_TYPES staticlib)`
FILTERS the available crate-types, it does not FORCE Cargo to
emit one. Result: `Found no targets in 35 packages` configure-time
error. Fix: created `packages/xrce/nros-rmw-xrce-cffi-staticlib/`
sibling crate carrying the `staticlib` crate-type, mirroring the
`nros-rmw-{zenoh,dds}-staticlib` pattern. Root `CMakeLists.txt`
xrce branch now imports the wrapper. cffi rlib stays Zephyr-safe.

**Native talker / Cpp talker (160.L.1 — 2/2 PASS).** Root cause:
the alias TU's `z_clock_now()` (added in Phase 129 along with the
generic-platform clock variants) routed through
`nros_platform_time_now_ms` (CLOCK_REALTIME). zenoh-pico's
`z_clock_*` contract is the *monotonic* clock — `unix/system.c:247`
uses `CLOCK_MONOTONIC`; `z_time_*` is the wallclock variant. The
mismatch meant `zpico_spin_once`'s cv-deadline
(`z_clock_now() + timeout_ms`) was a REALTIME-epoch number
(~1.78e15 ms in 2026), while `nros_platform_condvar_wait_until`
interpreted the deadline against `nros_platform_clock_ms`
(CLOCK_MONOTONIC, ~uptime ms). `rel_ms` came out ~55 YEARS;
`pthread_cond_timedwait` then blocked the executor thread forever
and the C/C++ talker's 1 Hz timer callback never fired
(`Publishing messages…` printed but no `Published:` line).
Reproduced by attaching gdb mid-spin and confirming a stuck
`futex_wait_queue` on `g_spin_cv+40`; addr2line traced the
linker-resolved `_z_condvar_wait_until` to the alias TU at
`platform_aliases.c:284`. Rust talker was unaffected because it
calls `nros::Executor::spin_blocking` whose internal cv path is
the Rust-side `std::sync::Condvar`-backed `wake_cv` — that one
uses `std::time::Instant` (monotonic) consistently. Only the C
talker hits zenoh-pico's `g_spin_cv` deadline path. Fix: route
`z_clock_now` + `z_clock_elapsed_*` through
`nros_platform_clock_ms` so the deadline epoch aligns with what
`nros_platform_condvar_wait_until` expects.

**`test_zenoh_overflow_detection` (160.L.2 — PASS).** The
original `Receive error` printf assertion never matched because
oversized payloads are dropped INSIDE the zenoh-pico C shim
(`packages/zpico/zpico-sys/c/zpico/zpico.c:595`: `if (payload_len
> r->payload_stride)` → call `notify` with NULL payload + return
without advancing `ring_tail`). Drops never reached the Rust
executor ring, so `try_recv_raw` returned `Ok(None)` forever
and never produced an `Err` to log. The test originally
expected zenoh-pico to surface `MessageTooLarge` upstream;
post-Phase-124 the silent-drop path was the only behaviour.

Fix: wire a process-wide `OVERFLOW_DROPS: AtomicU32` counter
that the `subscriber_notify_callback` bumps whenever the C
shim signals an oversized drop (`len > SUBSCRIBER_BUFFER_SIZE`
+ NULL payload + ctx is a valid subscriber). Expose via
`nros_rmw_zenoh::overflow_drops_total()`. `stress-zenoh`'s
`run_listener` prints `overflow_drops=<N>` in the `RECV_DONE:`
summary; the test parses + asserts `overflow_drops >= 1`.
Production code paths see zero overhead (one fetch_add per
oversized-drop on the C-callback edge — only fires when a
real overflow happens). Verified: 1/1 PASS in 15 s.

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
| B. Zephyr Cortex-A9 DDS Rust | 4 | Missing armv7a-none-eabi rust target + Zephyr 3.7 net_if.h field-access drift | **3/4 CLOSED 160.B** + 1 Embassy follow-up |
| C. Zephyr cross-host bridge | 11 | NOT cascade — zenoh `_z_send_tcp -> -100` on Zephyr/NSOS | New (160.C) |
| D. NuttX C/C++ rtos_e2e | 6 | Phase 159 fix landed | **CLOSED 2026-05-19** |
| E. ESP32 emulator | 3 | alias TU ↔ vendor `_z_sys_net_*_t` ABI mismatch (RV32 inline-vs-hidden-ptr) + drift guard via `_Static_assert` | **CLOSED 2026-05-19** |
| F. RTIC + serial bare-metal | 6 | RTIC (5): same alias-TU/vendor ABI mismatch as E (cortex-m3 6-byte endpoint, inline vs hidden ptr). Serial (1): Phase 132.3 deferred. | **5/6 CLOSED 2026-05-19** via cluster-E fix; serial → Phase 132.3 |
| G. cmake_platform_matrix cross | 4 | **phantom — already `[SKIPPED]`** | none (artifact of raw fail list) |
| H. nano2nano + bridges | 6 | 2 phantom (fixture skip); 2 PASS no changes; 2 XRCE throughput drops (real) | **4/6 closed 2026-05-19**; throughput → 160.H.1 |
| I. ThreadX-Linux rtos_e2e | 3 | fixture staleness | **CLOSED 2026-05-19** (rebuild) |
| J. RV64 C pubsub | 1 | recipe + Phase 159 fix landed | **CLOSED 160.J** (recipe `23e5650d`) |
| K. NuttX + ThreadX-Linux DDS | 2 | per-platform dust-dds bring-up | Phase 117-adjacent |
| L. Native + c_xrce + qos | 8 | c_xrce: Corrosion CRATE_TYPES misuse; talker: alias-TU `z_clock_now` epoch mismatch (REALTIME vs MONOTONIC); overflow: silent-drop counter unsurfaced | **CLOSED 160.L + 160.L.1 + 160.L.2** (8/8) |
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
