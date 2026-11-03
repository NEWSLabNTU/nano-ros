# Phase 521 — The real-time promise and the one wait

**Status (2026-11-01). Approved; W0 and W1 done; W3 done, including W3.b (issue 2198, the NuttX rv-virt link); W2's THREAD half and W4's POSIX/NuttX, lwIP, NetX and Zephyr UDP parts done; the rest of W2 and W4, and W5–W8, open. Revised the same day against RFC-0115 (phase-524): the boards the book names are Zephyr boards on a zenoh serial link, so Zephyr moves to the front of W4 and the polled UART read joins W6.** Implements
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

### W3.b — The park links on NuttX rv-virt (issue 2198) — DONE (#2977)

Fixed by `CONFIG_PIPES=y` in the rv-virt defconfig, so the park's wake pipe
links and NuttX keeps the Waitable form, and by dropping the image's stale
`-Wl,--wrap=poll`. The three `just nuttx build-riscv-*` recipes build, and the
rv-virt C talker boots in QEMU and publishes to `rmw_zenohd`. The third
acceptance point below is not met: no merge-gating lane compiles NuttX riscv,
so the nightly stays the only witness.

The plan as written:


W3 shipped a regression: four nightly `nuttx-riscv` rows fail to link. Fix it
before new W4–W6 work lands, since each later item adds to the same park. Of
the three options issue 2198 lists, prefer one that keeps NuttX on the
Waitable form: W6 refuses a session with no readiness form, so leaving the
park off NuttX only moves the failure. Wake the park through a primitive the
rv-virt configuration has (`eventfd`, or `CONFIG_PIPES`), and wait in a way
the image's `--wrap=poll` resolves, or stop wrapping `poll` there.

**Acceptance:** the four rows build in the nightly `nuttx` job; a NuttX riscv
cell runs the park; a merge-gating lane compiles the park for NuttX riscv, so
the next park change cannot repeat this.


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

### W4 — XRCE delivers readiness on every transport — POSIX/NuttX, lwIP and NetX DONE

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

FreeRTOS + lwIP and ThreadX + NetX (maintainer ruling 2026-11-01: patch our
forks rather than reach stack internals or add a watcher task per session):

- The lwIP fork adds `lwip_socket_set_rx_notify` behind `LWIP_SOCKET_RX_NOTIFY`
  (on in the shared board `lwipopts.h`). It is called from the socket layer's
  netconn event callback on every receive or error event, in the tcpip thread.
  The setter goes through `tcpip_api_call`, so after a removal returns no call
  of the old callback is still in flight.
- The netxduo fork adds `nx_bsd_socket_set_rx_notify`, called from the IP
  thread holding the BSD mutex whenever a UDP packet or TCP data is queued on
  the socket. The setter takes the same mutex. The threadx-linux NSOS shim is
  host sockets, not NetX, and answers `-1`.
- `nros_platform_udp_set_rx_notify(sock, notify, ctx)` is a new required net
  symbol. FreeRTOS and ThreadX call the fork hooks. POSIX, Zephyr and Rust
  stacks return `-1`.
- At open, XRCE probes the hook with a removal call. Where it exists, the
  session reports Notify, and `set_wake_callback` (now filled) hands the
  executor's wake callback straight to the stack.
- The runtime now decides whether a session wakes the executor from its
  stated `readiness_source`, when the backend fills that slot, not from
  whether `set_wake_callback` is present. Otherwise an XRCE session over a
  transport with no hook would park on a wake object nothing signals.
  `readiness_park` covers it with a stub that fills the slot and reports NONE;
  reading the slot instead fails it.
  That rule caught zenoh: it reaches the runtime through
  `RustBackendAdapter`, which always fills the slot, and the slot's trait
  default reads `supports_wake_callback()`, which the zenoh shim never
  overrode. So every zenoh session stated `None` and would have lost its
  park. #2990 (W6a) made the shim state its readiness itself (`Notify` in
  THREAD mode, `None` in CALLER mode, where an arrival does not end a park),
  and `rust_adapter`'s two wake tests pin the adapter seam both ways.
- Measured on QEMU, before and after. A C listener (`rclc_executor_spin_some`
  with 1 s timeouts) received from the Agent while `ros2 topic pub` sent at
  2 Hz. Latency is from publish to listener print:

  | Stack | Hook | p50 | p90 |
  |---|---|---|---|
  | FreeRTOS + lwIP (mps2-an385) | on, 5 runs | 2.2–4.0 ms | 5.1–8.5 ms |
  | FreeRTOS + lwIP | off: old receive-blocking path, 2 runs | 72–253 ms | 336–467 ms |
  | FreeRTOS + lwIP | on, notify call suppressed | 109 ms | 445 ms |
  | ThreadX + NetX (rv-virt) | on | 1.8 ms | 5.1 ms |
  | ThreadX + NetX | on, notify call suppressed | 421 ms | 463 ms |

  The last two lwIP runs are at the pinned fork tip, which also carries the
  fork's earlier lazy netconn-semaphore commit (`de76e778`), shipped here for
  the first time. These are manual runs. No RTOS XRCE fixture row exists, so making them a
  lane test is W7's.
