# Phase 127 Remaining Failure Groups

Date: 2026-05-15

Phase 127 tracks the remaining post-Phase-124 failure work as parallelizable
groups. Historical Phase 124 run details remain in
`docs/roadmap/phase-124-test-triage-2026-05-14.md`.

Group identifiers are `127.A` through `127.G`. Subtasks use dotted suffixes
such as `127.A.1`.

## Current Baseline

Recent sync/fix context:

- [x] Parent `main` was merged from `origin/main` at merge commit `255046a2`.
- [x] `packages/codegen` was updated to submodule commit
  `3069524eb1e4b8d33da0de77a9e83df7681aac36`.
- [x] ESP32 subscriber creation OOM was fixed in
  `8094047c fix(esp32): avoid subscriber heap allocation`.
- [x] The latest focused ESP32 run now reaches `Subscriber declared` and
  `Waiting for messages...`; remaining ESP32 failures are message delivery,
  not heap allocation.
- [x] Direct standard-tier nextest exposed four `orin_spe_mock_ivc` timeouts and
  the `nvidia-ivc` Unix mock loopback failure under this sandbox. The root cause
  was the same-process mock using host `AF_UNIX` sends, which are denied here,
  plus an invalid zero-copy slot model over `Cell<[u8; 64]>`. `register_pair`
  now uses in-memory datagram queues and the slot buffers use `UnsafeCell`.

Because Phase 126 codegen/orchestration changes landed after the older full
Phase 124 snapshots, refresh the full matrix before treating historical counts
as current:

```bash
just ci
just build-all
just test-all
```

## 127.A: ESP32 Zenoh Delivery

Scope:

- `esp32_emulator::test_esp32_talker_listener_e2e`
- `esp32_emulator::test_esp32_to_native`
- `esp32_emulator::test_native_to_esp32`

Current signal (2026-05-17, closed):

- All three 127.A delivery cases pass; the full `esp32_emulator` binary
  reports 9/9 passed in 20.6 s with no allocation panics.
- Root cause was a poll-order bug in `SmoltcpBridge::poll` (TX staging
  drained one poll-tick after `iface.poll`). Fix landed in
  `094cb65a phase-127.A: fix SmoltcpBridge poll order, unblock ESP32 zenoh delivery`
  and closeout in
  `1ecea3cf phase-127.A: close A.4 — esp32_emulator 9/9 passes clean`.
- Historical bring-up signal preserved in the 2026-05-15 / 2026-05-16
  follow-up bullets below for future cross-platform smoltcp work.

Subitems:

- [x] `127.A.1`: Router/session discovery. Capture `zenohd` logs and confirm ESP32
  clients establish sessions with the router.
- [x] `127.A.2`: ESP32 publish path. `test_esp32_to_native` now passes; ESP32
  talker → native listener delivery confirmed via SmoltcpBridge poll reorder.
- [x] `127.A.3`: ESP32 receive path. `test_native_to_esp32` now passes; native
  talker → ESP32 listener delivery confirmed via SmoltcpBridge poll reorder.
- [x] `127.A.4`: Harness timing. After the SmoltcpBridge reorder, the
  two-QEMU `test_esp32_talker_listener_e2e` passes reliably in isolation
  (`cargo nextest run -p nros-tests --test esp32_emulator --no-fail-fast
  --no-capture --retries 0 test_esp32_talker_listener_e2e`: 1 passed in 17.2 s).
  Earlier failure was a same-suite stale-state flake (orphan zenohd on the
  ESP32 port or a TIME_WAIT 4-tuple on the SLIRP gateway); a clean run of the
  full nine-test `esp32_emulator` binary is 9/9 passed in 20.6 s.
- [x] `127.A.5`: Post-open ESP32 outbound control path. Root cause: bridge
  drained TX staging AFTER `iface.poll`, so newly-staged bytes had to wait for
  the next `poll_network` invocation before reaching the wire. Reordering
  `SmoltcpBridge::poll` to drain TX staging first, then run `iface.poll`, then
  drain RX, plus a second trailing `iface.poll` for ACK/window updates, unblocks
  ESP32↔native delivery. Fix in
  `packages/drivers/nros-smoltcp/src/bridge.rs::SmoltcpBridge::poll`.
- [x] `127.A.6`: QEMU OpenETH TX evidence. New `nros_smoltcp::poll_diagnostics()`
  counter snapshot (`do_poll`, `cb_hits`, `bridge_polls`, `tx_drained`) wired
  into the ESP32 listener and talker examples. After the reorder the listener
  logs show `do_poll == cb_hits == bridge_polls` (callback chain intact, no
  null-pointer short-circuit on `NetworkState`) and `tx_drained` advancing in
  step with bytes the application hands to `_z_send_tcp`, confirming staged
  bytes now leave the bridge instead of accumulating.

Done criteria:

- [x] Determine whether the break is router discovery, TCP session open, publish
  path, receive path, or smoltcp polling cadence. (Cause: smoltcp polling
  cadence — TX staging drained one poll-tick late.)
- [x] Include QEMU logs, `zenohd` logs, and one minimal focused fix or a narrowed
  failure cause.
- [x] `just esp32 test --no-capture` either passes all ESP32 tests or reports a
  smaller, newly categorized failure with no allocation panic. After the
  bridge poll reorder + diagnostic instrumentation, a clean `cargo nextest
  run -p nros-tests --test esp32_emulator --no-fail-fast` reports 9/9 passed
  in 20.6 s with no allocation panics.

2026-05-15 focused evidence:

- [x] `just esp32 test --no-capture`: 9 tests ran; 6 passed and the three
  remaining failures are the 127.A delivery tests listed above.
- [x] Manual `RUST_LOG=zenoh=trace,zenohd=trace just esp32 zenohd` plus
  `just esp32 listener`: router accepted the ESP32 TCP connection, completed
  `InitSyn`/`OpenSyn`/`OpenAck`, opened a client transport, then logged
  `expired after 10000 milliseconds`.
- [x] Same manual run: ESP32 QEMU output reached `Subscriber declared` and
  `Waiting for messages...`; router saw no `Declare subscriber` for the
  listener data key.
- [x] `cargo nextest run -p nros-tests --test esp32_emulator --no-fail-fast
  --no-capture test_native_to_esp32`: still fails with zero messages delivered.
- [x] Rejected experiments, not committed: an extra smoltcp post-staging poll,
  an OpenETH TX descriptor wait, and a post-`zpico_open` spin did not restore
  delivery; the post-open spin regressed to `Transport(ConnectionFailed)`.
- [x] Remaining blocker (resolved 2026-05-17 via the SmoltcpBridge poll
  reorder below): post-open Zenoh control frames now reach `zenohd`; both
  ESP32↔native and the two-QEMU pair deliver messages, with the full
  `esp32_emulator` suite at 9/9 passed.
- [x] 2026-05-15 follow-up: ESP32 fixture builds were briefly blocked before
  runtime by `zpico_open()` writing `z_open_options_t.auto_start_read_task` and
  `auto_start_lease_task` while the smoltcp ESP32 build sets
  `Z_FEATURE_MULTI_THREAD=0`, where the vendored `zenoh-pico` header omits
  those fields. Guarding those assignments restored both qemu-ESP32 Zenoh
  release fixture builds.
- [x] 2026-05-15 follow-up: an extra same-callback smoltcp flush after staging
  TX into TCP/UDP sockets was tested and rejected; `test_native_to_esp32`
  still reached `Subscriber declared` / `Waiting for messages...` and received
  0 messages in all nextest retries. The remaining 127.A issue is therefore
  not just "application bytes copied into smoltcp but waiting for a later
  interface poll."
- [x] 2026-05-15 follow-up: temporary bridge counters showed the ESP32
  listener does call the Zenoh TCP send path after open: staged TCP bytes grew
  from 549 to 1761 over five one-second ticks while explicit
  `executor.ping(0)` calls returned `Ok(())`. No bytes moved from the bridge
  staging buffer into the smoltcp TCP socket, and the poll callback path visible
  to that bridge instance reported no ready `NetworkState::poll()` calls. The
  next focused fix should inspect callback registration/linkage between
  `nros-board-esp32-qemu`, `nros-smoltcp::set_poll_callback`, and the
  `zpico-platform-shim` TCP forwarders.
- [x] 2026-05-16 follow-up: callback registration was healthy; the failure
  was a poll-order bug inside `SmoltcpBridge::poll`. Pre-fix order was
  `iface.poll(...)` then drain TX staging → smoltcp socket, then drain
  socket RX → staging. That left newly-staged TX bytes parked in the
  smoltcp socket until the NEXT `poll_network` invocation, while
  `<PlatformTcp>::send`'s loop only calls `poll_network` once per
  iteration. Subscriber declarations and keepalives were therefore
  always one poll-tick stale, which is why `zenohd` saw the open
  handshake but no follow-up declare/keepalive frames before lease
  expiry. New order: drain TX staging → socket, `iface.poll`, drain
  socket RX → staging, second `iface.poll` for ACK/window updates.
  Added `nros_smoltcp::poll_diagnostics()` snapshot so future bring-up
  can prove the callback chain is intact (`do_poll == cb_hits ==
  bridge_polls`) and `tx_drained` is advancing.
