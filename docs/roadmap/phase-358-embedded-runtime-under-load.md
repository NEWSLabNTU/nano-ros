# Phase 358 — Embedded runtime under load: footprint, overrun, overload

**Status (2026-08-15). PLANNING — nothing implemented.** Five embedded issues
about what the runtime does when it does not fit, or does not keep up. Grouped
because each is a *policy* gap rather than a broken mechanism.

**Owns:** [issue 0271](../issues/0271-orin-spe-btcm-footprint-regression.md),
[issue 0505](../issues/0505-timer-backlog-replay-no-overrun-policy.md),
[issue 0506](../issues/0506-transport-band-unbounded-preemption.md),
[issue 0557](../issues/0557-zephyr-cyclone-action-ddsrt-thread-reuse.md),
[issue 0579](../issues/0579-nuttx-boot-tier-priority-never-applied.md).

**Related:** [issue 0567](../issues/archived/0567-zpico-rx-cannot-resume-partial-buffer.md)
(RESOLVED 2026-08-14 — this UNBLOCKS #506's device half; see W3), [phase-352](phase-352-platform-clock-ns.md)
(COMPLETE), [phase-349](phase-349-rtos-integration-shells.md),
`docs/reference/platform-implementation-notes.md`.

## The common shape — no policy, so the default is "whatever happens"

* **#505** — after a stall, periodic timers replay the whole backlog (6 control
  callbacks 88 µs apart). No overrun policy, and no overrun counter, so it is
  invisible as well as unbounded.
* **#506** — transport above application tiers is the right default but has no
  budget; inbound overload preempts every tier for ~200 ms bursts.
* **#579** — the NuttX boot tier never adopts its declared priority, so a
  `[tiers.*.nuttx] priority` ordering can silently INVERT.
* **#271** — a footprint regression nothing gates, so the image stopped fitting
  256 KB between two commits.
* **#557** — a Zephyr boot failure that the readiness timeout HIDES.

In each case the system does something defensible-looking and wrong, without
saying so.

---

## W1 — Re-measure the Orin SPE footprint before acting (#271)

The regression is ~+195 KB between `d9af52be` and `21a3a4248`; a minimal
`Executor::open`+spin image no longer fits 256 KB BTCM.

**This number is likely stale in the project's favour.** `58d271471`
(2026-08-15, issue 0563) landed "carve the remap table — Executor 11632 → 4992
bytes". Re-measure before designing anything; a large part of #271 may already
be recovered.

CLAUDE.md's rule applies directly: re-measure any perf number on cleanly rebuilt
fixtures before acting on it (archived issues 0148/0164).

**Acceptance.** A current BTCM figure for the minimal image on this tree, and
either #271 closed with evidence or restated with the remaining delta. If it
still overflows, a size gate is the follow-on — the regression went unnoticed
because nothing measured it.

**ATTEMPTED 2026-08-15 — the figure cannot be produced, and the reason is the
result.** The repro lives in `autoware_sentinel`, which pins nano-ros by git rev
— and that pin is still `d9af52be`, the GOOD one, so the sentinel as checked out
builds the image that FIT. Re-measuring means bumping to current main, where
three things it names are gone: `nros-board-orin-spe` (crate removed),
`platform-orin-spe` (feature removed — phase-337 W7.b, a back-compat alias for
`platform-freertos`), and `rmw-zenoh` (retired by RFC-0054; the same removal
that had broken `just book`, issue 0581).

So W1 is a CONSUMER PORT before it is a measurement, and until that port lands
#271's number can never be refreshed — which blocks anything downstream that
needs to know the current footprint. Recorded in the issue with the table of
what moved. This phase's suspicion that `58d271471` already recovered much of
the regression is plausible and remains UNTESTED; testing it needs an armv7r
build of the minimal image, i.e. the port.

A host-side `EXECUTOR_OPAQUE_U64S` under #271's knob set was measured (18031
u64s ≈ 144 KB) and deliberately NOT offered as the answer: the budget is a
256 KB BTCM on armv7r, where pointers are half the width.

