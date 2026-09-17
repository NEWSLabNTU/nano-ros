---
id: 1314
title: "`lane_build_covers_run::a_native_build_satisfies_the_tier1_run` failed when
  run directly — a defect, or an unmet precondition of how it was invoked?"
status: resolved
type: bug
area: testing, ci
severity: low
related: [issue-0828, issue-0922, issue-1226, issue-1016, issue-1313]
---

## What was seen

On 2026-09-11, in the worktree for branch `test/rv-virt-threadx-c-workspace`
(PR #936), `packages/testing/nros-tests/tests/lane_build_covers_run.rs` ran
10/11. `fn a_native_build_satisfies_the_tier1_run` (line ~330) failed. The
agent's diagnosis:
- `CiLane::run_scope` hard-codes `Tier1` to COORDINATE scoping;
- the lane script refuses to run when `NROS_TEST_COORDS` is unset;
- so a test asserting "a native build satisfies the tier-1 run" cannot pass in
  an environment that does not set it.

Neither reads `matrix::CELLS`, so the PR's cell additions did not cause it.

## The contradiction to resolve first

In two OTHER worktrees the same day, `just test-lane-contracts` passed 17/17
(branches `fix/1284-backing-meets-default` and `fix/1285-followup-rtos-
substring`). So either:
- this test is not in that recipe's set, in which case it is in no lane, which
  is itself the issue-1226 shape; or
- it is, and passes when invoked through the recipe, in which case the direct
  run lacked a precondition the recipe supplies, and the only defect is that it
  fails with a confusing message instead of a `skip!` naming what is missing.

Establish which before changing any code.

## Fix, by outcome

- **In no lane:** put it in one, or delete it. `check-default-gates-run-somewhere`
  should have caught that; if it did not, its scope is narrower than its rule.
- **Precondition:** make the test state its precondition with
  `nros_tests::skip!` or a hard failure that names `NROS_TEST_COORDS`, per
  "tests must fail on unmet preconditions".
- **Real contradiction:** CLAUDE.md says tier 1 narrows its run by NAME
  (`NROS_TEST_SCOPE`) and tiers 2 and nightly by COORDINATE. If `run_scope` now
  says `Tier1` is coordinate-scoped, then either the doc or the code is stale.
  Fix the stale one, and cite issue 0828.

## Acceptance

Record the outcome in this issue, with the test green or explicitly skipped
under every supported invocation (`just test-lane-contracts`, bare `cargo test
--test lane_build_covers_run`, nextest).

---

## Resolution (2026-09-18)

**The verdict: BOTH the "in no lane" horn and the "real contradiction" horn are
true. The "unmet precondition" horn is false — the recipe supplies nothing,
because the recipe never ran the target.**

### What settled the contradiction

`just test-lane-contracts` runs

```
cargo nextest run … -p nros-tests --test lane_run_narrowing --test matrix_fixture_coverage
```

`lane_build_covers_run` is not in it. Measured on `origin/main`
(`fa6cf68d2`):

| invocation | result |
| --- | --- |
| `just test-lane-contracts` | `17 tests run: 17 passed, 0 skipped` — **none of them from this file** |
| `cargo nextest run -p nros-tests --test lane_build_covers_run` | `11 tests run: 10 passed, 1 failed` |
| `cargo test -p nros-tests --test lane_build_covers_run` | `8 passed; 3 failed` — two more, and for a different reason (below) |

So the 17/17 in the two other worktrees and the 10/11 here were reporting on
DISJOINT sets. The failure is deterministic under nextest — the same 10/11 this
issue opened on — and it was nobody's precondition. `test-all` does reach the
target, but only behind `_require-fixtures-ready`, so the one lane that could
afford these tests is the one that excluded them: issue 0922's defect (excluded
by CRATE where the real property is per-TARGET) with a third target left behind,
and issue 1226's shape once more.

`check-default-gates-run-somewhere` did not catch it and is not at fault: its
rule is about GATES and `ci gate` STEPS reaching a workflow event, which
`test-lane-contracts` does. "Every lane-contract test TARGET is in an
affordability lane" is a different rule and had no gate; it now has the only
one that can hold, which is membership of the recipe itself.

### The real contradiction: CLAUDE.md and the test were the stale ones

phase-395 W19 moved tier 1 from NAME scoping to COORDINATE scoping, on both
sides:

* `CiLane::Tier1.run_scope()` → `RunScope::LaneCoords`;
* `nros_lane_build_lane tier1` → `tier1`, no longer `native`;
* `just ci` exports `NROS_FIXTURE_LANE=tier1 NROS_TEST_COORDS="$coords"`
  (`just/ci.just:126`), not `NROS_TEST_SCOPE`.

