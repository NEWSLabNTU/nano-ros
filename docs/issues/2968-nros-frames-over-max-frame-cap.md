---
id: 2968
title: "nano-ros frames over the 4 KiB stack-frame cap, held in the max-frame baseline until they shrink"
status: open
type: tech-debt
severity: low
area: [stack-floor, nros-node, boards]
related: [issue-2815, issue-2070, phase-557]
found: 2026-11-25
---

## What

`just check max-frame` caps any single stack frame in nano-ros's own code at
`max_frame` (4,096 B, `config/stack-floors.toml`), because one `sub sp` larger
than an MPU guard jumps it with no fault (issue 2815). Frames that already
exist are held in the shrink-only baseline `config/max-frame-baseline.json`.

The release tier-2 run (run-matrix 45533259451) found frames the baseline did
not hold. Half of them were baseline rows under another symbol spelling, and
some came from other lanes' leftover builds; the check now keys and scopes
correctly. The rest are real frames that existed before the baseline was
written but were never in a build it read. They are baselined, not shrunk,
so the gate passes today. This issue lists them so they shrink.

Each row is in the baseline; `seen_in` names a build that has it. Measured on
host 175 with `just build-test-fixtures lane=tier2-nightly` at main 939ac840b2,
then `python3 scripts/check-max-frame.py --write-baseline --merge`.

| frame | size | where | why it is big | fix |
| --- | ---: | --- | --- | --- |
| `Executor::register_lifecycle_services_named` | 8,120 B | Zephyr C lifecycle images (mps2) | `Box::new(LifecycleRuntimeState { state_machine: LifecyclePollingNodeCtx::new(), .. })` builds the machine on the stack, then moves it. Out-of-lining the per-service helpers was measured: 8,112 B. | Build the state in its allocation, as issue 2070's `LifecycleServiceServers::try_new_boxed` does for the servers. |
| `node_runtime::install_in_with_launch_params<T>` | 7.4-9.1 KB per node type | every Rust image | one instance per node type; already baselined for the in-tree types, new for `param_five_node`'s five | measure what the instance holds on the stack; likely the same by-value build |
| `Executor::register_action_client_raw` | 6,888 B | C action client (FreeRTOS mps2) | `register_action_client_raw_sized::<400, 400, 400>` inlined into its wrapper; `_sized` is baselined at 6,920 / 9,424 B | shrink `_sized`; the wrapper follows |
| `Executor::pass` | 4,512 B | NuttX isr-wake image | not yet looked at | measure first |
| FreeRTOS `app_task_entry_runtime` / `_bare` / `_sched_tasks`, mps2 `run_bare` | up to 10,056 B | board entries | generic over the user's app closure, which is inlined, AND the board's own boot bring-up / executor open / `init_hardware` inline beside it, so the frame is part ours, part the user's | call the user closure out of line (`#[inline(never)]` on a small trampoline): then the board's own frame is measurable on its own, and only the closure, which is the user's, needs a `[[frame_allow]]` |
| `create_entity` closure (`out_of_line<...create_entity...>`) | 5,744 B | Zephyr Rust talker | grew 24 B since the baseline | with the registration-path work above |
| raised rows | +8 to +120 B | board entries, `create_entity` | since the baseline was written (2026-11-22): every FreeRTOS `app_task_entry_runtime` grew 24 B (5,776 to 5,800 B), `app_task_entry_bare` 120 B, `_sched_tasks` 40 B, mps2 `run_bare` 8 B; param_five_node's `app_task_entry_runtime` is 7,072 B because its app is larger | the board-entry fix above |

## Acceptance

Each frame shrinks below 4,096 B, or is shown to be inherent and gets a
reasoned `[[frame_allow]]`. Its baseline row is lowered or deleted in the same
commit, measured on the build its `seen_in` names.
