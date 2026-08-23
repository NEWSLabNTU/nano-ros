---
id: 736
title: "`realtime_tiers` nuttx-arm/rust: the 10 ms tier's TIMER fires 7 times in
  1000 spins — the tier is scheduled, its clock is not advancing with it"
status: open
type: bug
area: core, platform, testing
related: [phase-281, issue-0636, issue-0623]
---

## Symptom

```
cargo nextest run -p nros-tests --test realtime_tiers_e2e --retries 0

nuttx-arm/rust: high-tier /ctrl counter 2 is not >= 3x the low-tier /telem
counter 21 — the 10 ms tier is not outrunning the 100 ms tier
```

The FAST tier delivers fewer messages than the SLOW one, inverted ~70x.
Deterministic.

## The measurement that reframes it

Run the fixture by hand for 30 s (`qemu-system-arm -M virt -cpu cortex-a7
-icount shift=auto -kernel <nuttx_entry> -netdev user,...`, with `just zenohd
tcp/0.0.0.0:8291` for the baked locator) and the tier reports on itself:

```
nros: tier `high` alive — 1000 spin(s), 7 timer(s) fired, 0 sub callback(s), 0 error(s)
```

**1000 spins, 7 timer fires.** The tier is being scheduled — a thousand times.
Its spin period is 1000 us and its timer period is 10 ms, so the timer is due
every ~10 spins and should have fired ~100 times. It fired 7, i.e. the timer's
clock advanced ~70 ms across 1000 spins that asked for 1 s.

So this is NOT a scheduling defect, and everything the assertion's wording
suggests ("the 10 ms tier is not outrunning the 100 ms tier") points the reader
at the wrong layer. The tier runs. Its sense of time does not keep up with it.

The `low` tier is the control: 10 ms spin, 100 ms timer — the same 10-spins-per-
fire ratio — and it delivers at roughly its declared rate. Whatever this is, it
bites the 1 ms spin and not the 10 ms one.

## Ruled out, with the measurement for each

* **SCHED_SPORADIC.** `[tiers.high.nuttx] budget_us=5000 period_us=10000`
  engages it, and `sched_ss_max_repl` was hardcoded to 1 — genuinely wrong (see
  below). Raising it to `CONFIG_SCHED_SPORADIC_MAXREPL` left the symptom bit for
  bit unchanged (ctrl 2, telem 21). Compiling the sporadic call out entirely
  gave ctrl 5 / telem 13 — moved, still inverted. Not the cause.
* **#636 / the boot-tier choice.** `boot_tier_index` did move `high` from the
  boot thread to the spawned path, which is why this became reachable, but the
  spawned tier gets its priority (`tier priority set tier=high prio=110`) and
  runs 1000 spins. It is scheduled.
* **phase-359 W10's clock ruling.** NuttX has had no `std` since W7, so both
  `Clock::now()`'s `no_std` body and `default_clock_us_fn`'s
  `not(rmw-cffi)` arm are the same before and after W10. Checked against
  `121b555c9^`.
* **A museum binary.** This issue's first draft claimed `lane=native` does not
  rebuild the row so every run used a stale fixture. Wrong: the row produced
  runtime output, so the freshness gate passed it. It has since been rebuilt
  with `just nuttx build-fixtures` and behaves identically.
* **"Fails solo, passes in the sweep."** Also this issue's first draft, also
  wrong, and worth recording because it invented a mystery. `just ci` is tier 1;
  `nros_tests::lane_scope::admits` puts the nuttx rows OUT OF LANE. Measured:
  `NROS_TEST_SCOPE=native` -> "4 row(s) ran, 12 out of lane". The 47 s vs 21.6 s
  was 16 rows against 4, not a load effect.

## Where to look next