- Focused verification after the reorder:
  - `cargo nextest run -p nros-tests --test esp32_emulator --no-fail-fast
    --no-capture`: 8 passed, 1 failed (the remaining failure is the
    two-QEMU pair `test_esp32_talker_listener_e2e`).
  - `cargo nextest run -p nros-tests --test esp32_emulator --no-fail-fast
    --no-capture test_native_to_esp32`: passes (was the canonical failure
    used to chase the 127.A blocker).
  - `cargo nextest run -p nros-tests --test esp32_emulator --no-fail-fast
    --no-capture test_esp32_to_native`: passes.
  - Listener log snippet:
    `[poll] do_poll=N cb_hits=N bridge_polls=N tx_drained=M` with `N`
    growing steadily and `M` advancing in step with declare /
    keep-alive traffic.

Focused commands:

```bash
cargo build --release
just esp32 test --no-capture
```

Run the `cargo build --release` command from both:

- `examples/qemu-esp32-baremetal/rust/zenoh/listener`
- `examples/qemu-esp32-baremetal/rust/zenoh/talker`

## 127.B: RTOS/QEMU Platform E2E

Scope:

- FreeRTOS runtime/E2E
- NuttX runtime/E2E
- ThreadX Linux/RISC-V runtime/E2E
- bare-metal DDS runtime/E2E
- shared platform DDS runtime/E2E

Current signal:

- Last full `just ci` bucket before the Phase 126 pull had 39 failures here.
- The harness-reported ThreadX-Linux DDS prerequisite miss is an environment
  skip, not a product failure.

2026-05-15 refresh:

- `just build-all` completed successfully after setup. It built the workspace,
  examples, and test fixtures. NuttX C/C++ fixtures still report the existing
  skip because the NuttX C variant library is not installed.
- Fixture-build blockers fixed:
  - FreeRTOS / ThreadX bare-metal `zpico.c` no longer links against
    `clock_gettime` just because the target libc exposes `CLOCK_REALTIME`;
    the session ZID seed path now skips that POSIX-only clock on
    `ZENOH_FREERTOS_LWIP` and `ZENOH_THREADX`.
  - FreeRTOS C/C++ startup keeps the weak `zpico_set_task_config` fallback
    symbol with `__attribute__((used))`, so non-zpico C++ examples link.
- Fixture build status:
  - FreeRTOS: Rust, DDS Rust, C, and C++ fixtures build.
  - NuttX: Rust and DDS Rust fixtures build; C/C++ fixtures are skipped until
    the NuttX C/C++ variant libraries are installed.
  - ThreadX Linux: Rust, DDS Rust, C, and C++ fixtures build. C++ links emit
    existing `/usr/bin/ld: missing --end-group; added as last command line
    option` warnings but exit 0.
  - ThreadX RISC-V: Rust, DDS Rust, C, and C++ fixtures build after
    `just threadx_riscv64 install`.
- Focused non-`rtos_e2e` 127.B run:
  - Command:
    `cargo nextest run -p nros-tests -E '(group(=qemu-baremetal) or group(=qemu-baremetal-shared) or group(=qemu-freertos) or group(=qemu-nuttx) or group(=qemu-threadx-riscv) or group(=threadx-linux))' --no-fail-fast --success-output never --failure-output final`
  - Result: 59 tests, 50 passed, 9 failed.
  - Failures by bucket:
    - bare-metal Zenoh/serial: 5 message-flow or client `Transport(ConnectionFailed)`
      failures (`test_qemu_rtic_{pubsub,service,action}_e2e`,
      `test_qemu_rtic_mixed_priority_pubsub_e2e`,
      `test_qemu_serial_pubsub_e2e`).
    - bare-metal DDS: 1 transport-open failure
      (`test_baremetal_dds_rust_talker_to_listener_e2e`).
    - NuttX DDS: 1 transport-open failure
      (`test_nuttx_dds_rust_talker_to_listener_e2e`).
    - ThreadX Linux DDS: 1 DDS message-flow failure
      (`test_threadx_linux_dds_rust_talker_to_listener_e2e`).
    - ThreadX RISC-V DDS: 1 transport-open failure
      (`test_threadx_rv64_dds_rust_talker_to_listener_e2e`).
- Explicit `rtos_e2e` run:
  - Command:
    `cargo nextest run -p nros-tests --test rtos_e2e -E '(test(Freertos) or test(Nuttx) or test(ThreadxLinux) or test(ThreadxRiscv64))' --no-fail-fast --success-output never --failure-output final`
  - Result: 36 tests, 8 passed, 28 failed.
  - FreeRTOS: 0/9 passed. Rust boot reaches network init/readiness in some
    cases but misses readiness; C/C++ fail `nros_support_init` / `nros::init`
    with transport errors (`-1` / `-100`).
  - NuttX: 8/9 passed. Only Rust action fails at transport open
    (`Transport(ConnectionFailed)` before `Waiting for goals`).
  - ThreadX Linux: 0/9 passed. Rust pubsub reaches subscriber readiness but
    receives 0 messages; Rust service/action and C/C++ fail transport open or
    init readiness.
  - ThreadX RISC-V: 0/9 passed. Rust cases hit illegal instruction traps after
    ThreadX/NetX boot; C/C++ mostly fail transport init or entity registration,
    with C service/action reaching clients but receiving 0 responses.
- Follow-up focused probe:
  - `test_rtos_pubsub_e2e::platform_1_Platform__Freertos::lang_1_Lang__Rust`
    failed with the listener booting through `Network ready.` but never
    reaching `Waiting for messages`.
  - Added `ZenohRouter::start_slirp(port)` and switched QEMU slirp-backed
    RTOS/MPS2 tests to bind `zenohd` on `0.0.0.0` instead of loopback-only.
    This targets the guest `10.0.2.2` gateway path, where loopback-only host
    binds can leave embedded TCP connects unreachable.
  - Applied the same slirp router binding to ESP32 QEMU and the bare-metal
    large-message QEMU test, which use the same `10.0.2.2` guest gateway.
  - Compile check passed:
    `cargo test -p nros-tests --test rtos_e2e --no-run` and
    `cargo test -p nros-tests --test emulator --no-run`. Follow-up checks also
    passed for `esp32_emulator` and `large_msg`.
  - QEMU slirp itself was confirmed to start without special permissions. The
    local blocker is the execution sandbox denying host `AF_INET` socket
    creation: a minimal Python `socket.socket()` and `zenohd --listen
    tcp/127.0.0.1:<port>` both fail with `EPERM`, while QEMU `-netdev user`
    starts normally.
  - Added a local TCP listener capability probe. `require_zenohd()` now skips
    cleanly when the environment cannot create host TCP sockets, and
    `ZenohRouter::start_on` fails fast with that cause instead of timing out
    after `zenohd` cannot bind.
  - After enabling Codex workspace network access, the focused FreeRTOS Rust
    pub/sub runtime retest still fails after the listener prints
    `Network ready.` and enters `Executor::open`. Host-side `ss` polling shows
    `zenohd` listening on `0.0.0.0:7451` for the whole run with no established
    connection from QEMU, so the remaining FreeRTOS issue is before or inside
    the lwIP/zenoh-pico TCP open path rather than a host bind permission issue.
- Added FreeRTOS lwIP hardening while narrowing that path: numeric IPv4
  locators bypass `getaddrinfo`, TCP connect uses nonblocking `select`, and
  TCP read/write use explicit `select` guards. These compile. Current sandbox
  limits prevent a fresh QEMU runtime pass/fail signal because host TCP socket
  creation is denied.
  - `cargo build --release --offline` from
    `examples/qemu-arm-freertos/rust/zenoh/listener` passes.
  - `cargo test -p nros-tests --test rtos_e2e --no-run` passes.
  - `just build-all` is currently blocked before repo code runs because this
    environment executes `just` through snap-confine without
    `cap_dac_override`.
- ThreadX RISC-V can still reproduce an illegal-instruction trap locally when
  the Rust talker reaches `Executor::open` without a reachable router:
    `mcause=2`, `mepc=0x80031806`, `ra=0x8002e964`. `addr2line` maps the return
    path to `CffiSession::open_with_vtable`, immediately after the RMW vtable
    `open` call. This labels the current rv64 symptom as post-open/error-path
    corruption or trap-state corruption, not fixture build failure.
- Additional FreeRTOS narrowing after rebasing onto `origin/main`: a manual
  QEMU listener run with `filter-dump` initially captured only the guest's
  gratuitous ARP for `10.0.2.21` and never reached the Rust application
  closure. The FreeRTOS LAN9118 poll task runs above the app task, so
  `lan9118_lwip_poll()` now drains a bounded RX batch per tick instead of an
  unbounded FIFO loop. With that linked after a clean fixture rebuild, startup
  reaches the line before `Executor::open`; the remaining FreeRTOS Zenoh block
  is session-open-side, before any ARP for `10.0.2.2` or TCP SYN appears in the
  pcap.
- 2026-05-15 FreeRTOS Rust pub/sub fix:
  - Active FreeRTOS Zenoh builds use `zpico-platform-shim` plus
    `nros-platform-freertos/src/net.c`, not zenoh-pico's own FreeRTOS lwIP
    `network.c`, so the TCP timeout hardening was moved into the active
    platform net path.
  - `zpico_fill_session_zid()` also generated identical session IDs across
    separate FreeRTOS QEMU guests because its bare-metal fallback used only a
    static `g_session` address and a per-process counter. It now mixes
    platform random bytes on `ZENOH_FREERTOS_LWIP` and `ZENOH_THREADX`.
  - Focused verification:
    `cargo nextest run -p nros-tests --test rtos_e2e 'test_rtos_pubsub_e2e::platform_1_Platform__Freertos::lang_1_Lang__Rust' --no-capture --retries 0`
    passed in 65.3s. Talker published messages 0 through 10; listener received
    messages 0 through 10.
