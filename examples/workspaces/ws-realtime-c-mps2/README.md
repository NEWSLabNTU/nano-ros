# ws-realtime-c-mps2 — the two-tier C demo cross-compiled to FreeRTOS/QEMU

The **C** sibling of [`ws-realtime-cpp-mps2`](../ws-realtime-cpp-mps2/): a
2-node / 2-tier realtime demo — the C projection of
[`ws-realtime-c`](../ws-realtime-c/) — deployed embedded: FreeRTOS on QEMU
MPS2-AN385, one RTOS task per tier over one shared session (RFC-0015 Model 1).

phase-281 W2 closes the `C × freertos` cell of the execution-model convergence
matrix: the shared C `nros_board_freertos_run_tiers` glue is already exercised
by the C++ FreeRTOS e2e, but no C-*node* multi-tier test existed. Observing
`[ctrl] tick=` AND `[telem] tick=` proves that shared C `run_tiers` impl drives
a C node.

## What it shows

- `src/ctrl_pkg` / `src/telem_pkg` are **C** nodes (`NROS_C_COMPONENT`), reused
  verbatim from `ws-realtime-c`. Each prints `[<tier>] tick=N` only when its
  `publish` succeeds.
- `src/freertos_entry/` replaces `native_entry` (there is **no** native entry
  here). It has no `src/` at all: `nano_ros_entry(NAME freertos_entry LANG c
  BOARD mps2-an385-freertos … TYPED DEPLOY mps2-an385-freertos)` generates the
  whole entry — per-tier setup functions and `nros_app_main` →
  `FreertosBoard::run_tiers` (the codegen routes embedded-C through the C++
  emitter, so the C nodes are instantiated via their `extern "C"` seam).
- `system.toml` adds `[tiers.high.freertos] priority = 5` /
  `[tiers.low.freertos] priority = 2` beside the posix priorities — the same
  tier names drive both deployments.
- The root CMakeLists selects `cmake/toolchain/arm-freertos-armcm3.cmake` when
  `NANO_ROS_BOARD=mps2-an385-freertos`.

Nodes/topics: `ctrl_node` → `/ctrl` @10 ms (high), `telem_node` → `/telem`
@100 ms (low) — both `std_msgs/Int32`.

## Build & run

```sh
source ./activate.sh          # FREERTOS_DIR etc.
just freertos build-fixtures  # workspace lane: scripts/build/workspace-fixtures-build.sh freertos c
```

The e2e (`realtime_tiers_c_freertos_e2e`) boots the image under
`qemu-system-arm -cpu cortex-m3 -machine mps2-an385` against a router at the
baked locator (`NROS_ENTRY_LOCATOR=tcp/192.0.3.1:17861`).

## Expected output (QEMU console)

```
[ctrl] tick=N
[telem] tick=N     # ~10× fewer than [ctrl]
```

## Copy-out notes

Standard workspace copy-out. Fixture id `workspace-c-freertos-realtime`.