- `rclc_executor_spin_period` is unaffected: rclc's fixed-rate loop runs
  `spin_some(0)` and then a plain platform sleep, so arrivals within a period
  wait for the period boundary by its own definition. Whether that sleep
  should park on the wake object is a W6 question.

Remaining, in order (revised 2026-11-01 against RFC-0115):

1. **Zephyr XRCE over UDP — DONE.** RFC-0115 D4 puts every board the book
   names on Zephyr (Nucleo-F767ZI in Renode, Pico W in Wokwi), so Zephyr is no
   longer one port among four. zsock has no per-socket callback, so Zephyr
   takes the Waitable form, the same as POSIX: the wake object carries a
   non-blocking ZVFS eventfd (made on the first handle park; `signal` and
   `signal_from_isr` bump it after the `k_sem_give`, and a non-blocking
   eventfd write is ISR-safe), `nros_platform_wake_park_handles_us` is a
   `zsock_poll` over it and the sessions' sockets, and
   `nros_platform_udp_readiness_handle` returns the zsock fd.
   `NROS_RMW_XRCE` implies `ZVFS` and `ZVFS_EVENTFD`; without them the
   handle is -1 and the tier keeps the old wait.
   The Notify form, Zephyr's `NET_SOCKET_SERVICE`, was not taken: it is
   EXPERIMENTAL on the 3.7 line, its define macro changes in 4.x, and its one
   dispatcher thread defaults to the LOWEST preemptible priority, so a busy
   tier would delay every session's wake behind it. The park runs at the
   tier's own priority with no thread on its behalf.
   Measured on `native_sim` (the C listener over the Agent, `ros2 topic pub`
   at 2 Hz, latency from publish to print, each arrival paired with the
   nearest publish line because a fast delivery can print before `ros2 topic
   pub` does):

   | Zephyr native_sim XRCE | p50 | p90 |
   |---|---|---|
   | fd park, 3 runs | 0.4–1.1 ms | 1.4–4.7 ms |
   | eventfd off: old receive-blocking path, 2 runs | 3.2–6.3 ms | 4.5–7.1 ms |

   The run also showed the idle tier waking every 10 ms. That is not the park:
   the entry runtime spins `spin_once(10 ms)` in a loop (W6, below).
2. **zenoh on Zephyr needs no new hook.** Zephyr runs zenoh in THREAD mode
   (W2), and with `CONFIG_UART_INTERRUPT_DRIVEN` the serial reader blocks on a
   `k_sem` that the UART ISR gives (issue 0852), so the twin boards' serial
   link already meets D2. Without that symbol, the reader falls back to
   `uart_poll_in` plus `k_yield()`, which is a busy wait; W6 retires that path.
3. **Custom-transport `rx_notify` lands with W5.** Its one in-tree consumer
   is bare metal: `mps2-an385-baremetal/rust/talker-xrce` runs XRCE over the
   board's CMSDK UART through the custom-transport ops, and its `read`
   busy-polls the UART until the timeout. The notify needs the UART's RX
   interrupt, which is W5's port obligation on that board, so the field, its
   four mirrors and the board's ISR land together there. (An earlier revision
   of this item said every in-tree serial cell was zenoh; `talker-xrce` was
   missed.)
4. The "zero receive calls between deadlines" measurement (W7).

- POSIX/NuttX UDP: Waitable over `udp_bridge.fd`.
- FreeRTOS + lwIP, ThreadX + NetX: Notify from the stack's receive callback
  (done). Zephyr zsock: Waitable or Notify, per item 1.
- Serial / custom transport: Notify through `rx_notify`, wired on
  mps2-an385 with W5 (item 3).

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
bare-metal board that has a network or serial cell: today
`mps2-an385-baremetal` (smoltcp and serial). Its network half exists: issue
2022 gave the LAN9118 a receive interrupt and a SysTick one-shot under the
smoltcp wait loop (idle QEMU 0.9–1.4 % host CPU, against 99.9 % polling). Its
serial half does not: the CMSDK UART's RX interrupt is a placeholder, so the
zenoh `serial-*` cells and XRCE `talker-xrce` busy-poll the UART. W5 adds that
interrupt and, through it, the custom-transport `rx_notify` (W4 item 3). `nros-board-esp32-qemu` is dormant
(issue 1525) and is refused rather than ported. RFC-0115 names no bare-metal
book board, so W5 has no twin-board work; a future bare-metal book board
inherits this obligation through the board-entry contract.

**Acceptance:** each such board's QEMU idle run is under 2 % host CPU and
delivers its pair.

**Serial half landed (2026-11-02).** mps2-an385's serial link gets the same
tickless idle as its network: UART0's RX interrupt (`UARTRX0`) plus the SysTick
one-shot under PRIMASK and `wfi`, installed into a new
`zpico_serial::set_wait_idle` seam. The zenoh serial read and
`zpico_spin_once`'s idle between CALLER-mode executor rounds wait on it instead
of `z_sleep_ms` polls, and the board's XRCE custom-transport `read` sleeps on it
instead of spinning. `test_qemu_serial_pubsub_e2e` and
`test_qemu_xrce_pubsub_e2e` deliver. The CPU acceptance is not met in a lane:
under the SDK's QEMU 11 an idle serial listener costs a host core with or
without the hook, against about 2 % under QEMU 9 either way (issue 2209).