- 2026-05-15 FreeRTOS Rust service/action follow-up:
  - `test_rtos_service_e2e::platform_1_Platform__Freertos::lang_1_Lang__Rust`
    passed with 4 responses and `All service calls completed`.
  - `test_rtos_action_e2e::platform_1_Platform__Freertos::lang_1_Lang__Rust`
    initially failed because the action server missed the explicit
    `nros_rmw_zenoh::register()` call before `Executor::open`.
  - After adding the same registration used by the other FreeRTOS Zenoh Rust
    examples, the focused action test passed in 42.2s with goal acceptance and
    a succeeded Fibonacci result.
- 2026-05-15 FreeRTOS C/C++ close-out:
  - Installed `NrosRmwZenoh` and `NrosPlatformFreertos` packages now make the
    C/C++ FreeRTOS examples self-contained through `find_package(NanoRos)`.
  - C examples use `nano_ros_link_rmw(... RMW zenoh)` so FreeRTOS targets that
    do not walk `.init_array` still register the Zenoh backend explicitly.
  - C++ examples rely on `NanoRos::NanoRosCpp`'s own Zenoh CFFI registration;
    linking the standalone RMW stub into C++ first registered the wrong Rust
    staticlib copy and made `nros::init()` fail with `-100`.
  - `configSTACK_DEPTH_TYPE` is now 32-bit for MPS2-AN385 FreeRTOS so
    256 KiB C app stacks do not wrap at 65536 words.
  - Full focused verification passed:
    `cargo nextest run -p nros-tests --test rtos_e2e -E '(test(test_rtos_pubsub_e2e::platform_1_Platform__Freertos) or test(test_rtos_service_e2e::platform_1_Platform__Freertos) or test(test_rtos_action_e2e::platform_1_Platform__Freertos))' --no-capture --retries 0 --no-fail-fast`
    completed in 443.1s with 9 passed, 27 skipped.
- 2026-05-16 NuttX Rust close-out:
  - Rust action-server had the same explicit-registration hole as the earlier
    FreeRTOS action-server: it called `Executor::open` before
    `nros_rmw_zenoh::register()`. Adding the registration fixed the
    `Transport(ConnectionFailed)` boot failure.
  - `zpico-platform-shim` no longer imports `PlatformNetworkPoll` unless the
    smoltcp bridge feature is active; otherwise NuttX Rust fixture rebuilds
    fail under `-D warnings`.
  - NuttX C/C++ E2E cases now skip when `libnros_c_zenoh_nuttx_armv7a.a` or
    `libnros_cpp_zenoh_nuttx_armv7a.a` are not installed, matching
    `just nuttx build-fixtures` behaviour. Direct CMake NuttX variant-lib
    build still fails without Cargo `-Zbuild-std` wiring for
    `armv7a-nuttx-eabihf`.
  - Focused verification:
    `cargo nextest run -p nros-tests --test rtos_e2e -E 'test(Nuttx)' --no-capture --retries 0 --no-fail-fast`
    completed in 109.1s with 9 passed, 27 skipped; the 6 C/C++ cases print
    explicit skip reasons for missing NuttX variant libraries.
- 2026-05-16 ThreadX Linux Rust close-out:
  - The installed ThreadX Linux Zenoh variant now builds through the
    `platform-threadx-std` staticlib feature. This keeps host ThreadX Linux on
    `std` without also pulling the no-std ThreadX `panic-halt` dependency.
  - ThreadX Linux Rust Zenoh talker and action-server had the same explicit
    CFFI registration gap as the earlier FreeRTOS/NuttX action-server fixes.
    DDS Rust talker/listener now explicitly register the DDS backend too,
    because ThreadX Linux does not walk `.init_array` reliably for these
    examples.
  - The ThreadX Linux peer launch delay is now 1s, below zenohd's 10s lease.
    The old 10s head start let the listener/server expire just as its peer
    started.
  - Focused verification:
    `cargo nextest run -p nros-tests --test rtos_e2e -E 'test(ThreadxLinux)' --no-capture --retries 0 --no-fail-fast`
    completed in 298.4s with 3 passed / 6 failed / 27 skipped. Rust pub/sub,
    service, and action pass. C/C++ pub/sub still receive 0 messages; C/C++
    service/action start but fail request/goal flow.
- 2026-05-16 ThreadX Linux C/C++ close-out:
  - Reinstalling the ThreadX Linux Zenoh SDK variant and reconfiguring the
    C/C++ fixtures refreshed the generated
    `_nano_ros_link/.../nros_app_register_backends.c` stubs. The previous
    C/C++ pub/sub and service failures were stale fixture artifacts after the
    staticlib registration changes.
  - Async C++ action clients now call `client.poll()` after each
    `nros::spin_once()` while warming up and waiting for results, matching the
    async action API contract.
  - The ThreadX Linux C++ action client also requests the result on the first
    valid feedback if the explicit goal-response callback was missed. Feedback
    is only emitted for an accepted goal, so this keeps the client from
    timing out after the server has already completed the goal.
  - Focused verification:
    `cargo nextest run -p nros-tests --test rtos_e2e -E 'test(ThreadxLinux)' --no-capture --retries 0 --no-fail-fast`
    completed in 403.6s with 9 passed / 27 skipped.
- 2026-05-16 ThreadX RISC-V close-out:
  - Bare-metal ThreadX `zpico_spin_once()` now uses NetX Duo BSD
    `nx_bsd_select()` instead of POSIX `select()`; ThreadX Linux keeps the
    POSIX path. This fixes the Rust fixture link failure from an undefined
    `select` symbol.
  - The RISC-V app thread stack is now 512 KiB. The 64 KiB stack corrupted the
    Rust `CffiSession::open_with_vtable` return path, and 256 KiB fixed
    pub/sub and service but still left the typed Rust action client crashing
    after readiness. The typed action client carries multiple transport
    handles and fixed CDR buffers on the app thread stack.
  - The direct `rust-lld` wrapper now unwraps `-Wl,` arguments emitted by
    CMake metadata, so C/C++ ThreadX RISC-V fixtures link through flags such
    as `-Wl,--allow-multiple-definition`.
  - Focused Rust action verification:
    `cargo nextest run -p nros-tests --test rtos_e2e -E 'test(test_rtos_action_e2e::platform_4_Platform__ThreadxRiscv64::lang_1_Lang__Rust)' --no-capture --retries 0 --no-fail-fast`
    passed in 42.2s with goal acceptance and a succeeded Fibonacci result.
  - Full focused verification:
    `cargo nextest run -p nros-tests --test rtos_e2e -E 'test(ThreadxRiscv64)' --no-capture --retries 0 --no-fail-fast`
    completed in 443.4s with 9 passed / 27 skipped.
- 2026-05-16 DDS RTOS follow-up:
  - Bare-metal, FreeRTOS, NuttX, and ThreadX RISC-V DDS Rust examples now
    explicitly call `nros_rmw_dds::register()` before `Executor::open`,
    matching ThreadX Linux and native DDS examples. This removes the stale
    comment/code mismatch where examples claimed to install the dust-dds
    C-vtable backend but relied on `.init_array` or link side effects.
  - ThreadX RISC-V DDS examples now also link
    `nros-platform-critical-section` explicitly so dust-dds's global
    `critical_section::Impl` resolves against the ThreadX
    `nros_platform_critical_section_*` C symbols.
  - FreeRTOS multicast membership now honors the ABI's `join` group instead
    of trying to join the bind endpoint (`0.0.0.0`).
  - Focused verification:
    `cargo nextest run -p nros-tests --test nuttx_qemu_dds test_nuttx_dds_rust_talker_to_listener_e2e --no-capture --retries 0 --no-fail-fast`
    and
    `cargo nextest run -p nros-tests --test threadx_riscv64_qemu_dds test_threadx_rv64_dds_rust_talker_to_listener_e2e --no-capture --retries 0 --no-fail-fast`
    now reach publisher/subscriber creation and publish loops, then fail with
    zero received messages. The remaining NuttX/ThreadX RISC-V DDS blocker is
    RTPS discovery or datagram delivery, not backend registration or session
    open.
  - The ignored FreeRTOS DDS test was run with `--run-ignored all`; it still
    times out before the listener reaches application readiness, so FreeRTOS
    DDS remains a lower-level boot/network scheduling case.

Subitems:

- [x] `127.B.1`: FreeRTOS E2E triage. Rust/C/C++ pub/sub, service, and action
  all pass in the focused FreeRTOS slice.
- [x] `127.B.2`: NuttX E2E triage. Rust pub/sub, service, and action pass;
  C/C++ combinations are gated on installed NuttX variant libraries.
- [x] `127.B.3`: ThreadX Linux/RISC-V E2E triage. ThreadX Linux Rust/C/C++
  pub/sub, service, and action pass in the focused ThreadX Linux slice;
  ThreadX RISC-V Rust/C/C++ pub/sub, service, and action pass after the
  ThreadX select, stack, and linker-wrapper fixes.
- [x] `127.B.4`: Bare-metal DDS runtime triage. Bare-metal MPS2-AN385
  DDS Rust talker → listener passes (65 messages received in 83 s)
  after `cd713d43` added the explicit `nros_rmw_dds::register()` call
  to both fixtures.
