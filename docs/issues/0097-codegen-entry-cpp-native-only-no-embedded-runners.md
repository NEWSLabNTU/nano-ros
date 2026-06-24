---
id: 97
title: "`nros codegen entry` (C/C++) is native-only — no embedded board runners, blocking embedded workspace entries"
status: open
type: enhancement
area: codegen
related: [phase-263, rfc-0043]
---

## Summary

The C/C++ entry emitter `nros codegen entry` only emits a **native** runner. Its `--board`
help reads: *"Defaults to `native` — the only Entry-pkg target the C/C++ surface supports
today (Phase 212.L.2)"*. The generated TU is always:

```c
int main(int argc, char** argv) {
    ...
    return nros_board_native_run_components(&__nros_entry_setup);
}
```

So a CMake **workspace** Entry (`nano_ros_entry(LAUNCH …)`) cannot target an embedded board:
even configured for ThreadX/FreeRTOS/NuttX (and even passing `--board zephyr`), the emitted
`int main()` + `nros_board_native_run_components` clashes with the board's startup contract
(e.g. ThreadX `startup.c` provides `main()` → `tx_kernel_enter` → an app thread that calls
`app_main`), so the binary **links but SIGSEGVs at boot**.

This blocks phase-263 **Track C / C2** (embedded C/C++/mixed workspace entries).

## Why the single-node path works but LAUNCH does not

`nano_ros_node_register(... DEPLOY <rtos>)` (the standalone single-node carrier) emits the
**board-correct** shape via a per-platform CMake template —
`cmake/templates/threadx_entry_main_c_typed.cpp.in`,
`freertos_entry_main_c_typed.cpp.in`, `nuttx_entry_main_c_typed.cpp.in` — which define the
board's boot symbol (`NROS_APP_MAIN_REGISTER_VOID()` / `app_main`) and route the component to
the board runner (`ThreadxBoard::run_components`, etc.), deferring to the board `startup.c`.

The **multi-node** LAUNCH emitter (`nros codegen entry`) has no embedded board runners — it
always emits the native runner. So the two paths diverge: single-node embedded works,
multi-node (workspace LAUNCH) embedded does not.

## Fix direction

Teach the C/C++ `nros codegen entry` emitter to produce **per-board embedded runners** for the
LAUNCH case — the app_main / board-run shape the `node_register` templates already use, but for
the N-node launch (register each `(pkg, exec)` component, then hand to the board runner inside
the board's app entry). Either:
- emit board-specific entry TUs keyed on `--board <rtos>` (reuse the existing template shapes), or
- emit a board-agnostic `app_main` that the board `startup.c` calls (move the native-vs-board
  selection to a single `nros_board_run_components` seam).

## Adjacent finding (cmake side, ready to reapply)

Two CMake gaps were implemented + verified during the phase-263 C2a spike (then reverted,
pending this codegen work):

1. **`nano_ros_entry` had no embedded link pass.** Its LAUNCH executable only called
   `nros_platform_link_app` for `NANO_ROS_PLATFORM == posix` (`cmake/NanoRosEntry.cmake:201–206`);
   embedded fell through unlinked. Fix: add an embedded branch calling `nros_platform_link_app`
   when a board is loaded (`NANO_ROS_BOARD IN_LIST DEPLOY`). Verified: it correctly pulled the
   ThreadX startup + board TU + kernel/netstack into the workspace entry.
2. **The embedded `node_register` carrier branches (threadx, freertos) were missing the
   documented `DEPLOY` gate.** `NanoRosNodeRegister.cmake:142–148` states the carrier branches
   gate on `<rtos> IN_LIST _NRC_DEPLOY` (so a reusable workspace node stays component-only) —
   but only the **nuttx** branch had it; threadx + freertos fired the carrier for any node,
   turning a workspace node into a standalone app (and hitting a broken template path). Fix:
   add `AND _NRC_DEPLOY` to those guards. This is a real bug vs the documented contract.

Plus a workspace-root change (accept `-DNANO_ROS_PLATFORM`/`-DNANO_ROS_BOARD` overrides so the
workspace configures as one board per build dir). All three are correct and small; they are
useful only once this codegen gap is closed.

## Repro

Configure a C workspace for ThreadX-on-Linux (`-DNANO_ROS_PLATFORM=threadx
-DNANO_ROS_BOARD=threadx-linux`) with the two cmake fixes above + a launch Entry: it builds and
links, but the generated `int main()` (native runner) SIGSEGVs under the ThreadX kernel boot.
