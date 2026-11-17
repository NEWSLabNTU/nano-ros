---
id: 2687
title: "An out-of-tree FreeRTOS board package must copy the family's FreeRTOSConfig.h, lwipopts.h and arch/cc.h"
status: open
type: tech-debt
severity: low
area: [boards, freertos, build]
related: [phase-577, phase-575]
found: 2026-11-18
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
