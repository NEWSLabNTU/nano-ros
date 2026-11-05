---
rfc: 0113
title: "Every supported way to run nano-ros keeps the real-time promise; one wait serves every platform and RMW"
status: Stable
since: 2026-10
last-reviewed: 2026-11
implements-tracked-by: [phase-521]
amends: [RFC-0106, RFC-0112]
supersedes: []
superseded-by: null
---

# RFC-0113 — The real-time promise, and the one wait that keeps it

**Status:** Draft (2026-10-31), approved by the maintainer the same day. Written before any code. It sets a support
rule that outranks RFC-0112's tiers, and it gives the executor one way of
waiting for every platform and every RMW. Implemented by
[phase-521](../roadmap/archived/phase-521-rt-unified-wait.md).

## Summary

The real-time promise is the first thing nano-ros offers. From this RFC on, a
way of running nano-ros is SUPPORTED only if nano-ros can make that promise in
it. The promise holds only where nano-ros decides which callback runs and at
what priority: its own runtime (tiers on OS threads, `nros::main!`, the
runners, `spin`), and a Tier 1 framework whose scheduling nano-ros generates
and can analyse (RTIC).

Two consequences follow.

1. **The host-driven contract retires.** RFC-0112's Tier 0 let any foreign
   loop (tokio, Embassy, Qt, GLib, a PX4 work item) drive the executor. In
   that shape the host's scheduler decides when callbacks run, so no response
   time, jitter or priority bound can be stated. The surfaces that exist only
   for it are deleted (D7).
2. **The runtime waits one way, everywhere.** Each tier thread waits on its
   own wait object, at its own priority, and that object fires for exactly two
   reasons: data for this tier arrived, or this tier's next deadline came due.
   No tier ever polls, busy-waits, or blocks inside a transport's `recv`. Every
   backend reports arrivals through one RMW slot in one of two forms, and every
   backend's deadline names a real event.

## Motivation

Measured on main, 2026-10-31:

- **`spin_async` busy-loops on zenoh.** zenoh's `next_deadline_ms` returns a
  constant 3333 ms (`nros-rmw-zenoh/src/shim/session.rs`), so a deadline is
  always pending and a `spin_async` with no sleep function yields forever. With
  `spin_async_with_sleep` it sleeps, but in the host's timer, on the host's
  thread, at the host's priority.
- **The same constant wakes every idle zenoh tier every 3.3 s.** Since
  zenoh-pico 1.10 (RFC-0110), a THREAD-mode build sends keep-alives and checks
  the lease on zenoh-pico's own executor thread. The nano-ros executor has
  nothing to do at that deadline. It is a wasted wake on every microcontroller.
- **Poll-only backends block inside `recv`.** XRCE and uORB cannot end a park
  (RFC-0106 D5), so a tier waits in the transport's receive. That wait has
  1 ms granularity, and a guard condition triggered from another thread can
  only be noticed between 10 ms slices (`POLL_ONLY_WAKE_SLICE_MS`, deleted in
  phase-521 W6). Neither is a bound a real-time tier can be designed against.
- **A foreign loop cannot carry the promise.** tokio is work-stealing with no
  priorities; Qt and GLib run one main thread; an Embassy schedule is the
  user's own. The integration contract RFC-0112 opened (issues 2143–2146)
  makes these loops efficient, but it cannot make them real-time, and a
  supported shape whose timing nano-ros cannot state is a promise nobody can
  check.

## Design

### D1 — The support rule

A way of running nano-ros is supported only if nano-ros decides which callback
runs next and at what priority, and can state the bounds in D2 for it.

| Shape (RFC-0112 vocabulary) | Supported | Why |
|---|---|---|
| A: nano-ros owns the loop (`owned-spin`, the runners, `spin*`) | yes | tiers map onto OS threads with real priorities (RFC-0002, RFC-0108) |
| C: a framework owns scheduling, and nano-ros generates the mapping (RTIC; PX4 work queues, D9) | yes, while Tier 1 admission holds | the priority mapping is generated and the framework's model is analysable |
| B: a host drives the executor (tokio, Embassy task, Qt, GLib, Arduino `loop()`, a hand-written PX4 work item) | **no** | the host decides when callbacks run |