The tier's spin loop and the clock its timer compares against are the two
suspects, and the 1 ms-vs-10 ms split is the discriminator. Concretely: does
`spin_once` return early without waiting its declared period (so 1000 spins
really did take only ~70 ms of guest time), or does it wait correctly while the
clock under-reports? The tier's own counters can tell these apart — a spin count
alongside the clock delta at the alive report would settle it in one run, and
that instrumentation does not exist yet.

Related but separate: the executor's contract monitor prints
`timer-overrun-runtime timer measured=8 declared=0` continuously. `measured=8`
means a publish costs ~8 ms on this emulated target while the deployment
declared no runtime at all, so the declaration is empty and the monitor cannot
do anything but complain. That was this issue's ORIGINAL title, before the
counters showed it to be a passenger rather than the driver.

## A second, different failure this uncovered

`nuttx-riscv/rust` also fails, and not the same way: `/ctrl` counter **0**, no
`tier priority set tier=high` line at all, no FIRST dispatch, and `sporadic
budget FAILED tier=high rc=22` (EINVAL) where the arm build succeeds. It was
invisible because its fixture had never been built — `realtime_tiers` reported
"16 ran, 10 skipped" until `just nuttx build-fixtures` brought it down to 5.

## Reproduce

```
just nuttx build-fixtures
cargo nextest run -p nros-tests --test realtime_tiers_e2e --retries 0
```

## Measured 2026-08-21 — the clock is not BEHIND, it is 6.5x AHEAD

The issue asks for "a spin count alongside the clock delta at the alive report
… that instrumentation does not exist yet". It exists now (the NuttX tier loop
reads `nros_platform_clock_ns` — the same monotonic source the timers compare
against, deliberately, since a second healthier clock would only prove two
clocks disagree) and it answers the question the other way round:

```
tier `low`  100 spin(s),   54 timer(s) fired, clock  6551000 us vs asked 1000000 us
tier `low`  200 spin(s),  125 timer(s) fired, clock 15460000 us vs asked 2000000 us
tier `low`  300 spin(s),  190 timer(s) fired, clock 24558000 us vs asked 3000000 us
tier `high` 1000 spin(s),   5 timer(s) fired, clock 25924000 us vs asked 1000000 us
tier `low`  400 spin(s),  246 timer(s) fired, clock 31448000 us vs asked 4000000 us
```

**"The tier's sense of time does not keep up with it" is backwards.** The clock
runs 6.5–7.9x AHEAD of the time the spins asked for — unsurprising under
`-icount` emulation, where a 1 ms spin cannot complete in 1 ms of guest time.
Neither candidate the issue named is what happened: `spin_once` is not returning
early, and the clock is not under-reporting.

### What the numbers actually say

Comparing the two tiers at the same point in guest time:

| | spins | clock | timer fires | fires the clock implies | ratio |
| --- | --- | --- | --- | --- | --- |
| `low` (10 ms spin, 100 ms timer) | 300 | 24.6 s | 190 | ~245 | **78 %** |
| `high` (1 ms spin, 10 ms timer) | 1000 | 25.9 s | 5 | ~2590 | **0.2 %** |

`low` is roughly keeping up. `high` is not, by three orders of magnitude — and
crucially **not** by a one-fire-per-`spin_once` cap either: that cap would give
`high` ~1000 fires, and it produced 5. `low` meanwhile manages 0.63 fires per
spin, so more than one fire per spin is clearly reachable.

Each `high` spin also consumes ~26 ms of clock for a declared 1 ms — so by
elapsed time the timer is ~26 periods overdue on EVERY spin, and still does not
fire.

### One hypothesis eliminated before it is proposed

Tick granularity is the obvious suspect and it is **not** a 10 ms-tick problem:
the built image has `CONFIG_USEC_PER_TICK=1000` with `CONFIG_SCHED_TICKLESS`
unset, so a 1 ms spin period is exactly one tick, not a sub-tick round-to-zero.
Recorded because it is where a reader would go first.

### Where that leaves it

