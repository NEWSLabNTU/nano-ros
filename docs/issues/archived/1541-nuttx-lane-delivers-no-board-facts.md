---
id: 1541
title: "The NuttX cargo lane runs `cargo build` with no board facts — the Zephyr-arm shape `check-board-facts-delivery` was written for"
status: resolved
resolved: 2026-09-29
type: bug
area: [build, boards]
severity: medium
found: 2026-09-28
related: [phase-472, 0460]
---

## What happens

`packages/api/nros-c/cmake/nros-nuttx.cmake` builds the NuttX image through
`cmake -E env … cargo build`, and delivers none of the board facts
(`NROS_BOARD_TOML`, `NROS_PLATFORM_NAME`, …) that `nros ws board-facts` emits for
every other lane. The Corrosion imports in nros-c's, nros-cpp's and the zenoh
staticlib's `CMakeLists.txt` carry none either.

`check-board-facts-delivery` reads only `cmake/*.cmake` and `zephyr/cmake/*.cmake`,
so it cannot see these files. Its own docstring's lesson is that the Zephyr arm
once shipped inert for exactly this reason. Copying either file into `cmake/`
makes the gate fail on it; at its real path it passes.

## What is NOT established

**Whether NuttX images need the facts.** If the NuttX lane resolves every knob it
consumes from somewhere else, this is a gate gap only. If any `nros-node` /
`nros-params` knob reaches a NuttX image through the RFC-0049 ladder, then NuttX
images are taking builtin defaults — issue 1388's class — and this is a live bug.
The audit did not build a NuttX image to find out.

## Fix

Establish which of the two it is, then either deliver the facts or record why the
lane does not need them; widen the gate's population in either case (phase-472 W5).

## Resolution

**Which of the two it was: a gate gap AND a missing delivery, with no size
change on the images measured.** Option A — deliver, and widen the gate.

### What changed

- `packages/api/nros-c/cmake/nros-nuttx.cmake` calls the shared
  `nros_resolve_board_facts()` (`cmake/NanoRosBoardFacts.cmake`, i.e.
  `nros ws board-facts`) and puts `${NROS_BOARD_FACTS_ENV}` on its
  `cmake -E env … cargo build` of `nros-nuttx-ffi`, beside the entity facts
  issue 1142 put there. No second resolver.
- The Corrosion imports in nros-c's, nros-cpp's and nros-rmw-zenoh-staticlib's
  `CMakeLists.txt` call the new `nros_board_facts_env_deferred(<target>)`, the
  board-facts twin of `nros_entity_facts_env_deferred` — deferred for the same
  reason (those files run during `nano_ros_workspace()`, before the entry's
  deploy is known). It wraps `nros_board_facts_env`; nothing new resolves.
- `check-board-facts-delivery` reads EVERY tracked `*.cmake` /
  `CMakeLists.txt` (+ `.in`), harvested from the git index (358 files), not two
  authored directories. It strips full-line comments, recognises cargo held in a
  variable (`"${_nnbe_cargo}" build` — the old `\bcargo\b` matched the NuttX
  file only by the accident of a later bare `cargo`), requires the facts list to
  be READ (`${NROS_BOARD_FACTS_ENV}` / `IN LISTS`), not merely named, and runs a
  negative control on every invocation (left the selftest ratchet). Two host-only
  Corrosion imports are exempt with reasons: the `nros` CLI and the
  `multi_pkg_workspace_mixed` test template.

### Measured

- Gate: origin/main's `nros-nuttx.cmake` under the widened gate → rc=1, naming
  it; the same file under the OLD gate → rc=0 ("5 … deliver, 2 exempt"). Each of
  the three `CMakeLists.txt` reverted to origin/main → rc=1 naming it. Fixed
  tree → rc=0 (9 deliver, 4 exempt).
- NuttX, `workspace-c-nuttx` (`qemu-armv7a-nuttx`), built before and after with
  only `nros-nuttx.cmake` differing: configure now prints `board facts … 5
  value(s) delivered to cargo` (`NROS_BOARD`, `NROS_BOARD_TOML`,
  `NROS_PLATFORM_NAME=nuttx`, `NROS_SDK_NUTTX`, `NROS_SDK_NUTTX_APPS`).
  `nros-node`'s build script now watches the board and platform descriptors
  (proof the platform/board rungs were loaded); every generated file under every
  build script's `OUT_DIR` is byte-identical, and so is the linked kernel ELF
  (`text 610296 / data 6152 / bss 411632`). As the issue suspected: NuttX
  declares no `[knobs]` and its board no `[board.knobs]`, so naming the platform
  swapped builtin for builtin. The live-bug half is latent, not active.
- ThreadX-Linux, `workspace-c-threadx-linux`, before/after with only the three
  `CMakeLists.txt` differing: `NROS_PLATFORM_NAME=threadx-linux` now reaches
  `_cargo-build_nros_c` and `_cargo-build_nros_cpp`, and both `nros-node` units
  change `EXECUTOR_BACKING_U64S` from `EXECUTOR_BACKING_DEFAULT_U64S` to the
  board's stated `11069` — the board rung reaching a C image's runtime crate for
  the first time. The linked `threadx_entry` is byte-identical (the board comment
  records 11069 as the unnarrowed default).

### NOT measured

- No runtime e2e was run: both images measured are byte-identical before and
  after, so there was no new binary to boot.
- Native (`linux`), FreeRTOS, ThreadX-riscv64, esp32 and NuttX-riscv images were
  not built before/after. None of their platforms declares `[knobs]` and only
  esp32/threadx-linux boards declare `[board.knobs]`, which is why no change is
  EXPECTED there — reasoned, not measured.
- `nros-rmw-zenoh-staticlib/CMakeLists.txt` is added by no in-tree
  `add_subdirectory` found in this sweep; its delivery is gated, not exercised.
