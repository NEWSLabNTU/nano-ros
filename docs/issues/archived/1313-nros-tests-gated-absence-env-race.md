---
id: 1313
title: "`gated_absence_is_a_hard_failure` races a sibling test on process env under
  plain `cargo test`"
status: resolved
type: bug
area: testing
severity: low
related: [issue-1314, issue-0196, issue-0584]
---

## What happens

`packages/testing/nros-tests/src/fixtures/binaries/mod.rs`,
`fn gated_absence_is_a_hard_failure` (line ~5849), reads an environment
variable that a sibling test in the same module clears. Under nextest each test
is its own process, so it passes. Under plain `cargo test -p nros-tests --lib`
the tests share one process and run on threads, and it fails intermittently:
observed 212/213 on 2026-09-11 (branch `test/rv-virt-threadx-c-workspace`,
PR #936). It passes when run alone. The test's own comment says it relies on
nextest's process-per-test.

## Why it matters

A red that goes away on retry teaches people to retry. And
`std::env::set_var` / `remove_var` are `unsafe` in edition 2024 precisely
because concurrent access is undefined behaviour, so this is a latent UB site,
not only a flake.

## Fix

Pick one:
- **Stop reading process env in the code under test for this path.** Inject the
  value, which is the better fix.
- **Serialise every test in the module that touches that variable** behind one
  `static` mutex, and document it.

Grep for siblings with the same shape: `rg -n 'set_var|remove_var'
packages/testing/nros-tests/src`.

## Acceptance

`cargo test -p nros-tests --lib` passes 20 times in a row with the default
thread count.

---

## Resolution (2026-09-18)

Fixed by **removing the dependency on process env in the code under test**
(1313's own preferred option), not by serialising. No mutex was added.

### Reproduced first, cheaply

The 1-in-213 rate is dilution, not rarity: with 214 `--lib` cases the two arms
rarely land on threads at the same time. Narrow the filter to the pair and they
always do:

```
cargo test -p nros-tests --lib fixture_absence_class_tests -- --test-threads=2
```

**49 of 60 runs FAILED** on `origin/main` (`fa6cf68d2`), in BOTH directions:

* `gated_absence_is_a_hard_failure` reaching the `Err` arm — the sibling had
  cleared the `NROS_TEST_SCOPE` it had just set;
* `ungated_absence_is_a_recoverable_error` reaching the PANIC — it saw the
  sibling's `NROS_TEST_SCOPE=native` and got
  `Test fixture binary MISSING for an in-lane coordinate`.

Two arms of one branch, each of which had to write what the other had to clear.

### The fix

`absent_fixture_verdict`'s two ambient inputs became a parameter,
`AbsenceEnv { fixtures_optional, gate_promised }`, read once at the resolver's
boundary by `AbsenceEnv::from_process_env()`. Production is unchanged and still
goes through the wrappers (`require_prebuilt_binary_checks`,
`require_prebuilt_row_binary`); each grew an `_in` sibling that takes the
struct, so the tests still drive the REAL resolver — that part was right, and a
test that reimplemented the branch would pass with the wiring bypassed (issue
0196) — and simply hand it two booleans.

`NROS_FIXTURES_OPTIONAL` was pulled in even though no test wrote it: the pair
READ as env-independent and was not. On a host with that variable exported the
gated arm's panic becomes a `skip!`, so
`#[should_panic(expected = "MISSING for an in-lane coordinate")]` fails for a
reason no message explains. Two new cases cover what the parameter exposed:

* `the_light_tier_opt_out_wins_over_the_gate_promise` — the opt-out is checked
  BEFORE the gate panic, which is not guessable from either doc comment and had
  no coverage at all.
* `from_process_env_reads_the_two_gate_variables` — because injecting the
  decision's inputs must not leave NOTHING checking that a real run derives
  them, which would be issue 0196's shape re-introduced by the fix. It runs the
  five env combinations in CHILD processes via `Command::env` on
  `current_exe()`: the environment is written by the safe API, this process
  never calls `set_var`, and no compilation happens. One recursive test rather
  than a test plus a probe, since a probe that only prints is what
  `check-no-vacuous-tests` forbids; and it asserts the child printed `1 passed`,
  because a libtest filter that selects nothing exits 0.

### Sweep — `rg -n 'set_var|remove_var' packages/testing/nros-tests/{src,tests}`

| site | verdict |
| --- | --- |
| `src/fixtures/binaries/mod.rs:5947-5948` `ungated_absence_is_a_recoverable_error` | **FIXED** — injects `gate_promised: false` |
| `src/fixtures/binaries/mod.rs:5962` `gated_absence_is_a_hard_failure` | **FIXED** — injects `gate_promised: true` |
| `src/fixtures/binaries/mod.rs:6737` `the_row_resolver_uses_the_carve_out_profile` | **FIXED** — same shape, and worse: `remove_var(NROS_TEST_COORDS)` plus an ORDERING requirement its own comment spelled out ("it must precede the first `lane::run_coords` call, which latches a `OnceLock`"), which no harness guarantees. Now passes `lane: None` through `require_coord_in_lane_within` — one spelling of the decision, environment lifted to the boundary |
| `tests/init_api.rs:42,49,59,60` | **SAFE, unchanged** — one `static` `env_lock()` taken by every live `#[test]` in the target, with RAII restore of the previous value (`EnvGuard`). The one case that does not lock is `#[ignore]`d with an empty body. That is 1313's option 2 done properly, and it is the right shape there: the code under test reads `ROS_DOMAIN_ID` &c. from the process environment BY DESIGN, so there is nothing to inject |
| `tests/rtos_e2e.rs:878` `enable_router_session_log` | **DIFFERENT SHAPE, surveyed not fixed.** Not this bug: no sibling writes or clears `ZENOHD_LOG`; the write is guarded on "still unset" and the only caller is one `#[rstest]` whose generated cases all write the same constant, so writers cannot disagree; and the value selects only whether the router keeps a log and at which level — never a verdict. What remains is the formal `setenv`-during-`getenv` hazard, and it cannot be removed the same way: the filter is consumed inside `ZenohRouter::start_on`, reached via `platform.zenoh_router_start(..)`, so injecting it is a change across ~64 spawn sites in a fixture-gated target. Worth its own issue; deliberately not folded in here |

`src/` now matches only PROSE (four doc-comment mentions, three of them
explaining this issue).

### Measured

| | before | after |
| --- | --- | --- |
| `cargo test -p nros-tests --lib fixture_absence_class_tests -- --test-threads=2` | **49/60 FAILED** | 60/60 green |
| `cargo test -p nros-tests --lib`, default thread count | ~1 in 213 red | **20/20 green, twice** (216 tests; `tmp/acceptance-1313/`) |
| `cargo nextest run -p nros-tests --lib` | green | green |

### Mutation checks

| mutation | result |
| --- | --- |
| `gate_promised_fixtures` drops its `NROS_TEST_COORDS` disjunct | FAIL: `AbsenceEnv::from_process_env misread the environment (marker 10): … NROS_TEST_COORDS=Some("/dev/null")` |
| `from_process_env` hardcodes `fixtures_optional: false` | FAIL: `… (marker 01): … NROS_FIXTURES_OPTIONAL=Some("1")` |
| the light-tier opt-out no longer precedes the gate panic | FAIL: `the_light_tier_opt_out_wins_over_the_gate_promise` |
| the child's `--exact` filter names a renamed function | FAIL: `the child ran no test, so nothing was asserted — the --exact filter no longer names this function` (the child itself printed `0 passed … 216 filtered out` and exited **0**, which is exactly why that second assertion exists) |
| removing `AbsenceEnv` and restoring the two `env::var_os` calls in the decision | not a mutation but the before-state above: 49/60 |

## Acceptance

* [x] `cargo test -p nros-tests --lib` passes 20 times in a row at the default
      thread count — measured twice, before and after the env-read coverage
      test was added.
* [x] Still green under nextest.
* [x] No `set_var`/`remove_var` CALL remains in `packages/testing/nros-tests/src`.
