---
id: 2863
title: "nros run cannot fail on a forbidden line, cannot keep watching after its expectations are met, and takes no --zephyr-workspace"
status: resolved
type: enhancement
severity: low
area: [cli, run]
related: [rfc-0118, issue-2353]
found: 2026-11-23
resolved: 2026-11-23
resolved_in: "fix/west-entry-and-run-gaps"
---
Found by a downstream safety-island port, whose FVP CI lane still grades two
phases by grepping the console because `nros run` cannot; seen on main
545ae12ec1.

## What

`nros run` (`cmd/run_exec.rs` `ExecArgs`, graded by `target_session.rs`) stops
with exit 0 on the line that completes the `--expect` set. Three things a test
lane needs are missing:

1. **`--forbid TEXT`.** A long-lived image is killed by the run, so a crash
   never changes the exit code. A Zephyr `ZEPHYR FATAL ERROR` that lands AFTER
   the success line (a stack overflow in a reporting thread) passes. The lane
   greps `console.log` afterwards.
2. **`--settle SECONDS`.** The run stops on the completing line, so the rest of
   a multi-line report (a thread-analyzer block) and a late fatal are never
   seen. The lane runs the model itself, waits a grace window, and greps.
3. **`--zephyr-workspace DIR`.** `nros run` rebuilds a stale image through
   `nros build` (`run_exec::build_image`), but the `Selection` it forwards has
   no Zephyr workspace, so the lane exports `NROS_ZEPHYR_WORKSPACE` for the
   implicit build. `nros build` takes the flag.

## Repro

```
$ nros run <bringup>:<zephyr image> --timeout 120 --expect "tests passed" --forbid "FATAL"
Error: unexpected argument '--forbid' found
```

and the same for `--settle 15` and `--zephyr-workspace <dir>`. With
`--expect` alone, a target that prints `tests passed` and then
`FATAL ERROR` exits 0.

## Resolution

- **One set of grading flags.** `run_exec::GradeArgs` (`--expect`,
  `--expect-regex`, `--forbid`, `--forbid-regex`, `--settle`) is flattened
  into `nros run`, `nros flash --monitor` and `nros monitor`. `--settle`
  requires an expectation (a clap group), and `--forbid-regex` is compiled
  at parse time, so either mistake is a usage error.
- **Grading.** `target_session::Grade` carries the forbidden patterns; the
  capture thread reports the first forbidden line (`Event::Forbidden`), which
  ends the session at once as `Outcome::Forbidden`, exit `TARGET_FAILED`,
  naming the pattern and whether it was `--forbid` or `--forbid-regex`.
  `Session::settle` keeps the session open after the match: a forbidden line
  or a failing exit inside the window fails the run, a clean window or a clean
  exit ends it matched. `--timeout` bounds the time to the match only.
- **`--zephyr-workspace`.** `nros run` and `nros flash` carry it in their
  `Selection` to the implicit `nros build` (and its dry plan), which resolves
  it as it always has.
- **Tests.** `target_session::tests::{a_forbidden_line_before_the_match_fails_at_once,
  a_forbidden_line_inside_the_settle_window_fails,
  a_clean_settle_window_ends_matched_after_the_window,
  a_target_that_fails_inside_the_settle_window_fails_the_run,
  a_forbidden_line_fails_a_run_with_no_expectation}`;
  `run_plan::tests::{a_selection_carries_the_zephyr_workspace_to_the_build,
  the_shared_grading_flags_parse_alike_on_every_verb}`.