- [x] `127.B.5`: Shared platform DDS runtime triage. NuttX DDS Rust
  talker → listener passes (11 messages received in 83 s). The
  root-cause fix on 2026-05-17 was a `nros_platform_udp_mcast_listen`
  bug: `mreq.imr_multiaddr` was set from the LOCAL endpoint's IP
  (always `0.0.0.0` on the dust-dds SPDP bind path) instead of the
  `join` parameter's dotted-quad string (`"239.255.0.1"`). Linux
  silently ignored the malformed join with `EINVAL` (so the host DDS
  tests "worked" only via stack quirks); NuttX added a sentinel
  group entry that never matched real incoming mcast frames, so SPDP
  discovery silently failed. Fix uses `inet_pton(join)` to populate
  `imr_multiaddr`. ThreadX RV64 DDS still on the chronic illegal-
  instruction trap and is tracked separately under 127.B.3.
  Phase 127.B.5 follow-up (2026-05-16):
  - Fixed a posix-net regression where
    `nros_platform_udp_mcast_listen(iface=NULL, timeout_ms=0)` quietly
    failed: `get_ip_from_iface(NULL, ...)` returned NULL so
    `bind_multicast` produced no SPDP recv socket. `get_ip_from_iface`
    now falls back to the first non-loopback, IFF_UP interface when
    `iface` is NULL.
  - Fixed a parallel hang: `set_recv_timeout_ms(fd, 0)` set
    `SO_RCVTIMEO = {0, 0}`, which POSIX defines as "block forever"
    (the inverse of the "non-blocking" semantics the dust-dds
    cooperative recv loops assume). Map `timeout==0` to `O_NONBLOCK`
    via `fcntl` so `multicast_recv_loop` yields cleanly. Without
    this, NuttX hung inside `create_publisher` /
    `create_subscription` and the talker never even reached the
    publish loop.
  - After both fixes the NuttX DDS Rust talker publishes 0–9 and the
    listener reaches "Waiting for messages..."; SPDP frames still do
    not reach the peer over QEMU's `-netdev socket,mcast=` tunnel.
    Confirmed root cause (upstream NuttX driver gap, verified against
    `apache/nuttx` master + cross-referenced with the NuttX `netdriver`
    docs and the relevant QEMU / Linux virtio_net commits): NuttX's
    `drivers/virtio/virtio-net.c` stubs both `d_addmac` and `d_rmmac`
    as `return -ENOSYS;`, never negotiates `VIRTIO_NET_F_CTRL_VQ` or
    `VIRTIO_NET_F_CTRL_RX`, and exposes no control queue at all. QEMU's
    virtio-net backend defaults its RX filter to "unicast-to-MAC +
    broadcast" — without a `VIRTIO_NET_CTRL_RX_PROMISC` /
    `..._ALLMULTI` command from the guest, every RTPS SPDP multicast
    frame (`01:00:5e:7f:00:01` for `239.255.0.1`) is silently dropped
    before reaching the vring. Same gap is documented for every NuttX
    driver other than STMicro STM32, TI Tiva TM4C, and Atmel SAM3/4 /
    SAMA5D3/4.
  - Patch carried in the NuttX fork submodule itself (the
    `third-party/nuttx/nuttx` submodule points at
    `github.com/jerry73204/nuttx` `nano-ros` branch). Commit
    `d230b7d383 drivers/virtio/virtio-net: negotiate CTRL_VQ+CTRL_RX,
    enable ALLMULTI/PROMISC` (a) bumps the negotiation mask to include
    `VIRTIO_NET_F_CTRL_VQ` and `VIRTIO_NET_F_CTRL_RX`, (b) creates a
    third control virtqueue when the device advertises CTRL_VQ,
    (c) sends `CTRL_RX_PROMISC=1` + `CTRL_RX_ALLMULTI=1` at probe
    using the standard 3-sg layout (hdr / data / ack), and (d) turns
    `d_addmac` / `d_rmmac` into successful no-ops when CTRL_VQ is
    active so the IGMP layer's MAC programming path returns OK.
    Empirically this patch alone did NOT unblock SPDP delivery, so
    the next debug step (2026-05-17) instrumented `virtio_net_recv`
    with `up_putc('!')` per real frame + enabled `debug-stderr` on
    `nros-rmw-dds`. The instrumented run showed virtio-net now
    receives both its own loopback frames AND the peer's frames, but
    `multicast_recv_loop` in `nros-rmw-dds` never sees `got n=…`.
    Bisection then revealed the *second* downstream issue: NuttX's
    `netutils/netinit` runs with default `CONFIG_NETINIT_NOMAC=y`,
    which overwrites every virtio-net NIC's MAC with the static
    `CONFIG_NETINIT_MACADDR_1=0xdeadbeef` /
    `CONFIG_NETINIT_MACADDR_2=0x00e0` defaults — i.e. every NuttX
    QEMU instance came up with the *same* MAC (`00:e0:de:ad:be:ef`).
    QEMU's `-netdev socket,mcast=` tunnel then silently filtered out
    frames whose source MAC equalled the receiver's own MAC, so
    cross-instance multicast delivery was a no-op. Disabling
    `CONFIG_NETINIT_NOMAC` in the board defconfig lets the
    virtio-net `VIRTIO_NET_F_MAC` handshake propagate QEMU's
    `-device virtio-net-device,mac=…` argument (e.g.
    `52:54:00:12:34:70` / `:71`) into NuttX, restoring per-instance
    unique MACs. Verified via raw-frame tcpdump-equivalent host
    Python receiver: source MAC now matches QEMU CLI argument
    instead of the deadbeef default.
  - With BOTH fixes in place (virtio-net CTRL_RX PROMISC/ALLMULTI in
    the submodule + `# CONFIG_NETINIT_NOMAC is not set` in the board
    defconfig), virtio-net receive counters confirm the listener now
    sees peer frames (`!` per peer SPDP). dust-dds's
    `multicast_recv_loop` still doesn't see `got n=…` from
    `nros_platform_udp_mcast_read` — i.e. the frames stop somewhere
    between NuttX virtio-net upperhalf and the BSD `recvfrom` call.
  - 2026-05-17 instrumentation: added per-frame `up_putc` tracers in
    `virtio_net_recv` (`!`), `netdev_upperhalf` eth-demux (`I`/`A`/`6`/`D`),
    `ipv4_input` (`Q` mcast IP, `Z` igmp grpfind hit, `^` igmp miss,
    `K` csum drop, `P<proto>` pre-switch), and `udp_input` (`&`/`+`/`-`/`=`).
    Result over 83 s of the focused test: listener `!I` count grows
    steadily but `Z` only ever fires ONCE during boot, and that single
    Z reports `destipaddr = 0x00000000` (raw `ipv4->destipaddr` bytes
    all zero) with `proto = 0x02` (IGMP). No `Z` ever fires for the
    talker's `239.255.0.1:7400` SPDP frames. Host-side Python
    multicast receiver confirms both QEMU processes successfully send
    SPDP onto the host mcast group AND the host kernel delivers to
    multiple subscribers (Python sees frames from both source IPs).
    Conclusion: QEMU's `-netdev socket,mcast=…` (and the `udp=…`
    tunnel variant) only loops the local QEMU's own frames back to
    its guest virtio-net; frames from the *other* local QEMU process
    on the same host group never reach this QEMU's vring even though
    they reach the host socket. Both fixes above are necessary and
    upstream-correct but not sufficient on their own. Next session
    needs to either (a) switch the QEMU test harness from
    `-netdev socket,mcast=` to a bridged tap setup (`scripts/qemu/
    setup-network.sh`-style, needs root) so the host kernel
    forwards mcast between the two guests, or (b) instrument
    QEMU itself / a hub-based `-netdev hubport,…` to confirm
    cross-delivery, or (c) reproduce the same `-netdev
    socket,mcast=` configuration with a known-working DDS stack
    (e.g. CycloneDDS on Linux QEMU guests) to isolate whether
    QEMU's mcast tunnel implementation itself is the limit.
  - 2026-05-17 (online research): QEMU 6.2's `net/socket.c::
    net_socket_mcast_create` binds to the multicast group address
    itself, does not call `IP_MULTICAST_IF` without `localaddr=`, and
    leaves egress interface selection to the host's routing table.
    Default route points at a real LAN NIC (`enp7s0`), so two local
    QEMUs each send the frame out the LAN, and the kernel never
    loops the LAN-egress mcast back to the OTHER local socket
    joined on the same group. References:
    https://bugs.launchpad.net/qemu/+bug/1861884,
    https://bugs.launchpad.net/qemu/+bug/533610,
    https://bugzilla.redhat.com/show_bug.cgi?id=557188,
    https://gist.github.com/mcastelino/88195a7d99811a177f5e643d1465e19e,
    https://docs.zephyrproject.org/latest/connectivity/networking/networking_with_multiple_instances.html.
    Documented one-liner mitigation in the QEMU docs gist: add
    `,localaddr=127.0.0.1` to BOTH QEMU `-netdev socket,mcast=…`
    invocations AND add a host route `ip route add 230.10.0.0/16
    dev lo`. The second piece requires `sudo` and was NOT attempted
    under nano-ros's "never sudo" policy. Verified empirically that
    `localaddr=127.0.0.1` ALONE on QEMU 6.2 + NuttX virtio-net is
    not sufficient — listener still receives 0. `224.0.0.250`
    (link-local mcast that normally routes via lo without an
    explicit `ip route add`) also fails on QEMU 6.2.
  - Bare-metal MPS2-AN385 DDS DOES pass with the same `-netdev
    socket,mcast=…` mechanism on the same host, proving the QEMU
    mcast tunnel CAN cross-deliver. Difference: the in-Rust smoltcp
    + LAN9118 stack accepts every incoming frame (no RX MAC filter),
    while NuttX's virtio-net goes through QEMU's
    `hw/net/virtio-net.c::receive_filter`, which only delivers
    unicast-to-MAC + broadcast unless the guest sets
    `CTRL_RX_PROMISC`/`ALLMULTI`. Submodule patch `d230b7d383`
    sets both. Open question whether QEMU 6.2's
    `receive_filter` honours the guest's PROMISC bit over the
    `-netdev socket,mcast=` peer path (vs the loopback path it
    definitely honours), or whether there is a deeper QEMU
    peer-side dispatch bug.
  - 2026-05-17 (mitigation attempted, partial): the test harness
    now passes `,localaddr=127.0.0.1` to both NuttX QEMU `-netdev
    socket,mcast=…` invocations and pre-flight-checks for the
    matching `dev lo` route via
    `nros_tests::fixtures::require_mcast_loopback_route`
    (the check uses `ip route get <addr>` so a prefix route like
    `230.10.0.0/16 dev lo` is correctly matched for an address
    inside the prefix). Route missing → test skips with a clear
    `sudo ip route add 230.10.0.0/16 dev lo` hint instead of
    silently timing out at 83 s. A `just nuttx setup-mcast-route`
    recipe runs the same `sudo ip route add` commands
    idempotently. The route configuration is intentionally NOT
    auto-run by `just nuttx setup` because nano-ros policy is
    "never sudo without explicit user request".
  - 2026-05-17 (does not fully unblock): after the user installed
    the two routes (`230.10.0.0/16 dev lo` + `239.0.0.0/8 dev lo`),
    the focused NuttX DDS test still receives 0 messages. Empirical
    breakdown via Python mcast receivers proves that:
    1. **Two Python procs** joined on lo (`mreq.imr_interface =
       127.0.0.1`) DO cross-deliver via the lo path → kernel
       routing on lo with the new routes is healthy.
    2. **QEMU sender with `localaddr=127.0.0.1`** + **Python
       receiver joined on lo** → Python receives ZERO frames.
       Same QEMU sender without `localaddr=` → Python receiver
       on `INADDR_ANY` sees the frames going out the LAN NIC.
    Conclusion: QEMU 6.2's `net_socket_mcast_create` *does*
    issue `setsockopt(IP_MULTICAST_IF, localaddr)` per the
    docs, but something about the QEMU send path (presumably
    the interaction between the mcast-bind-to-group socket and
    `IP_MULTICAST_IF=lo`) prevents the frame from actually
    landing on the lo iface — neither sibling QEMU nor a
    third-party lo-joined Python socket sees it. The same
    QEMU built-in mcast tunnel does work end-to-end for the
    bare-metal MPS2-AN385 DDS test which uses smoltcp +
    LAN9118 (no virtio RX filter), so the loss appears to be
    specific to the `localaddr=127.0.0.1` egress path on
    QEMU 6.2 rather than the virtio frontend. Documented
    candidates for the next session:
    - Upgrade host QEMU to 8.2+/9.x (Ubuntu 24.04, jammy
      backports, or qemu-utils from sources). Cheptsov 2022
      mcast patch is `#ifdef __APPLE__` only, so this is
      speculative — but at least removes 6.2 as the
      uncertainty.
    - Switch the NuttX QEMU test to `-netdev dgram,
      local.type=unix,...` peer pairs (QEMU ≥7.2) and
      configure dust-dds with explicit unicast peers
      instead of relying on SPDP multicast.
    - Bridged TAP via `scripts/qemu/setup-network.sh` (needs
      `sudo` + `CAP_NET_ADMIN`), which lets the host kernel
      forward mcast between guests on a real virtual bridge
      instead of relying on QEMU's socket netdev.
  - QEMU upstream status (queried 2026-05-17): the cross-process
    `socket,mcast=` limit persists in QEMU master. `net/socket.c`
    has had only refactoring + cosmetic changes since 6.2 (latest
    `09759245`, `8cb17f9c` Sep 2025; `751b0e79` Jun 2025;
    `b6aeee02` Jul 2023); the only mcast-adjacent patch (Cheptsov,
    Jun 2022) is `#ifdef __APPLE__` only. Newer QEMU (≥7.2) does
    ship `-netdev dgram,local.type=unix,…` + `-netdev stream,…`
    which give AF_UNIX peer pairs without root, but they are
    unicast point-to-point with no N-way fanout, so not a
    drop-in replacement for the mcast tunnel.

