# Phase 521 — The real-time promise and the one wait

**Status (2026-11-01). Approved; W0, W1 and W3 done; W2's THREAD half and W4's POSIX/NuttX half done; the rest of W2 and W4, and W5–W8, open.** Implements
[RFC-0113](../design/0113-rt-unified-wait.md). Amends
[RFC-0106](../design/0106-executor-park.md) D5 and withdraws
[RFC-0112](../design/0112-framework-support-tiers.md)'s Tier 0.

---

## Why

RFC-0113 makes the real-time promise a precondition of support. Two kinds of
work follow from that: deleting the surfaces that let a foreign loop schedule
the executor, and making the runtime's own wait meet RFC-0113 D2 on every
platform and RMW. Today a zenoh tier wakes every 3.3 s for nothing, and an XRCE
or uORB tier blocks inside `recv` with 1 ms granularity and 10 ms guard slices.

## Order

```text
W0 (decisions, done)
 ├─ W1 (retire host-driven surface, incl. setup!)
 ├─ W2 (zenoh truthful deadline)                       independent, small
 ├─ W3 (readiness slot + POSIX park) ─ W4 (XRCE) ─ W6 (retire fallback)
 ├─ W5 (bare-metal port obligation)                    parallel with W3/W4
 ├─ W8 (PX4 Tier 1 mapping)                            after W3
 └─ W7 (acceptance measurements)                       starts with W2, closes the phase
```

## Work items

### W0 — Settle the two open questions — DONE (2026-10-31)

- PX4: Tier 1 mapping (RFC-0113 D9), built in W8.
- `nros::setup!`: retired (RFC-0113 D10), in W1. Phase-520's owning session
  was told on 2026-10-31; it retires its own documents, and W1 deletes the
  code only after that session has stood down. This phase does not edit
  phase-520's documents.

### W1 — Retire the host-driven surface — DONE

Landed as one change. Two things it found that the list below did not name:
the nros-macros `'board` label block lost its only `break`, and `ci_lane`'s
documented cell counts moved (tier 1 26 -> 25, tier 3 255 -> 254) with the
`HostLoop` cell. A follow-up retired the per-entity async receivers too
(maintainer ruling 2026-11-01): `Subscription::recv().await`,
`RawSubscription::borrow().await`, `EmbeddedRawPublisher::loan().await` and
`LoanFuture`, `Promise`'s `Future` impl and `poll_until_ready`, both feedback
streams' `recv()`, and the `stream` feature's `Stream` impls. Each completed
only while something else spun the executor, and no in-tree code awaited
them. The sync forms (`try_*`, `wait_next`, `wait`, `loan_with_timeout`) and
the RMW `register_waker` slot, which the executor and the C/C++ arrival
callbacks use, stay.


Delete RFC-0113 D7's list in one change:

- Rust (`nros-node` `executor/spin.rs`): `spin_async`, `spin_async_with_sleep`,
  `spin_async_inner`, `arm_async_waker`, `WakeCtx::task_waker`,
  `set_ready_notifier`, `ready_notifier_supported`, `ready_fd`, and the
  `ready_fd`/notifier fields of `WakeCtx`. `next_deadline_us` becomes
  `pub(crate)`.
- C (`nros-c/src/executor.rs`, regenerate `nros_generated.h`):
  `rclc_executor_set_ready_notifier`, `rclc_executor_get_next_deadline_ns`.
- C++ (`nros-cpp/src/lib.rs`, `rclcpp/executor.hpp`, regenerate
  `nros_cpp_ffi.h`): `set_ready_notifier`, `next_deadline_us`; delete
  `tests/compile/ready_notifier_runtime.cpp` and the members it covered in
  `executor_node_members_runtime.cpp`.
- Examples and tests: `examples/linux/rust/{service,action}-client-async`,
  their `fixtures.toml` rows and `matrix::CELLS` cells,
  `linux_async_roundtrip_e2e.rs`, the `spin_async` tests in
  `entity_wake.rs` and `executor/tests.rs`.
- Docs: `book/src/user-guide/embassy-integration.md` (replaced by a short
  "not supported, and why" note pointing at RFC-0113), the `spin_async` text
  in `rtos-cooperation`, `ros2-comparison`, `no-std`, `status-events`,
  `dispatch-strategy`, `rust-api`, `c-api`, `nros::guide::services`,
  `std-alloc-requirements`, RFC-0001, RFC-0089.
- Ledger: the `api-parity-ledger` rows for the deleted symbols are reclassified
  (rclrs's `spin_async`-family rows become a stated non-adoption naming
  RFC-0113).
- Promise's `Future` impl: retired in the follow-up with the other async
  receivers (no owned path awaits it).
- `nros::setup!` (`nros-macros`), its `nros_setup` / `nros_deploy` emit,
  tests and book text (RFC-0113 D10), after phase-520's owner stands down.
