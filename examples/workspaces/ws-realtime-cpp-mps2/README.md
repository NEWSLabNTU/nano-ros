# ws-realtime-cpp-mps2 — the three-tier demo cross-compiled to FreeRTOS/QEMU

A 3-node / 3-tier realtime demo — building on
[`ws-realtime-cpp`](../ws-realtime-cpp/) — deployed embedded: FreeRTOS on QEMU
MPS2-AN385, one RTOS task per tier over one shared session (RFC-0015 Model 1).

Phase 144 (#144) adds the `aux` mid tier so the spawn chain has two hops
(boot high → mid → low): the mid tier is spawned BY a spawned tier, the exact
tier↔tier declare race the chained-spawn fix closes. Observing `[aux] tick=`
proves the fix delivered a tier spawned by another spawned tier.

## What it shows

- `src/freertos_entry/` replaces `native_entry` (there is **no** native entry
  here). It has no `src/` at all: `nano_ros_entry(NAME freertos_entry BOARD
  mps2-an385-freertos … TYPED DEPLOY mps2-an385-freertos)` generates the whole
  entry — per-tier setup functions and `nros_app_main` →
  `FreertosBoard::run_tiers`.
- `system.toml` adds `[tiers.high.freertos] priority = 5` /
  `[tiers.mid.freertos] priority = 3` / `[tiers.low.freertos] priority = 2`
  beside the posix priorities — the same tier names drive both deployments.
- The root CMakeLists selects `cmake/toolchain/arm-freertos-armcm3.cmake` when
  `NANO_ROS_BOARD=mps2-an385-freertos`.

Nodes/topics: `ctrl_node` → `/ctrl` @10 ms (high), `aux_node` → `/aux` @50 ms
(mid), `telem_node` → `/telem` @100 ms (low) — all `std_msgs/Int32`.

## Build & run

```sh
source ./activate.sh          # FREERTOS_DIR etc.
just freertos build-fixtures  # workspace lane: scripts/build/workspace-fixtures-build.sh freertos cpp
```

The e2e (`realtime_tiers_cpp_freertos_e2e`) boots the image under
`qemu-system-arm -cpu cortex-m3 -machine mps2-an385` against a router at the
baked locator (`NROS_ENTRY_LOCATOR=tcp/192.0.3.1:17851`).

## Expected output (QEMU console)

```
[ctrl] tick=N
[aux] tick=N       # ~5× fewer than [ctrl]
[telem] tick=N     # ~10× fewer than [ctrl]
```