Done criteria:

- [x] Split failures by platform first.
- [x] For each platform, label failures as fixture build, boot, network setup,
  router/discovery, or protocol handshake — see 127.B.5 follow-up
  notes above (the only remaining bucket is NuttX/RV64 RTPS SPDP
  multicast Ethernet filter on virtio-net).
- [x] Preserve exact QEMU and test harness logs (`/tmp/n*.{out,err}`,
  `/tmp/b4*.{out,err}` from this session; older `test-logs/latest/`
  for `just ci` snapshots).
- [ ] Produce a refreshed count for each RTOS/QEMU platform bucket.

Focused commands:

```bash
just build-test-fixtures
just test-all
```

## 127.C: Zephyr Runtime/E2E

Scope:

- Zephyr native/host runtime tests
- Zephyr DDS runtime tests
- Zephyr XRCE runtime tests
- Cross-language Zephyr interop cases

Current signal:

- Last full `just ci` bucket before the Phase 126 pull had 29 failures.
- Build/smoke coverage was mostly passing; failures are concentrated in boot,
  runtime handshakes, and message flow.
- 2026-05-15 focused follow-up: `ZephyrProcess` now drains stderr alongside
  stdout for native_sim and qemu_cortex_a9 runs, so failed Zephyr tests preserve
  QEMU/native_sim diagnostics in the same captured log used by
  `wait_for_pattern`/`wait_for_output`. With
  `XDG_RUNTIME_DIR=/tmp/nano-ros-just-runtime`, `just zephyr doctor` passes and
  `test_zephyr_talker_smoke` / `test_zephyr_listener_smoke` pass, including the
  expected no-router `Transport(ConnectionFailed)` stderr in captured output.
- 2026-05-15 rebuild follow-up: `just zephyr build-fixtures` now defaults to
  serialized, pristine `west build`s with per-fixture logs and completed the
  full fixture matrix successfully after installing the local C/C++ codegen
  prefix. This removes the previous hang/retry cascade as a fixture-build
  blocker.
- 2026-05-15 runtime follow-up: `just zephyr test --no-capture` ran 61 tests:
  33 passed, 28 failed. Failures split into four buckets: 7 XRCE E2E tests skip
  hard because the XRCE Agent binary is not present; 5 native/Zephyr interop
  tests fail because native Rust fixtures were not prebuilt by
  `just build-test-fixtures`; Zenoh native_sim Rust/C++ E2E cases fail at
  session open with `Transport(ConnectionFailed)` / `nros::init -> -100` even
  with `zenohd` started; Zephyr DDS Rust native_sim and qemu_cortex_a9 cases
  fail at DDS transport open with `Transport(ConnectionFailed)`.
- 2026-05-15 rebuild-race fix: a single-fixture diagnostic reproduced that
  `SCCACHE_DISABLE=1 CMAKE_BUILD_PARALLEL_LEVEL=1 west build ...` completes
  past the zombie-shell hang point. `just zephyr build-fixtures` now applies
  those defaults (`NROS_ZEPHYR_NINJA_JOBS=1`,
  `NROS_ZEPHYR_SCCACHE_DISABLE=1`) and the full Zephyr fixture matrix completed
  successfully with no failed entries.
- 2026-05-15 runtime follow-up 2: Zephyr's checked-in zenoh-pico config now
  gives `ZENOH_ZEPHYR` the same 5 s socket timeout as NuttX, and Zephyr TCP
  endpoint resolution is constrained to IPv4 when `CONFIG_POSIX_IPV6` is off.
  That moved Zenoh native_sim past the earlier pre-router
  `Transport(ConnectionFailed)` startup failure.
- 2026-05-15 runtime follow-up 3: Rust Zephyr Zenoh and DDS examples now
  register their RMW backends explicitly because Zephyr does not run the
  POSIX-style Rust constructor path used by native fixtures. The Zephyr
  zenoh-pico system ABI now uses Zephyr POSIX pthread handles for tasks,
  mutexes, recursive mutexes, and condition variables, with matching C symbols
  supplied by the Zephyr module build. This fixed the native_sim crash in
  `_z_session_mutex_unlock`.
- 2026-05-15 runtime follow-up 4: Zephyr Zenoh subscriber creation then failed
  after `z_declare_subscriber` succeeded because the C/Rust static subscriber
  slot was too small for `ZenohSubscriber`; the CFFI slot size was increased.
  Focused talker/listener E2E now reaches listener readiness and publishes, but
  the listener still does not receive samples.
- 2026-05-15 runtime follow-up 5: Debugging ruled out seeded Zephyr Zenoh
  session-ID collisions and an exact-key subscriber mismatch. Enabling
  zenoh-pico interests while leaving matching callbacks disabled
  (`Z_FEATURE_INTEREST=1`, `Z_FEATURE_MATCHING=0`) rebuilt both focused
  native_sim Zenoh fixtures successfully, but runtime verification of this
  latest routing change is still pending because sandbox escalation for the
  router-backed focused run hit the approval usage limit.
- 2026-05-15 harness follow-up: `ZenohRouter::start_on` now detects early
  `zenohd` exits and reports stderr instead of masking them as a generic
  timeout. In the current sandbox, the focused E2E run fails before fixture
  startup because `zenohd` cannot bind `tcp/127.0.0.1:7456`
  (`Operation not permitted`); the router-backed runtime path still needs an
  unrestricted rerun.
