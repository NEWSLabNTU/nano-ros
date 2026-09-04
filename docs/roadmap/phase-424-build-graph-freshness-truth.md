# Phase 424 — does the build graph tell the truth about what needs rebuilding?

**Status (2026-09-05).** **Six of eight closed and archived** — #0820 (a cargo
custom command with no `DEPFILE`), #0835 (the probe families' oscillation, plus
the six leaves that misreported their own platform), #0945 (the assumption
register), #1002 (three configures is the chain's depth, not a defect), #1046
(the PX4 stale-tree guard) and #1056 (the session-churn window). **Two remain
open on their second halves**: #1018 for the stale-CLI refusal, and #1050 for
defect (3), `nros::init()` taking slot 0.

Worth carrying up, because it is this phase's own thesis holding: #0945, #1050
and #1056 all closed by DERIVATION or by a configure-time assertion rather than
by widening a watch set, so the constraint in "What this phase must not do" was
never paid for. And three of the eight turned out to be a real defect whose
stated MECHANISM was wrong (#1018, #1050, #1056) — in each case the priced fix
would have bought nothing.

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
  generated interfaces with only a manual step connecting them. RESOLVED
  2026-09-05: the missing edge was not the one filed. The three cmake consumers
  each already depend on the tool binary; the `generated/` regeneration stamp
  (`scripts/build/codegen-stamp.sh`) watched ONE file in `nros-core` and nothing
  about the emitters, so in the one lane whose `nros sync` is conditional
  (`just/zephyr-ci.just`) a `rosidl-codegen` edit left every Zephyr Rust leaf on
  museum message crates. Fixed by hashing `nros codegen-fingerprint` into that
  stamp — the emit-keyed term this phase requires, not the binary.

The reason to hold them together is that the remedies collide. Every one of
these is tempting to fix by widening what the prober watches, and each widening
makes 0835's re-staling worse; 0945 is the standing warning that the mechanism
they all rest on is built on build-system internals nobody supports.

## The issues

| issue | layer | shape |
| --- | --- | --- |
| ~~[#0820](../issues/archived/0820-riscv-nuttx-c-talker-no-runtime-delivery.md)~~ | cmake seam | RESOLVED — a cargo custom command with no `DEPFILE` had no edge on the Rust it compiles. Three such commands exist; two were still missing one. Gated by `check-cargo-custom-command-depfile`, which WIDENS no watch set: the depfile is the graph cargo already computes, so 0835 is untouched |
| ~~[#0835](../issues/archived/0835-fixture-staleness-probe-families-restale-each-other.md)~~ | fixtures | **RESOLVED** — the re-staling was fixed and gated 2026-09-04 (`just check fixture-staleness-probes`); the residual duplicated ThreadX corrosion groups closed 2026-09-05 by keying the shared cargo dir on the resolved FEATURE SET rather than the platform label, which correlates with the artifacts instead of deciding them |
| [#0945](../issues/archived/0945-shared-cargo-dir-rests-on-unsupported-build-internals.md) | cargo | ~~the shared-cargo-dir campaign rests on five unsupported build-system assumptions~~ **RESOLVED** — six, classified, one gap named |
| [#1002](../issues/archived/1002-a-derived-knob-needs-three-configures-not-two.md) | cmake | RESOLVED — three is the chain's depth, not a defect; the defect was a bound counting the build dir's lifetime |
| [#1018](../issues/1018-a-codegen-change-invalidates-generated-interfaces-and-only-a-manual-step-connects-them.md) | codegen | ~~a codegen change invalidates every consumer~~ the configure-time half is closed (binary-keyed) and the `generated/` regeneration stamp is closed (fingerprint-keyed, no `.inputsig` moved); **open** for the stale-CLI refusal |
| [#1046](../issues/archived/1046-px4-stale-tree-guard-checks-a-surviving-directory.md) | px4 | RESOLVED 2026-09-05 — the guard asserted a DIRECTORY that outlives the build that linked it; it asserts `bin/px4`'s CONTENT now, and the three "not covered" sweeps came back empty |
| [#1050](../issues/1050-px4-demo-links-whatever-archive-was-built-last.md) | px4 | links whatever `libnros_cpp.a` was built last — the recipe (1) and the configure-time guard (2) are fixed; open for (3), `nros::init()` taking slot 0 |
| [#1056](../issues/archived/1056-session-churn-window-too-short-for-start-skew.md) | test window | ~~a check that can pass on the build it exists to reject~~ **RESOLVED** — it can, and no affordable window fixes it |

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
  fingerprints, i.e. 78 % of CLI rebuilds re-staling nothing. Re-measured
  2026-09-05 on the same host's grown cache: **168 binaries, 11 fingerprints**
  (93 %). #1018 is the first fix held to this — it added the fingerprint to a
  THIRD consumer (`codegen-stamp.sh`) and moved no `.inputsig` at all.

## Acceptance

* Each issue resolved or reassigned, with the reason recorded.
* For the ones that close by changing a watch set: a measurement that 0835's
  re-staling did not get worse, in the same commit.
* 0945's five assumptions are either supported by something we can point at, or
  written down as accepted risk with what would break if each fails.


## Resolved: #0945, the assumption register (2026-09-05)

The acceptance was *"either supported by something we can point at, or written
down as accepted risk with what would break if each fails"*. Every item now
carries a verdict, an evidence line and a detection line. Three findings are
worth carrying up here:

**A claimed mitigation did not exist.** Item 3 said both `.fingerprint`-parsing
tools have a `--self-test` that would surface a cargo schema change. Neither did:
`nros-leaf-graph`'s never opened a json file, and `nros-shared-dir-churn`'s wrote
the same key names it read, so a rename moves both ends together. Both now REFUSE
(`INCONCLUSIVE`, rc 2) when records were read and none carried the key the answer
depends on; both self-tests mutate the key and are mutation-checked; and both
now run that self-test from `main` on the NORMAL path, so the control is
exercised exactly when someone is about to quote a number out of the tool. (A
dedicated `check` gate for the pair was tried first and `check-gate-selftests`
refused it — *"a negative control nobody runs decays into a comment. Call it from
main."* It was right; the gate is gone.) Measured ~10 ms and ~50 ms.

**A sixth assumption was found, and it is the register's one real gap.** Cargo's
`[<triple>/]<profile>/<bin>` output layout inside a `--target-dir` is globbed by
`rust-fixture-stale.sh`. If it moves, the probe finds nothing, silently falls back
to the pre-0835 `"fresh":false` signal, and #0835's permanent oscillation returns
dressed as a self-healing WARNING. Measured today: 116 of 117 rust rows find an
artifact, 120 of 120 cmake cells do, and exactly one row
(`packages/testing/qemu-smoltcp-bridge`) is on the silent fallback right now. The
remedy is specified in the issue — a `DEGRADED\t` line beside the existing
`FAILED\t` bucket, widening no watch set — and **LANDED 2026-09-05** — deferred here
for a day for want of a way to verify it. Both fallback branches announce
themselves, `check-fixtures-stale.sh` buckets and counts them as a WARNING, and
the COUNT is the signal: one is expected, more means a layout moved. Gated by
case **E** of `check-fixture-staleness-probes`, which builds a library-only cargo
leaf — the real shape, since that is why `qemu-smoltcp-bridge` is on the fallback
— and is mutation-verified. Cases A–D all have the artifact PRESENT, so none of
them reached this branch.

One defect surfaced in the writing, and it would not have failed loudly: a
degrading probe prints its marker line AND, if the fallback fires, the stale
line, while `check-fixtures-stale.sh` reads probe output through `mapfile` or
through one `$( )` capture depending on whether GNU `parallel` is installed — so
bucketing without re-splitting would have classified the pair **differently
depending on a tool being present on the machine**.

**The register's biggest item is not the one it named.** Item 4's stated risk (a
future cargo target-dir GC) has never fired; the side channel having no OWNER has,
six times (issues 0360, 0834, 0978, 0985, 0987, 1031), because every consumer grows
its own rule for which copy is authoritative. And the differential probe families
are structurally blind to it: a wrong generated header produces the same wrong
bytes twice, so the verdict is FRESH.

### Carried forward as work

Item 4's consumer migration off `write_header_to_target_dir`. Counted, not
estimated: **128 hits across 26 files** (`git grep -n
'nros-c-generated\|nros-cpp-generated' -- ':!docs'`), including
`NanoRosNodeRegister.cmake`, `NanoRosVerbs.cmake`, `integrations/nuttx/Make.defs`,
`integrations/px4/NanoRosPx4Module.cmake`, `zephyr/CMakeLists.txt`, and 49 hits in
`just/check.just`. A wave, not an afternoon.

## Resolved: #1056, the session-churn window (2026-09-05)

Same shape as the PX4 pair, one layer over: the reported defect is real and its
stated MECHANISM is not, so the priced fix buys nothing.

`assert_no_session_churn` can pass on a build it exists to reject. But not because
of start skew — both nodes must participate for all 60 samples, so each one's
session life is at least that span whatever the skew, and the nine stored router
logs measure skew from 0.00 s to 20.0 s with the later node alive ~60 s in every
one. The real reasons are that **only the talker lapses** (the listener is fed a
1 Hz stream, which resets its lease; our fork's own `config.h` says a pure
publisher is the one that closes) and that **the lapse period is `2 x lease` only
while `2 x lease < 30 s`** — above that it is a beat against the router's 30 s
keep-alive, and it diverges: 899 s at a 29 s lease, 9000 s at 29.9 s.

So the filed acceptance ("every lease in `(0, 30_000)` fails this cell") is
unreachable at any window length, and the proposed 120 s moves the covered band
from `L <= 14 s` to `L <= 18 s` for +12 minutes across the twelve cells. Declined.
What shipped is the derivation, in the doc comments that had the wrong model, plus
`COVERED_LEASE_SECS` printed on every run so a PASS states its own scope. The
zenoh-pico default that a regression would revert to is 10 s, inside the band.

The free alternative is recorded and declined with a reason: dropping
`MAX_ROUTER_SESSIONS` to 2 buys the same `L <= 18 s` for no wall clock, but needs
a healthy-count measurement on all twelve cells first, and a flaky cell is worse
than four seconds of lease band.

## Narrowed: #1018, the codegen chain (2026-09-05)

**The reported defect was a false STOP; measuring it found a false FRESH one
layer over, and that is what got fixed.**

The issue said the staleness refusal "is the ONLY thing holding the chain".
Measured, it is not: the BUILD-time generator's codegen command has carried
`DEPENDS … ${_NANO_ROS_CODEGEN_TOOL}` since 2026-05-23, and touching the binary
in a minimal consumer re-runs codegen — then, thanks to `restat = 1` plus
codegen's write-if-changed, settles with nothing downstream rebuilt. (My first
hypothesis was that this edge would leave the command permanently dirty. Running
it refuted that; the cost of an emitter-identical `setup-cli` is one codegen run
per package, not a cascade.)

What has no edge is the CONFIGURE-time emitters, because `execute_process()`
cannot have one: their freshness is the question *does a configure happen*.
Four sites emit at configure time and **one** registered the tool — the
`nano_ros_entry()` site, inline, from issue #182. The Zephyr interfaces
generator carries the correct `IS_NEWER_THAN` predicate and it is unreachable on
an incremental build: measured, `build-rust-talker-zenoh`'s `RERUN_CMAKE` edge
has 3592 inputs and none under `packages/cli`. No `examples/zephyr/**`
CMakeLists calls `nano_ros_entry()`, so every single-example Zephyr C/C++ image
keeps museum generated code after a `nros` rebuild.

**Is this #0820's shape? No.** #0820's remedy is a `DEPFILE` on a `cargo` custom
command — a command that IS in the build graph, missing the file that lists its
inputs. Here the producer (`just setup-cli`) is deliberately not in the graph at
all, and the consumer half runs before the graph exists. The `DEPFILE` answer
does not apply; `CMAKE_CONFIGURE_DEPENDS` is the only edge a configure-time
emitter can carry.

**Remedy:** `nros_codegen_tool_reconfigure()` in `NanoRosCodegenCore.cmake`, one
spelling at all four sites, gated by `check-codegen-tool-reconfigure` (fast
line, 15 self-test cases on the normal path, mutation-verified at each site and
red on `origin/main` for all four).

**Widening cost: zero, measured.** Of 7 Zephyr build dirs here, the 3 that reach
a configure-time emitter already carry the CLI as a configure dependency; the
other 4 reach no emitter and the registration is at the emitter's call site.
#0835's re-staling is untouched.

**What the phase's fingerprint budget bought here — a refusal.** The obvious
narrowing is to key on `nros codegen-fingerprint` (41 binaries → 9 fingerprints;
78 % of rebuilds would cost nothing). It is wrong for three of the four sites:
the corpus covers the message/service/action emitters and NOT `codegen entry`
or `codegen-system`, so those two would report FRESH for a real emitter change.
And a configure cannot write its own configure input, so a fingerprint-keyed
file needs a producer on the CLI-build event first. Recorded in #1018 as the
follow-up rather than done badly.

**Still open in #1018:** the stale-CLI refusal itself. Measured: appending a
comment to `packages/cli/nros-cli-core/src/cmd/doctor.rs` — a file that cannot
change one emitted byte — makes every consumer `nros codegen` refuse. Narrowing
that watch set is rejected for now with the reason written down (the emitters
reach through `cargo-nano-ros`/`nros-cli-core`, so a crate-level closure is
nearly the whole closure and would not have excluded that file; and 0604
measured a hand-rolled closure wrong in both directions at once).

## Resolved: the PX4 pair (2026-09-04)

The two were one shape, and treating them as one is what found the third site.
#1050 is a FALSE FRESH — the link succeeds against the wrong input. #1046 is a
guard that cannot detect it. The remedy for both is the same sentence: **a check
must assert something that cannot outlive the build it reports on.** A directory
can. A file's CONTENT cannot.

**What was already on `main` when this started.** Both issue docs carried a
"FIXED" section while `status: open`, and both fixes had landed (`0a47f949a`,
`0364c5405`): the test guard now scans `bin/px4` for the module's command name,
and `build-sitl-example` builds the archive it links. Verified as ancestors of
`origin/main` rather than assumed from the prose.

**What was left, and it was live.** The class sweep of the PX4 integration found
`NanoRosPx4Module.cmake` guarding the per-build generated headers with
`IS_DIRECTORY` — 1046's predicate verbatim, one layer down, in the build system
rather than in a test. Measured in the shared checkout with no build running:
the C++ generated header present and naming a *zenoh* variant, and
`target/release/libnros_cpp.a` absent entirely. Both checks passed.

`integrations/px4/NanoRosArchivePairing.cmake` now pairs content against content:
the variant symbol is read OUT of the header (not recomputed — a second
derivation of a slug is the class `row_coord()` records) and the archive must
define it. That is exactly issue 0360's anchor condition, moved from LINK time
(~1100 targets into a ten-minute build) to configure time, where the message can
name the cargo command.

**Was this issue 0475's `LINK_DEPENDS` again? No — checked, and the answer is
no.** 0475's mechanism is a library reached through a raw `-Wl,` flag, which
CMake cannot see, so no edge exists. Here the archives reach the link as absolute
paths in `target_link_libraries()`, which CMake does resolve to a file-level
dependency. **Inferred, not measured** — no PX4 build was run — but the mechanism
differs from 0475 in the way that matters, so `LINK_DEPENDS` would have been
answering a question that was not being asked. The real defect was never a
missing edge; it was that *two independently-produced files had no assertion that
they agreed*, which no edge expresses.

**What this did not do:** it widened no watch set, so #0835's re-staling is
untouched by construction. The phase's own constraint held without needing to be
paid for.

**Boundary.** `third-party/px4/PX4-Autopilot` is an empty, uninitialised gitlink
on this host, so no PX4 build was run and nothing was verified through
`make px4_sitl_default`. The cmake predicate is measured against real
`libnros_cpp.a` artifacts and real generated headers, in both directions; its
behaviour *inside a PX4 configure* is inferred from it sitting at file scope in a
module every PX4 root includes.