The defect is in when the executor decides a timer is DUE, on a tier whose spin
period is 1 ms — not in scheduling, not in the clock, and not in the wait. The
next measurement is inside `spin_once_counted`: what it computes as the next due
time and what it compares against, for a 1 ms period versus a 10 ms one.

### Reproducing the numbers

The e2e window is too short to reach a heartbeat — the fast tier needs 1000
spins and gets ~39 per second of guest clock. Use the manual run, which now
works in the ROS distrobox (it needs both QEMU and a router, and until this
issue neither host had both — the box lacked `qemu`, and a ROS-less host has no
`rmw_zenohd`):

```
just zenohd tcp/0.0.0.0:8291 &
qemu-system-arm -M virt -cpu cortex-a7 -nographic -icount shift=auto \
  -kernel examples/workspaces/realtime-rust/target-fixtures/nuttx/armv7a-nuttx-eabihf/nros-minsizerel/nuttx_entry \
  -netdev user,id=net0 -device virtio-net-device,netdev=net0
```


## Measured 2026-08-21 (second pass) — the timer arithmetic is fine; it is barely DISPATCHED

The previous pass concluded "the defect is in when the executor decides a timer
is DUE ... the next measurement is inside `spin_once_counted`". That pointer was
one layer too deep. `spin_once_counted` is three lines
(`self.executor.spin_once(timeout); self.run_ticks();`), and the decision it
forwards to is `timer_try_process`, which was probed directly instead.

Probe placed BEFORE the `cancelled` early return — deliberately, since a probe
after it cannot tell "no longer dispatched" from "dispatched and refused there" —
printing `delta_us`, `elapsed_us`, `period_us`, `cancelled`, `oneshot&&fired`,
filtered to the FAST tier's timer (`period <= 10 ms`):

```
[T] calls=1  delta=3000 elapsed=0    period=10000 cancelled=0 done=0
[T] calls=2  delta=2000 elapsed=3000 period=10000 cancelled=0 done=0
[T] calls=3  delta=3000 elapsed=5000 period=10000 cancelled=0 done=0
[T] calls=4  delta=3000 elapsed=8000 period=10000 cancelled=0 done=0
[T] calls=5  delta=3000 elapsed=1000 period=10000 cancelled=0 done=0   <- fired, remainder kept
...
[T] calls=21 delta=4000 elapsed=9000 period=10000 cancelled=0 done=0
```

and then **nothing** — 21 calls in a 45 s run whose heartbeat reports
`tier high alive — 1000 spin(s), 8 timer(s) fired`.

### What that settles

* **The timer arithmetic is correct.** `elapsed_us` accumulates, crosses
  `period_us`, fires, and keeps the remainder on the phase grid, exactly as
  written. Every fire the tier managed is accounted for by these calls. Nothing
  to fix in `timer_try_process`.
* **It is not cancellation.** `cancelled=0` and `done=0` on every sample, and the
  probe sits ahead of that branch, so a cancelled timer would still print.
* **The defect is dispatch FREQUENCY.** ~21 dispatches against 1000+ spins. A
  timer is registered `InvocationMode::Always` with `has_data: always_ready`, and
  BOTH `spin_once` paths tick timers — the `!trigger_passes` branch loops every
  `EntryKind::Timer` before returning, and the main path drains
  `FifoReadySet(bits | always_mask)`. So "every spin dispatches every timer" is
  what the code says, and the measurement says otherwise.
* **`delta_us` is also wrong at those dispatches**: 2–4 ms, while the same
  tier's heartbeat has its clock advancing ~26 ms per spin. `delta_us` is
  `now - last_spin_end_us`, so on a tier spinning 1000 times in 26 s it cannot
  be 3 ms. Whatever accounts for the missing dispatches likely accounts for this
  too — the calls that DO arrive look like they came from a loop running at ~3 ms
  per iteration, not the one the heartbeat is counting.

### The next probe, and why the obvious one is not enough

