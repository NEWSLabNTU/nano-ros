# RFC-0106 — The executor waits until its next deadline, on one platform primitive

**Status:** Draft (2026-10-16). The mechanism is implemented and on `main`;
this RFC records the design that phase-436 built and, until now, held only in
its phase doc.

**Implemented by:** [phase-436](../roadmap/phase-436-poll-wake-revision-deadline-driven-executor.md)
(W1–W7, Paths A–E) and [phase-502](../roadmap/archived/phase-502-unbounded-condvar-wait-retirement.md)
(no condition wait without a deadline). **Sits beside:**
[RFC-0002](0002-rt-execution-model.md), which owns what the executor DISPATCHES
and in what order; this RFC owns how it WAITS between dispatches. **Applies:**
[RFC-0076](0076-platform-abi-ask-do-not-assume.md) to the wait: a port states
its granularity, the core never assumes one.

## Summary

An executor's idle time used to be bounded by one number, the caller's
`spin_once` timeout, narrowed by the backend's next internal event. It is now
the minimum over a set of declared deadlines: the caller's budget, the next
timer, the backend's next event, a supervised route's deadline, and any
deadline a port registers (smoltcp's `poll_delay`). The executor parks until
that minimum on ONE platform primitive, which data arriving can end early, and
it records which source won. A port declares how finely it can park, and the
executor rounds the bound UP to that, never down. Where a port or a backend
cannot support a park, the executor keeps the older wait, which is bounded by
the same minimum and never busy-loops.

## Motivation / problem

Four defects, one shape: the wait's bound came from whoever happened to call
`spin_once`, not from what the executor owed.

- A 10 ms timer slept through: `spin_default`'s 50 ms budget won, because no
  timer deadline narrowed it (issue 1192).
- `Duration::from_micros(500)` became a 0 ms wait through `as_millis()`, which
  is the non-blocking path, so `spin()` turned into a 100 % CPU loop with no
  warning (issue 1193).
- The jitter report judged wakes against a resolution the wait could not
  deliver: 1 ms was hardcoded where ThreadX ticks at 10 ms and POSIX waits to
  the nanosecond (issues 1194, 1242).
- A park nobody signals: an executor parked on its wake object while the
  backend signalled a different executor's, so the park slept out its whole
  bound with data waiting (phase-436 Path C, resolved by D6).

A real-time image has to be able to say, before it runs, when its executor
will wake and why. With one caller-chosen number that question had no answer.

## Design

### D1 — The bound is a minimum over declared deadlines

```text
bound = min(caller budget, next timer, backend next event,
            route deadline, every registered port source)
```

The caller's budget is one member, so the set is never empty and the park is
never unbounded. Each source answers in microseconds from now; `u64::MAX`
means "nothing pending" and does not shorten the park. A source that is
ASYNC (data arrival) contributes no deadline; it ends the park by signalling
instead (D4). A timer whose remainder is not wall time (ROS time) does not
bound the park, because the park waits in wall microseconds (issue 1321).

Ports register sources with `register_wake_source(next_fn, ctx)`; the table is
inline (`MAX_WAKE_SOURCES`), and registration past capacity is REFUSED, never
dropped, because a dropped source is a deadline the executor was told about
and then slept through.

### D2 — Many sources say WHEN; one primitive WAITS

```rust
pub type NextDeadlineFn = unsafe extern "C" fn(ctx: *mut c_void) -> u64;
pub type ParkUntilFn = unsafe extern "C" fn(ctx: *mut c_void, deadline_us: u64) -> i8;
executor.register_wake_source(next_fn, ctx)?;          // many
executor.set_park_primitive(park_fn, ctx, granularity_us); // one
```

A park answers `0` (signalled), `1` (deadline expired) or negative (cannot
park). A negative answer is NOT a park: the spin falls back to the wait it
would have had with nothing installed. Counting it as one would drop the
transport drain to non-blocking and loop hot, which is exactly what a Rust
port's trait default (`-1`) would otherwise cause.

When a park has run, the transport is drained non-blockingly. A park followed
by a blocking drive would sum the two and run the loop at up to twice its
cadence (#648's trap).

### D3 — The port declares its granularity; the bound rounds UP

`nros_platform_wake_park_granularity_us()` is the finest park the port can
express: a tick on an RTOS (`ceil(1e6 / configTICK_RATE_HZ)` on FreeRTOS,
10 ms on ThreadX's default 100 Hz), `clock_getres(CLOCK_MONOTONIC)` on POSIX
and NuttX. The executor rounds the bound up to it and records both the
requested and the achieved bound. A 1500 us request on a 1 ms port is
reported as achieving 2000 us, so the jitter monitor's yardstick and the wait
agree.

The declared granularity counts only while the park is USABLE (D5). An
executor that cannot park waits in whole milliseconds, so it reports 1 ms; a
microsecond granularity there would round the bound to microseconds that the
millisecond wait then truncates to zero.

### D4 — The park waits on the executor's OWN wake object

The backend's arrival callback signals the executor's wake object. A park on
any other object cannot be ended by data, and because the drain is
non-blocking after a park, it sleeps its full bound with a message already
waiting: worse than not parking, invisible in a quiet run, and passing every
test. So every installer hands the park the executor's own object
(`nros::port_park::install_port_park`, `nros_cpp_executor_wake_handle`), and
a build with no wake object installs no park.

### D5 — Park only when data can end the park

The executor parks only when its backend has a LIVE wake callback
(`async_wake_live()`). Otherwise it keeps the receive-blocking wait: it drives
the transport for the same bound, in the transport's own `recv`. That is the
right wait for a poll-only backend, not a degraded one.

- **XRCE** has no reader thread: the executor's own `drive_io` is what reads
  the transport, so a receive that blocks until the bound IS this backend's
  park, ended early by the socket rather than by a callback. A wake callback
  would need a thread nobody else wants. The cost is a millisecond
  granularity, which D3 now reports honestly.
- A park on such a backend would be a blind sleep, and a reliable stream whose
  ACK lands in a session nobody drives stalls (Phase 127.C.4).
- **A guard condition cannot end that wait.** A trigger from another thread
  signals the wake object, and nobody waits on it while the executor sits in
  the backend's `recv`. So while a guard condition exists, the transport wait
  is driven in slices of `POLL_ONLY_WAKE_SLICE_MS` (10 ms) and stops at the
  first slice that ends with the wake flag set. Measured on XRCE: 979 ms from
  trigger to dispatch in a 1 s spin before, about 5 ms after. An executor
  with no guard condition keeps the single blocking drive.

### D6 — One session wakes every executor over it

A backend session has ONE wake-callback slot, and several executors can share
a session (a tiered image's spawned tiers, several C executors over one
`rclc_support_t`). The slot holds a fan-out (`executor/wake_fanout.rs`): a
static pool entry keyed by the session whose callback signals every member.
Each executor joins on install and leaves on teardown; the callback comes off
the backend with the session's owner or its last member, after any callback
in flight. Cost: every arrival wakes every member, because the callback does
not say whose data arrived. Waking only the owner needs per-entity routing in
each backend, which no backend offers today.

### D7 — An optional platform symbol is reached WEAKLY

`nros_platform_wake_park_until_us` and `..._granularity_us` are OPTIONAL in
`platform.h`: Zephyr, ThreadX, FreeRTOS and POSIX (so Linux and NuttX) define
them; bare metal does not. Two kinds of installer, two rules:

- A board crate's entry links one known port, so it references the symbols
  plainly (`install_park` in each board, the Zephyr `zephyr_component_main!`).
- Shared C and C++ entry code is linked beside ANY port, so it references them
  WEAKLY and installs only when both resolve and the granularity is non-zero
  (`rclcpp::board::detail::install_platform_park` in `<rclcpp/detail/main.hpp>`, and the C runner's
  copy in `nros_rtos_run_components.c`). Zero granularity is the trait default
  a Rust-implemented port exports with the symbol; it means "no park".

The Rust `nros-platform-cffi` shim deliberately does not forward the two
symbols, because it is linked into every C port and forwarding made an
optional symbol a required one.

### D8 — Every entry installs the park, not just the tier runners

The tier runners and the single-executor entries are the same wait, so they
get the same park: Rust board entries (`run_entry`, `boot_hosted`, ThreadX's
single entry, `zephyr_component_main!`), the C runner
(`nros_board_rtos_run_components`) and C++ `run_components` (through
`detail::component_spin_loop`). A port with no park keeps the millisecond
path.

### D9 — Attribution

`last_park()` returns the bound and the source that won it (`CallerBudget`,
`Timer`, `Session`, `Platform(n)`, `RouteDeadline`); `last_park_achieved_us()` the
rounded value. "Why did the executor wake" is a number, not a guess. The
dispatch that follows a park is user code and is MEASURED, never bounded: the
budget is a floor on the wait, not a bound on the call.

## Coverage (2026-10-16)

| Port | Park | Granularity | Deadline sources |
|---|---|---|---|
| Zephyr | `k_sem_take(K_USEC)` | kernel tick | — |
| ThreadX | `tx_semaphore_get` | tick (10 ms default) | — |
| FreeRTOS | `xSemaphoreTake`, ceiling ticks | tick | — |
| POSIX (Linux), NuttX | `sem_clockwait` on MONOTONIC | `clock_getres` | — |
| ESP-IDF | shares FreeRTOS's code | — | never built in-tree |
| Bare metal | none (no deadline-armable timer) | 1 ms fallback | smoltcp `poll_delay` (mps2-an385, esp32-qemu) |

| RMW | Wakes the executor | Wait |
|---|---|---|
| zenoh | subscription, query and reply arrival | park |
| Cyclone DDS | data-available listener | park |
| XRCE | no callback (poll-only) | receive-blocking, same bound, 1 ms; 10 ms slices while a guard condition exists (D5) |
| uORB | no callback through this slot | receive-blocking, same bound |

## Open questions

- **Bare metal (phase-436 C3).** Not a park: every bare-metal backend is
  poll-only, so D5 never parks there. What remains is a tickless `wfi` inside
  the transport's wait, armed on its deadline, and it needs a NIC receive
  interrupt first; without one it is D5's blind sleep one layer down.
- **Per-entity wake routing.** Removes D6's spurious wakes; needs each backend
  to say whose data arrived.
- **Deadline sources on hardware.** smoltcp's is the only port source. It
  wins a park on QEMU when the budget leaves it room (phase-436 Path B's
  exit); no hardware run yet.

## Changelog

- 2026-10-16 — D5 gains the sliced drive for a guard condition on a poll-only
  backend (measured, fixed, tested). Bare metal's open question is reframed:
  it is a transport idle, not a park. Path B's exit met on QEMU.

- 2026-10-16 — Filed from phase-436's design text, with D5's poll-only
  reading, D7's weak-reference rule and D8 (single-executor entries) added in
  the same change.