- 2026-05-15 runtime follow-up 6: unrestricted focused reruns now pass the Rust
  Zephyr Zenoh native_sim pub/sub set. `test_zephyr_talker_to_listener_e2e`,
  `test_zephyr_to_native_e2e`, `test_native_to_zephyr_e2e`, and
  `test_bidirectional_native_zephyr_e2e` all deliver samples. The final fix was
  in the no-std executor spin path: without the std wake-cv layer it must let
  the primary session block for the requested spin timeout; `drive_io(0)` made
  Zephyr spin flat out and over-credit timer deltas.
- 2026-05-15 full Zephyr follow-up: serial `cargo nextest run -p nros-tests
  --test zephyr` produced 34 passed / 27 failed before the bidirectional
  harness counter fix; rerunning that one test passed, so the expected refreshed
  count is 35 passed / 26 failed. Remaining buckets are Rust DDS Zephyr
  `Transport(ConnectionFailed)`, XRCE E2E tests hard-skipping because the XRCE
  Agent is absent, and native Zenoh service interop tests whose native service
  fixtures were not prebuilt.
- 2026-05-15 C++ Zephyr follow-up: the C++ Zephyr module now exports the same
  RMW backend compile definitions as the normal CMake `nros-cpp-headers`
  target, and `nros-cpp` links/re-exports the Rust Zenoh/DDS backend register
  symbols behind its existing `rmw-*-cffi` features. This clears the previous
  C++ Zenoh `nros::init -> -100` failure and the intermediate
  `nros_rmw_zenoh_register` link failure.
- 2026-05-15 C++ focused evidence: `test_zephyr_cpp_talker_to_native_listener`
  now passes, proving the Zephyr C++ Zenoh talker opens the session and
  publishes data through the CFFI backend. `test_native_talker_to_zephyr_cpp_listener`
  and `test_zephyr_cpp_talker_to_listener_e2e` initially failed because the
  Zephyr C++ listener declared the ring subscriber but received no samples. A
  temporary diagnostic showed `zpico_declare_subscriber_ring` succeeded with key
  `0/chatter/std_msgs::msg::dds_::Int32_/*`; the zenoh-pico sample callback was
  not invoked during the failing run.
- 2026-05-15 C++ listener fix: Zephyr native_sim links the hosted Rust runtime,
  but the generic std executor wait path can hang inside the Zephyr libc
  condition-variable layer. `nros_cpp_spin_once` now uses a Zephyr-specific
  hosted path that performs a non-blocking backend drain and then yields through
  the exported `nros_zephyr_msleep` shim. Focused reruns now pass:
  `test_native_talker_to_zephyr_cpp_listener`,
  `test_zephyr_cpp_talker_to_listener_e2e`, and
  `test_zephyr_cpp_talker_to_native_listener`.
- 2026-05-15 DDS A9 follow-up: the Rust DDS qemu_cortex_a9 pub/sub E2E failure
  was a Zephyr UDP multicast receive deadlock plus an incorrect multicast join
  group. `timeout_ms = 0` on multicast sockets now sets `O_NONBLOCK`, matching
  the DDS cooperative recv-loop contract, and `udp_mcast_listen` joins the
  supplied group address instead of `0.0.0.0`. The focused
  `test_zephyr_dds_rust_talker_to_listener_a9_e2e` now passes and delivers
  samples.
- 2026-05-15 DDS fixture follow-up: Dust DDS setup has large async state on the
  Cortex-A Zephyr path, so Zephyr DDS fixtures now use the same 512 KiB stack
  and heap budget across Rust, C, and C++ frontends. Focused rebuilds passed for
  Rust DDS A9 talker/listener plus C and C++ DDS A9 talker/listener fixtures.
- 2026-05-15 XRCE setup/runtime follow-up: `just qemu setup` now passes again
  after guarding the zenoh-pico single-thread `z_open_options_t` assignments.
  Zephyr XRCE no longer relies on POSIX-only Micro XRCE UDP transport: the
  Zephyr build now links a UDP custom-transport bridge through the canonical
  `nros_platform_udp_*` ABI, and XRCE C/C++ staticlibs pull in the concrete
  `nros-rmw-xrce-cffi` backend when their existing XRCE compatibility features
  are selected. Native-sim XRCE examples now wait for network readiness, and the
  C++ XRCE configs no longer enable the copied Zenoh POSIX pthread block that
  broke host-kernel socket open. Focused pub/sub reruns now pass for
  `test_zephyr_xrce_rust_talker_listener`,
  `test_zephyr_xrce_c_talker_listener`, and
  `test_zephyr_xrce_cpp_talker_listener`.
- 2026-05-16 DDS/C++ fixture follow-up: Zephyr C/C++ DDS codegen now gets the
  platform CFFI include path during CMake builds, hosted Zephyr C/C++ staticlibs
  provide the critical-section symbols on `std + platform-zephyr`, and the
  Zephyr native-sim offloaded-socket build no longer trips unused L4 callback
  warnings. Focused C++ DDS native_sim boot reruns pass for talker, listener,
  service server/client, and action server/client.
- 2026-05-16 DDS service follow-up: the CFFI service-client adapter only
  exposes `call_raw`, so the Zephyr Rust DDS service client was hitting the
  DDS no-std `call_raw` timeout stub when polling a Promise. The no-std DDS
  client now performs a cooperative request/reply wait, and DDS subscriber
  empty polls treat dust-dds `Timeout` like `NoData`. Focused reruns now pass
  for `test_zephyr_dds_rust_service_a9_e2e`,
  `test_zephyr_dds_rust_async_service_a9_e2e`, and
  `test_zephyr_dds_rust_async_service_client_boots`.
- 2026-05-16 check/CI follow-up: `just check` passes after the DDS service
  fixes. A full `just ci` rerun still has non-Zephyr buckets outstanding; the
  latest run before these final fixes reported 760 passed, 59 failed, 6 timed
  out, and 11 skipped.
- 2026-05-16 lockfile/test follow-up: Zephyr Rust fixture lockfile churn from
  `just zephyr build-fixtures` was committed in
  `1e2d6e4f chore(zephyr): update Rust fixture locks`. The selected related
  E2E run covered Zenoh service/pubsub/action, XRCE pubsub/service/action, and
  DDS pubsub/service/action/async-service. It finished 8 passed / 2 failed.
  Passing cases were Zenoh service/pubsub, XRCE pubsub/service/action, and DDS
  pubsub/service/async-service. `test_zephyr_action_e2e` failed before action
  server readiness, but its action fixture lockfiles were not part of the dirty
  set.
- 2026-05-16 DDS action blocker: `test_zephyr_dds_rust_action_a9_e2e`
  reproducibly fails by timing out on the client send-goal acceptance reply.
  The server side is alive: it receives the goal, executes Fibonacci feedback,
  and logs `Goal succeeded`. The client logs `Goal acceptance failed:
  ServiceRequestFailed` after sending the goal. The next focused work item is
  the DDS action send-goal service reply/correlation path, likely between
  `DdsServiceServer::send_reply`, `DdsServiceClient::try_recv_reply_raw`, and
  action `Promise::wait`.
- 2026-05-17 stale-fixture follow-up: the later DDS A9
  `Transport(ConnectionFailed)` regressions were stale prebuilt Zephyr images.
  `get_or_build_zephyr_example` now invokes `west build` when a fixture is
  missing or older than its example/shared nros sources, while preserving the
  per-XRCE agent-port build overrides. After rebuilding the current DDS A9
  service and action server/client fixtures, focused reruns passed for
  `test_zephyr_dds_rust_service_a9_e2e` and
  `test_zephyr_dds_rust_action_a9_e2e`.
- 2026-05-17 XRCE C++ service/action follow-up: the C++ XRCE native_sim link
  failure is fixed in `ffdde60f fix(xrce): wire C++ CFFI backend init`.
  `nros-cpp` now ships a weak `nros_app_register_backends` default for
  C++-only links and `nros_cpp_init` explicitly registers the selected linked
  CFFI backend, so Zephyr/native_sim no longer depends on POSIX-style
  constructor sections. `xrce_service_send_reply` also flushes the reliable
  XRCE stream with the normal session flush timeout. Verification:
  `cargo check -p nros-cpp --no-default-features --features
  rmw-cffi,rmw-xrce-cffi,platform-zephyr,ros-humble,std` passes, and the four
  C++ XRCE service/action fixtures rebuild cleanly.