There are TWO copies of the `ctrl` timer. The guest console prints
`Control::register on a tier admitting group 'ctrl'` **twice** — once on the boot
executor and once on the spawned tier's — so both executors carry the node's
timers. The probe above cannot tell which copy it sampled, and "21 calls" may be
one copy's total while the other is never dispatched at all.

So the next probe must TAG the entry — the `TimerEntry` address is enough — and
report calls per copy. That distinguishes the two live hypotheses:

1. high's executor dispatches its timer ~21 times in 1000 spins (a dispatch bug), or
2. high's executor never dispatches its copy, and all 21 belong to the boot
   executor's copy (a wiring bug — the tier spins an executor whose timer is not
   the one its node registered).

Hypothesis 2 also explains the 3 ms `delta_us`, which is much closer to the boot
tier's early cadence than to the fast tier's measured 26 ms.

### A measurement trap worth not repeating

The first version of this probe put its `static CALLS` counter inside
`timer_try_process<F>`. That function is GENERIC over the callback type, so the
static is instantiated per `F` — one counter per tier. The "total" it printed was
one tier's, and both sampled lines were the 100 ms timer's while the 10 ms one
under investigation never appeared. The counter belongs in a non-generic helper.

## ROOT CAUSE, measured 2026-08-22 — the executor's own Sporadic budget gate

The fast tier's timer is not under-firing. It is not being DISPATCHED, and the
thing skipping it is `spin_once`'s cooperative Sporadic budget check:

```rust
// packages/core/nros-node/src/executor/spin.rs
if !has_budget {
    continue;          // <- entry not dispatched at all this spin
}
```

Counted per executor, keyed by arena address, one 45 s run:

| executor | `spin_once` entries | budget SKIPS | timer dispatches |
| --- | --- | --- | --- |
| boot / `low` (declares no budget) | 450 | **0** — no rows at all | **450** (1:1) |
| spawned / `high` (`budget_us=5000`, `period_us=10000`) | 1250 | **1200** | **3** |

96 % of the fast tier's spins never reach its timer. The low tier, identical in
every respect except that it declares no budget, dispatches on every single
spin. That is the whole defect.

The callback costs 2–4 ms (measured at the timer: `delta=2000..4000`, and the
`timer-overrun-runtime timer measured=1..3` warnings say the same). Against a
5000 µs budget, one or two activations exhaust it — and it then does not
recover, which is what 1200 consecutive skips means. Whether that is the
replenishment never running or the consumption being charged the whole
inter-spin `delta_us` (10–80 ms here) rather than the callback's own runtime is
the next question, and the code right below the gate names the suspect itself:
*"Phase 110.E.b follow-up — per-callback runtime accounting (replaces this
cycle-level attribution)"*.

### This retires the earlier SCHED_SPORADIC dead end, and explains it

An earlier pass suspected NuttX's KERNEL sporadic server, found
`sched_ss_max_repl` hardcoded to 1, fixed it, and measured no change — then
compiled the kernel sporadic call out entirely and still saw the inversion.
Both results were right, and now they make sense: the same
`[tiers.high.nuttx] budget_us/period_us` declaration is enforced TWICE, once by
the kernel and once cooperatively by the executor, and only the second one was
starving the tier. Removing the kernel half could never have helped.

### How each earlier reading was wrong, since three of them were mine

* "The tier's sense of time does not keep up with it" — backwards; the clock
  runs 6.5-7.9x AHEAD under `-icount`.
* "The defect is in when the executor decides a timer is DUE" — the timer
  arithmetic is correct and every fire is accounted for by the dispatches it
  received.
* "Two copies of the ctrl timer, and the spawned tier's may never be
  dispatched" — REFUTED: address-tagging shows exactly two timer entries in the
  process, one per executor, each at its own arena's offset 0. The wiring is
  right.

### Two probe traps, both of which produced confident wrong numbers

1. A `static` counter inside the generic `timer_try_process<F>` is instantiated
   PER `F` — one counter per tier. The "total" it printed was one tier's.
