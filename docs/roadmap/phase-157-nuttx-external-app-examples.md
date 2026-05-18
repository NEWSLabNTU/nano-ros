# Phase 157 — NuttX External-App Layout for C/C++ Examples

**Goal.** Migrate the 12 NuttX C/C++ examples (6 C + 6 C++ under
`examples/qemu-arm-nuttx/{c,cpp}/zenoh/`) so they ship as canonical
NuttX **external apps** (`apps/external/<name>/{Kconfig,Make.defs,
Makefile}` trio) on top of their existing `CMakeLists.txt`. CMake
stays the unified build entry across all platforms; NuttX gets thin
wrappers that delegate to it instead of duplicating the build logic.

**Status.** Active 2026-05-18.

**Priority.** P2. Closes the only platform where C/C++ example
fixtures bypass the RTOS-preferred build flow (audit landed in
commit `e24b29a5` — every other platform's C/C++ examples already
use their canonical build tool: Zephyr `west`, ESP-IDF `idf.py`,
PX4 `px4_add_module`; FreeRTOS / ThreadX have no canonical
user-facing tool so raw cmake is appropriate).

**Depends on.** None blocking. Phase 139.4 (`integrations/nuttx/`
shell) + Phase 152.7 (`Rust.mk` wiring for `nros-c` Cargo build
in NuttX `context::` phase) already provide the heavy lifting —
`libnros_c.a` is appended to `EXTRA_LIBS`, headers are on
`INCDIR_PREFIX`. Examples just need to register as siblings under
`apps/external/`.

**Related.** Phase 140 (add_subdirectory consumption model — kept
intact; CMakeLists are the canonical build), Phase 144 (in-tree
cmake variant header path), Phase 155 (RTOS E2E follow-ups, same
QEMU bring-up infrastructure).

---

## Overview

NuttX's canonical user flow for third-party apps:

```
nuttx/
└── apps/
    └── external/
        ├── nano-ros/                 # integrations/nuttx/ — Phase 139.4 shell
        │   ├── Kconfig
        │   ├── Make.defs
        │   ├── Makefile
        │   └── (symlink or clone of nano-ros repo)
        └── nano-ros-talker-c/        # one per example
            ├── Kconfig
            ├── Make.defs
            ├── Makefile
            └── (symlink or clone of examples/qemu-arm-nuttx/c/zenoh/talker/)
```

User runs `make menuconfig` → enables `CONFIG_NROS` +
`CONFIG_NROS_EXAMPLE_TALKER_C` → `make` from NuttX tree → NuttX's
`apps/external/*/Make.defs` discovery picks up both, runs Cargo
in `context::` (via `apps/tools/Rust.mk`) for the platform lib,
then compiles + links the example's `src/main.c` against the
prebuilt `libnros_c.a` + the headers the shell exposed.

Today's path (Phase 144.6) skips all of this:
`just nuttx build-fixtures` runs raw `cmake -S <example> -B <build>`
on each example as a standalone tree. Works for QEMU smoke but
doesn't exercise:

  * NuttX Kconfig integration (`menuconfig` discoverability).
  * `apps/tools/Rust.mk` wiring (Cargo invocation from
    `context::` phase).
  * `EXTRA_LIBS` / `EXTRA_LIBPATHS` resolution against the
    NuttX-tree libdirs.
  * `Application.mk` priority / stacksize / module registration.

Real NuttX users hit different bugs in those code paths that QEMU
cmake smoke never catches.

## Architecture

CMakeLists.txt remains the **canonical** build entry. Three new
files per example are thin wrappers that delegate:

```
examples/qemu-arm-nuttx/c/zenoh/talker/
├── CMakeLists.txt    # unchanged — works standalone for QEMU smoke
├── Kconfig           # NEW — ~5 lines, declares CONFIG_NROS_EXAMPLE_TALKER_C
├── Make.defs         # NEW — ~5 lines, registers into CONFIGURED_APPS
├── Makefile          # NEW — ~15 lines, includes apps/Application.mk
├── config.toml       # existing
├── package.xml       # existing
├── src/main.c        # existing
└── generated/        # codegen output
```

### `Kconfig` skeleton

```kconfig
config NROS_EXAMPLE_TALKER_C
    bool "nano-ros: C talker (zenoh)"
    default n
    depends on NROS && NROS_C_API
    ---help---
        Publishes std_msgs/Int32 on /chatter via zenoh-pico.
```

### `Make.defs` skeleton

```make
ifneq ($(CONFIG_NROS_EXAMPLE_TALKER_C),)
CONFIGURED_APPS += $(APPDIR)/external/nano-ros-talker-c
endif
```