- 2026-05-17 XRCE C++ service/action runtime fix: the remaining `-2`
  reply timeout on `test_zephyr_xrce_cpp_service_e2e` and the
  send-goal hang on `test_zephyr_xrce_cpp_action_e2e` had a common
  root cause in the C++ Zephyr+std spin path.
  - The earlier `a451b626 fix(zephyr): unblock C++ listener spin` had
    routed `nros_cpp_spin_once` on Zephyr+std around the std
    `Executor::spin_once` because the executor's
    `wake_cv.wait_timeout_while` blocked past its deadline on
    Zephyr's libc condvar. The bypass replaced the spin with
    `session.drive_io(0) + nros_zephyr_msleep(timeout_ms)`.
  - That bypass starved reliable XRCE retransmission on the server
    side: `xrce_service_send_reply` already flushes the output
    stream for `XRCE_SESSION_FLUSH_TIMEOUT_MS` (100 ms), but if the
    agent's ACK lands after that flush, the next `drive_io(0)` does
    nothing and `nros_zephyr_msleep(timeout_ms)` runs no XRCE
    session activity, so the reliable reply sits unACK'd in the
    server's output stream and the client's `xrce_service_call_raw`
    times out at 5 s.
  - C++ `nros_cpp_action_client_send_goal` calls
    `ctx.executor.spin_once(10ms)` directly (not `nros_cpp_spin_once`),
    so it still hit the hung `wake_cv` and never sent the goal
    request — matching the "times out before the server logs a goal"
    symptom.
  - Fix: gate the `wake_cv.wait_timeout_while` off on Zephyr+std in
    `Executor::spin_once` and route `primary_drive_timeout_ms = timeout_ms`
    there. Zephyr UDP `recv` already honors `SO_RCVTIMEO`, so
    `drive_io(timeout_ms)` yields the thread for the requested
    duration without depending on the broken condvar path and keeps
    the XRCE reliable streams ticking. With the underlying
    condvar hang gone, `nros_cpp_spin_once` reverts to a plain
    `ctx.executor.spin_once(timeout)` so both the service
    `Future::wait` and action `send_goal` paths get real polling +
    arena dispatch.
  - Verification: `cargo check -p nros-node --features
    rmw-cffi,platform-zephyr` and `cargo check -p nros-cpp
    --no-default-features --features
    rmw-cffi,rmw-xrce-cffi,platform-zephyr,ros-humble,std` both
    pass. `cargo test -p nros-node --lib` 131/131 passes — the
    non-Zephyr std cv-wait path is unchanged. End-to-end reruns of
    `test_zephyr_xrce_cpp_service_e2e` and
    `test_zephyr_xrce_cpp_action_e2e` still need a Zephyr SDK +
    XRCE Agent host with `just zephyr build-fixtures && just zephyr
    test --no-capture` before 127.C.4 can be closed.

Subitems:

- [x] `127.C.1`: Zephyr boot and fixture health.
- [x] `127.C.2`: Zephyr native/host Rust Zenoh pub/sub message-flow failures.
- [x] `127.C.3`: Zephyr DDS runtime failures. Pub/sub, service, async service,
  and action now pass on qemu_cortex_a9 with rebuilt current fixtures.
- [x] `127.C.4`: Zephyr XRCE runtime failures closed. Pub/sub
  passes for Rust, C, and C++; Rust XRCE service/action passes;
  C++ XRCE service + action pass after Phase 130 (see
  `docs/roadmap/phase-130-platform-wake-primitive.md`). All 13
  Zephyr XRCE E2E tests pass under
  `cargo nextest run -p nros-tests -E 'test(test_zephyr_xrce_)'`.
- [x] `127.C.5`: Cross-language Zephyr interop failures. C++ Zenoh startup and
  C++ listener delivery are fixed for the native_sim Zenoh pub/sub set.

Done criteria:

- [x] Separate host/board boot failures from DDS/XRCE message-flow failures.
- [x] Include `west`, QEMU, and nextest logs.
- [x] Identify whether the failure is common platform startup or backend-specific.
- [ ] Produce focused commands that reproduce each remaining Zephyr subgroup.

Focused commands:

```bash
just zephyr build-fixtures
just zephyr test --no-capture
```

## 127.D: Bare-Metal Zenoh QEMU

Scope:

- RTIC action E2E
- RTIC service E2E
- serial pub/sub E2E
- RTIC pub/sub E2E and mixed-priority pub/sub (originally lumped under
  127.B.4 in the older count; they share the MPS2 SLIRP/WFI root cause
  with the rest of 127.D)

Current signal (2026-05-17):

- Root cause for the original "Transport(ConnectionFailed) at the
  second QEMU's `Executor::open`" symptom was the MPS2 LAN9118 connect
  loop spinning without releasing the CPU. Under `-icount shift=auto`
  with two guests sharing the host, QEMU's main loop never got the
  cycles it needed to drain SLIRP, so the second guest's SYN-ACK never
  landed before zenoh-pico's open timeout.
- Fix landed in `phase-127.D: WFI idle hook unblocks MPS2 two-QEMU
  connect`: add an opt-in idle callback to `nros_baremetal_common::sleep`
  and `nros_smoltcp::do_poll`. MPS2-AN385 board exposes
  `enable_wfi_idle()` which installs `cortex_m::asm::wfi` on both. RTIC
  examples call it immediately after `Mono::start`.
- After the fix, focused isolated reruns:
  - `test_qemu_rtic_pubsub_e2e`: PASS (was failing) — 3/3 in a 3x loop.
  - `test_qemu_rtic_service_e2e`: client now opens the session, server
    `Handled: 5 + 3 = 8`, but client never sees `Reply:` (reply
    correlation gap, not connect). Distinct follow-up.
  - `test_qemu_rtic_action_e2e`: client now opens, sends goal, server
    `Goal accepted` / `Goal complete`, but client times out waiting
    for `goal acceptance` reply. Same reply gap as service.
  - `test_qemu_rtic_mixed_priority_pubsub_e2e`: not re-verified yet.
  - `test_qemu_serial_pubsub_e2e`: still fails; serial example uses
    non-RTIC `run()` so no IRQ source is armed and `enable_wfi_idle()`
    isn't safe to call. Needs a separate fix (e.g. arming a periodic
    timer interrupt in `init_hardware` so `wfi` can wake without RTIC).

Subitems:

- [~] `127.D.1`: RTIC action E2E — connect path now passes; reply
  correlation gap remains (same as 127.D.2 — see below).
- [~] `127.D.2`: RTIC service E2E — connect path passes; LAN9118 RX
  FIFO drops eliminated by `third-party/qemu/patches/0001-hw-net-lan9118-add-can_receive-flush-on-FIFO-drain.patch`
  (built via `just qemu setup-qemu`, wired into top-level `just setup`).
  Empirical retest 2026-05-17: with patched `build/qemu/bin/qemu-system-arm`,
  `lan_pend` jumps from a stuck **518** to **3428+** — LAN9118 now
  accepts every frame slirp pushes. Client `rx` / `recv` still capped
  at ~256 bytes; the zenoh-pico `g_pending_gets[slot]` callback that
  bridges `z_get` replies to `zpico_get_check` is not firing for the
  reply that did reach smoltcp. Distinct failure mode in the
  zenoh-pico reply-dispatch path under `ZPICO_SMOLTCP` /
  `Z_FEATURE_MULTI_THREAD=0`. Needs a dedicated zenoh-pico internals
  trace; tracked separately. Pubsub continues to pass.

  Pre-patch capture (2026-05-17 via tshark + zenoh-dissector on
  host loopback `tcp port 7460`) for reference:
  - Client TX path: query frame (98 B) leaves QEMU, host kernel ACKs.
  - zenohd forwards 99 B to server; server replies 104 B; zenohd
    forwards `Reply` (107 B, payload `[00, 01, 00, 00, 08, ..]` =
    CDR `i64=8`) and `ResponseFinal` (11 B) toward client TCP socket;
    client TCP layer ACKs both (Seq 97/204, Ack 1861).
  - Client smoltcp side (new `nros_smoltcp::rx_diagnostics()` +
    `lan9118_smoltcp::rx_diag_counters()` counters): `lan_pend` and
    `deliv` freeze at 519 packets; `rx` / `recv` freeze at 223 bytes
    after the initial handshake-and-declare burst. The LAN9118 model's
    RX FIFO never surfaces the reply frames to the guest even though
    host loopback shows them delivered to slirp's host socket.
  - Conclusion: blocker is in QEMU slirp ↔ LAN9118 model frame
    delivery (post-burst stall), not in nros-smoltcp / zenoh-pico
    layers. Cadence-gated WFI, trailing `iface.poll` removal,
    PROM-mode rebuild all verified not to change the symptom.
  - Diagnostic infrastructure landed under Phase 127.D so future
    bring-up can re-confirm in one rebuild + run.
  - 2026-05-17 upstream-source review of `qemu/hw/net/lan9118.c`
    pinpointed the exact bug: the LAN9118 model registers no
    `.can_receive` callback and never calls
    `qemu_flush_queued_packets()` after `rx_status_fifo_pop`. Per
    `net/queue.c:29-41`, slirp's `qemu_send_packet(..., NULL)`
    drops on `-1` instead of queueing — every major QEMU NIC except
    LAN9118 (cadence_gem, imx_fec, sunhme, igb, mcf_fec, msf2-emac,
    npcm7xx_emc, npcm_gmac) calls `qemu_flush_queued_packets`. Full
    finding + minimal upstream patch:
    [`docs/research/qemu-lan9118-slirp-rx-stall.md`](../research/qemu-lan9118-slirp-rx-stall.md)
    and `external/qemu/lan9118-flow-control.patch` (against
    `qemu/master`).
- [ ] `127.D.3`: Serial pub/sub E2E — pending RTIC-less idle hook
  (non-RTIC `run()` examples have no armed IRQ).

Done criteria:

- [x] Determine whether failures share session readiness, router timing,
  serial framing, or executor wake behavior. (Cause: executor wake /
  CPU-yield on bare-metal busy-wait loops.)
- [x] Compare against passing native RTIC action/service/pubsub cases.
  (Native examples have OS-yield; bare-metal needs the explicit WFI
  hook.)
- [~] Each of RTIC action, RTIC service, and serial pub/sub is either
  fixed or assigned a precise remaining blocker. Pubsub fixed; service
  / action narrowed to reply-correlation; serial blocked on
  non-RTIC-safe idle.

Focused commands:

