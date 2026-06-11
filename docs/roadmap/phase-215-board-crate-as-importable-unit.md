# Phase 215 — Board crate as importable unit (FVP first)

**Goal.** Turn `packages/boards/nros-board-<name>/` into a unit a
downstream Zephyr app (ASI is the reference consumer) imports with a
**single cmake call** plus a **`west fvp run`** invocation, with no
hand-curated `EXTRA_CONF_FILE` / `DTC_OVERLAY_FILE` / hardcoded board
id list / hand-rolled FVP launch flags on the consumer side. FVP
AEMv8-R is the driving case; the shape is generic across boards.

**Status.** OPEN. Driven by ASI's Phase 190 follow-up — ASI today
hand-glues every layer Phase 215 collapses.

**Priority.** P1 — unblocks ASI's actuation consumption story + every
future external Zephyr consumer of a nano-ros board crate.

**Depends on.** Phase 117.10–117.14 (FVP build smokes), Phase 199
(Zephyr 3.7 floor), Phase 214.A (local FVP runner; this phase moves
its surface from `just zephyr` into `west fvp`).

## Overview

The driving asymmetry today:

| Layer                                              | Today                                                       | Phase 215                          |
|----------------------------------------------------|-------------------------------------------------------------|------------------------------------|
| Zephyr board id                                    | ASI hardcodes `ZEPHYR_TARGET_LIST=(…)` in `build.sh`        | reads from `board.cmake`           |
| Base `prj.conf` (kernel, POSIX, networking)        | ASI duplicates parts of nano-ros's per-board `prj.conf`     | layered by `nano_ros_use_board()`  |
| Board overlay (DTS + per-board Kconfig)            | ASI carries its own `actuation_module/boards/<id>.{conf,overlay}` | layered from board crate         |
| Default RMW (`cyclonedds` on Phase-117 boards)     | `-DNANO_ROS_RMW=cyclonedds` typed out by every script       | board crate declares default        |
| Gated tool resolution (`ARM_FVP_DIR` etc.)         | ad-hoc env in consumer scripts                              | `nros doctor --board <name>`        |
| FVP runner                                         | nano-ros's `just zephyr run-fvp-aemv8r-cyclonedds`          | `west fvp run -d <dir>` extension   |

The board crate (`nros-board-fvp-aemv8r-smp/`) already CARRIES the
artifacts (`prj.conf`, `boards/<hwv2-id>.{conf,overlay}`, Rust
skeleton, Cargo features). What's missing is the **import surface**:
a single cmake fn that layers them into a downstream Zephyr build,
plus a `west` extension that owns FVP launch.

Net effect on ASI's `actuation_module/CMakeLists.txt`:

```cmake
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(actuation_module LANGUAGES CXX)
nano_ros_use_board(fvp-aemv8r-smp)   # <— ONE LINE replaces all glue
target_sources(app PRIVATE src/main.cpp ...)
```

And ASI's `build.sh` shrinks to:

```sh
west build -d build actuation_module                       # build
ARM_FVP_DIR=/opt/arm/Base_RevC_AEMv8R west fvp run -d build  # run
```

## Architecture

```
            ASI's CMakeLists.txt
                  │
                  │  nano_ros_use_board(fvp-aemv8r-smp)
                  ▼
   zephyr/cmake/nano_ros_use_board.cmake   (nano-ros Zephyr module)
                  │
                  │  include()
                  ▼
   packages/boards/nros-board-fvp-aemv8r-smp/board.cmake   (sidecar manifest)
                  │
                  │  declares: ZEPHYR_ID, TOOLCHAIN, GATED_PKGS,
                  │            DEFAULT_RMW, DEFAULT_TRANSPORT,
                  │            RUNNER, PRJ_CONF, BOARD_CONF, OVERLAY
                  ▼
   layered into the app build via:
       EXTRA_CONF_FILE  ← prj.conf + boards/<id>.conf
       DTC_OVERLAY_FILE ← boards/<id>.overlay
       BOARD            ← ZEPHYR_ID (if user didn't pass `-b`)
       NANO_ROS_RMW     ← DEFAULT_RMW (if not -D'd)

   ───────────────────────────────────────────────────────────

   west fvp run -d build/
                  │
                  │  reads CMakeCache.txt → NROS_BOARD_RUNNER=armfvp
                  ▼
   scripts/west_commands/fvp.py    (nano-ros west extension)
                  │
                  │  scripts/zephyr/resolve-fvp-bin.sh
                  ▼
   ARMFVP_BIN_PATH=<resolved>  west build -d build -t run
                  │
                  ▼
   zephyr/cmake/emu/armfvp.cmake (upstream Zephyr; UART → stdout)
```

