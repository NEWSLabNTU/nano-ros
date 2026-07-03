# ws-realtime-cpp-subnode — ONE node, two callback groups across tiers

The two-tier realtime demo collapsed into a **single node**: one component
declares two callback groups in code, and config maps each group to a
different scheduling tier (phase-273 / RFC-0047).

## What it shows

- One Node pkg (`src/subnode_pkg/`, class `subnode_pkg::SubNode`, a
  `ComponentNode` subclass). Its constructor creates two groups and one timer
  per group:

  ```cpp
  auto ctrl_grp  = create_callback_group("ctrl");
  auto telem_grp = create_callback_group("telem");
  create_timer_in<...on_ctrl>(ctrl_grp, 10);     // -> /ctrl
  create_timer_in<...on_telem>(telem_grp, 100);  // -> /telem
  ```

- `src/demo_bringup/system.toml` binds the groups:
  `group_tiers = { ctrl = "high", telem = "low" }` under the single
  `sub_node` component; `[tiers.high]` = 10 ms / posix prio 80,
  `[tiers.low]` = 100 ms / prio 10.

## Run

```sh
source ./activate.sh && nros ws sync
nros codegen-system --bringup demo_bringup
cmake -S . -B build && cmake --build build
zenohd --listen tcp/127.0.0.1:7447 &
./build/src/native_entry/native_entry
```

(Fixture id `workspace-cpp-native-realtime-subnode`; e2e
`realtime_subnode_cpp_e2e`.)

## Expected output

```
[subnode/ctrl] tick=N     # ~10 per one
[subnode/telem] tick=N
```

See [`ws-realtime-cpp-subnode-portable`](../ws-realtime-cpp-subnode-portable/)
for the proof that the node carries no tier names.
