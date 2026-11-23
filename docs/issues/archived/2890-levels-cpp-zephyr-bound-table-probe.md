---
id: 2890
title: "`levels-cpp`'s Zephyr image is refused at configure (the probe wrote no `builtin_interfaces` bound table), and the rt-sched census test fails"
status: resolved
type: bug
area: [zephyr, sizing, scheduling]
severity: medium
found: 2026-11-23
resolved: 2026-11-23
resolved_in: "fix(#2890): the probe cache discard keeps the probe's outputs (code half); #4682 (lane half)"
related: [phase-590]
---

## What fails

`host-tests.yml` job `real-time levels (rt-sched)` (run 45104661214, 2026-11-23, main
`90c2d0659f`):

```text
configure: demo_bringup:zephyr: not configured — `demo_bringup:zephyr`: message bounds not seeded,
because what it declares could not be derived: the metadata probe generated no bound table for
`builtin_interfaces` (examples/workspaces/levels-cpp/build/nros-metadata/metadata-probe-cmake/
build/nros-codegen/nano_ros_cpp/builtin_interfaces/nros_message_bounds.json) ... (NROS_PROBE_FAILURE=refuse)
FAIL [   0.844s] (3/5) nros-tests::rt_sched_levels_e2e linux_rust_thread_census_is_placed_by_the_priority_plan
```

`linux_levels_preempt_in_rank_order_on_one_cpu` passes in the same run.

## Next

Check whether the census failure follows from the refused configure or is separate. The missing
`builtin_interfaces` bound table is a probe-coverage question: which generator writes it for a
C++ image, and why not this one.

## Acceptance

The job is green in a scheduled `host-tests` run. A release blocker:
[phase-590](../../roadmap/phase-590-nros12-release-plan.md) W1.

## Resolved (2026-11-23): two causes, two fixes

**The census test** was UNMET because the rt-sched step never built the native
realtime-rust fixture it resolves. The step now builds exactly what
`rt_sched_levels_e2e` resolves (#4682, another session).

**The Zephyr refusal** came from the build lanes' `NROS_METADATA_PROBE_CACHE=0`
(issue 0522). That discard removed the probe's WHOLE build dir, which also holds
what later steps read:
- `nros-metadata.json`, the probe aggregate;
- `nros-codegen/nano_ros_<lang>/<pkg>/nros_message_bounds.{json,cmake}`, the
  bound tables the west message-bound seed and `nros locate` read;
- `CMakeCache.txt` and `cmake-args.used`, which the freshness check
  (`metadata_refresh::probe_build_present`) reads.

So in every lane, a C/C++ Zephyr image's configure refused, and the next
configure called every C/C++ sidecar "not backed by a complete probe build"
and probed again. The discard now prunes to those outputs
(`metadata_probe_cmake::probe_output_kept`) and removes the directories left
empty.

Measured on `examples/workspaces/levels-cpp`, cold, with
`NROS_METADATA_PROBE_CACHE=0 NROS_PROBE_FAILURE=refuse`:

| | before | after |
|---|---|---|
| `nros configure` | refuses `demo_bringup:zephyr` | configures it |
| the CI step (`workspace-fixtures-build.sh linux cpp`, the four levels rows) | the refusal, plus "not backed by a complete probe build" | neither; all four rows build |
| a second configure | probes again | probes nothing |
| probe tree left on disk | 0 (79 MB when kept) | 112 KB |

The same cold configure with the cache kept (the default) never refused; that
difference is how the knob was found. Test:
`the_cache_discard_keeps_the_probe_outputs`.
