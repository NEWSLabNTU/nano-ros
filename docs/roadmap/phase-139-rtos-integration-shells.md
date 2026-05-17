# Phase 139 — RTOS-Native Integration Shells

**Goal.** Ship one thin integration shell per supported RTOS / IDE so users consume nano-ros via the RTOS's native dependency mechanism (`west update`, ESP-IDF component, PlatformIO library, NuttX app, PX4 external module). Each shell is a few files in `nano-ros/integrations/<rtos>/` that re-export the Phase 137 / 138 root CMake under that RTOS's package convention.

**Status.** Landed (139.1–139.9). Smoke matrix validated 2026-05-18
on dev box after `just setup` + `just esp_idf setup`: all 5
integration smokes (nuttx, platformio, zephyr, px4, esp-idf) PASS
when their respective env is sourced:
- nuttx: no env needed (arm-none-eabi-gcc on PATH)
- platformio: no env needed (PlatformIO Core via Phase 142.1)
- zephyr: `ZEPHYR_BASE=$(pwd)/zephyr-workspace/zephyr`
- px4: `PX4_AUTOPILOT_DIR=$(pwd)/third-party/px4/PX4-Autopilot`
- esp-idf: `source esp-idf-workspace/env.sh`
The `[SKIPPED]` panics fire honestly when env unset — working as
designed per CLAUDE.md "Tests must fail on unmet preconditions".
Cross-links between contributor (`zephyr.md`, `nuttx.md`, `px4.md`)
and integration-shell (`integration-*.md`) pages landed in
`book/src/getting-started/`.

**Priority.** P2 — usability win, not a correctness blocker. Phase 137 + 138 are functional without 139; 139 lets a Zephyr / ESP-IDF / PlatformIO user `west update` / `idf.py add-dependency` / Library Manager-install nano-ros without manually wiring `add_subdirectory`.

**Depends on.** Phase 137 (root CMake to re-export), Phase 138 (per-platform modules each shell needs to surface).

**Related.** Phase 140 (`install-local` removal — these shells become the *only* supported consumption path on their RTOS).

---

## Overview

Phase 137's `add_subdirectory(third_party/nano-ros)` works on every CMake-driven build, but RTOS users don't write raw CMake. They write:

- `west.yml` + `west update` (Zephyr)
- `idf_component.yml` + `idf.py add-dependency` (ESP-IDF)
- `library.json` + PlatformIO Library Manager (PlatformIO / Arduino)
- `apps/external/<name>/Make.defs` + Kconfig (NuttX)
- `src/modules/<name>/CMakeLists.txt` per upstream contract (PX4 EXTERNAL_MODULES_LOCATION — already prototyped in `examples/px4/cpp/uorb/`)

Each ecosystem has its own discovery + dependency manifest. Phase 139 ships one shell per ecosystem; under the hood every shell points at Phase 137's `add_subdirectory(<root>)` (or its RTOS-equivalent).

---

## Architecture

### A. Target layout

```
nano-ros/
└── integrations/
    ├── zephyr/
    │   ├── module.yml                    ← west / zephyr discovery
    │   ├── west.yml                      ← workspace manifest
    │   └── CMakeLists.txt                ← Zephyr-shaped wrapper around root CMake
    ├── esp-idf/
    │   ├── idf_component.yml             ← ESP-IDF component manifest
    │   └── CMakeLists.txt                ← component registration
    ├── platformio/
    │   ├── library.json                  ← PlatformIO library spec
    │   └── examples/                     ← curated subset for PIO discovery
    ├── nuttx/
    │   ├── Make.defs
    │   ├── Makefile
    │   ├── Kconfig
    │   └── CMakeLists.txt
    └── px4/
        ├── README.md                     ← references existing examples/px4/cpp/uorb/ pattern
        └── module-template/              ← copy-out user template
```

### B. Per-RTOS shell contract

Each shell:

1. Re-exports `NanoRos::NanoRos` + `NanoRos::NanoRosCpp` under the RTOS's native target naming (`zephyr_library_link_libraries` style for Zephyr, `COMPONENT_REQUIRES` for ESP-IDF, etc.).
2. Sets `NANO_ROS_PLATFORM` to match the host RTOS BEFORE the root `CMakeLists.txt` is included.
3. Exposes `nano_ros_generate_interfaces(...)` under the RTOS's convention if different (Zephyr's `zephyr_library_*` style, etc.).
4. Documents the one-liner user incantation in its `README.md`.

### C. Zephyr shell (concrete example)

```yaml
# integrations/zephyr/module.yml
name: nano-ros
build:
  cmake: integrations/zephyr
  kconfig: integrations/zephyr/Kconfig
```

```cmake
# integrations/zephyr/CMakeLists.txt
zephyr_library_named(nano_ros)
set(NANO_ROS_PLATFORM zephyr CACHE STRING "" FORCE)
set(NANO_ROS_RMW      "${CONFIG_NROS_RMW}" CACHE STRING "" FORCE)
add_subdirectory(${ZEPHYR_NANO_ROS_MODULE_DIR}/../../ nano_ros_root)
zephyr_library_link_libraries(NanoRos::NanoRos)
```

User in their Zephyr `prj.conf`: `CONFIG_NROS=y` + `CONFIG_NROS_RMW="zenoh"`. `west update` pulls nano-ros; `west build` picks it up automatically.

### D. ESP-IDF shell (concrete example)

```yaml
# integrations/esp-idf/idf_component.yml
description: nano-ros — ROS 2 client for embedded RTOS
dependencies:
  idf: ">=5.1"
```

```cmake
# integrations/esp-idf/CMakeLists.txt
idf_component_register(
    SRCS ""
    INCLUDE_DIRS ../../packages/core/nros-c/include
    REQUIRES "log")

set(NANO_ROS_PLATFORM bare-metal CACHE STRING "" FORCE)
set(NANO_ROS_RMW      "zenoh"    CACHE STRING "" FORCE)
add_subdirectory(${COMPONENT_DIR}/../.. nano_ros_root)
target_link_libraries(${COMPONENT_LIB} INTERFACE NanoRos::NanoRos)
```

User in `main/idf_component.yml`: `nano-ros: { path: "../components/nano-ros/integrations/esp-idf" }` (or via the ESP Component Registry once published).

---

## Work Items

- [x] **139.1 — Zephyr shell.**
      Land `integrations/zephyr/{module.yml,west.yml,CMakeLists.txt,Kconfig}`.
      `Kconfig` exposes `CONFIG_NROS_PLATFORM` (frozen to `zephyr`),
      `CONFIG_NROS_RMW`, `CONFIG_NROS_ROS_EDITION`. `module.yml` makes
      `west` discover this dir as a module.
      **Files.** `integrations/zephyr/*` (new).

- [x] **139.2 — ESP-IDF shell.**
      Land `integrations/esp-idf/{idf_component.yml,CMakeLists.txt,Kconfig.projbuild}`.
      `Kconfig.projbuild` exposes the same `NANO_ROS_*` knobs.
      Pinned to ESP-IDF 5.1+.
      **Files.** `integrations/esp-idf/*` (new).

- [x] **139.3 — PlatformIO shell.**
      Land `integrations/platformio/library.json` + a curated
      `examples/` subset that PlatformIO's library manager
      discovers. PIO consumers add to `platformio.ini`:
      `lib_deps = nano-ros@*`.
      **Files.** `integrations/platformio/*` (new).

- [x] **139.4 — NuttX shell.**
      Land `integrations/nuttx/{Make.defs,Makefile,Kconfig,CMakeLists.txt}`.
      NuttX users copy or symlink to `apps/external/nano-ros/`;
      Kconfig surfaces under `Application Configuration → External
      Modules`.
      **Files.** `integrations/nuttx/*` (new).