`fn a_native_build_satisfies_the_tier1_run` still asserted the retired premise
("tier 1 narrows its run to host binaries, which is exactly what that build
produces"), so it failed on every invocation. The premise is false by
measurement, not merely obsolete: `lane-coords tier1` selects

```
linux,{c,cpp,rust}×{cyclonedds,xrce,zenoh}  linux,mixed,zenoh
threadx-linux,c,zenoh                       zephyr,rust,zenoh
```

and the `native` module builds neither of the last two. Refusing a `native`
build for a tier-1 run is CORRECT behaviour; the test was wrong.

CLAUDE.md carried the same retired claim ("Tier 1 narrows its run by NAME
(`NROS_TEST_SCOPE`) so it needs the broader `native` build"), and so did the
`CiLane::run_scope` comment, which had the pre-W19 paragraph sitting directly
above the W19 note that replaced it. All three are fixed — this is issue 0828's
lesson applied to prose: the build side and the run side of a lane are ONE fact,
and a doc that keeps the old half aims the next reader at the wrong one. It did
exactly that here: this issue's own first diagnosis reads `run_scope` as the
suspect.

### A third defect, same class as issue 1313

Plain `cargo test` lost two MORE cases than nextest, and they are not about
lanes at all: `tmpdir()` was keyed on `std::process::id()`, which is per-test
only under nextest. Under libtest every case shares one process, so they shared
one directory — and three cases write `.fixtures-built-tier2` into it and then
`remove_file` it, while two write `lane-coords-tier2.txt`. The two extra
failures were a sibling's cleanup (`sed: can't read …/.fixtures-built-tier2`)
and a sibling's mid-`fs::write` truncation read back as `NROS_TEST_COORDS is
unset or empty` — issue 0494's truncation race reproduced inside the test
helper. Issue 1016's resolution had already noted both this and the tier-1
failure as pre-existing and left them; nothing ran the target, so there was
nowhere for the note to become a red.

### Changes

1. `justfile` — `test-lane-contracts` also runs `--test lane_build_covers_run`.
   Admissible under `check-lane-contracts`: it drives `fixture-lane.sh` against
   a temporary `NROS_FIXTURE_STAMP` under `tmp/`, resolves no real fixture, and
   costs ~0.6 s. The recipe now runs 28 tests, not 17.
2. `lane_build_covers_run.rs` — `a_native_build_satisfies_the_tier1_run` →
   `a_tier1_build_satisfies_the_tier1_run_and_a_native_build_does_not`, stating
   the post-W19 truth in both directions, with the non-host reach READ from the
   lane's own selection so a tier 1 that ever becomes host-only again reports
   "this arm is now the wrong assertion" instead of passing vacuously. The
   ladder guard the old case really carried (`run_scope` must not widen, tier 1
   must not be re-priced) survives as its first two assertions.
3. `lane_build_covers_run.rs` — `tmpdir()` is per TEST (pid + libtest's
   per-test thread name), so no call site has to pass a tag.
4. `CLAUDE.md`, `CiLane::run_scope` and `gate_promised_fixtures` — all three
   copies of the retired NAME-scoping claim corrected. The third one is the
   reason this is worth enumerating: nothing sets `NROS_TEST_SCOPE` any more
   (five hits in the tree, every one a comment), so a reader taking that
   parenthetical at face value would look for a producer that does not exist.
5. `scripts/check-lane-contracts.py` — putting the target in the lane made the
   gate red for a reason that was about the GATE: it flagged
   `require_west_leaf_in_lane` appearing in an ASSERTION MESSAGE explaining
   that the resolver fails open. `_strip_rust_comments` already existed for
   exactly this class over COMMENTS (its docstring named the same resolver and
   the same file family) and its docstring said it stopped at strings. So this
   is the fix-the-class rule: it now blanks string literals too — plain, raw
   (`r"…"`, `r#"…"#`) and byte forms — plus the one-char literals, because
   `'"'` is the only way a char literal can open a phantom string while `&'a`
   must not. A call cannot sit inside a string, so the narrowing loses no true
   positive, and a self-test case asserts that in both directions.

### A fourth defect: putting it in a lane made 5 of 28 TIME OUT, and the obvious cause was the wrong one

The first run of the lane with the target in it gave `28 tests run: 23 passed, 5
timed out` and, in the log, `Blocking waiting for file lock on package cache`.
The five are every case that reaches `nros_lane_coords_file`, which prefers a
PREBUILT `lane-coords` and otherwise falls back to `cargo run -q -p nros-tests
--bin lane-coords` — a cargo nested inside this one, blocking on the build lock
until nextest's 60 s per-test timeout. That is issue 0523 part B exactly, and it
reads as "the lane does not build what it resolves", i.e. the
`check-lane-contracts` rule, so a `cargo build --bin lane-coords` step in the
recipe looked like the fix.

**It is not the cause.** Backdate the binary to 2020, drop the pre-build, and
`cargo nextest run -p nros-tests --test <names>` rebuilds it anyway — verified,
the mtime moved to the run. So the binary was fresh and the fallback still
fired, because BOTH resolvers scanned a hardcoded three profile dirs:

```
["nros-fast-release", "debug", "release"]     # lane_coords_bin(), and the
                                              # shell's _nros_lane_coords_bin
```

and the repo's development default is none of them —
`nros_cargo_profile::DEFAULT_PROFILE` is `nros-relwithdebinfo`. Two spellings of
one fact (the profile table, and a list written beside it), so on a default
machine the shell had ALWAYS fallen back to `cargo run`, and the Rust helper had
always either skipped or picked a stale `target/debug/lane-coords` left by an
earlier plain `cargo test` — which is precisely the museum-binary hazard its own
comment warns about, created by the list it was written next to. Invisible until
now only because the fallback works everywhere except inside a cargo.

Both now scan `target/*/lane-coords`, newest wins, freshness check unchanged —
a glob cannot drift when the profile table moves. The pre-build step was
removed again: a redundant step propped up by a wrong diagnosis is how a lane
accretes cost nobody can later justify removing.

| mutation | result |
| --- | --- |
| recipe does NOT pre-build (glob kept) | `28 passed` — the pre-build is redundant, which is why it is not there |
| resolvers scan only the old three dirs (pre-build kept) | 5 TIMEOUT at 60 s — the glob is the load-bearing half |
| bin backdated to 2020, no pre-build | `28 passed`, bin mtime moved to the run — nextest does build the package's bins under a `--test` filter |

### And the scanner was broken in the DANGEROUS direction too

Blanking string literals strictly removes matches, so the only risk of the fix
is the opposite of the bug — a real call the new scanner stops seeing. Checked
by replaying both scanners over every file in `packages/testing/nros-tests/tests`
(`tmp/scanner-diff.py`). One resolver was dropped, both occurrences prose, as
intended. One was **ADDED**, which should be impossible for a change that only
erases more:

```
ADDED    entry_e2e.rs: require_entry_binary
```

`entry_e2e.rs:297` carries the ROS parameter wildcard `` `/**: 999` `` inside an
assertion message. The comment-only scanner had no idea what a string was, so
that `/*` opened a block comment, no `*/` followed, and it **erased 26,588
characters from there to the end of the file** — swallowing the genuine
`nuttx::require_entry_binary("talker", "talker")` call at line 365.

So the pre-1314 scanner could MISS a RUNTIME resolver, and that is the direction
that matters: a false positive is a red somebody investigates, a false negative
is the rule silently not enforced. Nothing was actually waved through — no
affordability tier reaches `entry_e2e` today, which is why the gate reads green
before and after — the hole was simply open, waiting for the lane list to grow.
Which it just did: this issue's own change is the first addition to
`test-lane-contracts` since the scanner was written.

Both directions now have a self-test case (56 cases, was 53).

### Measured, after

| invocation | before | after |
| --- | --- | --- |
| `just test-lane-contracts` | 17 run / 17 passed (target absent) | **28 run: 28 passed, 0 skipped**, 0.6 s (23 passed / 5 TIMEOUT before the resolver fix below) |
| `cargo nextest run -p nros-tests --test lane_build_covers_run` | 11 run: 10 passed, **1 failed** | **11 run: 11 passed, 0 skipped** |
| `cargo test -p nros-tests --test lane_build_covers_run` | 8 passed / **3 failed** | **11 passed / 0 failed, 25 runs in a row** |
| `just check lane-contracts` | OK (17 targets), then FAILED once the target was in the lane | **OK — 18 test target(s) …; none resolves an artifact its job does not build** |

### Mutation checks

| mutation | result |
| --- | --- |
| `CiLane::Tier1 => RunScope::Native` | FAIL, 2 cases: `tier 1 must narrow its RUN to its own coordinates…` and `fixture-lane.sh and CiLane::build_lane disagree about what a tier1 RUN needs built`; restore → 11/11 |
| `tmpdir()` back to per-PROCESS | 25 runs of `cargo test`: **6 FAILED** (4× `9 passed; 2 failed`, 2× `10 passed; 1 failed`), reproducing both shared-path errors; restore → 25/25 green |
| the vacuity guard: force `non_host` empty | FAIL: `tier 1 selects only host coordinates now, so the arm below asserts the wrong thing…` |
| `check-lane-contracts.py`: disable the plain-string branch | self-test `53 passed, 2 failed` — both new cases fire; the real tree reports the false positive again |

## Acceptance

* [x] Outcome recorded here: **in no lane AND a real contradiction**, not an
      unmet precondition.
* [x] Green under `just test-lane-contracts`, bare `cargo test --test
      lane_build_covers_run`, and nextest — no skips in any of the three.
