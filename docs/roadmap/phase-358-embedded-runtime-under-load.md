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
