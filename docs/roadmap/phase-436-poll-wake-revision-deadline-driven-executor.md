# Phase 436 — The poll/wake revision: a deadline-driven, platform-agnostic executor

**Status (2026-09-07). W1-W6 IMPLEMENTED; W7 — wiring a port through the seam
— OPEN and prerequisite-blocked.** The executor computes a park deadline as a
`min` over registered sources and hands it to a platform primitive. **No port
registers anything through it yet.**

Four things the work changed about the phase as written:

* **W1-W5 met the behaviour and missed the seam.** They made the park
  deadline-driven, rounded and attributed — and hardcoded the source set, so
  nothing could contribute `k_poll`, `epoll`, a queue set or a device ISR. The
  phase is named for that extension point and it did not exist until W6,
  through five green work items and a full test suite. Behaviour is not
  architecture, and a passing suite does not tell them apart.
* **The porting axis is C entry vs Rust entry, not per-RTOS.** ThreadX, bare
  metal and Zephyr's Rust arm hold a typed `Executor`; FreeRTOS, NuttX and
  Zephyr's C arm hold an opaque `void*` with no export to call — including the
  arm the ASI lane takes.
* **`park_granularity_us()` is wrong in both directions** (issue 1242), and
  W3's jitter-granularity report inherits the error — the failure W3 exists to
  prevent, reproduced one layer below W3 by its own dependency.
* **W5's issue was wrong about its own evidence.** Issue 1196 said the
  unbounded `condvar_wait` had no callers; it has two. The grep behind that
  claim searched `nros_platform_cond_wait`; the symbol is
  `nros_platform_condvar_wait`.