## W2 — Timer overrun policy and counter (#505)

Two separable pieces:

1. **A counter** — make overrun observable. Small, and it turns every future
   report of this from anecdote into data.
2. **A policy** — what *should* happen after a stall: replay all, replay one,
   skip to now, or configurable. This is a design decision and should be stated
   in an RFC or in the RT scheduling design rather than chosen implicitly by the
   implementation.

Do (1) first; it is independently useful and it informs (2).

**Acceptance.** An overrun is counted and reportable. The policy is written down
with its rationale before it is coded, and the default is stated in the docs.

## W3 — A budget for the transport band (#506)

The history is worth stating precisely because it was measured: issue 0567 found `_zp_unicast_read` RESETS its receive buffer on every call, so
the drain loop cannot stop early without losing frames. Capping the loop at 4/16
frames improved cadence (stalls 10 → 4/5, missed periods 1.79 % → 0.59/0.85 %)
but collapsed inbound delivery 282 → 10 msg/s — **a drop policy, not a budget**.

**That blocker has since cleared.** #567 was RESOLVED on 2026-08-14: the reset
is now conditional in the zenoh-pico fork (`43ddb0ec`), which the superproject
pointer already carries. So the resumable rx path #506 was waiting on exists,
and this work item is **actionable now** — it was written as blocked and is not.

Re-read #506 against the post-`43ddb0ec` runtime before designing: the numbers
in it were measured against a receive path that reset unconditionally, so both
the overload behaviour and the cost of a cap may differ.

**Acceptance.** A budget that bounds preemption WITHOUT reducing steady-state
delivery. The control #567 established is the baseline any proposal must beat: a
cap of 1 degenerates to the pre-loop single-frame path and matched unbounded on
every column, while 4/16 improved cadence and collapsed delivery 282 → 10 msg/s.
Report the same columns so the comparison is direct.

**REVISITED 2026-08-15 — blocker confirmed cleared IN CODE; both tables are now
stale, including the baseline.** `43ddb0ec` makes the reset conditional (reset
only when the buffer is empty, else `_z_zbuf_compact`), and its own message says
the unconditional reset "is why … a budget on that loop is lossy rather than
deferring work". So **#567's conclusion — "a frame cap here is a drop policy,
not a budget" — is no longer true by construction**: an early exit leaves the
remainder buffered.

One amendment to the acceptance above: the #567 control cannot be *the baseline*
as written, because it too was measured pre-`43ddb0ec`. It has to be re-taken
alongside the capped cells. The falsifiable question is narrow — does a cap
still cost delivery? If inbound rx/s holds near unbounded while stalls and miss%
improve, the frame cap IS the budget and the remaining design work is choosing
the cap and exposing the deferral counter.

NOT run here: the four columns come from `NEWSLabNTU/nano-ros-rt-eval` on the
FreeRTOS mps2-an385 QEMU lane, which is not present on this host, and nothing
in-tree measures them (`nros-bench/stress-zenoh` is a native throughput bench).
Same shape as W1 — the analysis is in-tree, the measurement lives in a consumer
repo. Details and the restated experiment are in issue 0506.

## W4 — NuttX boot tier drops its declared priority (#579)

A `[tiers.*.nuttx] priority` ordering can silently invert. Filed 2026-08-14
alongside a stdout panic hook.

