# ws-derived-tiers-cpp - four C++ components, no authored tiers (phase-459 W0)

The fixture every phase-459 wave's gate runs against. It mirrors the shape of
the Autoware Safety Island: four `SHAPE rclcpp` components, one wall timer
each, two at 30 Hz and two at 10 Hz, registered with `CALLBACK_GROUPS main`
and NOTHING else. `demo_bringup/system.toml` declares no `[tiers.*]` and no
`group_tiers`; the rates live in the contract beside the launch file.

`examples/workspaces/realtime-cpp` is the authored-tier neighbour (every tier
written three times: tier, RTOS sub-table, binding). This workspace is what the
rate-monotonic derivation (`nros-orchestration-ir::derive`) has to reach from
what a cmake image actually authors - issue 1426 measured that today it does
not.

## What it declares

| Node | Package | Timer | Contract path | Expected derived tier |
| --- | --- | --- | --- | --- |
| `mrm_emergency_stop_operator` | `emergency_stop_pkg` | 33 ms | `on_timer` at 30 Hz | most urgent |
| `stop_mode_operator` | `stop_mode_pkg` | 33 ms | `on_timer` at 30 Hz | most urgent |
| `mrm_comfortable_stop_operator` | `comfortable_stop_pkg` | 100 ms | `on_timer` at 10 Hz | one below |
| `mrm_handler` | `mrm_handler_pkg` | 100 ms | `on_timer` at 10 Hz | one below |

Equal periods share one rank (the pinned ranker gives one fine group one rank),
so the expected table is two tiers of two members, 30 Hz before 10 Hz.

The group `main` is declared in cmake and never created in code: node-level
placement, which is the island's case and the one W6 keeps legal.

## Where each fact goes

* `src/<pkg>/CMakeLists.txt` - `nros_components_register_node(... CALLBACK_GROUPS main)`.
  The keyword reaches `build/<coord>/cmake/nros-metadata.json` as
  `"callback_groups": ["main"]` (verified on the native configure).
* `src/demo_bringup/launch/system.contract.yaml` - each node's
  `paths.on_timer.trigger.timer.rate_hz` and its `pub.*.min_rate_hz`. The two
  numbers are equal on purpose; see the comment there (issue 1372 / W7).
* `src/demo_bringup/system.toml` - the four `[[component]]` rows and the
  images. No tiers, no bindings.

## Configure

```sh
source ./activate.sh
cd examples/workspaces/derived-tiers-cpp
nros build demo_bringup:native --workspace . --offline
```

`src/zephyr_entry/` is the hand-written west application `[image.zephyr]`
claims (RFC-0065 D5), so the later waves can bake the same four components for
Zephyr and read the resulting `.config`.