### `Makefile` skeleton

```make
include $(APPDIR)/Make.defs

PROGNAME  = nuttx_c_talker
PRIORITY  = SCHED_PRIORITY_DEFAULT
STACKSIZE = 16384
MODULE    = $(CONFIG_NROS_EXAMPLE_TALKER_C)

CSRCS = src/main.c $(wildcard generated/*.c)
MAINSRC = src/main.c

include $(APPDIR)/Application.mk
```

Headers + `libnros_c.a` path come from the parent
`integrations/nuttx/Make.defs` (gated on `CONFIG_NROS`). Examples
don't duplicate any of that.

### What stays the same

  * `CMakeLists.txt` per example — Phase 144.6 shape preserved.
    `just nuttx build-fixtures` (cmake smoke) keeps running for
    fast regression coverage.
  * `package.xml`, `config.toml`, `generated/` — untouched.
  * `add_subdirectory(<repo-root>)` consumption model — Phase 140
    promise (no install step) holds for both paths.

### What changes

  * `integrations/nuttx/Kconfig` gains an
    `if NROS_C_API` block that sources each example's `Kconfig`
    via `osource "$APPDIR/external/nano-ros-*/Kconfig"` (NuttX's
    glob-include).
  * `just nuttx` gains a new `build-fixtures-make` recipe that
    stages the symlinks + runs `make` from a configured NuttX
    tree. Existing `build-fixtures` (cmake) stays. `build-all`
    invokes both.

## Work Items

### 157.A — Wrapper-file scaffolding per example

- [ ] **157.A.1 — Spike on `talker-c`.**
      Write `Kconfig + Make.defs + Makefile` for
      `examples/qemu-arm-nuttx/c/zenoh/talker/`. Verify
      hand-staged `apps/external/nano-ros-talker-c → <example>`
      + `apps/external/nano-ros → <repo-root>` symlinks +
      `menuconfig` shows both knobs.
      **Files:**
      `examples/qemu-arm-nuttx/c/zenoh/talker/{Kconfig,Make.defs,Makefile}`.

- [ ] **157.A.2 — Replicate across remaining C examples.**
      `listener`, `service-server`, `service-client`,
      `action-server`, `action-client`. Same skeleton, different
      `PROGNAME` + `CONFIG_NROS_EXAMPLE_*` symbol.
      **Files:**
      `examples/qemu-arm-nuttx/c/zenoh/{listener,service-server,
      service-client,action-server,action-client}/{Kconfig,
      Make.defs,Makefile}`.

- [ ] **157.A.3 — Replicate across all 6 C++ examples.**
      Same shape; Makefile uses `CXXSRCS` instead of `CSRCS`;
      Kconfig depends on `NROS_CPP_API` instead of `NROS_C_API`.
      **Files:**
      `examples/qemu-arm-nuttx/cpp/zenoh/*/{Kconfig,Make.defs,Makefile}`
      (6 dirs).

### 157.B — Integration-shell Kconfig glob-include

- [ ] **157.B.1 — `osource` each example Kconfig from the shell.**
      `integrations/nuttx/Kconfig` adds a glob include block that
      pulls every `apps/external/nano-ros-*` sibling's `Kconfig`
      under a "nano-ros Examples" menu. Gated on `NROS_C_API ||
      NROS_CPP_API`.
      **Files:** `integrations/nuttx/Kconfig`.

### 157.C — Justfile + CI wiring

- [ ] **157.C.1 — `just nuttx build-fixtures-make` recipe.**
      Stages a NuttX defconfig (`boards/arm/qemu/
      qemu-armv7a/configs/nsh` baseline + nano-ros knobs flipped
      via `kconfig-tweak`), symlinks `apps/external/nano-ros` +
      each `apps/external/nano-ros-<example>-<lang>`, invokes
      `make` from `nuttx/`. One build cycle covers all 12
      examples (NuttX builds a single binary; examples register
      as built-in apps).
      **Files:** `just/nuttx.just`,
      `scripts/nuttx/stage-external-apps.sh` (new helper).

- [ ] **157.C.2 — `just nuttx build-all` aggregates both.**
      Existing `build-all: build build-examples build-fixtures` →
      `build-all: build build-examples build-fixtures
      build-fixtures-make`. Cmake smoke stays primary
      (fast); make-based path runs as parity check.
      **Files:** `just/nuttx.just`.