2. An executor's arena BASE address is also the address of its first entry, so
   keying the spin-entry counter and the timer counter on the same value merged
   the two numbers this probe existed to compare. Keying `+1` separated them.

Both were caught only because the numbers disagreed with something else already
known. A probe that is wrong in the same direction as the hypothesis would not
have been.

### Where to fix

Not decided here, and the choice is a design question rather than a bug:

* If the budget is meant to bound the tier's CPU share, then 5 ms per 10 ms is
  simply not satisfiable for a callback that costs 2-4 ms on this emulated
  target, and the DECLARATION is wrong — but a declaration being unsatisfiable
  should be reported, not silently absorbed into a 0.2 % dispatch rate.
* If the budget is being over-charged (cycle-level `delta_us` instead of
  per-callback runtime), that is a straightforward accounting bug and the tier
  is entitled to its 10 ms period.

Either way, `has_budget == false` for 1200 consecutive spins with no
diagnostic is its own defect: exactly the silent-drop shape issue 0737 gated in
example callbacks, one layer in.

## Fixed the root cause, measured 2026-08-22 — and it exposes a different limiter

The budget gate above is fed by an accounting that charges the wrong quantity,
and the per-callback path that was supposed to replace it never ran anywhere.

```rust
// spin.rs, once per spin, for EVERY sporadic SC:
let delta_us_u32 = u32::try_from(delta_us).unwrap_or(u32::MAX);
for slot in self.sporadic_states.iter_mut().flatten() {
    let _ = slot.tick(now_ms, delta_us_u32);   // refill, then SUBTRACT delta_us
}
```

`delta_us` is the wall-clock gap between two spins — 10 000..80 000 µs here —
not CPU the callbacks spent. Against a 5 000 µs budget it saturates the SC to
zero on every spin no matter what ran, which is exactly the 1200-skips /
3-dispatches ratio, and exactly why the sibling tier (identical but declaring
no budget) dispatched 450/450.

**The per-callback replacement existed and was dead.** `consume_dispatch_runtime_us`
charges only `sporadic_atomic_states`, which is populated solely by
`Executor::register_sporadic_timer` — and the only callers of that in the whole
tree are two unit tests in `executor/tests.rs`. No board, no entry, no
`run_tiers` registers one. So on every shipped image the alloc arm charged a
state nothing populated and the `no_std` arm discarded the measurement outright
(`let _ = (sc_idx, elapsed_us);`), while the comment above it stated that "the
atomic path now records actual wall-clock per-callback runtime". It records it
under test. This is the codebase's own recurring shape: a capability behind a
path nobody enables reads as coverage.

### The change

* `SporadicState::tick(now_ms, delta_us)` → `refill(now_ms)` + `consume(us)`.
  Refill stays time-based; consumption is charged from measured callback
  runtime, the quantity a budget actually bounds.
* `consume_dispatch_runtime_us` charges the POLLED state unconditionally — the
  one every image runs — in addition to the atomic state when registered.
* The skip is no longer silent. `has_budget == false` was a bare `continue`;
  it now counts consecutive skips per SC and warns at 100, then every 1000:
  *"sporadic budget exhausted for N consecutive spins (sched context I): its
  callbacks are not being dispatched."* This was the part of the diagnosis that
  was not a design question, and it is what would have made the original
  investigation minutes instead of days.

### Measured

`workspace-rust-nuttx-realtime` rebuilt clean, `realtime_tiers_e2e`:

| | `/ctrl` (10 ms tier) | `/telem` (100 ms tier) | ratio |
| --- | --- | --- | --- |
| before | 2 | 21 | **0.1x** (inverted) |
| after, run 1 | 66 | 25 | 2.6x |
| after, run 2 | 61 | 25 | 2.4x |

The inversion is gone and the fast tier outruns the slow one — a ~30x move in
the quantity this issue is about. The new budget-skip warning never fires on
these runs, so the SC's skip streak stays under 100: the starvation this issue
diagnosed is not there any more.

