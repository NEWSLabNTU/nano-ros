---
id: 2683
title: "`check::cli-tests` passes with 80 s to spare on a quiet host and times out at 2400 s on a loaded one; `workspace-probes` does too"
status: open
type: bug
severity: medium
area: [testing, gates, ci]
related: [issue-2380]
found: 2026-11-17
---

## Measured

Reported by session nros 139 on 2026-11-17. Both runs are `just ci gate`, and
the gate harness's per-test timeout is 2400 s. Both failures are in step 8,
`check::build`.

| run | tree | step 8 | `cli-tests` |
| --- | --- | --- | --- |
| B3, green | `p554-w3-slice-b3` | 57 min 04 s | **passed in 2319.7 s**, about 80 s under the timeout |
| B4, red | base `2cfe9f0094`, 13:33Z–15:23Z | 1 h 49 min 31 s | **timed out at 2400.03 s**, 228 of 230 tests done |

In B4, `build::check::workspace-probes` also timed out, at 2400.06 s.

Tests still running past 60 s when B4's `cli-tests` was killed:

- `rmw_resolver::tests::every_announced_backend_resolves_through_the_scan`
- `rmw_resolver::tests::the_baked_table_and_the_scan_agree_on_every_in_tree_backend`
- `serdes_resolver::tests::a_provider_in_the_user_workspace_reaches_the_default_search_path`
- `serdes_resolver::tests::the_nano_ros_root_is_scanned_despite_its_own_nros_ignore`

Also slow: the `configure_*` tests, `deleted_generated_dir_reconfigures`, and
`run_dry_run_why_after_an_edit_*`.

Load during B4, from the gate's token-pool report:

- host runnable threads: p50 6, p90 15, max 73, with other sessions' builds
  on the same host;
- free jobserver tokens: 0 in 51 % of the samples;
- `cli-tests` peaked at 17 runnable against its weight of 4, and
  `workspace-probes` at 14 against a weight of 1.

So the same suite on a busy host needs more than the timeout. Its green
margin, 80 s of 2400, is 3 %, and a gate whose verdict depends on how busy
the host is says nothing about the change under test. Issue 2380 is the same
shape for the `linux-check` gate.

## Not the fix

Raising the timeout. That moves the line and keeps the dependence on load
(the manager's direction, 2026-11-17).

## Fix direction

Split the suite or make it faster, measured on a loaded host:

1. **Where the time goes.** Per-test wall time from both logs, and whether
   the scan tests (`rmw_resolver`, `serdes_resolver`) walk the whole checkout
   each, which a shared, once-per-process scan would answer.
2. **Split** `cli-tests` into gates of their own, so no single gate holds
   most of the work under one timeout.
3. **Weights.** `cli-tests` ran 17 runnable against a weight of 4, and
   `workspace-probes` 14 against 1. A weight that understates a gate lets the
   pool start more work than the host can run, which slows every gate at
   once.

## Done when

`check::build` passes on a host at B4's load, measured, with `cli-tests` and
`workspace-probes` each under half the timeout.

## Logs

On the reporting host: `/home/aeon/repos/nano-ros/.claude/worktrees/p555-rmw/tmp/`,
`gate-b3-full2.log` (green) and `gate-b4-run1.log` (red).
