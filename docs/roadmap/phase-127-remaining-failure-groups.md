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

Current signal:

- ESP32 listener/talker build and boot checks pass.
- Listener reaches `Subscriber declared` and waits.
- No messages are delivered across ESP32-to-ESP32, ESP32-to-native, or
  native-to-ESP32 paths.
- Router tracing on 2026-05-15 confirms the ESP32 client completes the TCP
  Zenoh session handshake (`InitSyn`, `OpenSyn`, `OpenAck`) and is registered
  as a client face by `zenohd`.
- After `OpenAck`, `zenohd` receives no subscriber declaration, publisher data,
  or keepalive from the ESP32 client; the router closes the transport after the
  10 second lease expires.
- The active ESP32 Zenoh-pico build has `Z_FEATURE_BATCHING=0` and negotiates a
  1024 byte unicast batch, so the current silence is not explained by an
  unflushed Zenoh-pico network-message batch.

Subitems:

- [x] `127.A.1`: Router/session discovery. Capture `zenohd` logs and confirm ESP32
  clients establish sessions with the router.
- [ ] `127.A.2`: ESP32 publish path. Trace ESP32 talker from timer callback through
  `publish_raw` and smoltcp TX.
- [ ] `127.A.3`: ESP32 receive path. Trace native/ESP32 inbound data through
  smoltcp RX, zenoh-pico poll, subscriber ring, and executor dispatch.
- [ ] `127.A.4`: Harness timing. Confirm startup ordering and polling windows are
  long enough after the OOM fix removed the earlier early-exit failure.
- [ ] `127.A.5`: Post-open ESP32 outbound control path. Trace
  `z_declare_subscriber` and `zp_send_keep_alive` through
  `_z_transport_tx_flush_buffer`, `_z_link_send_wbuf`, `PlatformTcp::send`, and
  `SmoltcpBridge::poll_network` after a successful `z_open`.
- [ ] `127.A.6`: QEMU OpenETH TX evidence. Add an instrumented run or packet
  capture equivalent that proves whether post-open bytes are queued in smoltcp,
  handed to OpenETH, or lost before `zenohd` can read them.

Done criteria:

- [x] Determine whether the break is router discovery, TCP session open, publish
  path, receive path, or smoltcp polling cadence.
- [x] Include QEMU logs, `zenohd` logs, and one minimal focused fix or a narrowed
  failure cause.
- [ ] `just esp32 test --no-capture` either passes all ESP32 tests or reports a
  smaller, newly categorized failure with no allocation panic.

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
- [ ] Remaining blocker: identify why ESP32 post-open Zenoh control frames
  (`Declare subscriber`, `KeepAlive`) do not reach `zenohd` even though the
  same TCP connection successfully carries the Zenoh open handshake.

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

Subitems:

- [ ] `127.B.1`: FreeRTOS E2E triage.
- [ ] `127.B.2`: NuttX E2E triage.
- [ ] `127.B.3`: ThreadX Linux/RISC-V E2E triage.
- [ ] `127.B.4`: Bare-metal DDS runtime triage.
- [ ] `127.B.5`: Shared platform DDS runtime triage.

Done criteria:

- [ ] Split failures by platform first.
- [ ] For each platform, label failures as fixture build, boot, network setup,
  router/discovery, or protocol handshake.
- [ ] Preserve exact QEMU and test harness logs.
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

Subitems:

- [x] `127.C.1`: Zephyr boot and fixture health.
- [x] `127.C.2`: Zephyr native/host Rust Zenoh pub/sub message-flow failures.
- [ ] `127.C.3`: Zephyr DDS runtime failures.
- [ ] `127.C.4`: Zephyr XRCE runtime failures.
- [x] `127.C.5`: Cross-language Zephyr interop failures. C++ Zenoh startup and
  C++ listener delivery are fixed for the native_sim Zenoh pub/sub set.

Done criteria:

- [ ] Separate host/board boot failures from DDS/XRCE message-flow failures.
- [ ] Include `west`, QEMU, and nextest logs.
- [ ] Identify whether the failure is common platform startup or backend-specific.
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

Current signal:

- Last full `just ci` bucket before the Phase 126 pull had 3 failures.
- Native RTIC pattern fixtures were repaired earlier, so these should be
  treated as bare-metal/QEMU-specific until proven otherwise.

Subitems:

- [ ] `127.D.1`: RTIC action E2E.
- [ ] `127.D.2`: RTIC service E2E.
- [ ] `127.D.3`: Serial pub/sub E2E.

Done criteria:

- [ ] Determine whether failures share session readiness, router timing, serial
  framing, or executor wake behavior.
- [ ] Compare against passing native RTIC action/service/pubsub cases.
- [ ] Each of RTIC action, RTIC service, and serial pub/sub is either fixed or
  assigned a precise remaining blocker.

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

Subitems:

- [ ] `127.G.1`: Run `just ci` and categorize nextest failures, skips, and
  environment skips.
- [ ] `127.G.2`: Run `just build-all` and isolate build-only regressions.
- [ ] `127.G.3`: Run `just test-all` after fixture builds and refresh the final
  phase table.

Done criteria:

- [ ] Produce a fresh table by category.
- [ ] Keep failed, skipped, and harness-reported environment skips separate.
- [ ] Include nextest run id, JUnit path, and `test-logs/latest/` path.
- [ ] Update this document and the historical triage doc with the refreshed
  authoritative counts.

Commands:

```bash
just format
just ci
just build-all
just test-all
```
