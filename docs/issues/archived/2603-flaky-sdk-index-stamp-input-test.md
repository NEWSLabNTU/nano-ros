---
id: 2603
title: "`configure_stamp::tests::the_sdk_index_is_a_stamp_input` fails under a full gate and passes alone"
status: resolved
resolved_in: "the change that archives this file"
type: bug
area: [testing, cli]
severity: medium
found: 2026-11-15
related: [phase-478, issue-2598]
---

## What happens

In one `just ci gate` run (`check::build`, the `check-cli-tests` lane) on the
issue 2598 branch, this test failed at its second assertion:

```text
panicked at nros-cli-core/src/configure_stamp.rs:1613:9:
assertion failed: matches!(is_current_with(p, "img", &real_env(), None).unwrap(),
    Currency::Stale(why) if why.contains("outside the project") &&
    why.contains("changed"))
```

The same tree then passed it 3 of 3 times alone, and the whole
`nros-cli-core` lib suite (2487 tests) passed. The host was busy at the time
(other cargo builds running), and the branch's own change does not reach the
code path (the key is `img`, not a workspace `<bringup>:<image>` key).

A red that reproduces only in a full lane costs every agent a gate re-run.

## What is known

- The assertion before it (`Currency::Current`) passed, so the recorded
  external read was there and current.
- `external_moved` compares content digests, not mtimes, so a file timestamp
  cannot explain it.
- The external-read recorder is thread-local (`begin_external_reads`), so a
  concurrent test cannot add reads to this one's record.
- The panic does not say what `is_current_with` returned instead: the
  `matches!` hides the reason, so the cause is still unknown.

## Next step

Make the assertion print the actual `Currency` (`assert!(matches!(..), "{got:?}")`)
so the next failure names its reason. Candidates to check against that
reason: a check `is_current_with` runs before the external reads and that
reads shared state (the process environment through `real_env`, the CLI's
planning digest, the codegen version), and the fixed scratch-dir names that
`tmp()` reuses across runs.

## Resolution (2026-11-18)

**Cause: a test that wrote `AMENT_PREFIX_PATH` into the shared test
process.** `check-cli-tests` runs `cargo test -- --test-threads=N`, so every
test in `nros-cli-core`'s lib is a thread of ONE process.
`prereq_resolve::tests::the_ros_rung_does_not_guess_a_prefix` removed
`AMENT_PREFIX_PATH` and restored it a moment later, with the comment "SAFETY:
single-threaded test", which it never was. `AMENT_PREFIX_PATH` is one of a
configure stamp's digest inputs (`ENV_INPUTS`), and the stamp tests read it
through `real_env()` twice: when they record and when they check. With ROS
sourced, a stamp recorded on one side of the removal and checked on the other
reads `Stale("inputs changed (recorded …, now …)")` with nothing edited. That
is why `the_sdk_index_is_a_stamp_input` failed at its SECOND `is_current_with`,
why only a host with ROS sourced sees it (on a host without it, removing an
unset variable changes nothing), and why it never reproduced alone.

**Measured** on newslab-118 (48 cores, load 15-42 from a concurrent tier-2
fixture build), `AMENT_PREFIX_PATH=/opt/ros/humble cargo test -p
nros-cli-core --lib -- --test-threads=48 configure_stamp prereq_resolve`:

| tree | runs | failed |
| --- | --- | --- |
| before the fix | 140 (40 + 100) | 4, each `Stale("inputs changed …")` in a stamp test (`deleted_generated_crate_is_stale` was one) |
| after the fix | 100 | 0 |

Without `AMENT_PREFIX_PATH` set, the whole lib suite passed 15 of 15 runs
under the same load: the variable must be set for the removal to move it.

**Fix:** the test asks the question through an injected value,
`ros_packages_in(None)`, and no longer writes the process environment.
`ros_packages()` is `ros_packages_in(<the process value>)`, unchanged.

**Sweep:** `git grep -nE 'env::(set_var|remove_var)\(' -- 'packages/cli/**/*.rs'`
(21 sites in `nros-cli-core/src`). Each other write is one of: a variable only
that test reads (`NROS_TEST_*`, the doctor and probe tests); the CLI's own
pre-thread writers (`set_process_path`, the offline knobs, configure's
serialised `set_env`); `test_support::isolate_model_discovery`'s once-only
`OUT_DIR` / `NROS_MODEL_DIR`; a deliberately harmless `LD_LIBRARY_PATH`
poison (`setup.rs`, a directory that does not exist); and
`second_configure_is_a_no_op_without_exported_search_path`, whose variable is
no digest input any more (the stamp reads the captured user value, Ruling
R45). None of those is a digest input written mid-run.