A request to support a new framework is answered with Shape A, or with a Tier 1
admission that shows how the D2 bounds hold under that framework (RFC-0112 D3
gains this as a fifth criterion).

### D2 — What the promise states

For every tier of a supported image:

1. **It never busy-waits.** Between dispatches its thread is blocked in the
   platform's wait, and the idle wake count is zero apart from D2.3's reasons.
2. **It waits on its own wait object, at its own priority.** Priority comes
   from the thread that waits, never from the object.
3. **It wakes for exactly two reasons:** data that this tier owns arrived, or
   this tier's next deadline came due. A guard condition's trigger is an
   arrival.
4. **Wake latency is bounded by the port, not by nano-ros.** An arrival
   reaches dispatch within the OS's wake latency. A deadline is honoured to the
   port's declared granularity, rounded up and recorded (RFC-0106 D3, D9).
5. **An arrival wakes the tier that owns it.** Where a backend attributes its
   arrivals, that is exact (phase-397). Where it cannot, the wake fans out to
   the tiers over that session, each tier's dispatch still runs at its own
   priority, and the fan-out is documented per backend as a known cost.
   **Its bound:** one arrival wakes at most every other tier on that session
   (N − 1 for N tiers), and each such wake is one thread wake plus one
   non-blocking pass that finds nothing of its own, never a wait. Measured on
   Linux at 5–7.5 µs of the woken tier's CPU (stated bound: 100 µs); measured,
   not asserted, at 6.8–9.4 µs over live zenoh before zenoh attributed
   (phase-436). The non-attributing backend in the tree is uORB (all three
   entity slots NULL); zenoh, XRCE and Cyclone attribute.
6. **No tier blocks inside a transport.** The transport is drained
   non-blockingly after a wake.

### D3 — One wait object per tier, two reasons to fire

RFC-0106's park becomes the only wait: `park_until(deadline)` on the tier's
own wait object, ended early by readiness. The deadline is the RFC-0106 D1
minimum, which stays internal. Nothing outside the runtime reads it.

The platform primitive is unchanged where a port has one (`k_sem_take`,
`xSemaphoreTake`, `tx_semaphore_get`, `sem_clockwait`). On POSIX and NuttX, a
tier that has a waitable source (D4) parks in `ppoll` over its own `eventfd`
plus those sources, with the deadline as the timeout. That is the one platform
whose wait must multiplex OS handles. Zephyr joins it if phase-521 W4 picks
the Waitable form there.

Zero idle wakes (D2.1) is also what lets a port's tickless idle (Zephyr's
tickless kernel, FreeRTOS `configUSE_TICKLESS_IDLE`, ThreadX low-power mode)
stop the tick; enabling it is port configuration, outside this RFC.

### D4 — Readiness: one RMW slot, two forms

Every backend reports arrivals through one new vtable slot,
`readiness_source`, in one of two forms:

