---
rfc: 0113
title: "Every supported way to run nano-ros keeps the real-time promise; one wait serves every platform and RMW"
status: Draft
since: 2026-10
last-reviewed: 2026-10
implements-tracked-by: [phase-521]
amends: [RFC-0106, RFC-0112]
supersedes: []
superseded-by: null
---

# RFC-0113 — The real-time promise, and the one wait that keeps it

**Status:** Draft (2026-10-31), approved by the maintainer the same day. Written before any code. It sets a support
rule that outranks RFC-0112's tiers, and it gives the executor one way of
waiting for every platform and every RMW. Implemented by
[phase-521](../roadmap/phase-521-rt-unified-wait.md).

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
  only be noticed between 10 ms slices (`POLL_ONLY_WAKE_SLICE_MS`). Neither is
  a bound a real-time tier can be designed against.
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
| uORB on PX4 | Notify through the existing `SubscriptionCallback` glue (`px4_callback_glue.cpp`) |
| uORB polling build (`NROS_RMW_UORB_BUILD_PX4_GLUE=OFF`) | none: refused under D5, kept only as a host test double |

### D5 — No silent fallback

A backend × transport × platform combination with no readiness form is
REFUSED when the session opens, naming the combination and this RFC. It is
never served by blocking in `recv`. The receive-blocking wait, its 10 ms guard
slices and `POLL_ONLY_WAKE_SLICE_MS` retire once the last in-tree combination
has a form (phase-521 W6). Until then they stay, and each remaining
combination is listed in the RFC's coverage table as not yet meeting D2.

### D6 — Deadlines name real events

A backend's `next_deadline_ms` reports a deadline only when the executor has
work to do at it.

- **zenoh, THREAD mode:** `None`. zenoh-pico's own executor thread runs the
  lease and keep-alive.
- **zenoh, CALLER mode:** the zenoh-pico executor's next wake-up time
  (`next_wake_up_time` in `src/runtime/executor.c`), reached through a `zpico`
  accessor, because `zpico_spin_once` is what runs those futures.
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
| the Embassy recipe (`book/src/user-guide/embassy-integration.md`) and the async section of `nros::guide::services` | — |
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

### D10 — `nros::setup!` retires

`nros::setup!` (phase-520) emits an image's assembly for a host that owns its
loop, which is Shape B. It is deleted with the rest of D7. A Tier 1 framework
gets its assembly from its own generated entry (D9 for PX4), never from a
host-callable function.

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

- **Fan-out bound.** D2.5's fan-out on non-attributing backends has no stated
  bound yet. Phase-521 W7 measures it.

## Changelog

- 2026-10-31 — Filed. PX4 decided as Tier 1 (D9) and `nros::setup!` retired
  (D10) by the maintainer the same day.
