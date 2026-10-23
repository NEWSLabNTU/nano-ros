# ws-levels-cpp - three preemption levels on Zephyr (phase-509 P9)

The fixture for RFC-0108's level runner. Three C++ components, one wall timer
each, and nothing else: no `[tiers.*]`, no `group_tiers`. The contract beside
the launch file states each timer's rate, and the derivation gives one
preemption level per distinct rank.

| Node | Timer | Callback | Level |
| --- | --- | --- | --- |
| `urgent` | 20 ms (50 Hz) | prints `[urgent] tick=N` | 0 (most urgent) |
| `middle` | 50 ms (20 Hz) | `[middle] begin=N`, runs 15 ms, `[middle] end=N` | 1 |
| `lazy` | 200 ms (5 Hz) | `[lazy] begin=N`, runs 60 ms, `[lazy] end=N` | 2 |

On Zephyr the long callbacks run with `k_busy_wait`, which keeps the thread
ready, so the only way another marker prints between a `begin=` and its
`end=` is a more urgent level's worker preempting it. The test
(`realtime_tiers_e2e`, cell `zephyr/cpp-levels`) checks the order: an
`[urgent]` tick inside a `[middle]` pair, `[urgent]` and `[middle]` markers
inside a `[lazy]` pair, and never a less urgent marker inside a more urgent
pair. It also checks each spawned level's stack is the derived one.

## Configure

```sh
source ./activate.sh
cd examples/workspaces/levels-cpp
nros sync
```

`just zephyr build-fixtures` syncs and builds the Zephyr image
(`examples/fixtures.toml`, row `workspace-zephyr-cpp-levels`).