- Phase-520 W3's host-loop deferred surface (PR #2896), unless W1 finds an
  owned-runtime consumer: `Executor::callback_mark` / `set_deferred_since`
  and `CallbackMark` (`nros-node` `executor/deferred.rs`, `types.rs`, and the
  `nros-node` / `nros` re-exports); `nros::node_runtime::{callback_mark,
  defer_registered_since, run_deferred, has_deferred, set_defer_notifier}`
  and their `__macro` re-exports; `deferred_component_idents`,
  `wrap_deferred_register_calls` and the `NROS_DEFERRED_COMPONENTS` /
  `nros_run_deferred` / `nros_has_deferred` / `nros_set_defer_notifier` emit
  in `main_macro.rs`; `examples/linux/rust/listener-host-loop` and its
  `dispatch = "deferred"` row; `HOST_LOOP_DEFERRED_DRAIN_MARKER` and the
  HostLoop cell in `linux_example_pubsub_e2e.rs`; their 8 ledger rows; a
  lowered unsafe-census baseline. The issue-2146 queue on `Executor`
  (`set_deferred`, `run_deferred`, `set_defer_notifier`) STAYS: RTIC and
  `[[component]] dispatch` under `nros::main!` reach it.
- Tests that use a deleted symbol: `nros-c/tests/run/ready_notifier.c`, the
  `ready_fd` cases in `signal_fd_wake.rs` (its `signal_fd` cases stay).
- Keep `nros_smoltcp::next_deadline_us`: a different symbol, the bare-metal
  wake source the mps2-an385 and esp32-qemu entries register.
- RFC-0112: mark Tier 0 withdrawn and add the fifth Tier 1 criterion
  (RFC-0113 D8).
- `changelog.d/<id>.breaking.md`.

**Acceptance:** `git grep -w` for each deleted name returns only archived
docs, changelog fragments and RFC-0113; `just ci gate` green;
`api-parity` and `rmw-api-parity` green.

### W2 — zenoh reports real deadlines — THREAD mode DONE

- THREAD mode (`ZPICO_DRIVE_THREAD`): `next_deadline_ms` returns `None`,
  through the new `zpico_drives_on_thread()`. Tested by
  `entity_wake::an_idle_zenoh_session_does_not_bound_the_park` (an idle
  4 s `spin_once` is bounded by the caller's budget and waits it out;
  mutation-checked: the old constant fails it at 3,333,000 us).