- [ ] **157.C.3 — `just nuttx test-e2e-make`.**
      Boots the make-built NuttX QEMU binary, runs the talker /
      listener E2E against a zenohd, asserts the same wire-level
      delivery the cmake-built fixtures already verify. Skips
      cleanly via `nros_tests::skip!` when `$NUTTX_DIR` isn't
      configured.
      **Files:** `just/nuttx.just`,
      `packages/testing/nros-tests/tests/nuttx_make_e2e.rs` (new).

### 157.D — User-facing documentation

- [ ] **157.D.1 — Book chapter on canonical NuttX flow.**
      `book/src/getting-started/integration-nuttx.md` already
      covers the integration shell; extend with the
      "clone-or-symlink each example as `apps/external/nano-ros-
      <example>`" walkthrough + `menuconfig` screenshot of the
      "nano-ros Examples" menu landed in 157.B.1. Cross-link
      from `examples/README.md` per-platform consumption table.
      **Files:**
      `book/src/getting-started/integration-nuttx.md`,
      `examples/README.md`.

- [ ] **157.D.2 — Roadmap follow-up: codegen helper.**
      If hand-written wrappers prove repetitive over time, promote
      to a `nros_generate_nuttx_app(<target> [PRIORITY ...]
      [STACKSIZE ...])` cmake function that emits the three files
      at configure time from the existing cmake target
      properties. Tracked as 157.D.2 to avoid premature
      abstraction — defer until ≥ 2 contributors complain.
      **Files (when activated):**
      `cmake/NanoRosNuttxApp.cmake` (new).

## Acceptance Criteria

- [ ] Every C/C++ NuttX example has `Kconfig + Make.defs +
      Makefile` siblings to its `CMakeLists.txt` (12 wrappers
      total, ~25 lines per example).
- [ ] `integrations/nuttx/Kconfig` surfaces all 12 examples
      under a "nano-ros Examples" menu after `make menuconfig`
      against a vanilla NuttX defconfig.
- [ ] `just nuttx build-fixtures-make` produces a NuttX binary
      with all 12 examples linked in (verified via
      `nm $NUTTX_DIR/nuttx | grep nuttx_<lang>_<example>`).
- [ ] `just nuttx test-e2e-make` reaches the same delivery
      assertions as the cmake-built `rtos_e2e Platform_Nuttx`
      tests for at least talker + listener (E2E parity gate).
- [ ] CMake smoke path (`just nuttx build-fixtures`) keeps
      working unchanged — no regression in QEMU smoke coverage.
- [ ] Book chapter `integration-nuttx.md` updated with the new
      external-app walkthrough.

## Notes

### Why wrappers instead of replacing cmake

User preference (2026-05-18): "I use CMake as the unified entry
for simplicity. if nuttx has a strong preference, I prefer turn it
to a wrapper instead of maintaining separate build scripts."

CMake stays the canonical build entry — it's the only platform
where every example builds identically (Phase 140
add_subdirectory shape). NuttX's `Kconfig + Make.defs + Makefile`
trio delegates to `apps/Application.mk` for the actual compile +
link; the heavy lifting (`libnros_c.a` build via Cargo, headers,
`EXTRA_LIBS` paths) was already done by the
`integrations/nuttx/{Make.defs,Makefile}` shell in Phase 152.7.

Per-example overhead is ~25 lines of boilerplate. Acceptable
without auto-generation; codegen helper deferred to 157.D.2 if
the boilerplate proves repetitive.

### Why not migrate FreeRTOS / ThreadX too

FreeRTOS + ThreadX have no canonical user-facing build tool —
upstream FreeRTOS-Kernel ships Make + CMake examples in parallel,
Microsoft's Azure-RTOS docs use both, and most users wire them
into whatever build system their board vendor ships. Raw cmake on
those platforms IS the closest thing to a canonical flow.

NuttX is different — `apps/external/*/Make.defs` discovery is the
documented + universal way to ship a third-party NuttX app. The
integration shell already commits to that path (Phase 139.4);
examples were the last hold-out.

### Carve-outs

  * **NuttX CMake mode** (newer NuttX configurations drive the
    build via CMake instead of Kconfig+Make): the
    `integrations/nuttx/CMakeLists.txt` sibling already handles
    this; example wrappers don't apply to CMake-driven NuttX
    builds. Phase 157 targets the Kconfig+Make path which is what
    `boards/arm/qemu/qemu-armv7a/` uses.
  * **Hardware boards beyond QEMU**: examples live under
    `qemu-arm-nuttx/` — same external-app layout works for real
    hardware (sim64, stm32f4discovery, etc.) but board-specific
    bring-up is out of scope for this phase. Documented as a
    follow-up under Phase 145 (cache discipline / user projects).