Deferred deliberately: the `wake_wait_ns` platform slot (the rounding contract
fixes issue 1193's harm without making five ports grow a required symbol), and
making the `handles.rs` wait loops wake off the backend's listener rather than
a fixed grid (issue 1195's larger half).

## The one change this phase is about

Today the executor asks:

> **How long am I allowed to block?**

The answer comes from one place — the caller's `timeout` argument — and is then
narrowed by exactly one other input, `session.next_deadline_ms()`.

It should ask:

> **When is the next thing I owe anybody?**

The answer is a `min` over a set of *declared deadlines*, of which the caller's
budget is only one member. That is the whole design. Everything below is a
consequence:

* a timer deadline can shorten the sleep (issue 1192),
* the wake time is a function of declared periods rather than an arbitrary
  caller constant, so it is **analyzable** — you can enumerate the sources and
  their periods without running anything,
* the bare-metal case stops being a degraded mode and becomes the same
  mechanism with a smaller source set,
* "no busy wait" becomes structural rather than incidental: there is always a
  deadline to park until, even when it is far away.

## What is already right, and must not be rebuilt

The review found the wake machinery in good shape. This phase extends it; it
does not replace it.

* **`NodeWake` is already a platform-agnostic waker.** `nros_platform_wake_{wait_ms,signal,signal_from_isr}`
  is implemented on five ports — Zephyr (`k_sem_take`/`k_sem_give`), ThreadX
  (`tx_semaphore_get`), FreeRTOS, POSIX, ESP-IDF — each with an **ISR-safe**
  signal variant. The core never names an RTOS.
* **Selection is a runtime probe, not a `cfg`.** `has_async_wake` comes from
  `supports_wake_callback()` (`spin.rs:2994`), which for C backends is
  "the vtable slot is non-NULL" (`cffi/src/lib.rs:2591`). A port that gains an
  async source gets the fast path without a core change. This is the extension
  point the phase builds on.
* **Degradation is already correct in kind.** No wake primitive, or a poll-only
  backend, means blocking in the transport's own `recv` for the full timeout —
  not a spin.
* **`spin_period` refuses to run without a clock** (`spin.rs:8343`, issue 0709)
  rather than silently free-running. That is the standard this phase applies to
  the remaining degradations: refuse, or declare, but never silently degrade.
* **Priority inheritance holds on all three RTOSes.** ThreadX `TX_INHERIT`
  (`platform.c:611`), Zephyr `k_mutex`, and — checked because recursive mutexes
  are a common exception — FreeRTOS recursive mutexes do inherit, via
  `xQueueTakeMutexRecursive` → `xQueueSemaphoreTake` → `xTaskPriorityInherit`
  (`third-party/freertos/kernel/queue.c:1791`). No work item here.

## The design

### 1. One time base, nanoseconds, no truncation

`spin_once` currently does `timeout.as_millis()` — a **truncation**
(`spin.rs:6374`). `Duration::from_micros(500)` becomes `0`, which selects the
non-blocking path and turns `spin()` into a 100 % CPU loop with no warning
(issue 1193). The floor is real: all five `nros_platform_wake_wait_ms` slots
take `uint32_t timeout_ms`, so sub-ms cannot reach the primitive on any target.

The revision:

* carry `u64` nanoseconds from the public API to the platform slot,
* add `nros_platform_wake_wait_ns`, keeping `_ms` as the fallback for ports
  whose primitive is genuinely ms-granular,
* add `nros_platform_wake_granularity_ns()` — each port **declares** what it
  can achieve,
* round deadlines **up** to that granularity, never down, and expose the
  achieved value.

The last point is what converts issue 1194 from a defect into a contract. A
1500 us request on a 1 ms-granular port is then reported as *achieving* 2000 us,
rather than silently waiting 1 ms while the jitter rule judges against 1500.
A monitor whose yardstick and whose mechanism disagree cannot support a safety
claim; declaring the rounding makes them agree.

### 2. The wake-source seam: many sources say WHEN, one primitive waits

That split is the design. Conflating them is what produced W1's first shape,
which hardcoded its sources and left no way for a port to contribute one.

```rust
/// When this source will next need the executor, microseconds FROM NOW.
/// `u64::MAX` = nothing pending.
pub type NextDeadlineFn = unsafe extern "C" fn(ctx: *mut c_void) -> u64;

/// Park until `deadline_us` from now, or until something signals.
/// 0 = woken by an event, 1 = deadline expired, <0 = cannot park.
pub type ParkUntilFn = unsafe extern "C" fn(ctx: *mut c_void, deadline_us: u64) -> i8;

pub const MAX_WAKE_SOURCES: usize = 8;   // inline table, no allocator

executor.register_wake_source(next_fn, ctx)?;   // many
executor.set_park_primitive(park_fn, ctx);      // one
```

`WakeSourceId` names the winner: `CallerBudget`, `Timer`, `Session`,
`Platform(u8)`.

An **async** source contributes `u64::MAX` and instead breaks the park by
signalling — which `nros_rmw_runtime_wake_cb` already does. So async and
deadline sources compose without a second mechanism.

Registration **refuses past capacity** rather than dropping silently: a source
quietly discarded would leave the executor sleeping past a deadline it had
been told about, which is the failure this phase exists to remove.

`Session` is a **candidate, not a pre-cap**. Folding the backend deadline into
the budget upstream meant a park the session won was attributed to
`CallerBudget` — the one number that explains a wake named the wrong source.

#### Per platform — surveyed, not assumed

Four parallel read-only surveys, one per port domain. The table below replaced
an earlier one written from a single reading; three of its rows were wrong.

**The real axis is not per-RTOS. It is C entry vs Rust entry.**

| Port | Entry holds a typed `Executor`? | Park primitive available | Native tick |
|---|---|---|---|
| **ThreadX** | **Yes** — `nros-board-threadx/src/entry.rs`, via `executor_mut()` | `tx_semaphore_get`, contract already `0/1/<0` | **10 ms** (`TX_TIMER_TICKS_PER_SECOND=100`) |
| **Bare metal** | **Yes** — `nros-board-mps2-an385/src/entry.rs` `boot()` | none — see below | free-running counters only |
| **Zephyr** | **Both** — Rust `entry_tiers.rs` yes; C `zephyr_run_tiers.c` no | `k_sem_take`, and `k_timeout_t` accepts `K_USEC` | 1 ms via the ABI, finer in the kernel |
| **FreeRTOS** | **No** — opaque `void*` behind `nros_cpp_*` | `xSemaphoreTake`, contract already `0/1/<0` | 1 ms (`configTICK_RATE_HZ=1000`) |
| **NuttX** | **No** — same opaque pattern | inherits POSIX | POSIX |
| **POSIX** | n/a — `nros_board_native_run_tiers` | `sem_timedwait` / `pthread_cond_timedwait`, **timespec-native** | nanosecond-capable |

**There is no `nros_cpp_executor_register_wake_source` or
`..._set_park_primitive` C-ABI export.** So FreeRTOS, NuttX and the C arm of
Zephyr — which is the arm the ASI FVP lane actually takes — cannot reach this
seam at all without new FFI surface. ThreadX, bare metal and Zephyr's Rust arm
can wire directly today.

**No port multiplexes.** `k_poll` does not appear anywhere (the single grep hit
is `nros_platform_network_poll`, a name collision with a no-op body). FreeRTOS
queue sets are unused and `configUSE_QUEUE_SETS 0` where set at all. ThreadX
event flags exist but only as zenoh-pico's task-join signal. Every port waits
on a socket the same way: blocking `recv` with `SO_RCVTIMEO`. So the "natural
extra sources" a first reading suggests are aspirations, not wiring waiting to
be connected.

#### The deadline sources that do exist

* **Zephyr has a one-shot timer facility, and it is dead code.**
  `nros_zephyr_timer_create_oneshot(timeout_us, cb, user_data)` and its
  periodic sibling, at `zephyr/nros_platform_zephyr_shims.c:220-278`, take
  **microseconds** and wrap `k_timer`. Written for Phase 110.E.b sporadic-server
  refill; **zero callers tree-wide**. That is the shape a `NextDeadlineFn`
  needs, already built and already paid for.
* **smoltcp could contribute one and does not.** `Interface::poll_at()` returns
  when the stack next needs servicing — the natural deadline source. It has
  **zero hits** in this repo; `SmoltcpBridge::poll` calls `iface.poll()` and
  discards the timing.

#### Bare metal: the argument, corrected

The design's justification was that `park_until(deadline)` lets a board arm a
timer compare before `wfi`, which `sleep(duration)` cannot express. That
argument stands. **What does not stand is the implication that any board here
can do it today.**

No bare-metal board in this tree has a one-shot, deadline-programmable timer:

* **MPS2-AN385** — CMSDK Timer0 as a free-running down-counter,
  `RELOAD = 0xFFFF_FFFF`, **no `IRQEN`**, compare register never written.
* **STM32F4** — DWT cycle counter, no match register ever written.
* **ESP32-QEMU** — clock read only; no compare or alarm facility.

The one interrupt-driving timer that exists is MPS2's RTIC `arm_tick_timer`,
a **fixed 1 kHz periodic tick** armed once at init to bound `wfi` yield
granularity. That is a period, not a deadline.

So an earlier sketch in this document showing

```rust
arm_timer_compare(deadline_us);   // ← no such function exists, on any board
cortex_m::asm::wfi();
```

presented as available something that must be **written from scratch, per
board**. The hardware supports it; the code does not exist. Recording that
plainly, because "a slot exists, therefore it works" is the overstatement this
phase has now made twice.

Worse than assumed, too: `sleep_ms` in `nros-baremetal-common` **always**
busy-loops on the clock condition, with `wfi` an optional per-iteration yield
rather than a block — and the default MPS2 non-RTIC boot never registers the
idle hook at all, so it spins with no yield whatsoever.

#### POSIX does not need this

POSIX already blocks in `NodeWake::wait_ms` → `sem_timedwait`, bounded by the
same min-of-sources. Installing a `ParkUntilFn` there would be redundant unless
paired with a microsecond primitive **and** a non-hardcoded granularity
(issue 1242). POSIX being well served is the design working, not a gap.

The seam is **additive**: with no primitive installed the path is unchanged and
every existing port keeps the behaviour it has. When one IS installed the
transport drain drops to non-blocking, because a park plus a blocking drive
would sum to up to twice the intended cadence — the same trap as passing a
period to `spin_once` instead of declaring it (#648).

### 3. The wait

```
deadline = min over sources of next_deadline_ns()      // never empty:
                                                       // CallerBudget is a member
park_until(deadline)                                   // platform primitive
drive_io(0)                                            // drain what arrived
```

Three-tier realisation, chosen at runtime from the platform probe:

| Port capability | Primitive | Result |
|---|---|---|
| Async wake **and** timed park | `wake_wait_ns` | Parks until deadline **or** signal. |
| Timed park only | `platform_sleep_ns` then `drive_io(0)` | Parks until deadline. |
| Neither (bare metal) | `WFI`/`WFE` + timer compare | Parks until interrupt or deadline. |

No row busy-waits. That is the point: "avoid busy waiting" stops being a
property each port has to remember and becomes a property of the only wait the
core knows how to perform.

### 4. Analyzability

Two additions, both cheap, both turning runtime behaviour into data:

* **Attribution.** Record which source won each park (a `u8` index plus a
  per-source counter). "Why did we wake" becomes a number instead of a guess,
  and it feeds the existing jitter and execution-time reporting.
* **A stated bound.** `spin_once`'s duration is `park + scan + dispatch`. Park
  is bounded by construction. Scan is `O(entries)`. **Dispatch is user code and
  the executor cannot bound it** — it can only measure it, which the
  execution-time high-water probe already does. The contract should say this
  plainly: *the budget is a floor on the wait, not a bound on the call.*
  Today `spin_period` silently absorbs an overrun by skipping its sleep, so a
  tier that chronically overruns looks fine and shows up only in the jitter
  counter.

## The `spin*()` API surface

The review's second half. The surface has grown three units and two meanings.

### Two meanings wearing one shape

```rust
executor.spin_once(timeout);   // timeout = a BLOCKING BOUND
executor.spin_period(period);  // period  = a CADENCE
```

Identical shape, different quantity. This is not hypothetical: nros-cpp's tier
loops paced with `platform_sleep_us(period_us)` while passing a hardcoded
`spin_once(…, 10)`, so `release-jitter-runtime` judged every tier against 10 ms
whatever the contract declared — a 1 kHz tier could run nine periods late and
register as on time. Fixed by making the cadence **declared**
(`set_spin_nominal_us`) rather than inferred from the timeout. That fix is
step one of this phase; the rest is making the distinction impossible to
confuse again, rather than merely corrected in two call sites.

### Three units

| API | Accepts | Reaches the wait as |
|---|---|---|
| `rclc_executor_spin_period` (C) | **nanoseconds** (`executor.rs:2960`) | ms |
| `nros::spin_once` (C++) | **milliseconds** (`nros.hpp:88`, `int32_t timeout_ms = 10`) | ms |
| `Executor::spin_once` (Rust) | `Duration` | ms |

The C API advertises nanoseconds and paces itself correctly in nanoseconds
(`invocation_time_ns += period_ns`, `sleep_ns`) — then truncates at the wait.
The C++ API cannot express sub-millisecond at all. §1 collapses this to one
unit.

### What the usage sites teach

* **The generated workspace entry is the good shape.**
  `actuation_entry_nros_main_generated.cpp` declares the cadence as *data* —
  `NativeTierSpec{ …, 5000ull, … }` — and calls `run_tiers`. It never calls
  `spin_once`. The cadence is declared, the mechanism is the runtime's choice,
  and a resolver can check it before the image is built. **This is the model
  the hand-written paths should converge on**, and the reason the tier-loop bug
  was invisible: the generated entry was right, and the runtime beneath it was
  not.
* **Hand-written C is the best of the three spellings.**
  `rclc_executor_spin_period(&app.executor, 100000000ULL)` — ns, absolute
  deadline accumulation, no drift.
* **Hand-written C++ taught the wrong idiom.** `while (…) { nros::spin_once(100); }`
  across ten examples: a bare bound, no declared cadence, no sleep. `nros::spin()`
  hardcodes `10` (`nros.hpp:176`). These are what the tier loops were modelled on.
* **The bridges sit exactly on the truncation floor.**
  `exec.spin_once(Duration::from_millis(1))` — one step finer and it busy-loops
  (issue 1193).

### Direction

Make the two quantities syntactically distinct rather than documented apart —
a cadence is declared once (as the generated entry already does) and a bound is
passed per call. Keep `spin_once(bound)` as the primitive; every cadence-shaped
wrapper should route through a declaration, so that "which number is this?"
cannot be answered wrongly by a caller who never reads the doc comment.

## Work items

Each is a filed issue; the issue holds the evidence.

* **W1 — [issue 1192](../issues/1192-executor-wait-ignores-next-timer-deadline.md):
  the wait is not bounded by the next timer deadline.** The `TimerSource` of
  §2, and the highest-value item — it is the one place the executor sleeps past
  a deadline it owns. Distinct from resolved issues 0505 (overrun policy) and
  0515 (grid quantization), both of which take the spin boundary as fixed;
  this moves the boundary.
* **W2 — [issue 1193](../issues/1193-spin-timeout-truncates-to-milliseconds.md):
  sub-ms timeouts truncate to a busy loop.** §1. Latent today — nros-cpp clamps
  `max(1_000)` and ASI runs `spin_period_us = 5000` — but `spin_period_us` is a
  microsecond field accepting values it cannot honour.
* **W3 — [issue 1194](../issues/1194-jitter-measured-in-microseconds-waited-in-milliseconds.md):
  jitter measured in µs, waited in ms.** Falls out of W2; the deliverable is
  the declared-granularity contract, not a coarser statistic.
* **W4 — [issue 1195](../issues/1195-promise-wait-polls-instead-of-using-the-waker.md):
  `Promise::wait` polls a 10 ms grid and overshoots shorter timeouts.** The one
  place the waker exists and is bypassed.
* **W5 — [issue 1196](../issues/1196-platform-condvar-wait-is-unbounded.md):
  the unbounded `condvar_wait` in the platform API.** Latent — no callers on
  the executor path — but "no unbounded wait" is only a system property if it
  is an API property.
* **W6 — the wake-source seam itself. DONE.** W1–W5 delivered the BEHAVIOUR —
  a deadline-driven park, rounded up, attributed — but hardcoded the sources,
  so the extension point this phase is named for did not exist. W6 adds
  `register_wake_source` / `set_park_primitive`, widens `WakeSourceId` with
  `Session` and `Platform(u8)`, and makes the backend deadline a candidate
  rather than a pre-cap. No issue: this is the phase's own design, not a
  defect found in the field.

* **W7 — wire a port through the seam. OPEN, and blocked on two prerequisites.**
  W6 built the extension point; nothing extends it. Four parallel port surveys
  established what each port needs, and produced two items that are not
  per-port work:

  * **W7.a — a C-ABI surface for the seam.** There is no
    `nros_cpp_executor_register_wake_source` / `..._set_park_primitive` export,
    so FreeRTOS, NuttX and Zephyr's C arm cannot reach it. That arm is the one
    the ASI FVP lane takes, so this gates the lane we ship. ThreadX, bare metal
    and Zephyr's Rust arm need none of it.
  * **W7.b — [issue 1242](../issues/1242-park-granularity-is-hardcoded-not-queried.md).**
    `park_granularity_us()` is a hardcoded 1 ms: wrong high on ThreadX (10 ms
    tick), wrong low on POSIX (timespec-native), and it never consults the
    installed primitive — so a port supplying a microsecond `ParkUntilFn` still
    cannot produce a sub-millisecond park. Until this is fixed, wiring any port
    buys nothing it does not already have.

  Then the per-port work, cheapest first:

  * **Zephyr deadline source** — `nros_zephyr_timer_create_oneshot` already
    takes microseconds, already exists, and has zero callers. The smallest real
    `NextDeadlineFn` available.
  * **ThreadX and Zephyr-Rust park primitives** — both entries hold a typed
    `Executor`, and both `wake_wait_ms` implementations already return the
    `0/1/<0` contract `ParkUntilFn` wants. Adapters are a unit change.
  * **smoltcp deadline source** — capture `Interface::poll_at()`, which the
    bridge currently discards.
  * **Bare-metal park primitive** — the largest, and the only one needing new
    hardware code: a one-shot compare, per board, that exists nowhere today.

## Sequencing

W2 before W3 (W3's contract needs W2's granularity probe). W1 is independent
and should go first — it is the largest real-time win and touches only the
core. W4 and W5 are independent of everything.

## What this phase does not do

* It does not make dispatch preemptive. Within one executor, dispatch stays
  non-preemptive; multi-tier preemption comes from the OS scheduler via
  `open_threaded`, unchanged.
* It does not bound user callbacks. It measures them and states that they are
  unbounded.
* It does not change `spin_period`'s drift compensation, which is already
  correct (absolute `next_us` accumulation, not `now + period`).
