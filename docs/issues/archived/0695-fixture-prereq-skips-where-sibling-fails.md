---
id: 695
title: "One missing CLI, two policies in one script — `cmake_fixture_prereqs_ok` skips green where `stage_and_check` fails hard"
status: resolved
type: bug
severity: medium
area: build, testing
related: [issue-0650, issue-0532]
---

## Symptom

Running `just build-test-fixtures lane=native` **without** `source ./activate.sh`
(so `nros` is not on PATH, though `packages/cli/target/release/nros` exists)
produces both of these, from the same script, for the same missing binary:

```
cmake-fixtures: nros CLI absent — skipping
compile-check: nros CLI not found — cannot resolve staged models (just setup-cli)
make: *** [.../compile-check-949970.mk:20: u4] Error 2
```

`scripts/build/compile-check-fixtures.sh:182` (`cmake_fixture_prereqs_ok`)
prints to stderr and returns 1 — the cmake fixtures are **skipped and the run
continues**. `:137` (in the staging path) returns 2 for the same condition and
takes the whole build down.

Observed while running the fixture build for phase-368; the operator error
(missing `source ./activate.sh`, which CLAUDE.md's sweep contract requires) is
real and mine. The defect is that the tree answers it two ways at once.

## Why the skipping half is the wrong half

The hard failure is the correct behaviour here, and the repo already says so in
two places:

* CLAUDE.md — "Tests must fail on unmet preconditions … Bare `eprintln!`+`return`
  reports PASS — never." The same reasoning applies to a BUILD step: a skipped
  cmake fixture is indistinguishable, later, from one that built.
* The build already HAS a mechanism for a visible skip — `nros_lane_skip_note`
  (`scripts/build/lane-skip.sh`, issue 0650), which records the reason so a lane
  reports SKIPPED instead of silently green. `cmake_fixture_prereqs_ok` does not
  use it. Its three skip paths (`cmake` absent, CLI absent, CLI lacks
  `codegen entry`) all print and vanish.

So the run's summary line — `fixtures built (check=1 build=0 cmake=0 cxx=0 …)` —
reports `cmake=0` for "skipped every one of them" exactly as it would for
"there were none to build".

## A second instance of the same theme, found the same day

`just test-all`'s junit rewrite turns `nros_tests::skip!` panics into
`<skipped>` — but only for the `nros-tests` suites. A `skip!` raised inside
another package's own test binary is not rewritten, so it lands in the summary
as a real failure. Observed with
`nros-rmw-zenoh::zenoh_integration two_sessions_deliver_cross_session_through_router`,
whose `skip!` ("second session refused — shim built with `ZPICO_MAX_SESSIONS=1`")
is the documented 0347 contract:

```
rewrite-skipped-junit: skips by class: capability=3  lane=1
rewrite-skipped-junit: rewrote 4 [SKIPPED] failure(s) to <skipped>
Real failures: 2 / 2 total failures        # one of the two is that skip
```

Same shape as the entry above — one condition, two policies depending on which
half of the tree raises it — and the same consequence: a reader has to know
which package a red came from before knowing whether it means anything. Related
to issue 0319, where a backend's own suite sitting outside the normal lane held
a red on main for two days.

## Resolution

Both halves, and they were fixed a fortnight apart.

### The prereq split (`scripts/build/compile-check-fixtures.sh`)

The four prereqs answered to ONE verdict and do not deserve the same one, so
they now split by whose fault the absence is:

| prereq | verdict | why |
| --- | --- | --- |
| `cmake` | recorded skip | a host that cannot build C at all. Genuinely optional. |
| `nros` on PATH | **fatal, exit 2** | the sweep contract. `stage_and_check` already took the whole build down for this exact binary. |
| `nros` has `codegen entry` | **fatal, exit 2** | a stale CLI — `just setup-cli`. |
| `play_launch_parser` | **fatal, exit 2** | same PATH, same `source ./activate.sh`. |

The fatal arms name the remedy (`source ./activate.sh`, `just setup-cli`)
instead of printing "skipping", so the operator error this issue was filed from
now fails at the prereq rather than twenty minutes later at a missing artifact.

The skipping half was also wrong on its own terms, independently of which arm
raised it: `cmake=0` in the summary read identically for "skipped every one of
them" and "there were none to build". Skips now go through `_note_lane_skip`
and the summary reports `cmake=SKIPPED(cmake absent)`, with every skipped lane
re-listed at the end. The same treatment went to the other two lanes with the
identical shape — `cxx-syntax: no C++ compiler` and `px4: PX4-Autopilot
submodule absent` — because the class is "a count that means two things", not
the one site where it was noticed.

Verified by running the prereq function against three environments:

```
full env                → PREREQS OK,   summary: cmake=7
nros off PATH           → rc=2,         "nros CLI not found — cannot codegen
                                         entries (source ./activate.sh, or just setup-cli)"
cmake absent            → rc=0 recorded, summary: cmake=SKIPPED(cmake absent)
```

### The junit half — already fixed

`scripts/test/skip_marker.py` (commit `58d4b62eb`, "a skip in system-err is
still a skip — one classifier, both readers") landed after this issue was
filed and closes the second half exactly. Its docstring opens on this issue's
own example: the `nros-rmw-zenoh::zenoh_integration` skip lands in
`<system-err>` rather than in `<failure>`'s `message`, and both readers
(`rewrite-skipped-junit.py`, `name-real-failures.py`) now go through the one
classifier. `check-skip-marker-matching.py` gates the Rust side against
hand-rolled marker matching.

### Residue — a gate narrower than its rule

`scripts/check-lane-skip-protocol.py` enforces the skip protocol, but its scan
set is `justfile` + `just/*.just`. This defect was in `scripts/build/*.sh`, so
the gate could never have seen it — the issue-0196 shape again. Widening it was
considered and not done: the banned pattern there is a RECIPE-level "announce a
skip, `exit 0`", and inside a build script the same regex would flag every
legitimate per-item skip (an uninstalled cross target for one fixture id), which
is a case *inside* an already-running step and not a lane precondition at all.
A gate that noisy gets deleted. Recorded here instead.

## Fix sketch (as filed)

Either make the CLI-absent arm fatal like its sibling, or route all three
`cmake_fixture_prereqs_ok` arms through `nros_lane_skip_note` so the skip is
recorded and surfaces in the lane's skip summary. A genuinely optional prereq
(`cmake` absent on a host that never builds C) is a reasonable skip; a missing
`nros` on a build that needs it to resolve models is not — and today they are
spelled the same.

Not urgent: the sourced-environment path (the documented one) has the CLI, so
this only bites someone who skipped `source ./activate.sh`. It bit by producing
a *partial* fixture set that later reads as complete.