- **Notify** (preferred). The backend calls the executor's wake when data
  arrives. This is the existing wake callback, and it works with every
  platform's park unchanged. Sources: zenoh's and Cyclone's own threads, an
  lwIP `netconn` callback, NetX `nx_udp_socket_receive_notify`, a UART RX
  interrupt, a NIC RX interrupt (mps2-an385's LAN9118 today).
- **Waitable.** The backend hands over a platform-native handle that the park
  multiplexes: a file descriptor on POSIX and NuttX (XRCE's
  `udp_bridge.fd`).

The custom-transport ops that a board fills (`set_custom_transport_ops`, XRCE
serial and similar) gain an `rx_notify` hook, so a board's UART interrupt can
deliver Notify without the RMW knowing the board.

| Backend × transport | Form after phase-521 |
|---|---|
| zenoh (THREAD mode), Cyclone DDS | Notify (exists) |
| zenoh (CALLER mode: bare metal, ThreadX) | Notify from the transport's RX event |
| XRCE over UDP on POSIX / NuttX | Waitable (socket fd) |
| XRCE over lwIP (FreeRTOS), NetX (ThreadX) | Notify from the stack's receive callback |
| XRCE over zsock (Zephyr) | Waitable through `zsock_poll` over the socket plus an `eventfd` park, unless a receive callback proves cheaper; phase-521 W4 measures and picks |
| XRCE over serial / custom transport | Notify through `rx_notify` |
| smoltcp on bare metal | Notify from the NIC RX interrupt |
| uORB on PX4 | Notify through the `SubscriptionCallback` glue (`px4_callback_glue.cpp`), bound per subscription to the owning tier's work item |
| uORB polling build (`NROS_RMW_UORB_BUILD_PX4_GLUE=OFF`) | none: refused under D5, kept only as a host test double |

### D5 — No silent fallback

A backend × transport × platform combination with no readiness form is
REFUSED when the session opens, naming the combination and this RFC. It is
never served by blocking in `recv`. The receive-blocking wait, its 10 ms guard
slices and `POLL_ONLY_WAKE_SLICE_MS` are deleted (phase-521 W6). The refusal
is asked once, where every runtime session opens
(`nros_rmw_cffi::refuse_without_readiness`): it returns
`TransportError::Unsupported` and logs the reason.

### D6 — Deadlines name real events

A backend's `next_deadline_ms` reports a deadline only when the executor has
work to do at it.

- **zenoh, THREAD mode:** `None`. zenoh-pico's own executor thread runs the
  lease and keep-alive.
- **zenoh, CALLER mode:** zenoh-pico's next due future, the link read
  futures excluded (`zp_next_wake_ms`; phase-521 W5). The read future only
  polls (`Z_RUNTIME_IDLE_READ_TASK_SLEEP`), so honouring it would read the link
  at 1 kHz; on a board whose park the RX interrupt ends it is not needed.
  `None` when nothing is scheduled. ThreadX's CALLER build runs zenoh-pico
  multi-threaded and keeps one keep-alive interval (~3.3 s).
- **XRCE:** unchanged (`xrce_deadline_next` already names real events).

On bare metal, a deadline is served by a hardware one-shot timer that the
board arms (mps2-an385's SysTick one-shot today). This is a port obligation,
the same as the RX interrupt.

### D7 — The host-driven surface retires

These exist only so a foreign loop can schedule the executor. They are deleted
in one breaking change, with a `.breaking.md` entry:

| Surface | Languages |
|---|---|
| `spin_async`, `spin_async_with_sleep` | Rust |
| `set_ready_notifier`, `ready_notifier_supported`, `ready_fd` | Rust; C `rclc_executor_set_ready_notifier`; C++ `rclcpp::Executor::set_ready_notifier` |
| public `next_deadline_us` (it stays crate-internal for the park) | Rust; C `rclc_executor_get_next_deadline_ns`; C++ `rclcpp::Executor::next_deadline_us` |
| the tokio examples (`examples/linux/rust/{service,action}-client-async`) and `linux_async_roundtrip_e2e` | — |
| the Embassy recipe (`book-internals/src/realtime/rtic.md`) and the async section of `nros::guide::services` | — |
| `nros::setup!` and its `nros_setup` / `nros_deploy` emit (phase-520, issue 2150) | Rust |
| phase-520 W3's host-loop deferred surface: `callback_mark`, `set_deferred_since`, `CallbackMark`, the `nros::node_runtime` host-drain functions, and `examples/linux/rust/listener-host-loop` | Rust |

These stay, because nano-ros's own runtime uses them:

- `spin`, `spin_once`, `spin_some`, `spin_period`: the runtime's own verbs.
- `wake()`, `wake_handle()`, `signal_fd()`: an interrupt or signal handler
  waking the runtime's executor.
- The deferred-dispatch queue (issue 2146): the RTIC path runs on it.

### D8 — RFC-0112 and RFC-0106 after this RFC

- **RFC-0112:** Tier 0 is withdrawn as a support promise. Tier 1 admission
  gains a fifth criterion, "the D2 bounds hold under the framework's
  scheduler". Tier 2 (an out-of-tree board crate) is promised what an in-tree
  board gets: the owned runtime, through the board-entry traits.
- **RFC-0106:** D5's receive-blocking wait is replaced by D4/D5 here. D1, D2,
  D3, D4, D6 and D9 stand unchanged.

### D9 — PX4 is a Tier 1 framework

PX4 opens nano-ros to drone applications, so it is supported, and under D1
that means Shape C: nano-ros generates the mapping and states the bounds.
Each tier maps onto a PX4 work queue, whose thread carries a real priority
(`px4::wq_configurations`), and a tier's wait is that work item's schedule.
uORB arrivals reach it through `SubscriptionCallback` (D4's Notify), and a
deadline through `ScheduleDelayed`/`ScheduleOnInterval`. A PX4 tier never
polls `orb_check` in a loop. The hand-written `ScheduledWorkItem` modules in
`examples/px4/cpp/*` are replaced by the generated entry. RFC-0112 D3's
five admission criteria apply: an in-tree board (PX4 SITL), a delivering CI
cell, a need the owned shape cannot express (PX4 owns `main` and the threads),
a named owner, and the D2 bounds measured under the work-queue scheduler.

**Rulings (maintainer, 2026-11-03).**

- **A PX4 tier waits through a yielding park slot.** No next-deadline hook is
  added and no executor API widens; D7 stands. The executor already calls its
  park slot with the bound in microseconds. A PX4 tier's park slot arms
  `ScheduleDelayed(us)` (or `ScheduleClear()` when there is no deadline) and
  returns a "yielded" code, so `spin_once` returns to the work queue instead
  of blocking. The code is `NROS_PLATFORM_PARK_YIELDED` (2) in the park ABI
  (`nros/platform.h`, `nros_node::PARK_YIELDED`); a wake already pending when
  the slot yields keeps the pass running. A uORB arrival's wake callback is
  bound to the tier's `ScheduleNow()`.
- **Owner.** The maintainer owns PX4 as a Tier 1 framework (RFC-0112 D3's
  named-owner criterion), recorded here and in PX4's entry in
  `board-support.toml`.

The work queues come from one rank-ordered table,
`nros_orchestration_ir::px4::PX4_WORK_QUEUES`: the most urgent tier takes
`rate_ctrl`, then `hp_default`, `nav_and_controllers` and `lp_default`. More
tiers than queues is refused at generation, never shared (phase-521 W8 a).

The entry is generated C++ (phase-521 W8 b): `nros dev codegen entry`'s PX4
emitter (`codegen/entry/px4.rs`) renders a PX4 module with one
`nros_px4::SchedTaskWorkItem` subclass per tier on the queue its rank picks, and
the work item is the runtime half (`integrations/px4/include/nros_px4/
sched_task_work_item.hpp`). The boot tier opens the session and the others borrow
it, as on every tier runner. A `Run()` is one pass whose first park answers
EXPIRED, so it drains and dispatches without blocking, then passes until the
park slot yields: the slot arms `ScheduleDelayed(bound)`, or
`ScheduleClear()` when no deadline falls inside the executor's idle budget
(`NROS_CPP_ENTRY_IDLE_BUDGET_MS`). Arrivals reach the tier through
`SchedTaskWorkItem::notify()`, which a clear cannot lose, and they reach `notify()`
through the slot's wake half: `nros_cpp_executor_set_wake_hook`, a hook the
executor's wake object calls on every signal, ahead of the platform signal
(issue 2225, maintainer ruling 2026-11-03). Every backend's arrival therefore
schedules the tier — uORB's from the publisher's context, zenoh's and XRCE's
from their transport threads — and an attributed arrival signals only the
owning tier's wake object, so only that tier runs. A node whose groups span
two tiers is refused: a work item per tier has no shared executor to carry a
sched context per tier.

### D10 — `nros::setup!` retires

`nros::setup!` (phase-520) emits an image's assembly for a host that owns its
loop, which is Shape B. It is deleted with the rest of D7. A Tier 1 framework
gets its assembly from its own generated entry (D9 for PX4), never from a
host-callable function.

## Coverage

Which in-tree combinations meet D2 today, and the evidence. Every row either
meets D2 or is refused at open under D5 (phase-521 W6); none is served by the
deleted receive-blocking wait.

| Combination | Readiness | Evidence |
|---|---|---|
| zenoh THREAD mode (POSIX, NuttX, Zephyr, FreeRTOS) | Notify | idle Linux listener: 0 voluntary context switches in 10 s (was 1000); `entity_wake::an_idle_zenoh_entry_wakes_only_for_its_deadlines` 4 spins in 3.5 s with a 1 s timer, dispatch 0.4 ms late; Zephyr mps2/an385 (in-kernel IP stack, QEMU 11): 10 executor switch-ins over five 1 s expiries, dispatch on the tick |
| zenoh CALLER mode, mps2-an385 (LAN9118 / UART0) | Notify (RX IRQ + SysTick one-shot) | idle listener, QEMU 9.0.2, 20 s: 41 one-shot arms (1,892 with the old 10 ms drain); the same under the SDK's QEMU `11.0.0-nros8` (QEMU process idle at 0 % host CPU; `-nros7`'s `wfi` did not halt and cost 120 %, issue 2209) |
| Cyclone DDS | Notify — the listener posts the executor's wake from Cyclone's receive thread; on the host-simulator boards (freertos-posix, threadx-linux), where that thread may not enter the kernel, it posts a port-owned forwarder (`nros/platform_foreign_wake.h`) that signals from kernel context. A build with neither states `None` and is refused at open (issue 2217) | threadx-linux `test_threadx_linux_cyclonedds_*` (pubsub, C++ pubsub, service, action) and freertos-posix `freertos_posix_{c,cpp}_entry_delivers_over_cyclonedds` pass with the forwarder |
| XRCE over UDP, POSIX / NuttX | Waitable (socket fd) | phase-521 W3 |
| XRCE over zsock, Zephyr | Waitable (`zsock_poll`) | native_sim arrival→dispatch p50 0.4–1.1 ms (was 3.2–6.3 ms); mps2/an385 (in-kernel IP stack, QEMU 11): 10 executor switch-ins over five 1 s expiries, dispatch on the tick |
| XRCE over lwIP (FreeRTOS), NetX Duo (ThreadX) | Notify (stack receive callback) | p50 2.2–4.0 ms (was 72–253 ms); NetX 1.8 ms |
| C++ native tier runner, `rclc_executor_spin_period` | parks on the wake object | `node_guard_condition.c`: a trigger in the period gap dispatched 0 ms after it lands (was: next period) |
| zenoh CALLER mode, ThreadX | Notify — the link's TCP socket notifies through `nros_platform_tcp_set_rx_notify` (NetX Duo's receive notify; on threadx-linux a lowest-priority NSOS watcher thread), and `zp_next_wake_ms` is real in the multi-thread build | idle threadx-linux listener, 10 s: 4,599 context switches against 53,061 with the blind `z_sleep_ms`; `rtos_e2e` pubsub/service/action pass on threadx-linux and rv-virt-threadx |
| esp32-c3 bare metal | **retired** (phase-521 W6) — no RX interrupt or one-shot behind a wake object; every board name is refused naming D5 | `nros_entry_lower::REMOVED_BOARD_KEYS` |
| XRCE over a board's custom transport (mps2-an385 UART0, `talker-xrce`) | Notify (`nros_rmw_xrce_set_custom_transport_rx_notify`, from `UARTRX0`) | 1 Hz talker, QEMU 9.0.2, 20 s: 42 one-shot arms (1,832 with the receive-blocking drain) |
| XRCE over a custom transport with no receive notification | **refused** at open (D5) | `readiness_park::a_session_with_no_readiness_form_is_refused_at_open` (a stub backend stating none, with and without a wake slot) |
| RTIC images (mps2-an385, LAN9118) | Notify — an RTIC hardware task bound to `ETHERNET` posts the wake object; the run loop parks with the entry budget (SysTick one-shot at the next deadline) | idle `qemu-rtic-listener`, QEMU 9.0.2, 20 s: 0 SysTick one-shots and 2 host CPU ticks (the router's keep-alive arrives first); the retired 1 ms dual-timer tick woke it 20,000 times. `test_qemu_rtic_{pubsub,service,action,mixed_priority_pubsub}_e2e` pass |
| uORB polling build | none: refused under D5 | host test double only |
| uORB, PX4 glue build | Notify — each subscription's `uORB::SubscriptionCallback` (publisher's context) fires the per-entity callback, so only the owning tier's executor wake is signalled, and that wake's hook calls the tier's `SchedTaskWorkItem::notify()` (phase-521 W8 d, issue 2225) | PX4 SITL: the generated `nros_uorb_demo` dispatches `vehicle_status` on arrival on its work item; `px4_uorb_interop_e2e` passes. The glue had never been linked before (issue 2218, archived) |

Measured on Linux over `rmw_zenohd` (`entity_wake`, `required-features-tests`
lane, 2026-11-02): an idle entry with one 1 s timer runs 4 spins in 3.5 s and
dispatches each expiry 74-183 us late (D2.1, D2.3); an idle executor sharing a
session with one taking 936 arrivals/s runs 6 spins in 3 s on 120 us of thread
CPU, i.e. it is woken only by its own deadlines (D2.5's fan-out on an
attributing backend is zero); `a_shared_session_member_is_not_woken_by_another_members_arrivals`
pins the same per member.

Measured on Zephyr (mps2/an385 under QEMU 11, Zephyr's own IP stack and the
LAN9118 driver; `zephyr_cortex_m_qemu::zephyr_cortex_m_idle_{zenoh,xrce}_image_wakes_only_for_its_deadlines`,
2026-11-03): an image with one idle subscription and one 1 s timer counts its
executor thread's switch-ins with Zephyr's user tracing hook. Over five
expiries (5,000 ms) the count is 10 on zenoh and 10 on XRCE, each expiry
dispatched on its 1 ms tick (0 us late, 0 us early), in every run. That is two
switch-ins per deadline and none between them; the receive-blocking wait woke
on a 10 ms slice, ~500 times in the same window. The test bounds the count at
two per expiry plus two, and the lateness at 50 ms. The XRCE cell is the first
XRCE image on an in-kernel Zephyr stack, and it found that XRCE's per-attempt
receive timeout was ignored there: `CONFIG_NET_CONTEXT_RCVTIMEO` was selected
for zenoh only (issue 2105), so the first CREATE_CLIENT's receive never
returned. `NROS_RMW_XRCE` selects it now.

Arrival→dispatch on the same board (`zephyr_cortex_m_qemu::zephyr_cortex_m_{zenoh,xrce}_arrival_is_dispatched_within_the_port_bound`,
2026-11-03): the witness image, with a host talker publishing on `/chatter`
at 1 Hz, stamps the last LAN9118 receive interrupt and the callback's
dispatch on the 25 MHz cycle counter. Over five samples, three serial runs
each: zenoh 435–877 µs (mean 697–874), XRCE 323–647 µs (mean 516–582). Every
sample lands inside one 1 ms Zephyr tick, because an arrival ends the park
through the wake object, not on a tick. The test states the port's bound as
one tick plus the receive path, 5 ms, and a lower bound of 1 µs, so a stamp
that is not measuring the receive path fails.

D2.5's fan-out, measured on Linux (`sched_task_wake_fanout`,
`required-features-tests` lane, 2026-11-03) over a stub backend registered
under its own name in two shapes that differ only in the entity slot: four
tiers share one session, each on its own thread parked in a 1 s spin, and
200 arrivals land for the lowest. Without attribution (the slot NULL, the
arrival a session wake, uORB's shape) one arrival wakes 2.99–3.00 of the 3
high tiers, at 5.0–7.5 µs of each woken tier's CPU (three runs; the bound the
test states is 100 µs). With attribution (the slot filled, the arrival
naming its entity) it wakes none of them: each parks its whole budget, and
the low tier is woken by its own arrival.

