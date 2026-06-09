# Platform & RMW implementation notes

Implementation-level detail relocated out of `CLAUDE.md` so the auto-loaded instruction file
stays a thin router. This is **reference** material (how the current impl behaves), not design
rationale — for the *why*, see the RFCs under [docs/design/](../design/). Pitfall one-liners
that agents need hot stay indexed in `CLAUDE.md`; the detail is here.

## Spin / yield

`zpico_spin_once` event-driven wake:

- POSIX/Zephyr: `_z_condvar_wait_until` on `g_spin_cv`.
- FreeRTOS: `xSemaphoreTake(g_spin_sem, …)`.
- NuttX: `sem_timedwait(&g_spin_sem_posix, …)` (pthread condvar hangs — archived Phase 55.12).
- Bare-metal: single-thread `zp_read` loop.

Cooperative yield (`PlatformYield`): POSIX/NuttX `sched_yield()`, Zephyr `k_yield()`, FreeRTOS
`vPortYield()`, ThreadX `tx_thread_relinquish()`, bare-metal `core::hint::spin_loop()` default,
opt-in `cortex_m::asm::wfi()` via `BoardIdle`. RTOS yields are not ISR-safe; `spin_loop()` is.

**Multi-threaded `zpico_spin_once` pitfall (critical):** on multi-threaded platforms
(Zephyr, POSIX) use `z_sleep_ms()`, not `select()`. `select()` returns immediately on any zenoh
protocol traffic (keep-alives, interest msgs), burning a `Promise::wait()` budget of 500
`spin_once(10)` iterations in ~39 ms instead of 5000 ms. FreeRTOS uses `vTaskDelay()`, smoltcp a
clock loop, single-threaded `select()+zp_read()` — all correct. Zephyr native_sim also needs
`CONFIG_NATIVE_SIM_SLOWDOWN_TO_REAL_TIME=y` for service/action clients.

## smoltcp multicast (bare-metal)

- `Interface::join_multicast_group(addr)` needs a multicast addr; smoltcp 0.12 returns
  `Unaddressable` for `0.0.0.0`. Pass the GROUP (`239.255.0.1`).
- `set_recv_timeout(_, 0)` in `define_smoltcp_platform!` = non-blocking poll.
- LAN9118 emulator filter rejects multicast unless `MAC_CR.MCPAS`; promiscuous (`PRMS`)
  recommended for QEMU `-nic socket,…`.
- `MAX_UDP_SOCKETS` default 4. RTPS needs 3/participant; zenoh/xrce 0..=1.

## NetX Duo BSD (ThreadX)

- `SO_RCVTIMEO` takes `struct nx_bsd_timeval *`, NOT `INT` ms. Wrong type →
  `wait_option = NX_WAIT_FOREVER` → deadlock. Use `nros-platform-threadx::set_recv_timeout_ms`.
- `fcntl(F_SETFL, O_NONBLOCK)` works (toggles `NX_BSD_SOCKET_ENABLE_OPTION_NON_BLOCKING`).
- NSOS-NetX shim translates `SO_RCVTIMEO` for threadx-linux. Accepts INT-ms and `nx_bsd_timeval`.

## Board transport features

`ethernet` (default MPS2-AN385/STM32F4/ESP32-QEMU) or `wifi` (ESP32) → TCP/UDP via
`zpico-smoltcp`; `serial` → UART via `zpico-serial` (bare-metal) or zenoh-pico built-in (ESP32,
Zephyr). `Config` fields are `#[cfg(feature)]`-gated. ≥1 transport required (`compile_error!`).
Transports coexist (locator selects). ESP32/ESP32-QEMU use zenoh-pico's serial (no `zpico-serial`).

## Parameter services

`param-services` feature in `nros-node` → `~/get_parameters`, `~/set_parameters`, etc. Uses
`nros-rcl-interfaces`. Handlers return `Box<Response>`.

## XRCE embedded build

`nros-rmw-xrce-cffi` (C FFI shim) gates `UCLIENT_PROFILE_{UDP,TCP,SERIAL}` +
`UCLIENT_PLATFORM_POSIX` + `transport_posix_{udp,serial}.c` on `target_os = linux|macos|*bsd`.
Bare-metal (`target_os = "none"`) gets only `UCLIENT_PROFILE_{DISCOVERY,CUSTOM_TRANSPORT,
STREAM_FRAMING}` and must inject its own custom transport. `just check-workspace-embedded`
excludes `nros-rmw-xrce{,-cffi,-cffi-staticlib}` (header-only backend's `internal.h` references
UDP types unconditionally; the staticlib sibling needs panic_handler resolution at compile time).
The `-staticlib` sibling lets Corrosion import a real `staticlib` target without forcing the cffi
rlib to emit one.