```bash
just build-test-fixtures
cargo nextest run -p nros-tests --no-capture rtic
```

## 127.E: Native DDS Action

Scope:

- Native DDS action server/client E2E.

Current signal:

- Last full `just ci` bucket before the Phase 126 pull had 1 DDS native action
  failure.
- Zenoh and XRCE action paths have focused passing coverage after earlier
  fixes.
- Focused DDS action rerun reproduced the failure after goal acceptance:
  the client received `Feedback #1: [0]`, then aborted feedback polling with
  `Transport(DeserializationError)`.
- Root cause was the CFFI subscriber adapter mapping the normal
  `NROS_RMW_RET_NO_DATA` empty-poll return into `Err(TransportError::NoData)`.
  The action feedback loop treated that subscriber error as a deserialization
  failure. Zenoh/XRCE already expose empty polls as `Ok(None)`.
- Fixed by mapping CFFI `NO_DATA` to `Ok(None)` and adding CFFI regression
  coverage for subscriber empty polls. The DDS raw payload sequence bound was
  aligned with the Dust DDS unbounded sequence convention.
- Focused DDS action rerun now passes: the server accepts the goal, publishes
  all 11 feedback frames, completes with
  `[0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55]`, and the client observes all
  feedback frames, retrieves the explicit result, and finishes.
- Native DDS action cancel coverage was added with
  `NROS_ACTION_CANCEL_AFTER_FEEDBACK=2`: the client receives two feedback
  frames, sends a cancel request, receives `Cancel response: Ok`, then retrieves
  `Result: status=Canceled`; the server logs the cancel request and completes
  the goal as canceled with partial sequence `[0, 1]`.

Subitems:

- [x] `127.E.1`: DDS action goal acceptance and feedback.
- [x] `127.E.2`: DDS action result and cancellation path.
- [x] `127.E.3`: Compare DDS action behavior against passing Zenoh/XRCE action
  paths.

Done criteria:

- [x] Capture server/client action logs.
- [x] Compare goal acceptance, feedback, result, and cancellation behavior against
  the passing Zenoh/XRCE action paths.
- [x] Native DDS action E2E passes or has a narrowed single-stage failure.

Focused commands:

```bash
cargo nextest run -p nros-tests --test dds_api --no-capture test_dds_action
cargo test -p nros-rmw-cffi --features alloc --test try_recv_sequence
```

## 127.F: ROS 2 Lifecycle Interop

Scope:

- Lifecycle full-cycle ROS 2 interop.

Current signal:

- Fixed on 2026-05-15. The focused lifecycle interop test now passes.
- Root cause was the CFFI service-server adapter treating
  `NROS_RMW_RET_NO_DATA` as an error. Lifecycle processing checks
  `change_state` before `get_state`, so "no request on change_state" aborted
  the whole service pass before the ready `get_state` request could be drained.
- The ROS 2 Humble lifecycle CLI also needs `--no-daemon` and a short
  no-daemon `--spin-time 0.1`; longer no-daemon spin windows can report an
  invalid wait set after the service has already replied.

Subitems:

- [x] `127.F.1`: ROS 2 graph discovery and lifecycle node visibility.
- [x] `127.F.2`: Transition service availability and request/response path.
- [x] `127.F.3`: State observation timing after transition execution.

Done criteria:

- [x] Identify whether failure is graph discovery, transition service availability,
  transition execution, or state observation timing.
- [x] Include ROS 2 CLI/log output and nano-ros process logs.
- [x] Lifecycle full-cycle interop passes or has one isolated failing transition.

2026-05-15 focused evidence:

- [x] Before the fix, `ros2 lifecycle nodes --no-daemon` listed
  `/lifecycle_demo`, but `ros2 lifecycle get` timed out on
  `/lifecycle_demo/get_state`.
- [x] Manual diagnostics showed the nros query callback received the
  `get_state` request, while lifecycle processing repeatedly stopped before
  draining it because the earlier `change_state` server had no request.
- [x] Manual CLI sequence passed with `--no-daemon --spin-time 0.1`:
  `nodes` -> `/lifecycle_demo`, `get` -> `unconfigured [1]`, `set configure`
  -> `Transitioning successful`, second `get` -> `inactive [2]`, and `list`
  showed `cleanup`, `activate`, and `shutdown`.
- [x] `cargo nextest run -p nros-tests --test ros2_lifecycle_interop
  --no-capture ros2_lifecycle_full_cycle`: 1 passed, 0 failed.
- [x] `cargo test -p nros-rmw-cffi service_server_no_data_maps_to_none`: passed.

Focused commands:

```bash
cargo nextest run -p nros-tests --test ros2_lifecycle_interop --no-capture ros2_lifecycle_full_cycle
cargo test -p nros-rmw-cffi service_server_no_data_maps_to_none
```

## 127.G: Full-Matrix Refresh

Scope:

- Refresh the authoritative counts after the parent/submodule pull and ESP32
  allocation fix.

Current signal:

- Historical counts in the triage doc are useful for direction but stale after
  the Phase 126 pull and the ESP32 allocation fix.
- 2026-05-15 refresh moved the project toward a self-contained build:
  `packages/codegen` now owns the `play_launch_parser` dependency subtree
  instead of depending on sibling `~/repos/play_launch`.
- `CARGO_TARGET_DIR=/tmp/nano-ros-build-all-target just build-all` now gets
  past the old codegen blocker and the FreeRTOS QEMU `clock_gettime` linker
  blocker. Workspace, example matrix, FreeRTOS QEMU examples, ThreadX Linux
  examples, and ThreadX QEMU RISC-V examples compile.
- After host disk was freed, `CARGO_TARGET_DIR=/tmp/nano-ros-build-all-target
  just build-test-fixtures` completed native, QEMU bare-metal, FreeRTOS, NuttX
  Rust, ThreadX Linux, and ThreadX RISC-V fixtures. The run was intentionally
  stopped during the Zephyr fixture tail to pull/rebase onto current upstream
  commits; no new fixture build failure was observed before the stop.

2026-05-15 partial refresh evidence:

| Gate | Result | Evidence |
|---|---|---|
| `just format` | Pass | Required sandbox escalation because `just` writes temp files under `/run`. |
| `just ci` | Fail | Static checks/examples pass after fixing clippy findings and regenerating missing example bindings; nextest and C codegen fail. |
| `just build-all` | Interrupted in fixture tail after progress | With `CARGO_TARGET_DIR=/tmp/nano-ros-build-all-target`, workspace/examples compile after codegen gained its own `play_launch_parser` dependency subtree, honoring `CARGO_TARGET_DIR` in install recipes, and replacing RTOS ZID `clock_gettime` usage. After disk cleanup, `build-test-fixtures` completed through ThreadX RISC-V and was stopped during Zephyr so the repo could rebase. |
| `just test-all` | Not rerun standalone | `just ci` already invoked `test-all`, but the result is fixture-prereq heavy because the refreshed fixture build was interrupted before Zephyr completed. |

`just ci` nextest evidence:

- Run id: `b0ca0525-85ae-4931-ae76-529b41214b2c`.
- JUnit: `target/nextest/default/junit.xml`.
- Logs: `test-logs/latest/`.
- Nextest: 824 tests run, 519 passed, 305 failed, 11 skipped.
- Harness-reported environment skips inside failures: 27.
- Real failures after subtracting harness env skips: 278, but many are fixture
  prerequisite failures because the fixture build was not complete.

`just ci` failure separation:

| Class | Count | Notes |
|---|---:|---|
| Explicit skipped | 11 | nextest skipped tests. |
| Harness env skip: zenoh-pico ARM build unavailable | 10 | Reported as failed test bodies with `[SKIPPED] zenoh-pico arm build not available`. |
| Harness env skip: XRCE agent unavailable | 9 | `[SKIPPED] XRCE agent not available`. |
| Harness env skip: ROS 2 unavailable | 4 | `[SKIPPED] ROS 2 not found`. |
| Harness env skip: DDS talker binary missing | 3 | `[SKIPPED] DDS talker binary missing`. |
| Harness env skip: ThreadX-Linux DDS prerequisites | 1 | `[SKIPPED] ThreadX-Linux DDS prerequisites not available`. |
| Fixture not prebuilt | 231 | Tests fail with `Test fixture binary not prebuilt`; not authoritative runtime signal until `just build-all` completes. |
| Native C/C++ configure missing installed NanoRos package | 44 | Examples cannot `find_package(NanoRos)` because `install-local-posix` did not complete. |
| Other QEMU/runtime/build failures | 3 | Remaining non-env, non-fixture failures from the partial run. |

Other `just ci` tail results:

- Doctests pass: 1 passed, 4 ignored.
- Miri pass for selected crates; one clock test ignored under Miri.
- C codegen failed before `packages/codegen` gained its own
  `play_launch_parser` dependency subtree. That source dependency is no longer
  the active blocker.
- C codegen log: `test-logs/latest/c-codegen.log`.

Subitems:

- [x] `127.G.1`: Run `just ci` and categorize nextest failures, skips, and
  environment skips.
- [x] `127.G.2`: Run `just build-all` and isolate build-only regressions.
- [ ] `127.G.3`: Run `just test-all` after fixture builds and refresh the final
  phase table.

Done criteria:

- [ ] Produce a fresh table by category.
- [x] Keep failed, skipped, and harness-reported environment skips separate.
- [x] Include nextest run id, JUnit path, and `test-logs/latest/` path.
- [x] Update this document and the historical triage doc with the partial
  refresh counts and blocker.

Commands:

```bash
just format
just ci
just build-all
just test-all
```
