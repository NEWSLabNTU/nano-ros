---
id: 2863
title: "nros run cannot fail on a forbidden line, cannot keep watching after its expectations are met, and takes no --zephyr-workspace"
status: open
type: enhancement
severity: low
area: [cli, run]
related: [rfc-0118, issue-2353]
found: 2026-11-23
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