Under PX4's work-queue scheduler (D9; PX4 v1.17.0 SITL,
`px4_sitl::work_queue_sched_task_wakes_only_for_deadlines_and_arrivals`, the
hosted `px4-sitl` nightly job, 2026-11-03): the generated `nros_uorb_demo`
runs one tier, a 1 s timer and a `vehicle_status` subscription, as a PX4
work item. The tier counts its own runs and the component its dispatches;
over a 10 s window, in each of three runs, the work item ran 30 times for 10
deadlines and 20 arrivals: no run answered neither (the retired 10 ms slice
would add ~1,000). Arrival→dispatch is measured on the real clock from the
uORB callback's `notify()` to the run that serves it: 14–29 µs, against a
stated bound of 5 ms. SITL runs lockstep, so PX4's `hrt` is virtual time
that moves in steps: a publish-to-dispatch delta on it is 0 by construction,
and a 1 s tick's jitter on it is exactly one 16 ms step in every run, which
is what the test bounds (20 ms) rather than a real-time claim.

Zephyr's serial link receives only through its interrupt-driven ring: the
polled `uart_poll_in` + `k_yield` path is retired (phase-521 W6).

## Alternatives considered

- **Adapter crates** (`nros-tokio`, `nros-embassy`): good integration UX, a
  version matrix to follow (Embassy breaks yearly), and no real-time promise.
  Rejected by D1.