**zenoh CALLER mode parks (2026-11-02).** mps2-an385's port gets a wake
object (`nros_platform_mps2_an385::wake`): a flag, parked on through the
board's tickless sleep, which ends at the deadline, on a signal, or when the
link has received data (LAN9118 or UART0 RX interrupt). The board states that
with `nros_rmw_zenoh::set_caller_rx_wakes_park`, and the zenoh session in
CALLER mode then states `Notify`, so the executor parks between spins instead
of draining the link from `drive_io` every 10 ms. Its `next_deadline_ms` is
zenoh-pico's own next due future, link reads excluded (fork commit
`zp_next_wake_ms`, through `zpico_next_wake_ms`), and `None` when nothing is
scheduled; smoltcp's timers already bound the park as a platform wake source
(phase-436 B2). ThreadX's CALLER mode runs a multi-thread zenoh-pico build
whose executor cannot be read without its lock, so it keeps the keep-alive
interval and its blind `z_sleep_ms` (W6).

Measured on the idle `qemu-bsp-listener` (zenoh over LAN9118 to `rmw_zenohd`,
no talker, 20 s, distro QEMU 9.0.2, counting SysTick one-shot arms with gdb):
41 arms with the park against 1,892 with the flag cleared (the old 10 ms
drain), and 12 against 671 host CPU ticks over the window (gdb's breakpoint
included in both). Under the SDK's QEMU 11 the same image arms 175,157 times:
`wfi` returns at once there on the network link too (issue 2209, which is not
about the serial chardev). `test_qemu_bsp_pubsub_e2e`,
`test_qemu_serial_pubsub_e2e`, `test_qemu_xrce_pubsub_e2e`,
`test_qemu_rtic_pubsub_e2e`, `test_qemu_rtic_service_e2e` and
`test_qemu_lan9118_driver` pass.

**XRCE custom transport reports arrivals (2026-11-02).** A board's XRCE
custom transport registers a receive notification
(`nros_rmw_xrce_set_custom_transport_rx_notify`, a separate setter so the ops
struct and its Rust mirror keep their layout). The backend hands it the
executor's wake callback through `set_wake_callback`, and the session states
`Notify`. mps2-an385's UART0 transport registers one, called from `UARTRX0`.
Measured on `qemu-talker-xrce` (serial to `MicroXRCEAgent`, publishing at
1 Hz, 20 s, QEMU 9.0.2): 42 SysTick arms and 20 host CPU ticks, against 1,832
and 703 with the registration suppressed (the receive-blocking 10 ms drain).
`test_qemu_xrce_pubsub_e2e` passes.

Still open: the board-entry refusal of a bare-metal board without an RX
interrupt and a one-shot timer (esp32-c3 is the in-tree one), which W6's
open-time `None` refusal delivers.

### W6 — Retire the receive-blocking fallback

Once W4 and W5 cover every in-tree combination: delete the receive-blocking
wait, the guard slices and `POLL_ONLY_WAKE_SLICE_MS`; add the open-time
refusal for a combination with no readiness form; amend RFC-0106 D5.

Added 2026-11-01 (RFC-0115): retire zenoh-pico's polled Zephyr UART read
(`uart_poll_in` plus `k_yield()` when `CONFIG_UART_INTERRUPT_DRIVEN` is unset).
A serial zenoh link on Zephyr then requires that symbol, and a build without
it is refused by name. The twin boards already set it (phase-524 W1's serial
Kconfig), so this costs the book boards nothing. This is a change on our
zenoh-pico fork's patch branch.

Also decide `rclc_executor_spin_period` (from W4): whether its inter-period
sleep parks on the wake object.

Added 2026-11-01 (found by W4's Zephyr run): the entry runtimes spin on a
fixed 10 ms cadence, so an idle tier wakes 100 times a second whatever its
park can do. One spelling replaces them: the entry spins with a budget bounded
only by the tier's next deadline. The sites: `nros::main!`
(`packages/api/nros/src/lib.rs`), `node_runtime.rs`,
`nros_rtos_run_components.c`, nros-cpp's `spin` loops and `driven_future.hpp`,
and the board entries of FreeRTOS, ThreadX, NuttX, mps2-an385 and esp32.
A gate then refuses a literal spin budget in an entry loop.

Surveyed 2026-11-01 and not yet started, because raising the budget alone
breaks four things. Each needs its own fix before the entries can stop
passing 10 ms:

1. **zenoh CALLER mode is not woken by an arrival.** On ThreadX the read is a
   blind `z_sleep_ms(timeout)` (`zpico_spin_once`), and the wake callback
   fires only from `drive_io`. A long budget would leave each arrival waiting
   for zenoh's next keep-alive deadline (~3.3 s). CALLER sessions keep a short
   slice until W5 gives them an RX notification. Bare-metal CALLER reads
   already return on a delivered sample.
2. **Component ticks run once per spin.** `ExecutorNodeRuntime::spin_once`
   runs every component's `tick` after the pass, and ticks poll service
   clients and drive action servers. A long park would slow those to the
   park's rate. Either ticks become deadlines the executor knows, or a runtime
   with tick-bearing components keeps a cadence.
3. **Termination does not end a park.** The POSIX termination guard's handler
   only sets a flag, and the hosted loops poll it between spins. The handler
   must also signal the executors' wake objects (the port's
   `nros_platform_wake_signal` is signal-safe). `Executor::cancel` has the
   same gap: it raises the wake flag but does not post the wake object, so a
   cancel from another thread waits out the park.