### Still red, for a different reason — publishes are failing

The assertion wants >=3x (75) and gets 61-66, and the run is full of:

```
[ERROR] on_ctrl:  publish to /ctrl  FAILED: Runtime
[ERROR] on_telem: publish to /telem FAILED: Runtime
```

on BOTH tiers. The timer now fires and the callback now runs; the publish
inside it fails, so the delivered counter undercounts what was dispatched.
That error path is not new — it dates to #572's diagnostics work — it was
simply not the limiter while the tier was reaching its callback 0.2 % of the
time.

So this issue's diagnosis is resolved and its cell is still red. Whoever takes
the residue should start at the transport, not the scheduler: the question is
why a ~100 Hz publish on this image returns `Runtime`, and whether the 25 the
SLOW tier delivers is also short of its own declared rate (100 ms over the same
window should be more than 25 if the window is the ~2.5 s the counts imply).
Filing that as its own issue would be reasonable; it is not the defect this
one describes.

## The residue, traced to the wire — 2026-08-23

The publishes fail with **`_Z_ERR_TRANSPORT_TX_FAILED` (-100)**, zenoh-pico's
transport TX error, on BOTH tiers.

Getting that took four probes, because four layers each discard the cause and
each reports its own generic:

```
z_publisher_put()            -> -100  _Z_ERR_TRANSPORT_TX_FAILED
  zpico.c                    -> -8    ZPICO_ERR_PUBLISH        (rc dropped)
    zpico.rs                 ->       ZpicoError::Publish
      shim/publisher.rs      ->       TransportError::PublishFailed  (`map_err(|_| ..)`)
        handles.rs           ->       NodeError::Transport(PublishFailed)
          node_runtime.rs    ->       NodeDeclError::Runtime   (`map_err(|_| ..)`)
            ctrl_pkg         ->       "publish to /ctrl FAILED: Runtime"
```

Every step compiles, every step is honest about *that* it failed, and the one
fact worth having — the wire said TX failed — survives none of them. Worth
naming because #572 fixed the same shape one layer further out (a discarded
`Result` made "the timer never fired" and "every publish failed" the same
observation) and the chain below it was left intact.

### One thing fixed here, because it was a genuine conflation

`CellResolver::publish_raw` returned `NodeDeclError::Runtime` for BOTH a
transport rejection and `lookup_publisher` missing — `.unwrap_or(Err(Runtime))`.
Those are different bugs in different layers, and from a serial console they
were one line. Split: a lookup miss is now `NodeDeclError::UnknownPublisher`
("no publisher declared for that entity"). Measured on this cell — the arm that
fires is the TRANSPORT one, so the wiring is fine and the publishers resolve.
That eliminated a hypothesis in one run instead of a debugging session, which is
the whole argument for the split.

The remaining collapses are left alone deliberately: widening `NodeDeclError`
into a payload-carrying error crosses the FFI/plugin boundary its `message()`
exists to serve, and that is a design change, not a bug fix.

### Where the next reader should start, and it is issue 0506

`high` is `SCHED_FIFO` 110 and `low` is 100 (`[tiers.*.nuttx]` in
`realtime-rust/src/demo_bringup/system.toml`). zenoh-pico's read and lease
threads on this port are pthreads created at the inherited default — 100. So
the fast tier sits ABOVE the transport and the slow tier sits level with it,
which predicts exactly what is measured: TX failing on both, worst on `high`.
That is issue 0506's subject ("transport tasks above application tiers is the
right default but has no budget") reaching this cell, and it is the same class
as 0623 one layer down.

**Not measured, so not claimed.** The evidence is the error code plus the
priority arithmetic; the experiment that would settle it is to run the tiers
below the transport threads' priority and see whether the TX failures stop. If
they do, the cell's threshold is not a scheduler question at all and this issue
can close pointing at 0506.