- [x] **139.5 — PX4 shell consolidation.**
      `examples/px4/cpp/uorb/` already prototyped the
      EXTERNAL_MODULES_LOCATION pattern with the shim
      `src/modules/<name>/CMakeLists.txt` (Phase 131.C.2). Lift the
      generic part into `integrations/px4/module-template/`; example
      becomes a 5-line wrapper around the template.
      **Files.** `integrations/px4/module-template/*` (new),
      `examples/px4/cpp/uorb/CMakeLists.txt` (shrunk).

- [x] **139.6 — Per-shell smoke tests.**
      One test per integration in
      `packages/testing/nros-tests/tests/integration_<rtos>.rs`.
      Each builds a tiny consumer project via the RTOS's native
      build system (`west build`, `idf.py build`, `pio run`, NuttX
      `make`, PX4 `make px4_sitl_default`), asserts the binary
      exists. Gated by RTOS toolchain presence (`[SKIPPED]` cleanly
      when SDK missing).
      **Files.** `packages/testing/nros-tests/tests/integration_*.rs` (new).

- [x] **139.7 — Doc updates.**
      `book/src/getting-started/` gets one page per integration
      (`zephyr.md`, `esp-idf.md`, `platformio.md`, `nuttx.md`,
      `px4.md`). Each: SDK prereqs → one-liner add-dep command →
      minimal user main.c. `book/src/SUMMARY.md` lists them under
      Getting Started.
      **Files.** `book/src/getting-started/{zephyr,esp-idf,platformio,nuttx,px4}.md` (new),
      `book/src/SUMMARY.md`.

- [x] **139.8 — Registry publishing.**
      Where each ecosystem has a public registry, publish (or
      document publishing) so users get one-line install:
      - Zephyr: included in `west.yml`'s default project list (or
        documented for downstream's manual add)
      - ESP-IDF: published to ESP Component Registry as `nano-ros`
      - PlatformIO: published to PlatformIO Library Registry
      - NuttX / PX4: no central registry; doc points at git
        submodule pattern
      **Files.** `docs/release/registry-publishing.md` (new).

---

## Acceptance

- [ ] `west update` against a workspace containing
      `nano-ros/integrations/zephyr/west.yml` makes
      `examples/zephyr/c/zenoh/talker` buildable with NO other
      manual setup.
- [ ] `idf.py add-dependency nano-ros` (against a local checkout)
      makes an ESP-IDF project link `NanoRos::NanoRos`.
- [ ] `pio lib install nano-ros` (against a local lib_deps pointer)
      makes a PlatformIO project link nano-ros.
- [ ] NuttX `make menuconfig` → enable nano-ros app → build →
      binary contains the C/C++ talker.
- [ ] PX4 `make px4_sitl_default` with `EXTERNAL_MODULES_LOCATION=…/integrations/px4/module-template`
      picks up the module + the user's nros_register_check fires at
      boot.
- [ ] All five `integration_<rtos>` smoke tests pass when their SDK
      is present; `[SKIPPED]` cleanly otherwise.
- [ ] `just ci` green.

---

## Notes

- **Why per-RTOS shells, not one universal CMake.** Each RTOS's
  build system has its own conventions for discovery, naming,
  visibility, install layout. A "one CMake to rule them all"
  attempts to fight those conventions and loses. Phase 138 already
  provides the unified internal CMake; Phase 139 thin-shells it
  per ecosystem.
- **Shell ≠ duplication.** Each shell is <50 lines pointing back
  at the root via `add_subdirectory`. No platform logic lives in
  the shell. The contract: shells translate the RTOS's native
  manifest format to root-CMake cache vars + `add_subdirectory`.
- **Registry publishing is out-of-band.** 139.8 documents but does
  not automate publishing — credentials live with the maintainers,
  not in CI. The how-to is the deliverable.
- **PX4 path already in flight.** Phase 131.C.2's
  `examples/px4/cpp/uorb/` set the pattern (shim
  `src/modules/<name>/CMakeLists.txt` that `include()`s the hoisted
  file). 139.5 generalises that one example into a reusable
  template under `integrations/px4/`.
