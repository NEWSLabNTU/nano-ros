---
id: 97
title: "`nros codegen entry` (C/C++) is native-only — no embedded board runners, blocking embedded workspace entries"
status: resolved
type: enhancement
area: codegen
related: [phase-263, rfc-0043]
---

## Status (2026-06-25) — RESOLVED: embedded entry path proven end-to-end on threadx-linux C

The embedded `nano_ros_entry(LAUNCH)` path now builds, boots, AND **delivers** — the C2a
runtime e2e (`tests/c_threadx_entry_e2e.rs`) is GREEN: the threadx-linux host-sim entry's
talker publishes `/chatter` to a separate native listener through a zenoh router (cross
process per issue 0096), single host, **no veth bridge / no root**. Both follow-ups below
are fixed.

The codegen + cmake half (W1–W3) is done and verified on threadx-linux C:
- **W1 (C++ emitter, `emit_cpp.rs`):** for non-native boards, emit `nros_app_main` +
  `NROS_APP_MAIN_REGISTER_VOID()` + `#include <nros/app_main.h>` (the locator-less
  `run_components` overload) instead of `int main` (`board_is_embedded()` helper).
- **W2 (C emitter, `codegen.rs` dispatch):** route an embedded **C** entry through the C++
  emitter (it produces a `.cpp` TU, calling each C node via its `extern "C"`
  `__nros_c_component_*` seam). Native C keeps the pure-`.c` `nros_board_native_run_components`.
- **W3 (cmake):** `nano_ros_entry` embedded `nros_platform_link_app` pass (gated
  `NANO_ROS_BOARD IN_LIST DEPLOY`); `.c`→`.cpp` ext switch for embedded C; the
  `node_register` threadx + freertos carrier `AND _NRC_DEPLOY` gate (the documented bug);
  workspace-root `-DNANO_ROS_PLATFORM`/`-DNANO_ROS_BOARD` override.

**Verified:** a ws-c ThreadX entry now BUILDS + LINKS + BOOTS the ThreadX kernel and
dispatches to the generated `app_main` → `ThreadxBoard::run_components` — **no segfault**
(the original 0097 symptom is gone; the emitted shape is correct, `nm` shows a strong
`app_main` + `nros_app_main`).

**Two follow-ups (both FIXED in the C2a work — they were two MORE pieces of wiring the
`nano_ros_entry` LAUNCH path was missing that the `nano_ros_node_register` carrier already
had; root cause for both: the LAUNCH path builds the exe itself, so the carrier's wiring
never ran for it):**
1. **threadx-linux host-sim runtime delivery — FIXED (locator bake).** `run_components`
   returned before the spin because the generated TU calls the locator-LESS
   `<Board>::run_components(&setup)` overload, which reads the compile-time
   `NROS_ENTRY_LOCATOR` macro — default `""` → backend discovery, which finds no router over
   the nsos-netx POSIX-`connect()` shim, so `nros::init` fails. The carrier bakes the locator
   via its `configure_file` template; the LAUNCH path did not. Fix: `nano_ros_entry` now
   defines `NROS_ENTRY_LOCATOR` on the embedded entry target (precedence
   `-DNROS_ENTRY_LOCATOR` cache > `LOCATOR` arg > per-board default; threadx-linux dials
   loopback, QEMU boards dial slirp 10.0.2.2). With a baked `tcp/127.0.0.1:<port>` the host
   sim connects + the talker publishes — not a network-stack bug.
2. **nros-c/nros-cpp sizes-header mirror staleness — FIXED (OBJECT_DEPENDS ordering).** The
   generated `.cpp` TU could compile BEFORE Corrosion mirrors the per-build
   `nros_{,cpp_}config_generated.h` (the include DIR is on the lib's INTERFACE but gives no
   build-order edge — issue 0088/0090), reading the in-tree stub (`*_OPAQUE_U64S
   undeclared`). The carrier adds `add_dependencies` on the mirror targets + a file-level
   `OBJECT_DEPENDS` on the mirrored header globals; the LAUNCH path did not. Fix:
   `nano_ros_entry` now applies the same `_nros_node_register_config_header_deps` +
   `OBJECT_DEPENDS` to its generated TU. A fresh embedded build dir now compiles clean — no
   manual header copy.

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

## Fix direction (mapped 2026-06-25)

The N-node `__nros_entry_setup` body is already board-agnostic and correct; only the **outer
entry wrapper** is wrong for embedded. The board run-classes
(`::nros::board::{Threadx,Freertos,Nuttx,Zephyr}Board::run_components`) already exist
(`packages/core/nros-cpp/include/nros/main.hpp:213–395`), each with a locator-less overload that
reads the compile-time `NROS_ENTRY_LOCATOR` macro — so **the codegen needs no locator**.

The emitters are inline string builders (no template files):

- **C++ — `packages/cli/nros-cli-core/src/codegen/entry/emit_cpp.rs` (~383).** Already
  board-aware (`board_cpp_path()` maps `threadx-linux → ::nros::board::ThreadxBoard`, etc.) but
  emits `int main(){ return <Board>::run_components(&__nros_entry_setup); }` for **all** boards.
  For an RTOS that **double-mains** with the board `startup.c` (which owns `main` → kernel enter
  → app thread). **Fix:** for non-native boards emit `#include <nros/app_main.h>` +
  `extern "C" int nros_app_main(int,char**){ return <Board>::run_components(&__nros_entry_setup); }`
  + `NROS_APP_MAIN_REGISTER_VOID();` (the same shape as the single-node templates). Native keeps
  `int main`.
- **C — `emit_c.rs` (~128).** Fully native-only: hardcodes `nros_board_native_run_components`
  (a C symbol) and ignores `plan.board`. The embedded board runners are C++ only. **Fix:** for
  non-native boards, emit a **C++ TU** (output extension `.cpp`) with the same `nros_app_main`
  shape as C++, but invoke each component via its existing `extern "C"`
  `__nros_c_component_<pkg>_create/configure` seam (exactly what
  `cmake/templates/threadx_entry_main_c_typed.cpp.in` does — a C component with a C++ entry TU).
  Native keeps the pure-`.c` shape.
- **cmake (`NanoRosEntry.cmake`).** (1) `nano_ros_entry` must pass the **real** board key to
  `--board` (so `board_cpp_path` selects `ThreadxBoard`, not the `zephyr` auto-derive — boards
  differ in spin/init). (2) Reapply the C2a-spike fixes: the embedded `nros_platform_link_app`
  pass + the `node_register` `DEPLOY` gate (above). (3) An embedded C entry links `NanoRosCpp`
  (the entry TU is C++), like a TYPED-C native entry.

So the whole fix is: per-board outer wrapper in both emitters + a `.c`→`.cpp` switch for embedded
C + the cmake board-key/link reapply. The setup body, board run-classes, app_main macro, and
templates all already exist.

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