- **A runtime-agnostic contract** (the raw deadline, waker and handles,
  wired by the user): no version matrix, but every user re-implements the
  wiring, and a forgotten handle is a missed wake that a quiet test run never
  shows (RFC-0106 D4's class). No real-time promise. Rejected by D1.
- **One readiness fd for any event loop** (an `epoll` set of RMW handles plus
  a `timerfd`): the best non-real-time integration surface for C, C++ and
  Rust. Rejected by D1. Its internal half survives as D3's POSIX park.

## Open questions

- ~~**Fan-out bound.**~~ Closed 2026-11-03 (phase-521 W7): D2.5 states it.

## Changelog

- 2026-11-04 — Coverage: the zenoh CALLER mps2-an385 row holds under the
  SDK's QEMU 11 too; `-nros8` carries upstream's WFI fix (issue 2209).
- 2026-11-04 — Coverage: Cyclone states its form from its listener (a
  `readiness_source` slot), and the host-simulator boards forward Cyclone's
  arrival through a port-owned foreign-wake bridge (issue 2217).
- 2026-11-03 — D9: the park slot gains its wake half. The executor's wake
  object calls a hook (`nros_cpp_executor_set_wake_hook`) on every signal,
  and a PX4 tier binds it to `notify()`, so a networked backend's arrival
  schedules the tier instead of waiting for its next deadline. uORB's own
  arrival target (`nros_rmw_uorb_set_arrival_target`) is retired into it
  (issue 2225).
- 2026-11-03 — Stable: phase-521 is complete and archived. Every
  combination in the coverage table states Notify or Waitable or is refused
  at open, and D2 is measured on Linux, Zephyr and PX4.
- 2026-11-03 — Coverage: D2 measured under PX4's work-queue scheduler on
  SITL (runs = deadlines + arrivals, arrival→dispatch 14–29 µs), the cell run
  by the hosted `px4-sitl` nightly job; PX4 admitted to Tier 1 in RFC-0112 D3
  (phase-521 W8 e/f).
- 2026-11-03 — D4/D9: uORB states Notify. Its glue is linked (a weak stub
  had shadowed it) and bound to the owning tier's work item. The PX4 firmware
  example runs on the generated entry (phase-521 W8 d).
- 2026-11-03 — D9: the generated PX4 entry and its tier work item land
  (phase-521 W8 b).
- 2026-11-03 — D9: the yielding park slot's code, `NROS_PLATFORM_PARK_YIELDED`,
  lands in the park ABI (phase-521 W8 c).
- 2026-11-03 — D2.5's fan-out bound stated and measured
  (`sched_task_wake_fanout`); the open question closes. Zephyr arrival→dispatch
  measured on mps2/an385 (phase-521 W7).
- 2026-11-03 — D9: the maintainer's rulings recorded (a PX4 tier waits
  through a yielding park slot; the maintainer owns PX4), and the work-queue
  table landed (`px4` joins `FRAMEWORKS`, phase-521 W8 a).
- 2026-11-03 — Zephyr idle-wake measured on mps2/an385, zenoh and XRCE
  (phase-521 W7); `NROS_RMW_XRCE` selects `NET_CONTEXT_RCVTIMEO`.
- 2026-11-02 — RTIC row: the arrival path is an RTIC task on the RX
  interrupt (phase-521 W6e); the 1 ms tick is gone.
- 2026-11-02 — ThreadX zenoh CALLER row: Notify through the new platform
  TCP receive notification (phase-521 W6f).
- 2026-11-02 — The esp32-c3 bare-metal board retired rather than ported
  (maintainer ruling); its coverage row reads retired.
- 2026-11-02 — D5 implemented: the refusal at open, and the receive-blocking
  wait, its guard slices and `POLL_ONLY_WAKE_SLICE_MS` deleted (phase-521 W6).
- 2026-11-02 — Coverage section added (what meets D2, the evidence, the
  "not yet" rows); RFC-0106 D5 amended to match D4/D5.
- 2026-11-02 — D6: zenoh CALLER mode reports zenoh-pico's next due future
  (phase-521 W5).
- 2026-10-31 — Filed. PX4 decided as Tier 1 (D9) and `nros::setup!` retired
  (D10) by the maintainer the same day.