4. **The tiered loops sleep, not park.** nros-cpp's native tier loops run
   `spin_once(10)` and then `platform_sleep_us(period)`, so an arrival during
   the sleep waits for the period. They should park for the period instead.

Rule for the helper once those land: the long budget applies only when every
session of the executor states `Waitable`, or `Notify` with no caller-owed
deadline. Anything else keeps the 10 ms slice, which W6 then retires. The
budget itself must stay within what every port expresses: FreeRTOS's
`pdMS_TO_TICKS` wraps above ~71 minutes at 1 kHz.

**Acceptance:** RFC-0113's coverage table has no "not yet" row; a test opens a
session through a backend stub with no readiness form and asserts the refusal.

**W6a — the entry budget (2026-11-02).** Every entry runtime spun
`spin_once(10 ms)` forever, so an idle tier woke 100 times a second whatever its
park could do. Four things stood in the way of a longer budget, and each is now
fixed:

1. `Executor::cancel` raised a flag but did not post the wake object, so a
   cancel from another thread waited out the park. It posts now
   (`readiness_park::cancel_from_another_thread_ends_a_long_park`,
   mutation-checked: without the post the 5 s park runs to its end).
2. The POSIX termination guard only set a flag. Its handler now posts every
   live wake object, registered by `nros_platform_wake_init` in a
   function-pointer table in `termination.c`, so that file still references
   nothing in `platform.c` (issue 1776)
   (`nros-platform`'s `termination_wake` test).
3. zenoh stated `ReadinessSource::None` in every mode through
   `RustBackendAdapter`. It now states `Notify` in THREAD mode and `None` in
   CALLER mode, where an arrival does not end a park.
4. Component ticks run once per spin. `ExecutableNode::HAS_TICK` (default
   `true`, `false` from `declarative_component!`) says whether a tick works, and
   an executor or runtime holding a ticking component keeps the slice.

`Executor::entry_spin_budget` is the one answer: 60 s (`ENTRY_IDLE_BUDGET`,
inside what FreeRTOS's 32-bit `pdMS_TO_TICKS` expresses at 1 kHz) when every
session states `Waitable`, or `Notify` with its wake callback live, and no
ticking component is enrolled; else 10 ms (`ENTRY_POLL_BUDGET`). Its forms are
`ExecutorNodeRuntime::entry_spin_budget`,
`NodeDispatchRuntime::entry_spin_budget_ms` and
`nros_cpp_entry_spin_budget_ms`. Every entry funnel asks it: `nros::main!`, the
two generated forever-spins in `nros-macros`, the FreeRTOS, ThreadX, NuttX,
mps2-an385 and esp32 board entries, `nros_rtos_run_components.c`, the native
C++ `run_components`, `rclcpp::spin_in`, the C++ entry's spin loop and the
driven future's waits. `nros_cpp_spin_for` no longer spins past its own end.
`check-entry-spin-budget` refuses a literal budget in those funnels. The
spin-quantization audit ignores the idle budget, which is no cadence.
Every in-tree component with no `tick` (57 files: examples, templates, the
`nros new` scaffold, test bins) states `HAS_TICK = false`.

Measured on Linux (`examples/linux/rust/listener` over `rmw_zenohd`, no
talker, 10 s): the main thread's voluntary context switches went from 1000
(the 10 ms slice) to 0. A talker started afterwards was heard, and SIGTERM
ended the parked listener in 38 ms with exit status 0.

The first W7 Linux witness lands with it:
`entity_wake::an_idle_zenoh_entry_wakes_only_for_its_deadlines` runs entry
spins for 3.5 s over a live router with one 1 s timer and asserts at most 8
spins (measured 4; the 10 ms slice made ~350) and each expiry dispatched within
5 ms early / 50 ms late of its deadline (measured 0.4 ms late).

**W6b — tier loops and the period spin park (2026-11-02).** The native C++
tier runner (`nros_board_native_run_tiers*`) ran `spin_once(10)` and then slept
the tier's `spin_period_us`, on the boot thread and on every spawned tier, so
an arrival during the sleep waited for the period and an idle tier woke at that
rate. Each loop now parks in `spin_once(entry_spin_budget_ms)`. Shutdown
cancels every registered tier executor (`TierLiveSet`, one lock so a cancel
never reaches an executor its tier is dropping), and a bounded run clamps the
last park to what remains. `rclc_executor_spin_period` keeps its rate (one
`spin_some(timeout_ns)` per period) but its gap between invocations is now a
park on the wake object (`spin_some` until the invocation point), so an
arrival, a guard condition or a timer inside the gap is dispatched when it
happens and `rclc_executor_cancel` ends it. `node_guard_condition.c` covers the
gap: a trigger 150 ms into a 1 s period is dispatched 0 ms after it lands and a
cancel 250 ms later ends the spin 0 ms after it is raised (mutation-checked:
with the old sleep the trigger waits to the next period and the spin runs its
full 1000 ms). `realtime_tiers` (native cpp, cpp-rclcpp) and
`realtime_subnode_cpp_e2e` pass on the parked tiers.

Not done yet: the native LEVEL runner still bounds each level's park by its
period, the receive-blocking path itself, and the refusal of a session with no
readiness form.

**W6c — Zephyr's polled serial receive retired (2026-11-02).** zenoh-pico's
Zephyr serial link read with `uart_poll_in` + `k_yield` wherever
`CONFIG_UART_INTERRUPT_DRIVEN` was off (the mps2/an385 serial row was one).
The fork now has only the interrupt-driven ring (an `#error` without it, a
failed open when the ISR cannot bind the UART), and
`CONFIG_NROS_ZENOH_LINK_SERIAL` selects `UART_INTERRUPT_DRIVEN`. RFC-0106 D5
is amended by RFC-0113 D4/D5, and RFC-0113 gains its coverage table. Compiled
through `examples/zephyr/c/talker` on mps2/an385 with the serial conf stack:
the Rust serial row cannot configure for an unrelated reason (issue 2211).

**W6e — RTIC's arrival path is an RTIC task (2026-11-02).** Maintainer
ruling: RTIC stays supported, since nano-ros generates its scheduling. The
1 ms dual-timer tick that bounded each `wfi` is gone. `nros::main!` emits
`#[task(binds = ETHERNET, priority = 2)]` for `rtic-mps2-an385`, which calls
the new `RticBoardEntry::on_rx_irq` hook; the board masks the LAN9118
interrupt and posts its wake object, as the owned-spin entry's `#[interrupt]`
does. The run loop spins with the entry budget instead of a literal 10 ms,
and the board installs the SysTick one-shot park for RTIC images too. RTIC's
dispatchers take `UARTRX0`/`UARTTX0`, so an RTIC image has no serial wake;
every RTIC example is ethernet. Measured on the idle `qemu-rtic-listener`
(QEMU 9.0.2, 20 s): 0 SysTick one-shots and 2 host CPU ticks.
`test_qemu_rtic_{pubsub,service,action,mixed_priority_pubsub}_e2e` pass.

**W6f — ThreadX zenoh CALLER mode parks (2026-11-02).** Maintainer ruling:
port it. The platform ABI gains `nros_platform_tcp_set_rx_notify`, the TCP
twin of the UDP receive notification W4 added (NetX Duo's fork already
notifies TCP; POSIX, Zephyr and FreeRTOS answer "unsupported", since their
zenoh runs in THREAD mode). zpico installs the executor's wake callback on
the link's TCP socket and re-installs it after a reconnect, so the session
states `Notify`. The zenoh-pico fork's `zp_next_wake_ms` now answers in a
multi-thread build that drives its own background executor, so the CALLER
session's `next_deadline_ms` is zenoh's next due future rather than a blind
`z_sleep_ms`. On threadx-linux the NSOS shim had no receive notification
(`ENOSYS`), so it gains one: a lowest-priority ThreadX thread blocks in host
`poll()` over the armed sockets, fires once per readable socket and re-arms
on the next receive. That also gives threadx-linux XRCE over UDP its
`Notify`. Measured on the idle threadx-linux Rust listener, 10 s: 4,599
context switches against 53,061 before. `rtos_e2e` pubsub, service and
action pass on threadx-linux; pubsub and service on rv-virt-threadx
(Rust, C, C++).

**W6d — the ESP32-C3 board retired (2026-11-02).** Maintainer ruling: retire
rather than port. It had no receive interrupt and no one-shot timer behind a
wake object, so it was a "not yet" row with no in-tree consumer (dormant since
issue 1525). The board, platform crate, OpenETH driver, examples, smoke bins,
`just` module, launch script, QEMU fork submodule, matrix platform and cells,
index rows and `esp32` framework are deleted; every board name it answered to
is refused naming RFC-0113 D5 (`nros_entry_lower::REMOVED_BOARD_KEYS`, a
removal table beside the rename table), and `driver_for("esp32", …)` refuses
whatever the image's languages. RFC-0113's coverage row reads retired.

**W6g — the receive-blocking wait deleted, and D5's refusal (2026-11-02).**
With ESP32-C3 retired (W6d), ThreadX's CALLER mode on a TCP notify (W6f) and
RTIC's arrival path an RTIC task (W6e), no in-tree combination needed the
fallback. Deleted: the `else { timeout_ms }` drive that blocked inside a
transport's `recv` (the wait now always lands on the executor's wake object,
except in a build whose port links no wake object), `drive_primary`'s guard
slices, `has_cross_thread_waker` and `POLL_ONLY_WAKE_SLICE_MS`. Added: the
refusal, asked once where every runtime session opens
(`nros_rmw_cffi::refuse_without_readiness`, reached through `CffiRmw`): a
session stating `None` is closed and the open returns
`TransportError::Unsupported`, logging why and naming RFC-0113 D5.
`readiness_park::a_session_with_no_readiness_form_is_refused_at_open` drives
it through a stub backend, with and without a wake slot, against a stub
stating `Notify` that opens. `ENTRY_POLL_BUDGET` stays as the cadence of a
runtime whose components tick, and for the two degenerate cases a park
cannot see (a wake fan-out with no free member, more waitable sessions than
one park multiplexes); neither is a receive wait.

Two combinations surfaced while landing it. The metadata probe's recorder
backend stated none and was refused (the probe failed); it has no transport,
so it now states `Notify`. uORB states none in both builds, including the PX4
glue build, so a uORB session is refused until W8 wires the glue's arrival to
the wake callback (issue 2218, held with W8). And a Cyclone build without
foreign wake (freertos-posix) reads as `Notify` from its filled wake slot
though nothing calls it (issue 2217).

The native level runners (Rust `LinuxBoard::run_levels`, C++
`nros_board_native_run_levels_in`) passed each level's spin period as its
park bound, so an idle level woke once a period. They now pass the entry
budget (`LevelCore::entry_spin_budget`, `nros_cpp_entry_spin_budget_ms`); a
level's park already ends at its band's next timer or an arrival, a bounded
run caps the boot level's park at its remainder, and stopping the levels
wakes every parked worker (`Executor::wake_all_workers`) so each sees its
stop flag at once.

### W7 — Acceptance measurements for the promise

Make RFC-0113 D2 a measured property, not a reading:

- idle wake count = 0 beyond deadlines (per RMW, Linux + one RTOS lane;
  the RTOS lane is the Zephyr family's QEMU `mps2_an385` witness, since
  RFC-0115 D4 asserts family breadth there and the twin boards inherit it.
  An RTOS XRCE fixture row is added for this, since none exists);
- arrival→dispatch and deadline→dispatch latency against the port's
  granularity;
- a low-priority tier's arrival does not wake a high-priority tier on an
  attributing backend; the fan-out cost on a non-attributing one, measured and
  written into RFC-0113 (its third open question).

**Acceptance:** each measurement is a test in a lane that runs (not an
`eprintln!`), with numbers recorded in RFC-0113's coverage section.

**W7 — the Zephyr idle-wake witness and the RTOS XRCE row (2026-11-03).** The
RTOS lane's witness is `packages/testing/nros-tests/bins/idle-wake-zephyr-mps2`:
one subscription nobody publishes to and one 1 s timer, with Zephyr's user
tracing hook (`CONFIG_TRACING_USER`) counting how often the executor's thread is
switched in. Two fixture rows build it for mps2/an385, over zenoh and over
XRCE. The XRCE row is the RTOS XRCE row this item called for, and the first
XRCE image on this board. They are the (ZephyrQemuCortexM, C, {Zenoh, Xrce},
IdleWake) cells, run by `zephyr_cortex_m_qemu`. Measured: 10 switch-ins over
five expiries on both, with dispatch on the tick, in every run. The XRCE cell
first hung after one CREATE_CLIENT: Zephyr ignores `SO_RCVTIMEO` without
`CONFIG_NET_CONTEXT_RCVTIMEO`, which issue 2105 selected for zenoh only, so
`NROS_RMW_XRCE` now selects it too. Still open in W7: arrival→dispatch
latency on the RTOS lane, and the fan-out cost on a non-attributing backend.

**W7 — arrival→dispatch on the RTOS lane, and D2.5's fan-out bound
(2026-11-03).**

- **Arrival→dispatch.** The idle-wake witness's subscription moved to
  `/chatter`, and the image stamps the last LAN9118 receive interrupt
  (`sys_trace_isr_enter_user`, IPSR naming the IRQ) against the callback's
  dispatch. With the native Rust talker as the host peer (its zenoh and XRCE
  rows `serve` the two cells), `zephyr_cortex_m_qemu` asserts each sample at
  most 5 ms (one 1 ms tick plus the receive path) and at least 1 µs.
  Measured over five samples, three serial runs each: zenoh 435–877 µs, XRCE
  323–647 µs.
- **Fan-out.** The non-attributing backend in the tree is uORB (every entity
  slot NULL); zenoh, XRCE and Cyclone attribute. `tier_wake_fanout`
  (`required-features-tests` lane) measures both shapes over a stub backend:
  four tiers on one session, each on its own thread.
  - Without attribution, one low-tier arrival wakes 2.99–3.00 of the 3 high
    tiers, at 5.0–7.5 µs of each woken tier's CPU (bound stated: 100 µs).
  - With attribution, it wakes none of them.

  RFC-0113 D2.5 states the bound, and its open question is closed.

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

**Rulings (maintainer, 2026-11-03), recorded in RFC-0113 D9.** (c) A PX4
tier waits through a yielding park slot: no next-deadline hook and no wider
executor API (D7 stands). The executor calls its park slot with the bound in
µs; a PX4 tier's slot arms `ScheduleDelayed(us)` (or `ScheduleClear()` with no
deadline) and returns a "yielded" code defined in the park ABI and its header,
so `spin_once` returns to the work queue. The uORB wake callback is bound to
the tier's `ScheduleNow()`. Unit tests in nros-node: a yielding park makes
`spin_once` return, and the bound it received equals `next_wake_bound`. (f)
The owner is the maintainer, recorded in RFC-0113 D9 and in PX4's entry in
`board-support.toml`, which marks PX4 Tier 1 once (e) delivers.

