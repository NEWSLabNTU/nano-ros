# ws-derived-sched-tasks-cpp - four C++ components, no authored sched tasks

The fixture the sched-task derivation gates run against. It mirrors the shape of
the Autoware Safety Island: four `SHAPE rclcpp` components, one wall timer
each, two at 30 Hz and two at 10 Hz, registered with `CALLBACK_GROUPS main`
and NOTHING else. `demo_bringup/system.toml` declares no `[tasks.*]` and no
`group_tasks`; the rates live in the contract beside the launch file.

`examples/workspaces/realtime-cpp` is the authored-sched-task neighbour (every sched task
written three times: sched task, RTOS sub-table, binding). This workspace is what the
rate-monotonic derivation (`nros-orchestration-ir::derive`) has to reach from
what a cmake image actually authors.

## What it declares

| Node | Package | Timer | Contract path | Expected derived sched task |
| --- | --- | --- | --- | --- |
| `mrm_emergency_stop_operator` | `emergency_stop_pkg` | 33 ms | `on_timer` at 30 Hz | most urgent |
| `stop_mode_operator` | `stop_mode_pkg` | 33 ms | `on_timer` at 30 Hz | most urgent |
| `mrm_comfortable_stop_operator` | `comfortable_stop_pkg` | 100 ms | `on_timer` at 10 Hz | one below |
| `mrm_handler` | `mrm_handler_pkg` | 100 ms | `on_timer` at 10 Hz | one below |

Equal periods share one rank (the pinned ranker gives one fine group one rank),
so the expected table is two sched tasks of two members, 30 Hz before 10 Hz.

The group `main` is declared in cmake and never created in code: node-level
placement, which is the island's case and the one W6 keeps legal.

## Where each fact goes

* `src/<pkg>/CMakeLists.txt` - `nros_components_register_node(... CALLBACK_GROUPS main)`.
  The keyword reaches `build/<coord>/cmake/nros-metadata.json` as
  `"callback_groups": ["main"]` (verified on the native configure).
* `src/demo_bringup/launch/system.contract.yaml` - each node's
  `paths.on_timer.trigger.timer.rate_hz` (what the schedule ranks by) and its
  `pub.*.min_rate_hz` (a publication floor, never a schedule input). The two
  numbers differ on purpose, so that ranking by the floor would invert the
  schedule; see the comment there.
* `src/demo_bringup/system.toml` - the four `[[component]]` rows and the
  images. No sched tasks, no bindings.

## Configure

```sh
source ./activate.sh
cd examples/workspaces/derived-sched-tasks-cpp
nros build demo_bringup:linux --workspace . --offline
```

`src/zephyr_entry/` is the hand-written west application `[image.zephyr]`
claims, so the later waves can bake the same four components for
Zephyr and read the resulting `.config`.
