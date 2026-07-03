# ws-realtime-cpp — two-node, two-tier realtime demo (C++)

The C++ base of the scheduling-tiers differentiator (RFC-0015 §4.2): a control
node and a telemetry node run on two priority tiers declared **in config**,
not in node code.

## What it shows

| Node | Class | Group → tier | Publishes |
| --- | --- | --- | --- |
| `ctrl_node` | `ctrl_pkg::Ctrl` | `ctrl` → `high` (10 ms spin, posix prio 80) | `/ctrl` `std_msgs/Int32` every 10 ms |
| `telem_node` | `telem_pkg::Telem` | `telem` → `low` (100 ms spin, posix prio 10) | `/telem` every 100 ms |

Tiers live in `src/demo_bringup/system.toml` (`[tiers.high]` / `[tiers.low]` +
`[[component]].group_tiers`); the components are configure-shape
(`Result configure(nros::Node&)`).

## Run

```sh
source ./activate.sh && nros ws sync
nros codegen-system --bringup demo_bringup
cmake -S . -B build && cmake --build build
zenohd --listen tcp/127.0.0.1:7447 &   # or: just native zenohd
./build/src/native_entry/native_entry
```

(Fixture lane: `just native build-workspace-fixtures`; e2e
`realtime_tiers_cpp_e2e` asserts the ~10× cadence ratio.)

## Expected output

```
[ctrl] tick=1
[telem] tick=1
[ctrl] tick=2
...        # ~10 [ctrl] lines per [telem] line
```

## Variants

`-mps2` (FreeRTOS/QEMU), `-rclcpp` (ComponentNode IS-A-node), `-subnode`
(one node, two callback groups), `-subnode-portable` (renamed tiers) — each
has its own README.