**XRCE-DDS RMW bugs (critical):** `uxr_buffer_request_data` must be flushed with
`uxr_run_session_time` immediately after the call — unflushed request_data in the reliable output
stream causes intermittent timeouts when `call_raw` buffers a service request later (fixed in all
three `create_*` methods). Reliable streams need `STREAM_HISTORY >= 2` (we use 4); history=1 fails
to recycle the single slot. The service client (requester) needs `uxr_buffer_request_data` to
receive replies, same as subscribers and repliers.

## Probe-only opaque sizes

`EXECUTOR_OPAQUE_U64S` etc. derive from `nros::sizes::EXECUTOR_SIZE` via the `nros_sizes_build`
rlib probe — no hand-math upper bound. Per-consumer `const _: () = assert!(size_of::<Ty>() <=
STORAGE_SIZE …)` enforces compile-time correctness. Probe=0 only on `cargo check
--no-default-features` (warns + 1-word placeholder; the resulting rlib must not be linked).
`CppContext` adds explicit `CPP_CONTEXT_OVERHEAD = 8` (u32 domain_id + alignment padding) on top
of `Executor`.

## Wrapper timing

`Future::wait()`, `Stream::wait_next()`, `Executor::spin(duration_ms)` budget by wall-clock via
`nros_cpp_time_ns()`. Iteration-count loops collapse on early-wake from signaled condvars
(keep-alives, discovery gossip).

## cbindgen output as canonical FFI

nros-cpp `*.hpp` headers `#include "nros_cpp_ffi.h"` directly; per-file hand-written
`extern "C"` redeclaration blocks were removed (drift broke things once). `qos.hpp` keeps a
fallback redef under `#ifndef NROS_CPP_FFI_H`. Exceptions: `parameter.hpp` cross-references
nros-c's `<nros/parameter.h>`; `action_{client,server}.hpp` `reinterpret_cast` `goal_id` at FFI
callsites (cbindgen renders `*const [u8; 16]` as ptr-to-array); `set_callbacks` excluded from
cbindgen via `[export.exclude]` and declared locally with plain fn-ptr typedefs. cbindgen variants
are prefixed with the enum name (`prefix_with_name = true`) to avoid C++ name collisions.

## QEMU networked tests

- Slirp networking (no TAP/sudo/bridges).
- Per-platform zenohd ports in `nros_tests::platform`: baremetal=7450, freertos=7451,
  nuttx=7452, threadx-riscv=7453, esp32=7454, threadx-linux=7455, zephyr=7456.
  `ZenohRouter::start(platform::FREERTOS.zenohd_port)`. Bridge-net (threadx-linux veth):
  `ZenohRouter::start_on("0.0.0.0", port)`.
- Subscriber first, then publisher. 5–10 s stabilization. Per-platform nextest groups
  (`max-threads = 1`); platforms run in parallel.