**Two faces of the manifest.** `board.cmake` is the cmake face;
Cargo.toml `[package.metadata.nros.board]` mirrors the same facts
for `nros doctor` + future Rust/CLI consumption (Phase 212.L.8
alignment). Both must stay in lock-step — Phase 215.F audit guards
against drift.

## Work Items

### 215.A — Sidecar `board.cmake` per board crate

- [ ] **215.A.1** Define the `board.cmake` schema. Variables:
      `NROS_BOARD_ZEPHYR_ID` (Zephyr `BOARD` string),
      `NROS_BOARD_TOOLCHAIN` (SDK abi target, e.g. `aarch64-zephyr-elf`),
      `NROS_BOARD_GATED_PKGS` (semicolon list keyed on
      `nros-sdk-index.toml` `[gated.*]` names),
      `NROS_BOARD_DEFAULT_RMW` (`cyclonedds` / `zenoh` / `xrce`),
      `NROS_BOARD_DEFAULT_TRANSPORT` (`ethernet` / `serial` / …),
      `NROS_BOARD_RUNNER` (`armfvp` / `qemu` / `native` / …),
      `NROS_BOARD_PRJ_CONF` (absolute path),
      `NROS_BOARD_BOARD_CONF` (absolute path; per-board hwv2
      Kconfig fragment),
      `NROS_BOARD_BOARD_OVERLAY` (absolute path; per-board DTS overlay).
- [x] **215.A.2** Write `packages/boards/nros-board-fvp-aemv8r-smp/board.cmake`
      filling every variable. Absolute paths via
      `${CMAKE_CURRENT_LIST_DIR}`. _(landed `01ef6bd1a`, 2026-06)_
- [ ] **215.A.3** Documented schema cross-references this phase doc.
- **Files:** `packages/boards/nros-board-fvp-aemv8r-smp/board.cmake`
  (new), `docs/reference/board-cmake-schema.md` (new).

### 215.B — `nano_ros_use_board(<name>)` cmake fn

- [x] **215.B.1** New `zephyr/cmake/nano_ros_use_board.cmake` (~80 LoC
      hard cap). `function(nano_ros_use_board NAME)`: _(landed `2b9a909c9`, 2026-06; 86 LoC incl. 215.B.3 call-order guard)_
      1. Resolve `BOARD_DIR = ${NROS_REPO_DIR}/packages/boards/nros-board-${NAME}`.
      2. `FATAL_ERROR` if `${BOARD_DIR}/board.cmake` missing.
      3. `include("${BOARD_DIR}/board.cmake")`.
      4. If `BOARD` empty → set `BOARD ${NROS_BOARD_ZEPHYR_ID}` CACHE;
         else WARN on mismatch.
      5. `list(APPEND EXTRA_CONF_FILE  ${NROS_BOARD_PRJ_CONF}
                                       ${NROS_BOARD_BOARD_CONF})`
         + `PARENT_SCOPE` re-export.
      6. `list(APPEND DTC_OVERLAY_FILE ${NROS_BOARD_BOARD_OVERLAY})`
         + `PARENT_SCOPE`.
      7. If `NANO_ROS_RMW` undefined → cache `${NROS_BOARD_DEFAULT_RMW}`.
      8. Cache `NROS_BOARD_RUNNER` so `west fvp run` reads it from
         `CMakeCache.txt`.
