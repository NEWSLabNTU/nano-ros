# Phase 436 — The poll/wake revision: a deadline-driven, platform-agnostic executor

**Status (2026-10-24). DONE — archived.** Every nano-ros item has landed or
moved; what is left is not this repository's work:

* **Landed:** W1-W7, A2 (with A2b and A2c), A3's accessor, B1 (decided), B2,
  C1, C2, D1, D2, E1, E3, E4 and E5. E5's heading below still reads "OPEN,
  next"; its nano-ros half landed on 2026-10-13 (`a53733c48d`), as the
  "LANDED" paragraph under it records. The homed issue,
  [#1041](../../issues/archived/1041-timer-missed-deadline-policy-differs-from-rcl.md),
  is archived.
* **The safety island's side:** A1 (re-pin), A3's loaded FVP run and E5's exit
  (the FVP re-measure, then `CONFIG_INIT_STACKS` back on) are changes in the
  island's repository, tracked in its
  `docs/roadmap/phase-11-executor-evidence.md`. They wait on the island
  porting off `ComponentNode`.
* **Silicon:** Path B's board run and A3's absolute latency are
  [phase-512](../phase-512-on-hardware-measurements.md) H2 and H1.
* **C3, bare-metal tickless idle:** deferred, filed as
  [issue 2022](../../issues/archived/2022-bare-metal-tickless-idle-needs-nic-rx-irq.md).
  It buys power, not latency, and needs a receive interrupt `lan9118-smoltcp`
  does not have.
* **Per-entity wake routing** (checkpoint item 5) was folded into
  [phase-397](../phase-397-pushed-readiness-executor.md), which landed it.

The design is still the executor's: [RFC-0106](../../design/0106-executor-park.md)'s
park and wake-source seam is what every entry and tier runner installs, and
what [phase-509](phase-509-executor-preemption-levels.md)'s band workers park
on (`executor/port_park.rs`, `executor/scheduler.rs`). One line of "What this
phase does not do" is superseded: dispatch within one executor is no longer
only non-preemptive, because phase-509 (RFC-0108) adds preemption between
levels.

**The design now lives in [RFC-0106](../../design/0106-executor-park.md).**
This doc keeps the work breakdown and the evidence; the sections below that
argue the design are the record of how it was reached.

**Checkpoint (2026-10-16), for whoever picks this up next.** On `main`: W1-W7
(#849), A2 (#871), E3 + E4 (#881), A3's accessor (#899), B2 (2026-09-25), E5
(2026-10-13), and on 2026-10-15 A2b, A2c, C1, C2, D1, D2's ABI half and the
wake fan-out. Issue 1196 is resolved by
[phase-502](phase-502-unbounded-condvar-wait-retirement.md). What
remains, in the order I would take it:

1. **Single-executor entries park too — LANDED 2026-10-16.** The tier runners
   installed the park and the single-executor entries did not, so the most
   common embedded path kept the millisecond `wake_wait_ms` floor. See
   "Only the TIER runners install a park" under Path C.
2. **A poll-only backend reports the wait it has — LANDED 2026-10-16.** XRCE
   needs no wake callback: the executor's own `drive_io` reads its transport,
   so a receive bounded by the park's deadline IS its park (RFC-0106 D5). What
   was wrong was the granularity: an installed microsecond park still counted
   on an executor that could not use it. See Path C.
3. **ASI items, blocked on ASI's side.** E5's re-measure on the FVP image,
   then `CONFIG_INIT_STACKS` back on (E5 under Path E). A3's loaded run: ASI's
   controller derives from `ComponentNode`, which phase-427 W4 deleted, so ASI
   has to port to the one node type before it can pin past `af14492fe`; the
   readout waits on ASI branch `wip/a3-rt-probe-readout`, and ASI's
   `docs/roadmap/phase-11-executor-evidence.md` carries the ASI side.
4. **Path B's target run — MET on QEMU 2026-10-16.** With a live zenoh
   session and a 50 ms budget, mps2-an385 printed
   `park bounded by Platform(0) at 27000 us`; with the shipped 10 ms budget
   the source never wins. The hardware run moved to
   [phase-512](../phase-512-on-hardware-measurements.md) H2 on 2026-10-23. See
   Path B's exit.
5. **Per-entity wake routing — MEASURED 2026-10-16, folded into
   [phase-397](../phase-397-pushed-readiness-executor.md).** One zenoh session,
   an owner executor with a subscription and N tier executors over the same
   session with nothing registered, a publisher in another process at 1 kHz
   (native Linux, aarch64). Every idle tier spun 999-1000 times a second, at
   6.8-9.4 us of thread CPU per spurious wake, which is 0.7-0.9 % of a core
   per idle tier at 1 kHz. It is linear in arrival rate times idle members, so
   7 idle tiers cost about 5.6 % of a core; idle, every member spins once per
   second. A host number only: an MCU tier pays the same count at its own
   per-wake cost, unmeasured, and a woken higher-priority tier preempts the
   owner for that long.
   **Decision: no standalone ABI change here.** Routing needs the arrival to
   name its entity, and the entity its executor. That is RFC-0084's token,
   minted at entity-scoped wake registration, which phase-397 plans; the
   owning executor's context rides on the same registration, so the fan-out
   becomes the fallback for a backend that reports UNKNOWN. Doing it here
   first would add a second per-entity wake path that phase-397 then replaces.
   The probe also found
   [issue 1929](../../issues/archived/1929-zenoh-async-wake-is-one-process-global-slot.md):
   zenoh's arrival wake is one process-global slot, so with two zenoh sessions
   in one process the first executor is never woken by its own data
   (resolved 2026-10-16: the wake table is per session).
6. **C3, bare metal — REFRAMED 2026-10-16, still deferred.** It is not an
   executor park: bare-metal zenoh and XRCE are poll-only, so RFC-0106 D5
   keeps the receive-blocking wait there whatever park is installed. What C3
   can buy is a tickless idle INSIDE the transport's wait, and that needs a
   NIC receive interrupt before a one-shot compare is worth arming. See C3
   under Path C.
7. **A guard condition on a poll-only backend — FIXED 2026-10-16.** Measured
   on XRCE: a trigger 20 ms into a 100 / 500 / 1000 ms spin was dispatched
   79 / 480 / 979 ms later, the whole bound. The executor now drives a
   poll-only transport in 10 ms slices while a guard condition exists, and
   stops at the first slice that ends with the wake flag set: about 5 ms,
   bounded by one slice. `tests/xrce_guard_wake.rs` pins it (and fails at
   974 ms with the slicing disabled).

Two process lessons that cost time here:
- **When a parent PR merges, rebase its child before anyone queues it.** A
  queued branch is frozen (GH006), and a child still carrying the parent's
  commits goes UNMERGEABLE (#879, #899).
- **Parallel sessions share `~/.cargo`.** Any step that runs cargo can block
  for minutes on the package-cache lock; ASI's `build.sh --run` no longer
  does (ASI 0281ab1, 051178f).

**Status (2026-09-10). W1-W7 IMPLEMENTED for both entry shapes; the remaining
per-port work is DEADLINE SOURCES, and one of the two named below does not
exist under the name this doc gave it.** The executor computes a park deadline
as a `min` over registered sources and hands it to a platform primitive, and
three ports now install one: Zephyr's C arm (#792), then ThreadX and Zephyr's
Rust arm (#814). What no port yet does is *contribute a deadline* — every
installed primitive waits, none of them says when.

That gap was the honest reading of where this phase was. `set_park_primitive`
has three callers; `register_wake_source` had none outside tests until B2
(2026-09-25) gave it two — the mps2-an385 and esp32-qemu boards, contributing
smoltcp's `poll_delay`. The rest of this section is left as written, because
it is the argument that produced the seam.

Five things the work changed about the phase as written:

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
* **`park_granularity_us()` was wrong in both directions** (issue 1242), and
  W3's jitter-granularity report inherited the error — the failure W3 exists
  to prevent, reproduced one layer below W3 by its own dependency. Fixed in
  W7.b: the installed primitive declares its granularity and the report uses
  what the port states.
* **W5's issue was wrong about its own evidence.** Issue 1196 said the
  unbounded `condvar_wait` had no callers; it has two. The grep behind that
  claim searched `nros_platform_cond_wait`; the symbol is
  `nros_platform_condvar_wait`.
* **The one-shot timer this doc points at is the wrong SHAPE, not merely
  unwired.** `nros_zephyr_timer_create_oneshot` exists, takes microseconds and
  has zero callers, exactly as recorded below — but this doc also called it
  "the shape a `NextDeadlineFn` needs", and it is not. It takes
  `(timeout_us, cb, user_data)`: a CALLBACK timer. A `NextDeadlineFn` must
  answer *when*, not *call me later*. That is a design question still open,
  not an adapter waiting to be written.

Issues 1192, 1193, 1194 and 1242 are resolved and archived (W1-W3 landed via #849; 1242 via
W7.b). 1195 and 1196 stayed open after that stack, which delivered only part of each. Path
D1 closed 1195 on 2026-10-15; D2 took the unbounded symbol out of the ABI the same day, and
phase-502 closed 1196 over the zenoh-pico paths its 2026-10-15 design study found.

Deferred deliberately: the `wake_wait_ns` platform slot (the rounding contract
fixes issue 1193's harm without making five ports grow a required symbol) and
the bare-metal park primitive (Path C3). Everything else that remains is
grouped under [Follow-ups, by path](#follow-ups-by-path).

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
  `supports_wake_callback()` (`spin.rs:3823`), which for C backends is
  "the vtable slot is non-NULL" (`cffi/src/lib.rs:3142`). A port that gains an
  async source gets the fast path without a core change. This is the extension
  point the phase builds on.
* **Degradation is already correct in kind.** No wake primitive, or a poll-only
  backend, means blocking in the transport's own `recv` for the full timeout —
  not a spin.
* **`spin_period` refuses to run without a clock** (`spin.rs:10941`, issue 0709)
  rather than silently free-running. That is the standard this phase applies to
  the remaining degradations: refuse, or declare, but never silently degrade.
* **Priority inheritance holds on all three RTOSes.** ThreadX `TX_INHERIT`
  (`nros-platform-threadx/src/platform.c:668`), Zephyr `k_mutex`, and — checked because recursive mutexes
  are a common exception — FreeRTOS recursive mutexes do inherit, via
  `xQueueTakeMutexRecursive` → `xQueueSemaphoreTake` → `xTaskPriorityInherit`
  (`third-party/freertos/kernel/queue.c:1791`). No work item here.

## The design

### 1. One time base, nanoseconds, no truncation

`spin_once` **did** `timeout.as_millis()` — a **truncation**. (W2 landed;
the site is `spin.rs:7754`, which now carries the microsecond budget and the
note explaining what it replaced.) `Duration::from_micros(500)` becomes `0`, which selects the
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

**There was no `nros_cpp_executor_register_wake_source` or
`..._set_park_primitive` C-ABI export.** So FreeRTOS, NuttX and the C arm of
Zephyr — which is the arm the ASI FVP lane actually takes — could not reach
this seam at all without new FFI surface, while ThreadX, bare metal and
Zephyr's Rust arm could wire directly. *W7.a wrote that surface* — both
symbols are in `nros_cpp_ffi.h` (`:1846`, `:1881`) and
`nros-cpp/src/lib.rs` (`:4564`, and the park's neighbour), and
`zephyr_run_tiers.c:199` is the one C caller, for the PARK half. The DEADLINE
half of the C export still has no caller: B2's two are Rust boards.

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
  refill; **zero callers tree-wide**.

  It is NOT, however, the shape a `NextDeadlineFn` needs, as this line used to
  claim. It takes a callback and fires it; a `NextDeadlineFn` returns a
  deadline and blocks nothing. The same is true of the platform-ABI sibling
  `nros_platform_timer_create_oneshot` (`platform_timer.h:61`), which is
  implemented on five ports (esp-idf, freertos, posix, threadx, zephyr) and
  does have callers. Both are `call me later`; the seam wants `when`.
* **smoltcp could contribute one and does not.** `Interface::poll_at()` returns
  when the stack next needs servicing — the natural deadline source. It has
  **zero hits** in this repo; `SmoltcpBridge::poll` calls `iface.poll()` and
  discards the timing.

  *Done — B2, 2026-09-25.* `nros-smoltcp/src/deadline.rs` reads
  `Interface::poll_delay` (`poll_at` minus `now`, saturating, which is the same
  answer without the subtraction written twice) and the mps2-an385 and
  esp32-qemu boards register it. The driver cannot name an `Executor` — it sits
  BELOW `nros-node` — so it exports the raw C-ABI pair and the board joins them.

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

*Update after W7.b:* issue 1242 is met, so the second condition now holds and
the remaining reason to install a POSIX park is the `uint32_t timeout_ms`
floor. That is Path C2, and it covers NuttX for free.

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

* **W1 — [issue 1192](../../issues/archived/1192-executor-wait-ignores-next-timer-deadline.md):
  the wait is not bounded by the next timer deadline.** The `TimerSource` of
  §2, and the highest-value item — it is the one place the executor sleeps past
  a deadline it owns. Distinct from resolved issues 0505 (overrun policy) and
  0515 (grid quantization), both of which take the spin boundary as fixed;
  this moves the boundary.
* **W2 — [issue 1193](../../issues/archived/1193-spin-timeout-truncates-to-milliseconds.md):
  sub-ms timeouts truncate to a busy loop.** §1. Latent today — nros-cpp clamps
  `max(1_000)` and ASI runs `spin_period_us = 5000` — but `spin_period_us` is a
  microsecond field accepting values it cannot honour.
* **W3 — [issue 1194](../../issues/archived/1194-jitter-measured-in-microseconds-waited-in-milliseconds.md):
  jitter measured in µs, waited in ms.** Falls out of W2; the deliverable is
  the declared-granularity contract, not a coarser statistic.
* **W4 — [issue 1195](../../issues/archived/1195-promise-wait-polls-instead-of-using-the-waker.md):
  `Promise::wait` polls a 10 ms grid and overshoots shorter timeouts.** The one
  place the waker exists and is bypassed.
* **W5 — [issue 1196](../../issues/archived/1196-platform-condvar-wait-is-unbounded.md):
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

* **W7 — wire a port through the seam. Both prerequisites DONE; three ports
  park; no port yet contributes a deadline.**

  * **W7.a — a C-ABI surface for the seam. DONE.**
    `nros_cpp_executor_register_wake_source` (`nros-cpp/src/lib.rs:3703`),
    `nros_cpp_executor_wake_handle` (`:3737`) and
    `nros_cpp_executor_set_park_primitive` (`:3759`) are exported, so FreeRTOS,
    NuttX and Zephyr's C arm can reach the seam — the arm the ASI FVP lane
    takes.

    **It shipped broken and that is worth recording.** The three verbs were
    inserted between `nros_board_native_run_tiers`'s doc comment and its
    function. A doc comment IS an outer attribute, so the detached
    `#[cfg(all(rmw-cffi, env))]` bound forward:
    `nros_cpp_executor_register_wake_source` silently required `env`, the
    process-environment capability. None of FreeRTOS, NuttX or Zephyr's C arm
    build with it — **the verb was absent from exactly the images W7.a was
    written for**, and the contamination reached the generated C header. Fixed
    by placement only. This is the third instance of the same
    attribute-hijack class in this repo (cf. issue 0487): a mechanical
    insertion that anchors on the `pub fn` line and walks back past attributes
    without knowing doc comments are attributes.

  * **W7.b — [issue 1242](../../issues/archived/1242-park-granularity-is-hardcoded-not-queried.md).
    DONE.** `park_granularity_us()` now reports what the INSTALLED primitive
    declares (`park_granularity_declared_us`), and the port states it because
    only the port knows: ThreadX's 100 Hz tick is 10 ms, coarser than the ABI
    signature suggests; a timespec primitive is finer.

  Ports that now park, all three through the same seam:

  * **Zephyr, C arm (#792)** — `nros_platform_wake_park_until_us` /
    `..._granularity_us` in `nros-platform-zephyr/src/platform.c`
    (`k_sem_take` with `K_USEC`, granularity from
    `CONFIG_SYS_CLOCK_TICKS_PER_SEC`), installed by `zephyr_run_tiers.c`.
  * **ThreadX and Zephyr's Rust arm (#814)** — both via
    `nros::port_park::install_port_park`, which states the wake-object
    constraint once instead of per board.

  **A cost this took, named rather than absorbed.** W6's
  `[Option<WakeSourceSlot>; MAX_WAKE_SOURCES]` is 128 B in the executor
  header, which put `Executor` over the budget
  `the_executor_value_does_not_scale_with_the_knobs` guards. It has a named
  allowance there, so raising `MAX_WAKE_SOURCES` shows up as its own cost
  rather than eating the next feature's headroom.

  What W7 leaves open is grouped by path under
  [Follow-ups, by path](#follow-ups-by-path) below.

## Follow-ups, by path

W1-W7 built the mechanism. What remains falls into five paths that are
independent in code but not in value, so the order below is deliberate.

The one fact that shapes all of it: **nothing in this phase has been measured
on a loaded system.** Every item is unit-tested, and on the one system that
ships this executor — ASI — the probes that do run cannot be READ: the
spec-free rules fire, but only as a log line at a whole period late, and
nothing exports the measurements themselves. Path A exists to change that
before the other paths add more mechanism on top of machinery nobody has
watched run.

### Path A — Prove it (evidence). FIRST.

* **A1 — pin ASI to the phase-436 stack.** ASI pins `902ea135e`
  (2026-09-04), 851 commits behind main. Pin to this stack's head for the
  measurement work; re-pin to main once it merges.
* **A2 — make the jitter probe READABLE from a C entry. Accessors DONE; the
  cadence half is a design question, not the one-line call this said.**
  Nothing needs declaring: `release-jitter-runtime`, like timer-overrun and
  alive supervision, is spec-free. It judges against the spin cadence the
  caller already passes, so it runs on ASI as soon as the pin moves. (An
  earlier draft said a `MonitorSpec` was missing and "nothing arms the rule";
  only the rate and age rules take a declared table.) What was missing is a way
  out: a C entry's only signal was the violation line, at a whole period late.

  * **Delivered:** `nros_cpp_executor_release_jitter(handle, &max_us, &late,
    &total, &granularity_us)` and `nros_cpp_executor_last_park(handle,
    &bound_us, &achieved_us, &source, &platform_index)`, with `source` one of
    `NROS_CPP_WAKE_SOURCE_*`. Every output pointer is optional.
  * **Found while writing them:** `WakeSourceId`, `NextDeadlineFn`,
    `ParkUntilFn` and `MAX_WAKE_SOURCES` were never re-exported from
    `nros_node::executor`. `last_park()` returned a type no other crate could
    name or match on, and the two registration calls took types nobody could
    spell. Re-exported.
  * **Reversed: do NOT make the Zephyr C arm call `set_spin_nominal_us`.** This
    item said to, so the nominal would stop being inferred from the timeout.
    But `release_jitter_granularity_us()` reads a declared cadence as "the
    caller paces itself with a sleep finer than the wait" and reports 1 us. The
    C arm is paced BY the wait: `spin_once(period_ms)`, which since W7 is the
    tick-granular park. Declaring would make the report claim resolution the
    loop does not have, which is issue 1194's failure. The API couples two
    facts, the nominal to judge against and who paces the loop, and the C arm
    needs the first without the second. Until they are split, the inferred
    nominal (timeout == period) is the honest one there.
  * **A2b — LANDED 2026-10-15:** split the nominal from the pacing claim, so
    a wait-paced loop can declare its cadence without over-claiming.
    `Executor::set_spin_nominal_wait_paced_us` (C:
    `nros_cpp_executor_set_spin_nominal_wait_paced_us`) declares a cadence
    for a loop paced by the `spin_once` wait. It does not change what
    `set_spin_nominal_us` means, which still says the caller paces itself.
    The declared cadence becomes the period of the release grid that
    phase-474 I11 gave every `spin_once(timeout)` loop, in place of the
    timeout. The timeout still bounds each wait, so a loop that passes whole
    milliseconds for a 1.5 ms cadence wakes at 1 ms (not a release) and then
    parks the remaining 0.5 ms onto the grid point. The jitter is reported
    at the park granularity, as for an undeclared loop. No entry calls it
    yet. The RTOS tier loops pass the period as the timeout, so the two only
    differ where a period is not whole milliseconds or is raised to
    `SPIN_PERIOD_FLOOR_MS`. Moving those entries is a behaviour change on
    targets with no local run, so it is left to whoever owns them; on Zephyr
    that is the C arm ASI runs.
  * **Left for A3:** the ASI side printing these on a cadence.

* **A3 — a loaded FVP cross-check. IN PROGRESS: the readout is live on ASI,
  an idle A/B is measured, and the loaded run is next.** ASI's
  `controller_pkg::Controller` wrapper prints one `rt-probe:` line per
  window, behind `CONFIG_ASI_RT_PROBE_REPORT`, which `tracing_stats.conf`
  turns on. Four things were established before the loaded run, each changing
  what it can claim:

  * **The probe and the trace share a clock, so the comparison is
    self-consistent and not absolute.** With
    `CONFIG_TIMER_HAS_64BIT_CYCLE_COUNTER=y` on the ARM arch timer, the
    executor's clock is `k_cycle_get_64()` and CTF stamps events with
    `k_cycle_get_32()` (`ctf_top.h:55`), both CNTVCT. ASI's
    `rt_evaluation_zephyr.rst` records that the FVP advances that counter
    during WFI at a rate unrelated to elapsed time, and that the image is SMP
    while CTF events carry no CPU id. So A3 on FVP can check that the probe
    measures the right thing in the right place. Absolute latency needs
    silicon (S32Z), or a short, low-idle capture under the 4.295 s wrap. The
    silicon run is [phase-512](../phase-512-on-hardware-measurements.md) H1.
  * **E4 measured.** Same painted image, idle boot, with and without E4:

    | | without E4 | with E4 |
    |---|---|---|
    | wakes per second (declared cadence 5 ms) | ~26.7 | ~162 |
    | `timer-overrun-runtime` | ~6.5/s | ~0.2/s |
    | `main` CPU (thread analyzer) | 15-19 % | 1 % |
    | `main` average frame (counter ticks) | ~747 k | ~40 k |

    The analyzer reports 458 992 unused bytes on `main`'s stack, which is the
    length of every pre-E4 scan. Painted builds on any pin between #529
    (which arms the bound) and E4 run this way. That includes ASI's pinned
    `--trace-stats` / `--trace` lanes at the time of writing.
  * **The since-boot maximum is pinned by boot.** Both runs report
    `jitter_max_us` of about 34 ms, set at ~10 s and never exceeded, so the
    cumulative figure describes startup and hides the steady state.
    `nros_cpp_executor_clear_release_jitter_stats` exposes the existing Rust
    `clear_release_jitter_stats`, and the ASI readout clears after every
    report, so each line covers its own `window_ms`.
  * **A2c — LANDED 2026-10-15** as a separate count, so `late_wakes` keeps
    its meaning: `Executor::late_wakes_beyond_granularity()` (C:
    `nros_cpp_executor_late_wakes_beyond_granularity`), cleared with the
    other statistics. It counts the late wakes whose lateness exceeds
    `release_jitter_granularity_us()`. A unit test runs the A3 shape (a
    10 ms grid, a 1 ms-tick park, 300 us of work per release): 45 of 51
    wakes are late and none is beyond the granule, and one 2.5 ms overrun
    is. The original report follows.
  * **A2c — was open: `late_wakes` ignores the granularity it reports.** A wake
    counts as late at `interval - nominal > 0`, one microsecond over. On a
    loop paced by a relative `spin_once` timeout, every interval is nominal
    plus work plus tick rounding, so about 99 % of wakes count as late with E4
    in. A captured example: `park_bound_us=27 park_achieved_us=1000` is a
    timer 27 us away rounded up to the 1 ms tick by design. The executor
    already says the figure is worth `granularity_us=1000`; the count should
    agree with it (for example, a separate count of wakes late beyond one
    granule). This is A2b's coupling, showing in a number.

  **Exit:** a loaded `--drive` run on an E4 build, with the windowed readout
  and a CTF capture of the same run. The probe's per-window maximum and the
  trace's activation lateness must agree within `granularity_us` on the shared
  counter, and any disagreement must be explained, not averaged away.

  **Loaded runs (2026-09-11) and two corrections.** Evidence is under ASI's
  gitignored `log/a3/` (CTF capture, island logs, `a3-analyze.sh`,
  `probe-phases.py`).

  | phase | traced run: wakes/s | worst late wake per 1 s window (median) | untraced run: wakes/s | worst late wake per window (median) |
  |---|---|---|---|---|
  | before Autoware traffic | 164 | 33 ms | 165 | 33 ms |
  | traffic onset (traced only) | 12.5 | 316 ms (max 649) | | |
  | driving under load | 32.5 | 91 ms | 8.9 | 115 ms |

  * **The readout agrees with two independent sources.** During the traced
    run's 28-42 s collapse the probe showed ~300 ms spin intervals. The
    controller's own per-cycle log lines came 295-366 ms apart in the same
    span, so the 30 ms control timer was effectively running at 3.3 Hz. The
    CTF trace agrees: `idle` never runs from 28 s, and `main` is preempted by
    `rx_q[0]` and `recvUC`. Both of the log's clocks advance identically, so
    the intervals are not a counter artifact.
  * **Neither CTF nor the network burst causes the collapse.** Without CTF,
    and with no RX-buffer errors, the loop still ran at ~9 wakes/s under load.
    Still untested: the controller's three synchronous INFO lines per cycle
    (immediate log mode at 115200 baud, on the control thread) and MPC cost
    under preemption. The test needs a run in which autonomous mode engages
    (2 of 3 attempts did).
  * **Correction: the ~33 ms outlier in every window is E4's scan, not the
    readout's print.** The first reading blamed the `rt-probe` line itself. In
    deferred log mode the outlier is unchanged, and the trace shows `main`
    holding the CPU for ~32.6 ms about every 1.003 s during the quiet phase.
    That is E4's once-a-second scan of the painted stack, and it is why E5
    exists.
  * **Correction: deferred logging is not a fix on this image.** It crashed
    at ~108 s: `ASSERTION FAIL [out_ctx->control_block->offset <=
    out_ctx->size] @ zephyr/subsys/logging/log_output.c:122`, on two CPUs.
    `out_func` checks for a full buffer and then advances the offset as two
    separate steps, which is safe only with a single writer.
    `CONFIG_ASSERT=y` from nano-ros's Cyclone snippet makes the overflow
    fatal. That snippet also forces `CONFIG_LOG_MODE_IMMEDIATE=y` "to keep
    Cyclone init failures legible", so every Cyclone Zephyr image logs
    synchronously on whichever thread logs.
  * **Tooling fixed on ASI along the way.** The CTF contract check picked
    the last `timer@` handle (the 1 s readout timer) and compared periods as
    strings (b8378ae). The demo and `build.sh --run` re-entered cargo on
    every launch (0281ab1, 051178f).


### Path B — Deadline sources: the unused half of the seam.

`set_park_primitive` has three callers; `register_wake_source` has none
outside tests. Every wired port waits; none says WHEN.

* **B1 — what a timer source is. DECIDED (2026-09-11): no timer deadline
  source.** Both one-shot timers in the tree, `nros_zephyr_timer_create_oneshot`
  (`zephyr/nros_platform_zephyr_shims.c:249`, zero callers) and
  `nros_platform_timer_create_oneshot` (`platform_timer.h:61`, five ports),
  take `(timeout_us, callback, user_data)`. They CALL; a `NextDeadlineFn`
  TELLS. So the choice was between two mechanisms:

  * **A deadline source** bounds the park before it starts. W1 already does
    this for the executor's own timers (`next_timer_deadline_us()`), with
    attribution (`last_park()` says `Timer`), so the bound can be analysed.
  * **A wake source** ends a park early: a timer callback calls
    `wake_signal_from_isr`.

  For the executor's OWN timers a wake source buys nothing, and the reason is
  structural. The executor is single-owner (`&mut self`), so its timers can
  only be added by the thread that owns it, which is the thread that is
  parked. The next park recomputes the bound before anyone could have added an
  earlier one. The only paths that signal the wake object today are the
  RMW wake callback and the signal-fd reader, both events from outside the
  executor. It would not be more precise either: Zephyr's park already waits
  with `K_USEC(deadline)` on the same tick a `k_timer` fires on. And it costs
  what W1 does not: two `k_malloc`s per timer, ISR constraints on the
  callback, a second wake path that races the first, and the loss of
  attribution, since a signalled park reports only that it was signalled.

  The wake-source mechanism has two legitimate uses, neither of them a timer
  deadline source:
  * **The bare-metal park (C3).** `wfi` takes no deadline, so arming a
    one-shot compare whose interrupt signals the wake object IS the park
    there.
  * **A deadline raised mid-park by something outside the executor**, for
    example a sporadic-server budget refill, the job the Zephyr shim was
    written for. If that becomes real, it is a wake source with its own
    attribution code, not a deadline source.
* **B2 — smoltcp `poll_at()` as the first real `NextDeadlineFn`. LANDED
  2026-09-25.** `SmoltcpBridge::poll` (`nros-smoltcp/src/bridge.rs:730` and
  `:739` — the `:710` this line used to cite is now the doc comment above
  them; the code moved, the claim held) calls `iface.poll(timestamp, …)` and
  discarded the timing. `nros-smoltcp/src/deadline.rs` no longer does.

  Four decisions worth keeping, because each had a wrong answer that compiles:

  * **Where the registration lives: the BOARD, not the driver.** The layering
    runs `nros-smoltcp` → `nros-board-common`, so the driver is below
    `nros-node` and cannot name an `Executor`. It exports the raw C-ABI pair
    (`next_deadline_us` + `deadline_source_ctx()`); `nros-board-mps2-an385`
    and `nros-board-esp32-qemu`, which depend on both, join them — one line
    each, the same shape `install_port_park` gave the other half of the seam.
  * **The `ctx` lifetime.** A raw `ctx` outliving its bridge is the obvious
    hazard, so there is nothing to outlive: `SMOLTCP_DEADLINE_SOURCE` is a
    `'static` singleton holding two `AtomicPtr`s, which is what the bridge
    already is (`SOCKET_TABLE`, the staging buffers and the poll callback are
    all module statics). It is armed by `NetworkState::set` and disarmed by
    `NetworkState::clear` — the SAME call that arms the poll callback, not a
    second thing to remember — and reports "nothing pending" while unarmed.
  * **`Option<Instant>` over a `-> u64` ABI.** No new sentinel: the seam
    already spells the absent case `u64::MAX`, which is how an async source
    declines to shorten a park. `None` → `u64::MAX`, `Some(d>0)` → `d` in µs,
    `Some(0)` → `0`. The two ends are the two failures — `None` read as "due
    now" is a busy loop, `Some(0)` read as "never" sleeps through a
    retransmit — and each has its own test.
  * **Truncation (§1).** The bridge builds smoltcp's clock as
    `Instant::from_millis(now_ns / 1_000_000)`, a FLOOR, so a delay measured
    against it OVERSTATES the time remaining by up to 999 µs — the harmful
    direction, a park past the moment smoltcp asked for. The sub-millisecond
    remainder is subtracted back out, `div_ceil` and saturating, so the
    reported deadline is at worst one microsecond EARLY. Measured on a real
    `Interface`: smoltcp's initial SYN RTO reaches the seam as exactly
    `1_000_000` µs, and as `999_500` when asked 500 µs into the same
    millisecond.

  **Inert where nothing parks, verified not assumed.** Only three ports install
  a `ParkUntilFn` and no bare-metal board is one of them, so on mps2-an385 the
  bound never reaches a park primitive. It is not inert there either, and that
  is correct: `spin_once` still uses `park_achieved_us` as the transport
  drain's `timeout_ms`, so the executor comes back to service smoltcp instead
  of blocking out its 10 ms quantum. A source that reports `u64::MAX` changes
  nothing at all.
* **Exit for the path:** `register_wake_source` has a non-test caller, and a
  target run shows `last_park()` attributing a park to `Platform(n)`.

  **MET on QEMU (2026-10-16).** Two runs of
  `examples/mps2-an385-baremetal/rust/talker` against a live `rmw_zenohd`
  (slirp, `tcp/10.0.2.2:10500`), both publishing:

  * the shipped 10 ms budget: `no Platform park in 1000 spins; last was
    CallerBudget at 9031 us`. A live session does not change the reading
    below: smoltcp's nearest deadline is a 10 ms delayed ACK, which ties the
    budget and never beats it.
  * the budget raised to 50 ms (a local edit, not committed):
    `park bounded by Platform(0) at 27000 us` after about 30 messages.

  So the source works and wins where the budget leaves it room, which is
  where it was always going to pay. The shipped budget stays 10 ms: on this
  board nothing parks (D5), so a longer budget would only lengthen the
  transport's blocking drive. The hardware run is
  [phase-512](../phase-512-on-hardware-measurements.md) H2, since no board is on
  this bench. The original reading, from a sandbox with no router, follows.

  **Half met, and the other half MEASURED rather than left silent.** The
  non-test caller exists (two of them), and a QEMU mps2-an385 run of
  `examples/mps2-an385-baremetal/rust/talker` prints, verbatim:

  ```
  smoltcp deadline source registered as Platform(0)
  Application setup complete — entering spin loop.
  phase-436 B2: no Platform park in 1000 spins; last was CallerBudget at 10000 us
    — the smoltcp source is registered and further out than the budget
  ```

  So `last_park()` attributing a park to `Platform(n)` is **NOT** shown, and
  the run says why instead of saying nothing. A platform source only WINS when
  its deadline beats the caller's budget; this board's loop passes **10 ms**
  and smoltcp's deadlines are mostly longer (a SYN retransmit backs off from
  1 s). The run had no zenoh session — QEMU slirp could not reach the host
  router in this sandbox — so the stack sat in retransmit backoff throughout,
  which is the case that loses by the widest margin. The window where smoltcp
  wins is a delayed ACK (smoltcp's `ACK_DELAY_DEFAULT` is 10 ms, right at this
  budget) or an overdue stack, and neither occurred.

  **That is the honest reading of what B2 buys today, and it is the reason to
  finish Path C rather than to redo B2.** A deadline source pays off where the
  budget is LONG — `spin_default`'s 50 ms, a tier with a declared period, a
  port that parks. The three ports with a `ParkUntilFn` are exactly those, and
  none of them runs smoltcp. What would close this half: a run in which
  smoltcp has something due inside the budget (a live session, i.e. the QEMU
  networking this sandbox lacked) or a board whose park budget is longer than
  10 ms. The report line fires on the FIRST Platform-won park, so such a run
  answers immediately.

  The report is deliberately two-sided. Silence would be ambiguous between
  "the source is not wired" and "the source is wired and further out than the
  budget" — the failure class where nothing printing reads as nothing wrong —
  so it fires either way, once.

### Path C — Park coverage across ports.

Before this path only Zephyr (both arms) and ThreadX implemented
`nros_platform_wake_park_until_us`; W7.a made the rest reachable but none was
wired, so every other port parked on the millisecond `wake_wait_ms` floor.

* **C1 — FreeRTOS, and ESP-IDF with it. LANDED (2026-10-15).** `wake_wait_ms`
  converted with `pdMS_TO_TICKS`, which TRUNCATES: exact at the usual 1000 Hz
  tick, but at a coarser tick a sub-tick timeout became 0 ticks — the
  non-blocking path, a busy loop — the platform-layer twin of issue 1193.
  What landed:
  * One conversion, `nros_freertos_us_to_ticks_ceil` in
    `nros-platform-freertos/src/platform.c`: microseconds to ticks, rounded
    up, never zero for a non-zero request, saturating below `portMAX_DELAY`.
    Every timed wait and sleep in the port goes through it — `wake_wait_ms`,
    `condvar_wait_until`, `sleep_{us,ms,s}`, and the software-timer period in
    `timer.c`. Two siblings had their own defects: `sleep_us` rounded up to
    MILLISECONDS and used the count as ticks (at 100 Hz a 1 ms sleep lasted
    10 ms), and `timer.c` went through `pdMS_TO_TICKS`, so below 1000 Hz a
    period under one tick became 0 ticks, which `xTimerCreate` refuses.
  * `nros_platform_wake_park_until_us` (an `xSemaphoreTake` on the wake
    object with the ceiling tick count) and
    `nros_platform_wake_park_granularity_us` = `ceil(1e6 / configTICK_RATE_HZ)`.
    FreeRTOS counts the current, partly elapsed tick as one of N, so a park of
    N ticks can return up to one tick early; that band is the declared
    granularity, so it is stated rather than hidden.
  * Installed on both tier runners, the same sites Zephyr and ThreadX use: the
    Rust entry (`nros-board-freertos/src/entry.rs`, boot tier and every spawned
    tier) and the C runner (`c/freertos_run_tiers.c`, boot and spawned).
  * ESP-IDF shares `platform.c` (`ESP_PLATFORM`), so it gets the same code, but
    no IDF build exists in-tree (RFC-0065 D3) and none was run.

  Measured: the conversion compiled and run on the host against the FreeRTOS
  POSIX-port headers at 1000, 100 and 128 Hz (granularity 1000, 10000 and
  7813 us; 300 us -> 1 tick at every rate; 10001 us -> 11, 2 and 2 ticks).
  The realtime workspace fixtures were built for mps2-an385
  (`workspace-rust-freertos-realtime`, `workspace-c-freertos-realtime`), the C
  image's disassembly shows both `nros_cpp_executor_set_park_primitive` calls,
  and `realtime_tiers_e2e` passed those cells under QEMU. Every shipped
  FreeRTOS config ticks at 1000 Hz, so on today's boards the declared
  granularity equals the millisecond the executor already assumed; the change
  is what happens on a board configured coarser.

* **C2 — POSIX, which also covers NuttX. LANDED (2026-10-15).** NuttX has no
  platform source of its own: `nros-platform-nuttx` compiles
  `nros-platform-posix/src/platform.c`. What landed there:
  * `nros_platform_wake_park_until_us` turns the executor's microseconds-from-now
    into an ABSOLUTE deadline on `CLOCK_MONOTONIC` — the clock
    `nros_platform_clock_ns` reads, so the park and the executor that sized it
    measure time the same way — and waits with `sem_clockwait`. That function
    is POSIX.1-2024, NuttX has it, and glibc has had it since 2.30 (declared only
    under `_GNU_SOURCE`, which this file deliberately does not define, so the
    prototype is supplied when the header withholds it). Any other libc (musl)
    keeps `sem_timedwait` on a re-anchored `CLOCK_REALTIME` deadline, still at
    microsecond resolution. EINTR resumes against the same absolute deadline.
  * `wake_wait_ms` now goes through the same wait, so it no longer measures
    its deadline on `CLOCK_REALTIME` — the header had described it as
    `CLOCK_MONOTONIC` since phase 130, and the code had not been. A wall-clock
    step no longer moves a pending wait.
  * `nros_platform_wake_park_granularity_us` is `clock_getres(CLOCK_MONOTONIC)`
    rounded up to whole microseconds, never zero. One spelling answers both
    kernels, because each one's `clock_getres` already says the true thing:
    1 ns on a Linux host with hrtimers (so 1 us), and `NSEC_PER_TICK` on NuttX
    — 1000 us on the in-tree armv7-a export (`CONFIG_USEC_PER_TICK=1000`) and
    10000 us on the riscv export. Linux timer slack (50 us by default for
    `SCHED_OTHER`) is lateness, not granularity: the request is expressed
    exactly and the jitter monitor measures where it lands.
  * Installed on the tier runners of `nros-board-linux` (boot and spawned) and
    `nros-board-nuttx` (boot and spawned).

  Measured: four new host tests in
  `nros-platform-cffi/tests/c_port_posix_wake.rs` (run by
  `check required-features-tests`). A 300 us park never returns before its
  deadline and the fastest of 20 finishes inside a millisecond; on this host
  the 20 landed between 341 and 361 us, the ~50 us over being timer slack.
  Negative control: with the park rounded up to whole milliseconds the
  upper-bound test fails (fastest 1.046 ms). The others pin that a
  cross-thread signal breaks a 5 s park and reports 0, that a zero deadline is
  a non-blocking take, and that the declared granularity equals the clock's.
  `realtime_tiers_e2e` passed its native Rust cells
  (`workspace-rust-native-realtime`, `-derived`) with the park installed. The
  POSIX source was cross-compiled against the in-tree NuttX armv7-a export
  headers with no warnings, and `workspace-rust-nuttx-realtime` was built
  (the image links `sem_clockwait` and the park) and passed its
  `realtime_tiers_e2e` cell under QEMU (qemu-armv7a).

* **C3 — bare metal. DEFERRED, and REFRAMED (2026-10-16).** As written this
  was a park primitive: arm a one-shot compare, `wfi`, let the interrupt end
  the park. RFC-0106 D5 rules that out on its own. Every bare-metal backend
  is poll-only (zenoh-pico over smoltcp and XRCE both read the transport only
  inside `drive_io`), so the executor never parks there, and a park installed
  on mps2-an385 would never be called. The wait that does run is the
  transport's: `nros_smoltcp`'s poll loop, which already idles on `wfi`
  through `set_idle_callback` when an interrupt source is armed (the RTIC
  entry; `enable_wfi_idle`), and busy-polls otherwise.
  So the work, if it is ever wanted, is a TICKLESS idle in that loop: arm a
  one-shot compare at the wait's deadline and `wfi` until it or a packet.
  The precondition is a receive interrupt in `lan9118-smoltcp`, which has
  none (the FreeRTOS lwIP driver's `lan9118_lwip_rx_irq_enable` is the
  model); without one, a `wfi` armed only on the deadline is the blind sleep D5
  exists to prevent, moved down a layer. The gain is power, not latency.
* **Exit:** every port declares a granularity that matches its tick, so no
  port's jitter report claims resolution it lacks (issue 1242's rule,
  applied everywhere). **Met for every port with a park** — Zephyr, ThreadX,
  FreeRTOS, POSIX and NuttX — on the paths that install one. Bare metal keeps
  the 1 ms fallback, which is the floor of the `wake_wait_ms` ABI it exports
  and so is not an over-claim.

Two things Path C did not change, both shared with the Zephyr and ThreadX
wiring that came before it, and both worth a decision rather than a drift:

* **Only the TIER runners install a park. RESOLVED (2026-10-16).** The
  single-executor entries (`nros_rtos_run_components.c`, the Linux board's
  `boot_hosted`, NuttX's `run_entry`, the C++ `run_components`) installed
  nothing on any port, so the dominant embedded path still parked on
  `wake_wait_ms`. Now every entry installs it:
  * Rust board entries call the board's own `install_park`, as their tier
    runners do: FreeRTOS's and ThreadX's single entries, Linux `boot_hosted`,
    NuttX `run_entry`. `zephyr_component_main!` declares Zephyr's two symbols
    itself, since it expands only in a Zephyr image.
  * Shared C and C++ entry code is linked beside every port, bare metal
    included, and the park symbols are OPTIONAL, which is why the C runner was
    not simply extended before. It references them WEAKLY:
    `detail::install_platform_park` in `<rclcpp/detail/main.hpp>`, called by
    `detail::component_spin_loop`, and the C runner's copy in
    `nros_rtos_run_components.c` (its include path lacks the header). Both
    install only when the symbols resolve AND the granularity is non-zero,
    because a Rust-implemented port exports the symbols with the trait
    defaults (`-1`, `0`). Both platform symbols sit in the port's `platform.c`
    beside the required `nros_platform_wake_init`, so a weak reference never
    depends on archive extraction.
  * The executor stopped counting a negative park answer ("cannot park") as a
    park. It used to ignore the return, which with the trait default would
    have dropped the drain to non-blocking and looped hot.

  The same change found the poll-only half: `park_granularity_us()` returned
  the installed park's granularity on an executor that never parks (no live
  wake: XRCE). On POSIX that is 1 us, so the bound kept microseconds that the
  millisecond receive then truncated, and a sub-millisecond deadline became a
  non-blocking receive in a hot loop until it passed. The declared
  granularity now counts only while `park_can_be_woken()`; otherwise it is the
  1 ms of the wait actually performed. RFC-0106 D3/D5.
* **A park nobody signals is not broken by data. RESOLVED (2026-10-15).**
  The park waits on the executor's own wake object, which the backend
  signalled only for the executor that installed into its single wake slot.
  Every board's spawned tiers open over the boot executor's session, so their
  wake objects were never signalled and a tier park slept out its bound before
  seeing data; a poll-only backend (XRCE) has no callback at all. Two changes,
  both in `nros-node`'s executor:
  * **The park is gated on a live wake.** `spin_once` parks only when
    `async_wake_live()`; otherwise it keeps the receive-blocking wait, so a
    poll-only backend never sleeps blind (the Phase 127.C.4 stall). Builds
    without `alloc` + `rmw-cffi` have no wake callback and park as before.
  * **One session wakes every executor over it.** The backend's slot now
    holds a wake fan-out (`executor/wake_fanout.rs`): a `static` pool entry
    keyed by the session, whose callback signals every member. Each executor
    joins on install, which `open_with_session*` now does too, so spawned
    tiers and several C executors over one `rclc_support_t` are all woken. A
    member's teardown is leaving; the callback comes off the backend with the
    session's owner or its last member, after waiting out any callback in
    flight. The cost: every arrival wakes every member, since the callback
    does not say whose data arrived. Waking only the owning tier needs
    per-entity routing through each backend.

  Tested: five unit tests on the fan-out (every member woken, a left member
  not woken, release makes memberships dead and their late `leave` harmless to
  the entry's next tenant, a full table refuses, `leave` waits out a callback
  in flight), and part 5 of `nros-c`'s `executor_backend_wake.c` (two live C
  executors over one session: both woken, the slot survives the first `fini`,
  the last `fini` clears it). On `main`'s executor that part fails 4 checks:
  the first of the two executors waits its full 400 ms.

  **Measured (native Linux aarch64, zenoh, 2026-10-15).** A copy of
  `examples/workspaces/realtime-rust` with an Int64 subscriber on the spawned
  tier `high` and another on the boot tier `low`, both `spin_period = 100 ms`.
  220 messages at uniform 150-350 ms gaps, each carrying its CLOCK_MONOTONIC
  publish time:

  | build | n | median | p90 | max |
  |---|---|---|---|---|
  | before (`9e9f24392c`) | 220 | 48.88 ms | 93.22 ms | 100.20 ms |
  | after | 220 | 0.51 ms | 0.58 ms | 9.79 ms |

  Before is a blind park, confirmed by strace: the tier thread made exactly
  one futex wait per 100 ms, every one ending `ETIMEDOUT`, whatever the
  traffic. The cost is as stated: each of 100 messages for the boot tier woke
  the spawned tier once (1.0 spurious wake per foreign arrival). The run also
  found a second, zero-timeout wait on the same object after every park, a
  syscall that could only time out; it is skipped now. Native cells of
  `realtime_tiers_e2e`, `sched_dims_applied_e2e`, `linux_orchestration_tiers`
  (6/6) and both `realtime_subnode_cpp*` tests pass on this build; their RTOS
  cells were `[UNMET PRECONDITION] fixture not built`.

### Path D — Waits that still bypass the waker.

* **D1 — issue 1195's first consequence: MEASURE, then fix or close. DONE
  (2026-10-15); 1195 resolved.** Measured with a native service pair timing
  300 back-to-back calls (the tick path's `call_raw`, which loops
  `spin_once(10 ms)` exactly as `Promise::wait` does). Cyclone was never
  quantized — p50 66-77 us, 0 of 280 calls at or above 9 ms; its
  participant-level `on_data_available` listener reaches the wake object, so
  the 10 ms slice is a ceiling, not a quantum. **Zenoh was quantized** — p50
  10.1 ms, 280 of 280 calls a whole slice — because requests and replies land
  on zenoh-pico's read task and only the subscriber's arrival callback fired
  the runtime wake. `queryable_callback` and `reply_waker_callback` now call
  `fire_runtime_wake()` too: p50 0.19 ms, 0 of 280. Pinned by
  `nros-rmw-zenoh` `shim::service::tests::reply_arrival_fires_the_runtime_wake`.
  Numbers and method in the archived issue.
* **D2 — issue 1196. ABI half DONE (2026-10-15); the rest DONE in phase-502, which resolved 1196.**
  The last caller, `zpico-sys/c/zpico/platform_aliases.c`'s
  `_z_condvar_wait`, waits once on `condvar_wait_until` with a deadline the
  shim owns (1000 ms) and returns an expired slice as a spurious wakeup —
  legal for a condvar, and absorbed by every caller's predicate loop (all
  five upstream sites and `zpico.c`'s `z_get` checked). The unbounded
  `nros_platform_condvar_wait` then left the ABI: the header, every C port
  (esp-idf had already left the tree), the custom-platform example, the cffi
  export macro and the Rust `PlatformThreading` trait with its three
  bare-metal implementors, bindings regenerated.
  `check-no-unbounded-condvar-wait.sh` lost its exemption list and now refuses
  the symbol in any form. Issue 1196's design study (2026-10-15, beside issue
  1906) moved its closing condition to cover the two zenoh-pico paths that
  never went through this symbol — Zephyr's own system layer and FreeRTOS's
  vendored one — plus deleting `zpico_get` and a class gate for forever-waits;
  those remain, and [phase-502](phase-502-unbounded-condvar-wait-retirement.md)
  owns them.

### Path E — Stack headroom: wired, and on ASI blind.

* **E1 — issue 1232. RESOLVED** (the issue is archived; this item is kept
  for the record). Zephyr ignored a tier's declared `stack_bytes`: every
  SPAWNED tier gets the fixed pool slot. It matters for images with spawned
  tiers. It does not block ASI, whose only tier is the boot tier.
* **E2 — ASI's headroom rule. Already WIRED; the gap is that it cannot
  measure.** An earlier draft said wiring it would "report `main`'s headroom as
  the control tier's, a plausible number for the wrong thread". Wrong: the
  Zephyr C entry already derives the boot tier's bound from
  `nros_zephyr_main_stack_size()`, and `stack_unused_bytes()` reads
  `k_current_get()`, the thread `spin_once` runs on. For the boot tier that is
  `main`, so the thread measured is the thread sized. Nothing to wire.

  What IS wrong: Zephyr's accessor reports only with
  `CONFIG_INIT_STACKS && CONFIG_THREAD_STACK_INFO` and returns 0 otherwise,
  which the rule reads as "not instrumented" and skips. ASI's default FVP
  build has `THREAD_STACK_INFO` but not `INIT_STACKS`, so the rule is armed
  and returns early on every check. Only ASI's `--trace-stats` variant
  (`CONFIG_THREAD_ANALYZER` selects `INIT_STACKS`) measures. **Exit:** ASI
  decides where stack painting is paid for.
* **E3 — say so when the rule is blind. DONE.** An armed rule that cannot
  measure used to read exactly like a clean result. `set_min_stack_headroom_bytes`
  now asks the port once. If a bound is set and the port answers 0, it logs
  one warning naming the fix (`CONFIG_INIT_STACKS` on Zephyr) and records it,
  readable as `Executor::stack_headroom_blind()`. Entries set the bound at
  boot on the thread it describes, so the warning lands once per tier, beside
  the existing "stack-headroom bound NOT set" line. C entries get it for free
  through `nros_cpp_executor_derive_min_stack_headroom`.

* **E4 — throttle the headroom query. DONE.** On a painted Zephyr stack,
  `k_thread_stack_space_get` walks from the stack base one byte at a time
  until the first used byte (`z_stack_space_get`), so a query costs one read
  per UNUSED byte. The rule ran it on every `spin_once`. ASI's control tier
  runs on a 512 KiB main stack every 5 ms, which would put roughly half a
  megabyte of reads inside every iteration of the loop A3 is meant to
  measure. The high-water mark only grows, so a throttled check sees the same
  low mark and only sees it later. The query now runs at most once per
  `STACK_HEADROOM_CHECK_INTERVAL_US` (1 s): immediately the first time, then
  on the interval, or once per `STACK_HEADROOM_CHECK_SPIN_STRIDE` spins on a
  build with no clock. Setting a new bound re-arms an immediate check. This
  is what makes enabling `CONFIG_INIT_STACKS` on ASI's default build
  affordable (E2's exit).

* **E5 — make the query O(1) when the stack has not grown. OPEN, next.**
  E4 made the scan rare, not cheap: one ~33 ms scan per second on ASI's FVP
  image (458 992 unused bytes, measured in the CTF capture as a 32.6 ms
  `main` slice every ~1.003 s). Zephyr (`k_thread_stack_space_get`) and
  FreeRTOS (`uxTaskGetStackHighWaterMark`) both walk every unused byte;
  ThreadX reads a kernel-maintained pointer. A Zephyr port can reach
  `k_current_get()->stack_info.{start,size}`, so it can re-check only around
  the last known high-water mark and fall back to a full scan when that
  window is dirty. Stacks are not written contiguously, which is why Zephyr
  counts from the base, so the design needs a guard window and a periodic
  full re-scan to bound staleness. **Exit:** a painted image with no
  per-second stall in the CTF trace; then ASI turns `CONFIG_INIT_STACKS` back
  on.

  **AMENDED 2026-10-13: ask the rule's question, not the high-water mark's.**
  Still needed. Every tiered Zephyr and FreeRTOS image arms the rule by
  itself: `zephyr_run_tiers.c` and `freertos_run_tiers.c` call
  `nros_cpp_executor_derive_min_stack_headroom`, which sets the minimum to
  stack / 8 (`STACK_HEADROOM_DIVISOR`). So any such image built with stack
  painting pays the scan on every tier thread, not only ASI's.

  The window-and-rescan design above answers more than the rule asks.
  `check_stack_headroom` (`executor/monitor.rs`) needs only
  `unused >= min_bytes`. Unused is counted as painted bytes contiguous from
  the stack's deep end, so that holds exactly when the deepest `min_bytes`
  bytes are all still painted. Scanning only those bytes gives the same
  verdict:

  - **exact**, so it needs no guard window, no periodic rescan and no
    staleness bound. A write anywhere in that region is seen on the next
    check, however it got there.
  - **bounded by the minimum, not by the unused stack:** 1/8 of the stack at
    the derived default, so roughly 4-5 ms on ASI's image instead of 33 ms.
    It can be cut further by checking a slice of the region per spin.
  - **the number for the report** still needs the full count. Take it only
    on a breach, which is rare and is already reported once and then only
    when worse.

  What it needs is a bounded query beside `nros_platform_task_stack_unused_bytes`.
  It answers whether at least N bytes are unused, scanning at most N:
  - **Zephyr:** scan from `k_current_get()->stack_info.start` for the paint
    byte, under the same `CONFIG_INIT_STACKS` + `CONFIG_THREAD_STACK_INFO`
    gate (issue 1075). Mirror whatever `k_thread_stack_space_get` adjusts
    (the user-mode stack offset, the growth direction) rather than
    re-deriving it.
  - **FreeRTOS:** `uxTaskGetStackHighWaterMark` takes no limit, so the port
    scans from the task's stack base itself.
  - **ThreadX** is already O(1) and answers it directly. **POSIX** stays
    `0`, uninstrumented.

  Zephyr first, since it is the port ASI runs. The exit stays as written.

  **LANDED 2026-10-13 (the nano-ros half).**
  - **ABI:** `nros_platform_task_stack_unused_bytes_upto(limit)` in
    `nros/platform.h`, regenerated with the pinned bindgen, plus the trait
    method `PlatformThreading::task_stack_unused_bytes_upto` (defaulted to
    the full query, capped) and the free function
    `nros_platform_api::stack_unused_bytes_upto`.
  - **Zephyr:** reads at most `limit` bytes from `stack_info.start`. It falls
    back to the capped full query under CONFIG_USERSPACE,
    CONFIG_THREAD_STACK_MEM_MAPPED or CONFIG_NO_UNUSED_STACK_INSPECTION.
  - **FreeRTOS, ThreadX and POSIX** cap their full query.
  - **The executor** asks the capped query with the declared minimum. Below
    the cap the answer is the exact headroom, so a breach reports it without
    a second scan. The E3 "blind" probe asks `upto(1)`, so setting a minimum
    no longer runs a full scan either.

  **Measured on native_sim** (Zephyr 3.7, the C talker with CONFIG_INIT_STACKS
  and CONFIG_THREAD_STACK_INFO, a temporary boot probe that was not
  committed): full = 16,344 of 16,384 B. `upto(n)` equalled `min(full, n)`
  for n = 1, full - 1, full, full + 1, full / 8 and SIZE_MAX. The image
  links, which is issue 1075's failure mode for this configuration. No lane
  builds with stack painting on, so the probe was the only run, and the
  per-second stall on ASI's FVP image is still the island's to re-measure.
  That re-measure is the exit above.

### Order

A1 → A2 → A3 first (B1 is decided: no code). Then
B2 and D in parallel; C whenever — it is breadth, and each port is
independent. E3 and E4 are done; ASI's `CONFIG_INIT_STACKS` follows E4.
E1 only for images with spawned tiers.

## Sequencing (W1-W7, as delivered)

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

## Issues homed here (survey 2026-09-11, re-homed 2026-09-12)

The 2026-09-11 survey homed this issue in phase-430, which was ARCHIVED the next
day (`f3a0b7168`). A finished phase cannot own open work, so it moves here: the
defect is the executor's timer policy, which is this phase's subject.

| issue | why it belongs here |
| --- | --- |
| [#1041](../../issues/archived/1041-timer-missed-deadline-policy-differs-from-rcl.md) | a repeating timer CATCHES UP after a stall where rcl SKIPS — N callbacks instead of one. Partly fixed already: the executor arena defaults to `Skip`, and `nros-node/src/timer.rs:354` is the unswept sibling, which is the fix-the-class shape rather than a new defect |