- Domain ID: compile-time on embedded (Zephyr Kconfig `CONFIG_NROS_DOMAIN_ID`; others via each
  example's `config.toml` `domain_id` → generated `app_config.h`), runtime env on native via
  `nros_tests::unique_ros_domain_id()`. For Cyclone (RTPS ports = `7400 + 250*domain`), parallel
  fixtures bake a distinct domain per communicating role-set.
- Patched `qemu-system-arm` (Phase 143): use `nros_tests::qemu::qemu_system_arm_cmd()`, never
  `Command::new("qemu-system-arm")`. New justfile recipes gate through the `QEMU_BIN` path_exists
  check. See `book/src/internals/qemu-patched-binary.md`.
- QEMU clock: `-icount shift=auto` (sleep=on) makes virtual time track wall-clock during WFI;
  full detail in [qemu-icount.md](qemu-icount.md).

## FreeRTOS pitfalls

- Stack overflow → "Invalid mbox": `Executor` has an inline `arena: [MaybeUninit<u8>; ARENA_SIZE]`
  on the task stack. Action examples use `NROS_EXECUTOR_ARENA_SIZE=8192`; `APP_TASK_STACK` must be
  16384 words (64 KB) for headroom.
- Deterministic `rand()` starts from seed 1 → duplicate Zenoh session IDs across QEMU instances;
  `srand()` with an IP-based unique seed in `nros_freertos_init_network()`.
- Manual-polling action server: `create_action_server()` is not arena-registered, so `spin_once()`
  does not process get_result queries — call `server.try_handle_get_result()` after
  `complete_goal()`.
- Poll task priority must be ≥ 4 (same as zenoh-pico read/lease tasks) to drain the RX FIFO.
- Debug guide: [freertos-lan9118-debugging.md](../guides/freertos-lan9118-debugging.md).

## Zephyr POSIX resource limits

Defaults `CONFIG_MAX_PTHREAD_MUTEX_COUNT=5` / `CONFIG_MAX_PTHREAD_COND_COUNT=5` are too low;
zenoh-pico needs ~8+ mutexes (transport TX/RX/peer + a write-filter mutex per publisher under
`Z_FEATURE_INTEREST=1`). Exhaustion makes `pthread_mutex_init` fail → zenoh-pico returns -80.
Set `CONFIG_MAX_PTHREAD_MUTEX_COUNT=32` and `CONFIG_MAX_PTHREAD_COND_COUNT=16` in `prj.conf`
(cyclonedds action overlays use 2048, archived Phase 184.8).

## NuttX ↔ zenoh-pico cooperation (Phase 225.O)

zenoh-pico is **not** platform-agnostic on the C side and is not meant to be;
the Phase 227.3 "platform-agnostic" refactor (`365d5cdce`) only made the
**Rust shim** (`nros-rmw-zenoh`) generic (no `target_os`/NuttX branches —
just feature gates). zenoh-pico C keeps per-platform system layers and
`#ifdef ZENOH_NUTTX` accommodations. How NuttX wires up:

1. **Feature → define.** `nros/platform-nuttx` forwards
   `nros-rmw-zenoh?/platform-nuttx` → `zpico-sys/nuttx` → `CARGO_FEATURE_NUTTX`,
   and `nros-zpico-build` then `#define ZENOH_NUTTX` + selects the **`unix`**
   system layer (`zenoh-pico/system/common/platform.h`: NuttX is grouped with
   `ZENOH_LINUX`/`MACOS`/`BSD`) + `LinkPolicy::nuttx()`. The forwarding clause
   is load-bearing: without it `ZENOH_NUTTX` is undefined and the setsockopt
   guards below stay off.
2. **`unix` system layer = direct POSIX.** NuttX is a hosted POSIX RTOS, so
   `system/unix/system.c` backs the primitives directly — `z_malloc`→libc
   `malloc`, `_z_task_*`→`pthread_create`/`join`, `_z_mutex_*`→`pthread_mutex_*`,
   sockets→NuttX kernel BSD sockets. No bare-metal platform shim is needed
   (contrast bare-metal/ESP32, which route through `nros-platform-*` + smoltcp).
   `nros-platform-nuttx` is a thin C-only glue crate (`platform.c`/`net.c`).
3. **`ZENOH_NUTTX` accommodations** (6 branches in `system/unix/network.c`):
   skip `<ifaddrs.h>`/`getifaddrs` (NuttX lacks it → multicast binds
   `INADDR_ANY`); skip `SO_LINGER` (no `CONFIG_NET_SOLINGER`); skip
   `TCP_NODELAY` (host vs NuttX optname value mismatch under cross-compile);
   use `MSG_NOSIGNAL` on `send` and free `getaddrinfo` results (both shared
   with `ZENOH_LINUX`). The SO_LINGER/TCP_NODELAY skips are why
   step 1's `ZENOH_NUTTX` define matters — otherwise those setsockopts fail on
   NuttX and `_z_open_tcp` returns `Transport(ConnectionFailed)`.
4. **Backend registration is explicit on NuttX.** The unified-RMW
   `nros_rmw_register_backend!` macro expands to a `linkme` distributed-slice
   entry on supported targets but to **nothing** on NuttX (linkme unsupported),
   and the standalone flat image does not run the auto-register `.init_array`.
   So `nros-board-nuttx::run_entry` calls `nros_rmw_zenoh::register()`
   explicitly before `Executor::open` (feature `rmw-zenoh`, wired entry →
   `nros-board-nuttx-qemu-arm` → `nros-board-nuttx`) — same shape as the esp32
   board.
5. **pthread pool.** Like Zephyr, NuttX needs enough pthread mutex/sem
   resources (zenoh-pico uses ~8+ mutexes; transport TX/RX/peer + per-publisher
   write-filter under `Z_FEATURE_INTEREST=1`). The reference `qemu-armv7a`
   defconfig suffices for talker+listener; raise the NuttX pthread limits for
   heavier graphs.

Verified 2026-06-09: native_sim-less, on `qemu-system-arm -M virt -cpu
cortex-a7`, the NuttX workspace Entry boots → registers → publishes `/chatter`
→ an external native listener receives it cross-process.
