---
id: 1047
title: "zenoh-pico 1.8.0 tests a macro with `#if X == true`, which NuttX's
  `stdbool.h` cannot evaluate — every NuttX zenoh fixture fails to build on main"
status: open
type: bug
area: rmw, third-party, build
severity: high
found: 2026-09-04
related: [issue-1021, issue-0910, phase-415]
---

## Symptom

On `main` at `9a3ce6c24`, with the recorded zenoh-pico pin `0101b80d` (the
1.8.0 patch line), `just nuttx build-fixtures-arm` fails for every row:

    error: failed to run custom build command for `zpico-sys v0.5.0`
      .../nros-nuttx-export-arm/include/stdbool.h:79:25:
        error: missing binary operator before token "1"
      .../nros-nuttx-export-arm/include/stdbool.h:80:25:
        error: missing binary operator before token "0"

followed by a wall of `redefinition of '_z_chunk_special_includes'` and
siblings, which are downstream noise: the guard that should have excluded the
second copy is the directive that failed to parse.

The host build is clean (`cargo build -p zpico-sys` succeeds), which is why this
is invisible until someone builds for an RTOS.

## Cause, exactly

`keyexpr_match_template.h` is a multiple-inclusion template selected by a macro:

    include/zenoh-pico/session/keyexpr_match_template.h:132
      #if _ZP_KE_MATCH_TEMPLATE_INTERSECTS == true

    src/session/keyexpr.c:565   #define _ZP_KE_MATCH_TEMPLATE_INTERSECTS true
    src/session/keyexpr.c:567   #define _ZP_KE_MATCH_TEMPLATE_INTERSECTS false

`#if` evaluates a **preprocessor** expression, so `true` has to expand to an
integer constant. On glibc and most libcs it does. NuttX's does not:

    nros-nuttx-export-arm/include/stdbool.h:79
      #    define true  (bool)1
      #    define false (bool)0

A cast is not a preprocessor expression, so `#if X == true` is a syntax error
for BOTH values of the macro — the directive never selects a branch, both
template bodies are emitted, and the redefinitions follow.

Nothing here is nano-ros's: an unguarded `#if X == true` against C's own
`stdbool.h` guarantees is an upstream portability bug, and NuttX's cast-valued
`true` is legal (C99 only requires `true` expand to `1` in `#if` since it is an
integer constant expression — NuttX's cast breaks that, so both sides have a
claim, and neither is ours to arbitrate).

## Why it is severity high

* **Every** NuttX zenoh image — C, C++ and Rust, all twelve arm fixtures —
  cannot be built from `main`. `just nuttx build-fixtures-arm` is the documented
  entry point and it fails at the first row.
* **The nightly `nuttx` cell is red for exactly this, in CI, on every run in the
  window.** Corrects an earlier claim in this file that "CI does not build
  fixtures, so nothing red announces it" — the nightly platform lane does build
  them, and run `33847619657` (2026-09-04T07:11) carries the identical
  `stdbool.h:79:25: error: missing binary operator before token "1"` under
  `/__w/nano-ros/nano-ros/`. `just nightly-triage` reports `nuttx` red across all
  three scanned runs.
* **So the observer is down, not merely the build.** A cell red for every run in
  the window has no signal capacity — CLAUDE.md's own words: "a regression
  landing in it looks exactly like yesterday's failure". Everything the NuttX
  lane would otherwise report is currently invisible, including issue 0870's
  intermittent `-100`, whose whole remaining plan is "catch a failing run in a
  sweep".
* It bites the next person who tries to run a NuttX cell locally, and presents as
  an incomprehensible `redefinition` cascade rather than as a pin problem.
* The pin moved to 1.8.0 (`aefd6775e deps(zenoh-pico): the patch line moves to
  1.8.0`) while **PR #299, the phase-415 port, is still open**. So the version
  bump landed ahead of the port that makes it work, and this is one of the
  defects that port exists to absorb — issue 1021 is the same shape one feature
  flag over (`Z_FEATURE_MATCHING=0` on Zephyr), fixed by carrying `0343ad1b` on
  our patch line.

## Fix direction

Same as 1021: carry it on the `nano-ros` patch line of our zenoh-pico fork, and
offer it upstream separately. Three lines, and the shape is forced — the macro
must stop being a `bool` spelling:

    keyexpr.c:565   #define _ZP_KE_MATCH_TEMPLATE_INTERSECTS 1
    keyexpr.c:567   #define _ZP_KE_MATCH_TEMPLATE_INTERSECTS 0
    template:132    #if _ZP_KE_MATCH_TEMPLATE_INTERSECTS

Not attempted here, deliberately: phase-415 (PR #299) owns this file's line and
a second patch landing beside it is how a patch branch acquires two spellings of
one fix. Whoever lands #299 should fold this in, or take it as a follow-up
commit on the same branch.

## Reproduce

    git checkout main && git submodule update packages/rmw/zenoh/zpico-sys/zenoh-pico
    just setup-cli && just setup-launch-resolve
    just nuttx build-fixtures-arm     # fails at the first zpico-sys row

Verified on a clean checkout of `main` with no local modifications other than
the NuttX submodule's untracked build output.

## Consequence recorded elsewhere

phase-414 W3's remaining work — hunt a failing run of the NuttX C++ action cell
— cannot use `main`'s images while this stands. That hunt is being run against
the previous 1.7.2 pin instead, which is also the configuration every earlier
0870 measurement used, so the results stay comparable; the deviation is stated
in issue 0870 rather than left implicit.
