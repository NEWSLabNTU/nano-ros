---
id: 537
title: "Two of the four FVP build recipes produce an artifact nothing runs, and no FVP fixture is reachable from build-test-fixtures"
status: resolved
type: bug
area: testing, build
related: [phase-217, phase-298, issue-0232, issue-0535]
---

## Problem

`just/zephyr-setup.just` carries four FVP build recipes:

| recipe | artifact | runner |
| --- | --- | --- |
| `build-fvp-ws-entry` | `build-fvp-ws-entry/zephyr/zephyr.elf` | `fvp_runtime_ws.rs` + `verify-fvp-runtime` |
| `build-fvp-board-import` | `build-fvp-board-import/zephyr/zephyr.elf` | `fvp_smoke.rs` |
| `build-fvp-aemv8r-cyclonedds` | `build-fvp-aemv8r-cyclonedds-talker/zephyr/zephyr.elf` | **none** |
| `build-fvp-aemv8r-cyclonedds-rust` | `build-fvp-aemv8r-cyclonedds-rust-talker/zephyr/zephyr.elf` | **none** |

The last two build `examples/zephyr/{cpp,rust}/talker-aemv8r`. Their runners
were `fvp_runtime.rs` / `fvp_runtime_rust.rs`, DELETED by phase-298 W4
(`68a0a0b6f`, "retire false-green legacy tests (resolves 0232)"). The `run-`
recipes survive, so the lane still looks complete from the justfile.

This is already recorded, accurately, in the one place nobody reads while
adding a fixture — `examples_fixture_coverage.rs:62`, whose `TEST_DRIVEN_BUILDERS`
entry says:

> phase-321 W3.c: this used to say they were "run by `fvp_runtime.rs` /
> `fvp_runtime_rust.rs`", two files phase-298 W4 DELETED. Nothing runs these two
> examples today [...] A comment claiming a runner that does not exist is how a
> build-only lane reads as covered.

So the gate's allowlist is honest and the recipes are not.

## Second half: none of the four is reachable from the fixture build

`grep -rn build-fvp justfile just/` returns hits ONLY inside
`just/zephyr-setup.just`. `build-test-fixtures` fans out to
`just <platform> build-fixtures` for nine platforms; no path reaches
`build-fvp-*`. Both consuming tests therefore skip on a missing ELF:

```
fvp_runtime_ws.rs:112   "FVP ws-entry ELF missing at {}; run `just zephyr build-fvp-ws-entry` first"
fvp_smoke.rs:121        "FVP board-import fixture ELF missing at {}; run `just zephyr build-fvp-board-import` first"
```

A skip is the right behavior for a license-gated model (Arm FVP is
`[gated.arm-fvp]` in `nros-sdk-index.toml`; nano-ros does not download it). The
defect is that the skip is indistinguishable from "the gated SDK is absent" and
"nobody ever built this" — there is no fixture row, no coordinate, and no stamp
saying which.

## Context: phase-217 is OPEN

`docs/roadmap/phase-217-arm-fvp-local-runtime.md` — **Status OPEN, Track A
landed 2026-06-03.** The build half works; the run half is the phase's stated
goal. The two runnerless artifacts above are exactly the slice it was opened to
close ("unblocks every other FVP slice (rust example, smoke test, book
chapter)").

## Direction

Decide per artifact, and record the decision where the build is declared rather
than in a test allowlist:

* `talker-aemv8r` ×2 — either restore a runner under phase-217, or retire the
  recipes and the examples together. Do not leave a build with no consumer; that
  is what 0232 was about.
* All four — give them `[[fixture]]` rows (issue 0535) with the gated-SDK
  condition expressed as a row property, so "gated, not built" and "should have
  been built, wasn't" stop sharing one skip message.

## Resolved 2026-08-13 (phase-350 W3) — retired, not restored

Maintainer decision: FVP support is wanted later, but there is no effort for it
now, and unused code should not sit in the tree waiting. So the half with no
consumer is GONE and the half with consumers is kept intact, which is what a
future revival needs anyway.

**Retired:**

* `just zephyr build-fvp-aemv8r-cyclonedds` and `-rust`
* their `run-fvp-aemv8r-cyclonedds{,-rust}` siblings
* `examples/zephyr/rust/talker-aemv8r` and `examples/zephyr/cpp/talker-aemv8r`
  (~1 MB of source), and the rust one's root-workspace membership
* the two `TEST_DRIVEN_BUILDERS` entries that had been excusing them in
  `examples_fixture_coverage.rs` — an allowlist can only excuse a gap; deleting
  the code is what closes it

**Kept, because each still has a consumer:**

| kept | consumer |
| --- | --- |
| `fvp-aemv8r-smp` board crate + `nano_ros_use_board()` | the import surface itself |
| `west_board_import` fixture | `board_import.rs`, which runs in CI — it reads `CMakeCache.txt` and needs no FVP binary |
| `build-fvp-board-import` + `run-` | `fvp_smoke.rs` (gated on the model) |
| `build-fvp-ws-entry` + `run-` + `verify-fvp-runtime` | `fvp_runtime_ws.rs` (gated on the model) |
| the `[gated.arm-fvp]` installer + SDK index entry | provisioning, unchanged |

`build-fvp-all` now aggregates only `build-fvp-ws-entry`.

**Docs corrected rather than left dangling:** the ARM FVP book chapter documented
the deleted recipes as its Build/Run path and linked a README that no longer
exists; it now documents the ws-entry and board-import lanes. Same for
`supported-boards.md`, `environment-variables.md`, `examples/zephyr/README.md`
and the board crate's own README, which used the deleted example as its usage
sample.

**The second half of this issue stands and is NOT closed by the deletion:** none
of the surviving FVP artifacts is reachable from `build-test-fixtures`, so both
gated tests still skip with a message that cannot distinguish "license-gated SDK
absent" from "nobody built it". That is phase-217's to fix when FVP work
resumes; it is recorded in phase-350 W3 and in the phase-217 doc.
