# realtime-cpp-subnode-portable — tier names are deployment-owned

The portability proof for `realtime-cpp-subnode`:
the **identical** `SubNode` component (same groups `ctrl`/`telem`, same
topics) runs under a bringup whose tiers are named **`fast`/`bulk`** instead
of `high`/`low`. The package carries only group IDs — tier names belong to the
deployment.

## What it shows

- `src/subnode_pkg/` — functionally identical node logic to the `-subnode` variant.
- `src/deploy_bringup/` (note: not `demo_bringup`) —
  `group_tiers = { ctrl = "fast", telem = "bulk" }`, with `[tiers.fast]`
  (10 ms, posix prio 80) and `[tiers.bulk]` (100 ms, prio 10) in
  `system.toml`; the entry's `main.cpp` references
  `"deploy_bringup:system.launch.xml"`.

## Run

```sh
source ./activate.sh
nros build native
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd &
./build/src/native_entry/native_entry
```

(Fixture id `workspace-cpp-native-realtime-subnode-portable`; e2e
`realtime_subnode_cpp_portable_e2e`.)

## The same package, freestanding (phase-427 W12)

`[image.freertos]` builds the **identical** `subnode_pkg` for
FreeRTOS/mps2-an385 — `arm-none-eabi-g++`, `thumbv7m-none-eabi`, no OS
underneath. Nothing in `src/subnode_pkg/` changes: the move is
`[tiers.fast.freertos]` / `[tiers.bulk.freertos]` priorities plus the image
block, all in `deploy_bringup/system.toml`. That is the portability claim of
this workspace measured against a cross toolchain rather than a second tier
vocabulary.

Why it is worth a fixture: RFC-0089 correction 1 measured that deriving
`rclcpp::Node` costs nothing freestanding (no allocator, no exceptions, no
vtable), and correction 8 recorded that **no in-tree fixture exercised it** —
every consumer of a `SHAPE rclcpp` package was `platform = "linux"`. This image
is the consumer.

```sh
source ./activate.sh
nros build freertos
```

`SubNode`'s two callback groups land on two tiers, and a node whose groups span
tiers cannot take `run_tiers` (per-tier setup functions construct whole *nodes*)
— so the generated entry is `FreertosBoard::run_components` plus
`nros_cpp_create_sched_context_from_policy` / `nros_cpp_bind_group_sched`, a
different shape from the `workspace-{c,cpp}-freertos-realtime` entries.

Fixture id `workspace-cpp-freertos-realtime-subnode-portable`. **Build-only** —
there is no `matrix::CELLS` row and nothing boots the image under QEMU yet; the
baked locator `tcp/192.0.3.1:8092` is reserved for whoever adds one.

## Expected output

Identical to the `-subnode` variant — that is the point:

```
[subnode/ctrl] tick=N     # ~10 per one
[subnode/telem] tick=N
```

<!-- phase-331 W6 renamed this workspace and dropped the README with the old
     directory; content restored from a92778843^ with the ws- prefixes updated. -->