**W8 (a) — the framework and the work-queue table (2026-11-03).** `px4` is
in `nros_orchestration_ir::FRAMEWORKS`; `nros::main!` refuses it by name
(`CPP_ONLY_FRAMEWORKS`: the entry is generated C++), and the macro's
vocabulary test holds that it never grows a Rust shape by accident.
`nros_orchestration_ir::px4::px4_work_queues` maps tiers, most urgent first,
onto `rate_ctrl`, `hp_default`, `nav_and_controllers`, `lp_default`, and
refuses more tiers than queues, naming every tier. Unit-tested in
nros-orchestration-ir.

**W8 (c) — the yielding park slot (2026-11-03).** The park ABI gains its
third answer: `NROS_PLATFORM_PARK_YIELDED` (2), beside SIGNALLED (0) and
EXPIRED (1), in `nros/platform.h` and as `nros_node::{PARK_SIGNALLED,
PARK_EXPIRED, PARK_YIELDED}` (bindings regenerated with the pinned bindgen;
`the_park_answers_match_the_platform_header` holds the two sets equal). A slot
that answers YIELDED has armed its framework's wake at the bound it received
and not waited. The executor then returns `spin_once` before its readiness
scan, with no executor API widened (D7 stands). A wake already pending when
the slot answers keeps the pass running, so pending work never waits for the
framework's next run. Two nros-node unit tests, each mutation-checked:
- a yielding park makes `spin_once` return without running the pass, and the
  bound it received equals `next_wake_bound` (a 10 ms timer under a 50 ms
  budget);