This is the smallest and most clearly-wrong item in the phase: a declared value
is not adopted. It also has a nasty adjacency — the NuttX `pthread_attr_t`
mirror overflow (#569/#570/#572) was three issues written from three symptoms of
one overflow, and priority handling sits in the same area. Check whether #579 is
a fourth symptom before treating it as independent.

**Acceptance.** The boot tier runs at its declared priority, verified by
observing the ordering rather than by reading the code. The check for
"fourth symptom of the mirror overflow" is recorded either way.

**DONE 2026-08-15, except the runtime observation — which is blocked, and the
blocker is new.** The fix landed (`64fee4e60`): the boot tier adopts its
declared priority through the same shim its own board's C arm uses. Then the
guest run the acceptance demands turned up something else.

* **The gate was narrower than the rule.** `sched_dims_applied_e2e`'s
  tier-priority cell asked only whether the accept marker appeared ANYWHERE in
  the log; the spawned tier's line satisfied that for the whole image, so the
  cell was green throughout #579. Replaced with a per-tier, per-value shape
  (`EachTierOrFailNote`) — the issue-0196 class, and the reason the knob could
  be accepted and discarded unnoticed.
* **"Fourth symptom of the mirror overflow?" — no.** #579 already establishes
  this from an execution trace and records why the misreading repeats; verified
  independently here: `check-nuttx-libc-struct-sizes` is green
  (`pthread_attr_t` 56 B vs mirror 56 B).
* **The ordering could not be observed** — filed as **issue 0583**. On the
  `workspace-rust-nuttx-realtime` fixture the boot tier never resumes after
  spawning the low tier, so it never reaches the priority call; being the
  session owner, its stall means nothing is flushed and the router drops the
  guest on lease expiry ~7 s in. Evidence: guest console, a NIC packet dump
  (one TCP connection, guest silent after ~7 s, unanswered router FINs), a
  revert-rebuild at `64fee4e60^` producing an identical console, and the C++ arm
  of the SAME board running the same workspace correctly for 60 s at the
  expected ~10:1 tick ratio.

**W4 COMPLETE later the same day — 0583 was fixed, and the observation landed.**
0583 turned out not to be a scheduling bug at all: the image linked a `std`
compiled 2026-08-10 against crates.io `libc`'s 20-byte `pthread_attr_t` while
NuttX's is 56, so every thread spawn wrote 36 bytes past the attr on
`Thread::new`'s own frame and the caller returned to ~0. Issue 0570's fork fix
never reached those artifacts because the workspace fixture signature was blind
to the vendored libc and these rows set `skip_probe = true`. Fixed by hashing the
pin into the signature AND dropping the build-std artifacts when it moves.

With that cleared, the guest shows exactly what this item asked for:

```
nros: tier priority set tier=`high` prio=110      <- the boot tier
nros: tier priority set tier=`low`  prio=100
nros: tier `high` alive — 3000 spin(s), 2437 timer(s) fired, 0 error(s)
nros: tier `low`  alive —  300 spin(s),  142 timer(s) fired, 0 error(s)
```

Ordering observed rather than read: the ~10:1 ratio matches the declared 1 ms /
10 ms periods, both tiers publish, and the guest survives the full run. Issues
0579 and 0583 both resolved.

## W5 — Zephyr Cyclone action images fail at boot, hidden by a timeout (#557)

`tid … is in use!` and `rc=-100` at boot; the readiness timeout converts an
immediate, specific failure into a slow, generic one.

Fix the hiding first — that is phase-356's principle applied here, and it is
usually cheap. A boot failure that reports itself is a different debugging
problem from one that times out.

Note the Zephyr-specific hazard already documented: Cyclone on Zephyr now uses a
NATIVE ddsrt sync backend (`DDSRT_WITH_ZEPHYR` picks the types,
`nros_rmw_cyclonedds.cmake` swaps the TU) and **both halves move together or the
layouts disagree** — and `k_mutex` is recursive where a pthread NORMAL mutex
deadlocks, so a self-relock bug hangs natively and passes on Zephyr.

**Acceptance.** The boot failure surfaces as itself, with `rc=-100` and the tid
message attributed. Then the underlying cause.

---

## Deliberately not doing

* **No new executor.** Every item here is a policy or a diagnostic gap in the
  runtime that exists.
* **No footprint work before W1's re-measurement.** Explicitly the trap this
  phase opens by naming.
* **Not touching #532** (platform clock resolution). Phase 352 is COMPLETE and
  its title claims exactly that scope; #532 should be checked for staleness
  against it rather than re-planned here.