- [ ] **215.B.2** `zephyr/CMakeLists.txt` `include()`s the new fn so
      it's available to every downstream app once nano-ros's Zephyr
      module is on `ZEPHYR_EXTRA_MODULES`.
- [ ] **215.B.3** `nano_ros_use_board()` must be call-able BEFORE
      `find_package(Zephyr)` OR after — order tested both ways. The
      `EXTRA_CONF_FILE` / `BOARD` overrides need to land before
      Zephyr's board-resolution phase, so the fn either re-orders
      (sets variables) or `FATAL_ERROR`s on wrong-order call. Phase
      215.B.3 verifies via the 215.E.1 fixture.
- **Files:** `zephyr/cmake/nano_ros_use_board.cmake` (new),
  `zephyr/CMakeLists.txt` (include + invocation site).

### 215.C — Cargo.toml `[package.metadata.nros.board]` mirror

- [ ] **215.C.1** Add the metadata table to
      `packages/boards/nros-board-fvp-aemv8r-smp/Cargo.toml`:
      ```toml
      [package.metadata.nros.board]
      zephyr_board = "fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp"
      toolchain    = "aarch64-zephyr-elf"
      gated        = ["arm-fvp"]
      default_rmw  = "cyclonedds"
      default_transport = "ethernet"
      runner       = "armfvp"
      prj_conf      = "prj.conf"
      board_conf    = "boards/fvp_baser_aemv8r_fvp_aemv8r_aarch64_smp.conf"
      board_overlay = "boards/fvp_baser_aemv8r_fvp_aemv8r_aarch64_smp.overlay"
      ```
