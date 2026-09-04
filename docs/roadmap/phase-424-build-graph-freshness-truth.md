# Phase 424 — does the build graph tell the truth about what needs rebuilding?

**Status (2026-09-04).** Enabling work done, seven issues untouched. 0835 — the
one every other fix has to be checked against — has been re-measured, its
remaining scope narrowed to a disk-waste defect, and the property that ended its
oscillation is now gated by `just check fixture-staleness-probes`. The rest of
the phase is not started. What it adds beyond that is the constraint in "What
this phase must not do", which is the part that would otherwise be rediscovered
per issue — and which is now backed by numbers rather than by a warning.

**Opened 2026-09-04 as a HOME.** Eight open issues say the same thing from
different layers: something in this tree reports FRESH when it is not, or STALE
when it is not, or links an artifact that is neither. None of them had a phase.

## Why they are one phase and not eight bugs

Every one is a failure of the same contract — *the thing that decides whether to
rebuild knows what the artifact was built from* — and the failures are
symmetrical, which is what makes the grouping useful rather than tidy:

* **False FRESH** hands the test a museum binary, and the run reports on code
  that is not in the tree. Issues 0820 and 1050. **1050 turned out NOT to be a
  missing edge** (measured 2026-09-05): `bin/px4` carries a real `|` dependency
  on `libnros_cpp.a`, so it relinks when the archive moves. Nothing decided
  WHICH archive should be there — a different failure, one the build graph
  cannot express, and the remedy was a configure-time assertion rather than an
  edge or a wider watch set.
* **False STALE** hands the developer a rebuild they did not earn, and the ones
  here are not merely slow: 0835's two families re-stale *each other*, so the
  cost is unbounded rather than one wasted build.
* **Converges, but not when anyone reads it** — 1002's derived knob needs THREE
  configures where 0991 documented two, so the first two answers are wrong and
  nothing says so. RESOLVED 2026-09-05: three is correct and ninja runs all
  three, so a `west build` is right; the docs were one short and the BOUND was
  counting arms over the build dir's lifetime, which made a directory stop
  converging after two declaration edits.
* **No edge at all** — 1018's codegen change invalidates every consumer's
  generated interfaces with only a manual step connecting them.

The reason to hold them together is that the remedies collide. Every one of
these is tempting to fix by widening what the prober watches, and each widening
makes 0835's re-staling worse; 0945 is the standing warning that the mechanism
they all rest on is built on build-system internals nobody supports.

## The issues

| issue | layer | shape |
| --- | --- | --- |
| ~~[#0820](../issues/archived/0820-riscv-nuttx-c-talker-no-runtime-delivery.md)~~ | cmake seam | RESOLVED — a cargo custom command with no `DEPFILE` had no edge on the Rust it compiles. Three such commands exist; two were still missing one. Gated by `check-cargo-custom-command-depfile`, which WIDENS no watch set: the depfile is the graph cargo already computes, so 0835 is untouched |
| [#0835](../issues/0835-fixture-staleness-probe-families-restale-each-other.md) | fixtures | the cmake and rust families re-stale each other — **oscillation fixed + gated 2026-09-04**; open for the duplicated ThreadX corrosion group, which is wasted disk, not staleness |
| [#0945](../issues/0945-shared-cargo-dir-rests-on-unsupported-build-internals.md) | cargo | the shared-cargo-dir campaign rests on five unsupported build-system assumptions |
| [#1002](../issues/archived/1002-a-derived-knob-needs-three-configures-not-two.md) | cmake | RESOLVED — three is the chain's depth, not a defect; the defect was a bound counting the build dir's lifetime |
| [#1018](../issues/1018-a-codegen-change-invalidates-generated-interfaces-and-only-a-manual-step-connects-them.md) | codegen | a codegen change invalidates every consumer, connected only by a manual step |
| [#1046](../issues/archived/1046-px4-stale-tree-guard-checks-a-surviving-directory.md) | px4 | RESOLVED 2026-09-05 — the guard asserted a DIRECTORY that outlives the build that linked it; it asserts `bin/px4`'s CONTENT now, and the three "not covered" sweeps came back empty |
| [#1050](../issues/1050-px4-demo-links-whatever-archive-was-built-last.md) | px4 | links whatever `libnros_cpp.a` was built last — the recipe (1) and the configure-time guard (2) are fixed; open for (3), `nros::init()` taking slot 0 |
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

**The measurement now exists, so "measuring 0835" is a command rather than a
project** (2026-09-04). The numbers are in issue 0835 under "Measured
2026-09-04"; the short version a widening has to answer to:

* The 237 cmake + cargo rows are **differential** — their verdict is "did my
  artifact's bytes move", so they have no watch set and no widening can reach
  them. `just check fixture-staleness-probes` (2.2 s, on the fast line) holds
  them there, with the pre-fix rules as its own negative control.
* The 134 `.inputsig` rows are the ones with a watch set, and today it contains
  **zero build output** — 568 + 243 tracked files across the signature dirs with
  0 untracked-and-unignored, and 792 dep-closure paths with 0. That is what a
  widening must not change.
* The cost of a widening is arithmetic: `(rows that gain the path) x (how often
  the path moves)`. A tracked source path is free until edited; a path a build
  WRITES is unbounded, and is how 0835 happened.
* A new input hashed into all 134 rows must key on what the tool EMITS, not on
  its binary — measured 41 distinct `nros` binaries against 9 distinct codegen
  fingerprints, i.e. 78 % of CLI rebuilds re-staling nothing.

## Acceptance

* Each issue resolved or reassigned, with the reason recorded.
* For the ones that close by changing a watch set: a measurement that 0835's
  re-staling did not get worse, in the same commit.
* 0945's five assumptions are either supported by something we can point at, or
  written down as accepted risk with what would break if each fails.