- a yield with a wake pending still runs the pass.

The uORB arrival's binding to the tier's `ScheduleNow()` is PX4 glue (issue
2218). It lands with the generated entry (b) and the examples (d), where a PX4
build compiles it.

**W8 (b) — the generated PX4 entry (2026-11-03).** `nros codegen entry`'s
PX4 emitter (`nros-cli-core` `codegen/entry/px4.rs`, template
`px4_entry.cpp.jinja`) renders a PX4 module: `ModuleBase<>` boilerplate, the
C++ entry's node setups (same pack, same partials; the monitor statics moved
into a shared partial, every C++ golden byte-identical), and one
`nros_px4::TierWorkItem` subclass per tier on the queue
`px4_work_queues` picks by priority rank. A plan with no tier table is one
`default` tier; a group-split plan and more tiers than queues are refused.
The runtime half, `integrations/px4/include/nros_px4/tier_work_item.hpp`
(on the module's include path), opens the boot tier's session and borrows it
for the rest, installs the tier's park slot, and drives `Run()`: a first pass
whose park answers EXPIRED, then passes until the slot yields, arming
`ScheduleDelayed(bound)` or, with no deadline inside the idle budget
(`NROS_CPP_ENTRY_IDLE_BUDGET_MS`, new, asserted equal to
`ENTRY_IDLE_BUDGET`), `ScheduleClear()`. Arrivals go through `notify()`,
which re-queues a run the clear removed. Goldens `px4_tiers` (the two-tier
bringup: `high`@`rate_ctrl`, `low`@`hp_default`) and `px4_one`; four unit
tests (queue follows rank not table order, too many tiers, group split, bad
`main`). Both goldens compile clean with PX4 SITL's own module flags
(`-Wall -Wextra -Werror -Wshadow -Wfloat-equal ...`) against the real PX4
headers, which found two things: PX4's `ERROR` macro breaks `rclcpp_lifecycle`
unless nano-ros is included first, and `parameter_value.hpp`'s double `==`
failed `-Wfloat-equal` for every PX4 module including `rclcpp.hpp` (now
`std::equal_to`). The CLI flag and the cmake wiring that call the emitter land
with (d), which builds them.

**W8 (d), part 1 — the firmware example and uORB readiness (2026-11-03).**
`examples/px4/cpp/firmware` is a nano-ros workspace now:
- the component `uorb_demo_pkg::UorbDemo` (a 1 s timer, the `debug_key_value`
  publisher and a callback `vehicle_status` subscription);
- `uorb_demo_bringup`;
- a module directory holding only `CMakeLists.txt` and `Kconfig`.

`nros_px4_add_module(... BRINGUP <dir> COMPONENTS <pkg>:<exec>:<class>:<header>)`
asks `nros dev image model-path` for the model `nros sync` wrote, writes the
component index, and generates the entry with
`nros dev codegen entry --px4-main <MAIN>`. The plan resolves on the `linux`
board, because PX4 SITL is POSIX. `just px4 build-sitl-example` runs `nros sync`
first.

uORB (issue 2218, archived): the push-wake glue had NEVER been linked. A weak
stub in the same archive resolved the symbol first, so every subscription
polled. The fix has four parts:
- exactly one of the glue and the no-glue stub joins a build, and the stub is
  no longer weak;
- the session states Notify with the glue and none without it (refused at
  open, D5), and fills `set_wake_callback` and the per-entity callback; a
  subscription that cannot register is refused;
- the glue callback runs in the publisher's context, with no `lp_default` hop;
- `nros_rmw_uorb_set_arrival_target` binds each subscription a tier's setup
  creates to that tier's `notify()`.

Measured on PX4 SITL v1.17.0: the module starts on one work item (`default`
on `rate_ctrl`), publishes at 1 Hz, dispatches `vehicle_status` on arrival,
and `px4_uorb_interop_e2e` passes.

A networked backend's arrival on a PX4 tier is issue 2225.

**W8 (d), part 2 — the bridge (2026-11-03).** `examples/px4/cpp/bridge` is a
workspace too:
- the component is `uorb_bridge_pkg::UorbBridge`;
- its inward subscription dispatches on arrival, which replaces the
  200 ms `ScheduleOnInterval` poll;
- its second node opens the outward session by name (`NodeBuilder::rmw`).

The entry opens the bringup's `[system] rmw` (uORB) as the primary session.
`nros_px4_add_module` bakes it as `NROS_ENTRY_RMW` when BACKENDS lists more
than one, because the runtime refuses to guess between registered backends.
`message_equality.hpp`'s generic `==` failed `-Wfloat-equal` for a PX4 module
with a float field, so it is now compiled under a diagnostic pragma.

Measured on PX4 SITL with `rmw_zenohd`: `nros_uorb_bridge start` prints
`bridging ... (uorb) -> ... (zenoh)`, and samples from `px4_mavlink_debug`
are forwarded. `git grep ScheduledWorkItem examples/` finds nothing; the
only work items are the generated ones.

`px4_bridge_ros2_e2e` (the ROS 2 subscriber leg) needs a `px4_msgs` ament
overlay for the host's distro (`just px4 build-px4-msgs-ros <distro>`). That is
provisioning, part of W8 (e).

**Assessment (2026-11-02).** SITL is provisionable here: the main checkout
holds `third-party/px4/PX4-Autopilot` with a built
`build/px4_sitl_default`, `just px4 setup|doctor|build-sitl-cpp|test-sitl`
exist, and of PX4's Python build deps only `pyros_genmsg` is missing on this
host. Nothing below is built yet; this is the design and the breakdown.

*Design.* The generated entry is a C++ translation unit per image, emitted by
the C++ entry codegen (the same emitter `nros_board_native_run_tiers` images
use), not hand code:

1. One `px4::ScheduledWorkItem` subclass per tier (`NrosTier_<name>`), with
   `ModuleBase<>` boilerplate on the image's module, constructed on the work
   queue the tier's priority picks from `px4::wq_configurations` (a table in
   the codegen, rank-ordered: the most urgent tier gets `rate_ctrl`, then
   `hp_default`, `nav_and_controllers`, `lp_default`; more tiers than queues
   is refused at generation, never shared silently).
