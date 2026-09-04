# Phase 424 — does the build graph tell the truth about what needs rebuilding?

**Status (2026-09-04).** Not started — opened as a HOME for eight issues that
had none. Nothing here is in progress; what the phase adds today is the
constraint in "What this phase must not do", which is the part that would
otherwise be rediscovered per issue.

**Opened 2026-09-04 as a HOME.** Eight open issues say the same thing from
different layers: something in this tree reports FRESH when it is not, or STALE
when it is not, or links an artifact that is neither. None of them had a phase.

## Why they are one phase and not eight bugs

Every one is a failure of the same contract — *the thing that decides whether to
rebuild knows what the artifact was built from* — and the failures are
symmetrical, which is what makes the grouping useful rather than tidy:

* **False FRESH** hands the test a museum binary, and the run reports on code
  that is not in the tree. Issues 0820 and 1050.
* **False STALE** hands the developer a rebuild they did not earn, and the ones
  here are not merely slow: 0835's two families re-stale *each other*, so the
  cost is unbounded rather than one wasted build.
* **Converges, but not when anyone reads it** — 1002's derived knob needs THREE
  configures where 0991 documented two, so the first two answers are wrong and
  nothing says so.
* **No edge at all** — 1018's codegen change invalidates every consumer's
  generated interfaces with only a manual step connecting them.

The reason to hold them together is that the remedies collide. Every one of
these is tempting to fix by widening what the prober watches, and each widening
makes 0835's re-staling worse; 0945 is the standing warning that the mechanism
they all rest on is built on build-system internals nobody supports.

## The issues

| issue | layer | shape |
| --- | --- | --- |
| [#0820](../issues/0820-riscv-nuttx-c-talker-no-runtime-delivery.md) | cmake seam | passes after `rm -rf` on unmodified sources — no rebuild edge |
| [#0835](../issues/0835-fixture-staleness-probe-families-restale-each-other.md) | fixtures | the cmake and rust families re-stale each other |
| [#0945](../issues/0945-shared-cargo-dir-rests-on-unsupported-build-internals.md) | cargo | the shared-cargo-dir campaign rests on five unsupported build-system assumptions |
| [#1002](../issues/1002-a-derived-knob-needs-three-configures-not-two.md) | cmake | a derived knob converges after three configures, not the two 0991 documented |
| [#1018](../issues/1018-a-codegen-change-invalidates-generated-interfaces-and-only-a-manual-step-connects-them.md) | codegen | a codegen change invalidates every consumer, connected only by a manual step |
| [#1046](../issues/1046-px4-stale-tree-guard-checks-a-surviving-directory.md) | px4 | the stale-tree guard asserts a DIRECTORY that outlives the build that linked it |
| [#1050](../issues/1050-px4-demo-links-whatever-archive-was-built-last.md) | px4 | links whatever `libnros_cpp.a` was built last |
| [#1056](../issues/1056-session-churn-window-too-short-for-start-skew.md) | test window | a check that can pass on the build it exists to reject |

#1056 is here rather than with the RTOS work because its defect is the same
shape: a verdict whose window is too small to observe the thing it claims to
measure.

## What this phase must not do

**Widen a watch set without measuring 0835.** That is the move each of these
invites, and it is how the two families came to re-stale each other. Issue 1045
just landed the other half of the same lesson: a probe that examined NOTHING
reported FRESH across the whole cross-compiled tree, and the fix was to make the
degradation VISIBLE rather than to watch more.

## Acceptance

* Each issue resolved or reassigned, with the reason recorded.
* For the ones that close by changing a watch set: a measurement that 0835's
  re-staling did not get worse, in the same commit.
* 0945's five assumptions are either supported by something we can point at, or
  written down as accepted risk with what would break if each fails.