- CALLER mode: keeps one keep-alive interval. zenoh-pico's real next wake-up
  there is the idle read's 1 ms poll, so reporting it would poll the link at
  1 kHz (mps2-an385's issue-2022 tickless idle would regress). It moves to W5,
  where the read becomes RX-notified and the executor's next wake-up is then
  the honest answer (RFC-0113 D6).

**Acceptance (remaining):** an idle zenoh tier on one RTOS lane wakes zero
times in 30 s with no timers; covered by W7.

### W3 — The readiness slot and the POSIX park — DONE

How it was built:

- The vtable slot is `readiness_source(session, *kind, *handle)`, appended at
  the end, and `Session::readiness_source() -> ReadinessSource { None, Notify,
  Waitable(i32) }`. A NULL slot keeps the old inference from
  `set_wake_callback`. The Rust adapter installs the slot for every Rust
  backend, because it also installs `set_wake_callback` for every one.
- The park is a new required platform symbol,
  `nros_platform_wake_park_handles_us`. POSIX/NuttX implement it as `ppoll`
  over the handles plus a self-pipe on the wake object; the pipe is made on
  the first handle park, so no other executor pays a descriptor for it. Every
  other port returns `-1`, and the executor then keeps its old wait. The
  executor asks its sessions for Waitable handles on every spin, parks on them
  before any port-installed park, and drains the sessions non-blockingly
  afterwards.
- Tests: `nros-tests/tests/readiness_park.rs`. A stub backend with no wake
  callback is woken within ~100 ms of a socket write and is never asked for a
  blocking `drive_io`. Disabling the handle park makes it fail at 3 s. A
  negative control checks that the receive-blocking wait is still what a
  session with no readiness form gets. Port-level cases are in
  `c_port_posix_wake.rs`.
- `rx_notify` on the custom-transport ops is moved to W4. It is a field on a
  versioned struct mirrored in four places, and XRCE serial is its first
  consumer, so it lands with that consumer.
- The NuttX rv-virt images do not link the park: they have no `pipe` and no
  `__wrap_poll` (issue 2198, first seen in the 2026-11-01 nightly `nuttx`
  job). No merge-gating lane builds NuttX, so W3's checks did not catch it.


- `nros-rmw` C header: add `readiness_source` to the vtable (Notify or
  Waitable + handle); run `scripts/gen-abi-bindings.sh`; Rust trait default
  is Notify for backends that already deliver a wake callback.
- `nros-rmw-cffi`: forward the slot.
- Executor: a tier collects its sessions' Waitable handles. On POSIX and
  NuttX the park becomes `ppoll` over the tier's `eventfd` plus those handles,
  with the D1 deadline as timeout; other ports keep their semaphore park.
- Custom-transport ops gain `rx_notify`.

**Acceptance:** `check-abi-bindings`, `check-rmw-abi-shape` green; a unit
test parks a POSIX tier on a socket pair and measures wake on write with no
wake callback installed.

### W4 — XRCE delivers readiness on every transport — POSIX/NuttX half DONE

POSIX/NuttX half, landed with W3:

- `nros_platform_udp_readiness_handle(sock)` is a new required net symbol.
  The POSIX net provider returns the socket's fd; lwIP, NetX, Zephyr and Rust
  stacks return `-1`.
- XRCE records the handle at session open (UDP through the platform net ABI,
  or the POSIX serial fd) and reports it through `readiness_source` as
  Waitable. A board's custom transport reports None until `rx_notify` lands.
- Measured with `xrce_guard_wake` (renamed
  `guard_trigger_wakes_an_xrce_executor_parked_on_its_socket`): trigger to
  dispatch went from ~5 ms through the 10 ms slice to ~90 us on a Linux host.
  The bound is now 3 ms. A mutation that makes the session report no handle
  fails it at 5.4 ms.

Remaining: the RTOS transports and `rx_notify` below, and the "zero receive
calls between deadlines" measurement (W7).

- Custom-transport ops gain `rx_notify` (moved here from W3): the board's
  UART interrupt calls the notify the session installed.

- POSIX/NuttX UDP: Waitable over `udp_bridge.fd`.
- FreeRTOS + lwIP, ThreadX + NetX, Zephyr zsock: Notify from the stack's
  receive callback.
- Serial / custom transport: Notify through `rx_notify`; wire it on every
  in-tree board that has an XRCE serial cell.

**Acceptance:** `xrce_guard_wake` measures trigger→dispatch under 2 port
granularities (today ~5 ms through the slices); an idle XRCE tier makes zero
receive calls between deadlines.

### W5 — Bare metal: RX interrupt and one-shot timer as a port obligation

zenoh's CALLER-mode read moves from its 1 ms idle poll to the RX Notify, and
its `next_deadline_ms` then reports zenoh-pico's next executor wake-up instead
of the keep-alive interval (moved here from W2; RFC-0113 D6).

Generalise mps2-an385's pattern (issue 2022) into the board-entry contract:
a bare-metal board supplies an RX-interrupt Notify and a one-shot deadline
timer, or its sessions are refused (RFC-0113 D5). Port it to every in-tree
bare-metal board that has a network or serial cell.

**Acceptance:** each such board's QEMU idle run is under 2 % host CPU and
delivers its pair.

### W6 — Retire the receive-blocking fallback

Once W4 and W5 cover every in-tree combination: delete the receive-blocking
wait, the guard slices and `POLL_ONLY_WAKE_SLICE_MS`; add the open-time
refusal for a combination with no readiness form; amend RFC-0106 D5.

**Acceptance:** RFC-0113's coverage table has no "not yet" row; a test opens a
session through a backend stub with no readiness form and asserts the refusal.

### W7 — Acceptance measurements for the promise

Make RFC-0113 D2 a measured property, not a reading:

- idle wake count = 0 beyond deadlines (per RMW, Linux + one RTOS lane);
- arrival→dispatch and deadline→dispatch latency against the port's
  granularity;
- a low-priority tier's arrival does not wake a high-priority tier on an
  attributing backend; the fan-out cost on a non-attributing one, measured and
  written into RFC-0113 (its third open question).

**Acceptance:** each measurement is a test in a lane that runs (not an
`eprintln!`), with numbers recorded in RFC-0113's coverage section.

### W8 — PX4 as a Tier 1 framework (RFC-0113 D9)

- Add `px4` to `nros_orchestration_ir::FRAMEWORKS` and an `nros::main!` /
  C++ entry branch that emits one `ScheduledWorkItem` per tier, bound to a
  work queue chosen from the tier's priority (`px4::wq_configurations`).
- A tier's wait is its work item's schedule: uORB arrivals through
  `SubscriptionCallback`, deadlines through `ScheduleDelayed`; nothing polls
  `orb_check`.
- Replace the hand-written modules in `examples/px4/cpp/*` with the generated
  entry; refuse the uORB polling build as a deployment (D4, D5).
- Admission (RFC-0112 D3 plus the fifth criterion): PX4 SITL as the in-tree
  board, a delivering CI cell, a named owner, and W7's measurements run
  under the work-queue scheduler.

**Acceptance:** the PX4 SITL cell delivers through the generated entry; W7's
idle-wake and latency tests pass on it; `git grep ScheduledWorkItem examples/`
finds only generated code.
