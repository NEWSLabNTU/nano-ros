---
id: 2687
title: "An out-of-tree FreeRTOS board package must copy the family's FreeRTOSConfig.h, lwipopts.h and arch/cc.h"
status: resolved
type: tech-debt
severity: low
area: [boards, freertos, build]
related: [phase-577, phase-575]
found: 2026-11-18
resolved: 2026-11-23
resolved_in: "#4706: one always-present family include dir (nros_freertos_family_*.h) on both roads"
---

**Home: phase-577** (RFC-0135, board packages; its doc lands with PR #4177).
Not a precondition of phase-577 W8, which works around it by copying.

## What happens

An in-tree FreeRTOS board states two numbers and includes the family's
configuration by a path relative to its own directory:

```c
#define NROS_BOARD_CPU_CLOCK_HZ 25000000
#define NROS_BOARD_PRIO_BITS    3
#include "../../nros-board-freertos/config/FreeRTOSConfig.h"
```

A board package outside nano-ros cannot spell that path. The CMake road could
add `${NROS_SOURCE_DIR}/packages/boards` to the include path, but the cargo
road's C builds (`zpico-sys`, `nros-platform-freertos`, the board crates' own
`build.rs`) are handed only `FREERTOS_CONFIG_DIR`, a single directory, and see
nothing else. Measured on phase-577 W8's fixture: with the include written as
`"nros-board-freertos/config/FreeRTOSConfig.h"` the CMake half built and the
cargo half could not have, so the fixture took RFC-0064's rung 3 and owns the
whole of three files, copied from `packages/boards/nros-board-freertos/config/`:

- `FreeRTOSConfig.h` (252 lines; the board's facts are 3 of them);
- `lwipopts.h` (218 lines, none board-specific);
- `arch/cc.h` (55 lines, none board-specific).

Each copy goes stale the moment the family's file changes: a new config macro,
a changed heap default, a tracing hook.

## What would fix it

A seam both roads honour, so an outside board states only its facts. Two shapes
to weigh:

- the family config dir as a second, always-present include directory on
  every FreeRTOS C build (CMake and every `build.rs` that reads
  `FREERTOS_CONFIG_DIR`), with boards including it by a name that cannot
  collide (`nros_freertos_family_config.h`);
- or the family files generated into the board's config dir at configure time
  from the board's stated numbers.

Either removes the three copies from phase-577 W8's fixture
(`packages/testing/nros-tests/fixtures/out-of-tree-r5f/boards/nros-board-fixture-r5f-freertos/config/`),
which is the acceptance.

## Ruling (2026-11-23)

Shape (a): one family include directory, always present, with headers named so
they cannot collide. Nothing is generated. Two reasons:

- **No staleness class.** A configure-time copy of the family files into the
  board's dir is the shape of issues 0460 and 1115: a second carrier that has
  to be kept equal to the first, and goes wrong silently when it is not.
- **It reuses the carrier.** `FREERTOS_CONFIG_DIR` already reaches every
  FreeRTOS C build on both roads, and the family dir rides beside it.

## Fix

**The family headers carry their own names.** In
`packages/boards/nros-board-freertos/config/`:
- `FreeRTOSConfig.h` became `nros_freertos_family_config.h`;
- `lwipopts.h` became `nros_freertos_family_lwipopts.h`;
- `arch/cc.h` became `nros_freertos_family_cc.h`.

So the directory holds no name a board's config dir supplies, and putting it
on every include path can never shadow a board's own file. The test
`the_family_dir_holds_only_names_no_board_has` (nros-build-paths) keeps it so.

**One resolver per road**, each called by every FreeRTOS C compile that reads
`FREERTOS_CONFIG_DIR`, and placed after it:

- cargo road, `nros_build_paths::freertos_family_config_dir()`:
  - `add_freertos_includes` (the family crate, the overlay runner and every
    board crate it serves);
  - the overlay runner's `shared_config_dir`;
  - the POSIX board's kernel build;
  - the zpico size probe;
  - the descriptor token `{freertos_family_config_dir}`, used by the zenoh
    FreeRTOS port row in `nros-rmw.toml`.
- CMake road, `nros_freertos_family_config_dir()` (`nros-freertos.cmake`):
  - the `freertos_config` INTERFACE target;
  - `NROS_FREERTOS_INCLUDES`;
  - the Cyclone FreeRTOS flags.

**Boards include the family by name.** The in-tree boards' wrappers now read
`#include "nros_freertos_family_config.h"` and so on. The out-of-tree fixture
(`packages/testing/nros-tests/fixtures/out-of-tree-r5f/boards/nros-board-fixture-r5f-freertos/config/`)
dropped its three copies:
- `FreeRTOSConfig.h` holds the board's facts, the include and its tick hooks;
- `lwipopts.h` and `arch/cc.h` are one include each.

Its copy of `FreeRTOSConfig.h` had already fallen behind: it lacked issue
2812's `NROS_FREERTOS_SERVICE_HEAP_EXTRA_KB` term. That is the staleness the
issue predicted.

**Checked:** check-out-of-tree-board-images: OK — the board package's own knob reached its build script; C, C++ and Rust images from the out-of-tree board package (copied, reached by NROS_EXTRA_BOARD_PATH and FIXTURE_R5F_SDK_ROOT) all carry Tag_ABI_VFP_args: VFP registers and Tag_FP_arch: VFPv3-D16, and every member of every target archive is R-profile with that FPU, and every member stating a calling convention uses VFP registers; a user CFLAGS is merged and reported; selftest: a softfp object, an archive with an M-profile member and one with a softfp member are refused, an archive with a hard-float assembly member passes builds the copied-out
fixture's C, C++ and Rust images, both roads, with the three copies gone, and
now refuses a config dir that holds the family's body again. The POSIX board's
kernel compile () builds. Board-less integrations
(, ) supply their own
 and are unchanged; they may include the family's headers by
path.
