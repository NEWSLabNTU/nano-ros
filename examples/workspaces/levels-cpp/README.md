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

## The real-time verification images (phase-162)

Three more Linux images share this bringup, each with its own launch file.
The three-node `linux` and `zephyr` images above launch none of their nodes,
and those nodes are the only reason the bringup has `[tiers.*]` and
`group_tiers` at all.

| Image | Launch | What it runs |
| --- | --- | --- |
| `linux_s1` | `s1.launch.xml` | RFC-0002 S1: `attitude` (1 kHz, 300 us of work, 1 ms deadline) and `load` (5 ms of CPU at 10 Hz) on two derived levels |
| `linux_s1_flat` | `s1_flat.launch.xml` | the same two nodes bound to one authored tier: the negative control |
| `linux_budget` | `budget.launch.xml` | `load_capped` (10 ms of CPU per release) on the `capped` tier, 2 ms per 10 ms under `SCHED_DEADLINE`, beside the boot level `load_boot` |

`attitude` prints one `[attitude] window=` line per 1000 releases, with how
many release gaps exceeded period plus deadline. `load` prints one
`[<name>] tick=` line per release, with the CPU it burned, the wall time that
took, and its longest stretch off the CPU. `tests/rt_sched_levels_e2e.rs`
reads both. It runs only on a lane that claims `rt-sched`, because without
`CAP_SYS_NICE` the kernel refuses every level's priority and budget. The test
runs each image in a docker container granted it, so no `sudo`; the book's
[Real-Time Execution Model](../../../book/src/internals/rt-execution-model.md)
chapter says how to run it.

They share one bringup, not one each, because every Linux cmake image of a
workspace builds in one cmake tree (`build/posix-zenoh-linux/cmake`).

## Configure

```sh
source ./activate.sh
cd examples/workspaces/levels-cpp
nros sync
```

`just zephyr build-fixtures` syncs and builds the Zephyr image
(`examples/fixtures.toml`, row `workspace-zephyr-cpp-levels`).

## What a level costs

`python3 scripts/nros-mem-report.py --levels <zephyr.exe>` prints the per-level
cost and `--check` compares it with what the entry generated. Since phase-509
P9b the three levels are worker threads over ONE executor per core (RFC-0108
"As built in P9b"); native_sim, `.bss`:

| | P9 (an executor per level) | P9b (workers over one executor) |
| --- | --- | --- |
| executor contexts | 3 x 3,144 | 3,184 |
| executor backings | 3 x 11,288 | 18,408 (every level's members) |
| scheduler | 88 | 336 (3 seats of 104) |
| thread objects | 2 x 176 | 2 x 176 |
| total | 43,736 | 22,280 |

A spawned level is its seat (104 B: its worker record and its wake object's
waiter), its thread object (176 B) and its derived stack (16,384 B here, in
`.noinit`), plus its members' share of the one backing; each spawned level
also takes one platform wake object from the heap at open.