- [ ] **215.C.2** Schema struct + `deny_unknown_fields` in
      `nros-cli-core::orchestration::board_metadata`. (Mirrors
      Phase 212.B's strict cargo-metadata reader.)
- [ ] **215.C.3** `nros board info <name>` subcommand (read-only)
      prints both the Cargo.toml view AND the parsed `board.cmake`
      view side-by-side; flags drift between them. Phase 215.F audit
      hook.
- **Files:** `packages/boards/nros-board-fvp-aemv8r-smp/Cargo.toml`,
  `nros-cli/packages/nros-cli-core/src/orchestration/board_metadata.rs`
  (new), `nros-cli/packages/nros-cli-core/src/cmd/board.rs` (new).

### 215.D — `west fvp` extension (moves Phase 214.A runner)

- [ ] **215.D.1** `scripts/west_commands/fvp.py` — west command
      `class FvpRun(WestCommand)`. `do_run`:
      1. Argparse `-d/--build-dir` (default `build/`).
      2. Read `CMakeCache.txt` → `NROS_BOARD_RUNNER`. If not
         `armfvp`, error "board <name> is not an FVP target".
      3. Shell `scripts/zephyr/resolve-fvp-bin.sh` → `ARMFVP_BIN_PATH`.
      4. `os.execvpe('west', ['west', 'build', '-d', build_dir,
         '-t', 'run'], env=...)`.
- [ ] **215.D.2** `scripts/west-commands.yml` declares `fvp` subcommand:
      ```yaml
      west-commands:
        - file: scripts/west_commands/fvp.py
          commands:
            - name: fvp
              class: FvpRun
              help: run a nano-ros FVP board in the Arm Fast Models simulator
      ```
- [ ] **215.D.3** `zephyr/module.yml` adds `west-commands:
      scripts/west-commands.yml` so any workspace that has nano-ros as
      a west project gets the extension for free (no manual
      registration).
- [ ] **215.D.4** Phase 214.A `just zephyr run-fvp-aemv8r*` recipes
      are RETAINED (developer ergonomics inside nano-ros) but
      delegate to `west fvp run` instead of duplicating the resolver
      + `-t run` invocation. ~3-line recipes.
- **Files:** `scripts/west_commands/fvp.py` (new),
  `scripts/west-commands.yml` (new), `zephyr/module.yml` (edit),
  `just/zephyr.just` (retarget Phase 214.A recipes).

### 215.E — Fixture: minimal "ASI-shaped" consumer

- [ ] **215.E.1** `packages/testing/nros-tests/fixtures/board_import_fvp/` —
      minimal Zephyr app:
      `CMakeLists.txt` with **only** `find_package(Zephyr)` +
      `project()` + `nano_ros_use_board(fvp-aemv8r-smp)` +
      `target_sources(app PRIVATE src/main.c)`;
      `src/main.c` with a trivial `printk` + `nros::init` smoke;
      `prj.conf` empty (everything from the board crate);
      `package.xml` for Phase 210 closure;
      no per-board confs/overlays in the fixture.
- [ ] **215.E.2** `packages/testing/nros-tests/tests/phase215_e_board_import.rs`
      — `west build -d build fixtures/board_import_fvp` succeeds;
      asserts the generated `CMakeCache.txt` carries
      `NROS_BOARD_ZEPHYR_ID=fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp`
      + `NANO_ROS_RMW=cyclonedds`. Build only — Phase 215.G runs the
      FVP smoke.
- **Files:**
  `packages/testing/nros-tests/fixtures/board_import_fvp/` (new),
  `packages/testing/nros-tests/tests/phase215_e_board_import.rs` (new).

### 215.F — Drift audit: cmake vs Cargo metadata

- [ ] **215.F.1** `packages/testing/nros-tests/tests/phase215_f_manifest_drift.rs`
      — for every `packages/boards/nros-board-*/` carrying both
      `board.cmake` and `[package.metadata.nros.board]`: parse each,
      assert byte-equal field-by-field for the overlapping keys
      (`zephyr_board`, `toolchain`, `default_rmw`, `runner`, conf/
      overlay paths). Bare boards (Rust-only Phase 212.N tier-1
      shims without `board.cmake`) are skipped.
- [ ] **215.F.2** `nros board info <name>` (Phase 215.C.3) carries
      a `--check-drift` flag wired to the same audit; CI gate.
- **Files:**
  `packages/testing/nros-tests/tests/phase215_f_manifest_drift.rs`
  (new), `nros-cli/packages/nros-cli-core/src/cmd/board.rs` (extend).

### 215.G — End-to-end FVP smoke through the import surface

- [ ] **215.G.1** Extend the Phase 214.C runtime smoke
      (when it lands) to drive the fixture from 215.E.1 (not the
      example carve-out). `nros_tests::skip!` if `ARM_FVP_DIR`
      unset; otherwise `west fvp run -d build/board_import_fvp`,
      cycle-limited, grep for the `nros: smoke ok` line.
- [ ] **215.G.2** Document the test as the canonical "is the board
      crate import surface healthy?" gate.
- **Files:** `packages/testing/nros-tests/tests/phase215_g_fvp_smoke.rs`
  (new, gated).

### 215.H — ASI migration (consumer landing)

External landing in `github.com/NEWSLabNTU/autoware-safety-island`:
- [ ] **215.H.1** Bump nano-ros pin to the commit landing 215.A-D.
- [ ] **215.H.2** `actuation_module/CMakeLists.txt` — add
      `nano_ros_use_board(fvp-aemv8r-smp)` immediately after the
      `project()` call; delete the hand-written board overlay
      includes + the `-DNANO_ROS_RMW=cyclonedds` defines.
- [ ] **215.H.3** `actuation_module/boards/<hwv2-id>.conf` /
      `.overlay` — only ASI-specific deltas remain (e.g. autoware
      msg sizing); nano-ros base layered via 215.B.
- [ ] **215.H.4** `build.sh` — drop the ZEPHYR_TARGET_LIST hardcode
      for FVP (the value comes from `board.cmake`); keep the
      s32z270dc2 entry as-is until that board crate lands too.
- [ ] **215.H.5** Replace `west build && manual FVP launch` with
      `west build -d build && west fvp run -d build`.
- [ ] **215.H.6** `fvp/NOTES.md` cross-refs the new path; AVH cloud
      path stays separate.
- **Files** (external, ASI repo): `actuation_module/CMakeLists.txt`,
  `actuation_module/west.yml` (nano-ros pin), `build.sh`,
  `fvp/NOTES.md`.

### 215.I — Book chapter

- [ ] **215.I.1** `book/src/porting/board-crate-import.md` — the
      consumer guide. Cross-refs Phase 212.N.8 board-trait porting +
      Phase 214 FVP runtime + Phase 191 sdk provisioning.
- [ ] **215.I.2** SUMMARY.md update.
- **Files:** `book/src/porting/board-crate-import.md` (new),
  `book/src/SUMMARY.md` (edit).

## Acceptance

- [ ] A Zephyr app's `CMakeLists.txt` calls `nano_ros_use_board(<n>)`
      and NOTHING else of nano-ros-board-specific shape (no per-board
      conf, no overlay, no `EXTRA_CONF_FILE` hand-list, no
      `-DNANO_ROS_RMW=...`).
- [ ] `west fvp run -d build/` discovers the Phase 214.A resolver +
      launches `FVP_BaseR_AEMv8R` end-to-end, UART → stdout, exits
      clean on Ctrl-C.
- [ ] `nros board info fvp-aemv8r-smp` prints the board metadata
      from BOTH `board.cmake` and `Cargo.toml`; `--check-drift`
      exits 0 when they agree, non-zero with a clear field-by-field
      diff on drift.
- [ ] ASI's `actuation_module/CMakeLists.txt` includes
      `nano_ros_use_board(fvp-aemv8r-smp)` and builds clean against
      Zephyr 3.7 floor without ASI carrying any nano-ros-base
      Kconfig fragment.
- [ ] Phase 215.E fixture builds in CI; the fixture's
      `CMakeCache.txt` carries the expected `NROS_BOARD_*` keys.
- [ ] Phase 215.F drift audit passes for every
      `packages/boards/nros-board-*` carrying a `board.cmake`.

## Notes

- `nano_ros_use_board()` is **Zephyr-specific**. Non-Zephyr boards
  (native, freertos, threadx) carry the Phase 212.N Board trait
  impl in Rust; their consumption shape is `cargo` path-deps, not
  cmake. Two ecosystems, two surfaces — no unified verb attempted.
- The `west fvp` extension sits outside Phase 212 §Non-Goals
  (`nros build/test/flash/monitor`): `west` owns the verb, not
  `nros`. Phase 212.J `nros launch` precedent confirms `west` /
  ROS-tool ownership of run-time verbs is acceptable.
- Phase 214.A recipes stay in `just zephyr` as developer ergonomic
  thin shells over `west fvp run` (so `just zephyr build-* &&
  just zephyr run-*` stays the in-tree dev loop). ASI never reaches
  the justfile; nano-ros contributors do.
- Multiple FVP variants (Cortex-R52, Corstone-310, AEMv8-R aarch32)
  follow the same shape: one board crate each, sibling
  `board.cmake` files, all pointing at `runner = "armfvp"` so the
  west extension is variant-agnostic. Deferred until a real second
  variant lands.
- The board crate's Rust skeleton (`src/lib.rs` `init_hardware()` +
  `run<F>(config, app) -> !`) is **NOT** changed by Phase 215.
  Phase 212.N Board trait migration is orthogonal. Phase 215 only
  touches the CMake / Cargo-metadata / west surfaces — the Rust
  surface stays Phase 117.10 skeleton.
- `arm-fvp-installer` (Phase 214.B) is a hard prereq for
  `nros doctor --board fvp-aemv8r-smp` to be useful; if 214.B is
  still open when 215 lands, the doctor check stays warn-only on
  missing `ARM_FVP_DIR`.