### Current state of the cell

| | `/ctrl` (10 ms) | `/telem` (100 ms) | ratio |
| --- | --- | --- | --- |
| before the budget fix | 2 | 21 | 0.1x inverted |
| after, four runs | 55, 60, 66, 77 | 25-28 | 2.0x - 2.9x |

Threshold is 3x. The scheduler defect this issue diagnosed is fixed and the
tier dispatches; what is left is the transport dropping what it dispatches.

## The priority hypothesis is REFUTED, by controlled experiment — 2026-08-23

The section above proposed that the TX failures were issue 0506 reaching this
cell: tiers at `SCHED_FIFO` 110/100 above zenoh-pico's read/lease threads at the
inherited 100. It said the settling experiment was to change that order and see
whether the failures stop.

They do not.

| transport read/lease priority | `/ctrl` | `/telem` | publishes |
| --- | --- | --- | --- |
| 1 (below every tier) | **69** | 26 | still failing |
| inherited ~100 (baseline, 4 runs) | 55-77 | 25-28 | still failing |
| 111 (above every tier) | **46** | 24 | still failing |

Placing the transport ABOVE the tiers — the arrangement CLAUDE.md names as the
right default, and the one 0623 fixed for FreeRTOS — makes this cell slightly
WORSE, and `_Z_ERR_TRANSPORT_TX_FAILED` appears at every setting. So the TX
failure is not a scheduling-order problem and 0506 is not the parent of this
residue.

The spread between 46 and 69 is also the control that makes the negative result
mean something: the knob demonstrably reaches the threads, so "no effect on the
failures" is a finding rather than a no-op.

### What that leaves

`_Z_ERR_TRANSPORT_TX_FAILED` with an 8-byte payload and a 33-byte attachment, at
every priority arrangement, on both tiers. The remaining candidates are inside
the link rather than around it — a full TX batch / session buffer, or the
emulated NIC not draining as fast as the guest offers. Worth noting that this
guest's clock runs 6.5-7.9x AHEAD under `-icount` (measured earlier in this
issue), so a "10 ms" publish cadence is offered to the emulated link far faster
than 100 Hz of wall time. That would make the cell's threshold a property of the
emulator's link, not of the scheduler — but that is a hypothesis, and this
section exists because the last one was too.

The probe that would settle it: count `z_publisher_put` failures against
successes over a fixed window at two different tier spin periods. If the failure
RATE tracks offered load rather than priority, it is congestion.

## Fixed on the way: NuttX could not state its transport priority at all

Independent of the above, and kept because it is a real platform gap:
`zpico_set_task_config` DISCARDED the priority on NuttX.

```c
#else
    // Linux / macOS / NuttX: priority needs a policy this process may not be
    // allowed to request. Stack size only, as before.
    (void)read_priority;
    (void)lease_priority;
#endif
```

That reason is a HOSTED concern — SCHED_FIFO needing privilege — and NuttX is an
RTOS with no such gate; the board's own tier spawn sets SCHED_FIFO priorities
through the same pthread API a few files over. This is exactly issue 0626's
finding for Zephyr, left behind in the same `#else` when that one was fixed:
the class was fixed at the reported site only.

Added `zpico_nuttx_set_priority` — a RAW-priority sibling of the Zephyr helper,
deliberately separate because Zephyr's takes a NORMALISED 0-31 value and NuttX
tier priorities are authored raw (`[tiers.*.nuttx] priority = 110`); two scales
through one function is how 0623's inversion happened. `PTHREAD_EXPLICIT_SCHED`
is set, without which the policy and param are silently ignored and the thread
takes the creator's priority — the same silent inheritance this fixes.

**No board calls it, and that is deliberate.** The measurement above says the
value is not this issue's answer, and which side of the tiers a NuttX transport
should sit on is 0506's open question. Shipping the knob without choosing the
number is the point: "both orderings are legitimate; choosing by accident is
not."