2. `Run()` is one executor pass over the tier's groups with a ZERO wait
   (`spin_once(0)`), then reschedules itself: `ScheduleDelayed(next_us)` from
   the executor's internal next deadline (timers, routes, the session's
   `next_deadline_ms`), `ScheduleClear()` when there is none. The tier never
   parks a thread; the work queue is its wait object.
3. Arrivals: the uORB backend states `Notify`, and its wake callback (fired
   from `px4_callback_glue.cpp`'s `SubscriptionCallbackWorkItem` adapter) is
   bound to the owning tier's `ScheduleNow()`. Per-entity wake routing
   (phase-397) picks the tier, so a low tier's topic does not run a high
   tier's item.
4. The polling build (`NROS_RMW_UORB_BUILD_PX4_GLUE=OFF`) states `None` and
   is refused at open under D5; it stays a host test double.
5. `FRAMEWORKS += "px4"`; `nros::main!` refuses `px4` with a pointer to the
   C++ entry (a Rust PX4 module rides `px4-rs`, out of scope here).

*Work breakdown.* (a) IR: `px4` in `FRAMEWORKS`, the work-queue table and
its refusal, unit-tested in `nros-orchestration-ir`. (b) Codegen: the C++
entry emitter's `px4` arm, snapshot-tested against a two-tier bringup.
(c) Runtime: a doc(hidden) `Executor` hook returning the internal next
deadline for a driver that is not a thread (the work item), and the
`ScheduleNow` binding of the wake callback. (d) Replace
`examples/px4/cpp/{firmware,bridge}` modules with the generated entry.
(e) Provision `pyros_genmsg` in the runner image, add the SITL cell, run
W7's idle-wake and latency tests under the work-queue scheduler. (f) Name
the owner (RFC-0112 D3).

*Blocker.* Not started in this session for time: (a)-(f) are each a PR,
and (c) changes the executor's public surface that D7 just narrowed, so it
wants a short ruling first (a doc(hidden) hook for in-tree drivers only).
