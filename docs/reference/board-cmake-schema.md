# `board.cmake` schema

Sidecar CMake manifest carried by every importable nano-ros board crate.
The `nano_ros_use_board(<name>)` cmake function (Phase 215.B) `include()`s
it to layer the board's prj.conf / per-board Kconfig / DTS overlay /
default RMW / runner onto a downstream Zephyr app build.

This document is the contract: each board's `board.cmake` MUST set every
variable in the "Variables" section below, and EVERY variable MUST be
consumed by exactly one of the listed consumers. New variables land here
first, then in the matching consumer. The "Provisioning contract" section
(Phase 215.J) adds OPTIONAL fields — a Zephyr consumer board sets them to
be provisionable via `nros setup board`, non-Zephyr boards omit them.

Cross-ref: `docs/roadmap/phase-215-board-crate-as-importable-unit.md`
(work items 215.A.1–215.A.3 introduce this schema; 215.C.1 mirrors the
same field set into `Cargo.toml` `[package.metadata.nros.board]`).

## File location

```
packages/boards/nros-board-<name>/board.cmake
```

Path-resolution rule: every `*_FILE` / `*_CONF` / `*_OVERLAY` variable
MUST be an absolute path. Use `${CMAKE_CURRENT_LIST_DIR}` (NOT
`${CMAKE_SOURCE_DIR}` or `${PROJECT_SOURCE_DIR}` — those resolve into the
consumer's tree, not the board crate's).

## Variables

### `NROS_BOARD_ZEPHYR_ID`

Zephyr `BOARD` string passed to `west build -b <id>` (and equivalently
the `BOARD` cmake cache variable).

- Format: `<board>/<soc>/<variant>` for hwv2 boards, `<board>` for hwv1.
- Example: `fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp`.
- Consumed by: `nano_ros_use_board()` step 4 — sets the `BOARD` cache
  variable when the downstream user has not passed `-b` (and warns on
  mismatch when they have).

### `NROS_BOARD_TOOLCHAIN`

The SDK ABI target string that Zephyr-SDK selects (the
`<arch>-zephyr-<abi>` triple used by `*-zephyr-elf-gcc` etc.).

- Format: `<arch>-zephyr-<abi>` matching a Zephyr-SDK toolchain dir.
- Examples: `aarch64-zephyr-elf`, `arm-zephyr-eabi`,
  `riscv64-zephyr-elf`.
- Consumed by: `nros board info <name>` (Phase 215.C.3) — informational;
  `nros doctor --board <name>` cross-checks that the SDK install carries
  the matching toolchain.

### `NROS_BOARD_GATED_PKGS`

Semicolon-separated list of license-gated package names from
`nros-sdk-index.toml` `[gated.*]` that the board depends on.

- Format: a CMake list (semicolon-separated). Empty list = no gated
  deps.
- Valid values: any key under `[gated.*]` in `nros-sdk-index.toml`
  (e.g. `arm-fvp`, `nv-spe-fsp`).
- Consumed by: `nros doctor --board <name>` (Phase 215.C.3 / 215.F.2)
  — for each name in the list, check the matching `env` var
  (e.g. `arm-fvp` → `ARM_FVP_DIR`) is set + the install layout looks
  right. Hard failure if missing, with installer hint from the index.

### `NROS_BOARD_DEFAULT_RMW`

The RMW backend the board's reference example was built against.
Downstream consumer keeps the right to override via
`-DNANO_ROS_RMW=<rmw>`, but if nothing is passed `nano_ros_use_board()`
populates this default.

- Valid values: `cyclonedds`, `zenoh`, `xrce` (the three RMWs nano-ros
  ships post-Phase 169 — `dust-dds` is retired).
- Consumed by: `nano_ros_use_board()` step 7 — sets `NANO_ROS_RMW` in
  the cache if undefined.

### `NROS_BOARD_DEFAULT_TRANSPORT`

The transport layer the board's reference example wires up
(matches the board crate's default Cargo feature on the transport
axis — `ethernet` / `wifi` / `serial` etc.).

- Valid values: `ethernet`, `wifi`, `serial`, `loopback`.
- Consumed by: `nros board info <name>` — informational only today.
  Phase 215 does NOT plumb a `-DNANO_ROS_TRANSPORT=` cache var; the
  transport selection is a board crate Cargo feature, not a
  consumer-visible knob.

### `NROS_BOARD_RUNNER`

The Zephyr `west` runner the board uses for `-t run` / flash.

- Valid values: `armfvp` (Arm Fast Models), `qemu` (any of the
  qemu_* boards), `native` (native_sim / native_posix), or any
  upstream Zephyr runner name (`jlink`, `pyocd`, …).
- Consumed by: the `west fvp` extension (Phase 215.D) reads
  `NROS_BOARD_RUNNER` from `CMakeCache.txt` and refuses to run if it
  isn't `armfvp`. Generic `west <runner> run` dispatch from
  this variable is a Phase 215.D follow-up.

### `NROS_BOARD_PRJ_CONF`

Absolute path to the board crate's base `prj.conf` — the Kconfig
fragment that wires kernel sizing, POSIX, networking, and the nros
module bits common to every consumer of this board (irrespective of
hwv2 SoC variant).

- Format: absolute path. Use `${CMAKE_CURRENT_LIST_DIR}/prj.conf`.
- Consumed by: `nano_ros_use_board()` step 5 — `list(APPEND
  EXTRA_CONF_FILE …)` so Zephyr layers it on top of the consumer's
  own `prj.conf`.

### `NROS_BOARD_BOARD_CONF`

Absolute path to the per-board hwv2 Kconfig fragment under the board
crate's `boards/` directory.

- Format: absolute path. Use
  `${CMAKE_CURRENT_LIST_DIR}/boards/<hwv2-id>.conf`.
- The `<hwv2-id>` format is the slash-flattened Zephyr board id with
  `/` replaced by `_` (e.g. `fvp_baser_aemv8r_fvp_aemv8r_aarch64_smp`).
- Consumed by: `nano_ros_use_board()` step 5 — appended alongside
  `NROS_BOARD_PRJ_CONF` so the hwv2-specific deltas land on top.

### `NROS_BOARD_BOARD_OVERLAY`

Absolute path to the per-board DTS overlay under the board crate's
`boards/` directory.

- Format: absolute path. Use
  `${CMAKE_CURRENT_LIST_DIR}/boards/<hwv2-id>.overlay`.
- Consumed by: `nano_ros_use_board()` step 6 — `list(APPEND
  DTC_OVERLAY_FILE …)` so Zephyr layers it on top of the consumer's
  own DTS overlay.

## Provisioning contract (Phase 215.J)

`nano_ros_use_board()` layers the board *config* (the variables above),
but an `import:false` downstream Zephyr consumer does NOT inherit
nano-ros's *toolchain provisioning* (zephyr patches × zephyr-lang-rust ×
cyclonedds source). The following fields make the board crate the single
source of truth for what a consumer's tree needs, and drive
`nros setup board <name> --zephyr-workspace <dir>` (Phase 215.J.2).

All four are OPTIONAL — a board that omits them is not provisionable via
`nros setup board` (e.g. non-Zephyr boards consumed via cargo path-deps).
Their `Cargo.toml` mirror lives in `[package.metadata.nros.board]` as
`zephyr_line` / `requires_rust` / `rust_targets` / `rmw_source`.

### `NROS_BOARD_ZEPHYR_LINE`

The Zephyr support line (the `(zephyr × zephyr-lang-rust)` pair's zephyr
floor) — selects the patch set `scripts/zephyr/patches/<line>.sh`, which
takes the consumer's workspace dir as `$1`.

- Example: `3.7`.
- Consumed by: `nros setup board` step (b) — `bash
  scripts/zephyr/patches/<line>.sh <zephyr-workspace>`.

### `NROS_BOARD_REQUIRES_RUST`

Whether the board's build needs the Rust language module (`CONFIG_RUST`).

- Format: a CMake boolean token (`y` / `ON` / `true` / `1` → true).
  Mirrored as a TOML `bool` on the Cargo side.
- Consumed by: `nros setup board` steps (c) + (d) — gates `rustup target
  add` and the `zephyr-lang-rust` presence check.

### `NROS_BOARD_RUST_TARGETS`

Semicolon list of rustup target triple(s) the board's Zephyr build
compiles the nros staticlib for (what `zephyr-lang-rust`'s
`_rust_map_target` returns for the board's arch).

- Example: `aarch64-unknown-none` (AArch64 AEMv8-R).
- Consumed by: `nros setup board` step (c) — `rustup target add <t>` for
  each.

### `NROS_BOARD_RMW_SOURCE`

The `nros-sdk-index.toml` `[source.*]` name for the board's RMW source
tree. Index-driven — never a hardcoded path or a hand `git submodule
update`.

- Example: `cyclonedds-src`.
- Consumed by: `nros setup board` step (a) — `nros setup --source
  <name>` (provisioned into nano-ros's own tree, which the consumer
  links via `nano_ros_use_board()` / `add_subdirectory(packages/dds/…)`).

### `NROS_BOARD_RUST_SUPPORT_MODULE`

Absolute path to a board-shipped Zephyr Kconfig overlay MODULE
(`<dir>/zephyr/module.yml` + `<dir>/Kconfig`) that enables
`RUST_SUPPORTED` for the board's arch via a cross-file `default y if
<board>` extension — WITHOUT mutating the consumer's `zephyr-lang-rust`
tree (Phase 215.J.4, option B).

- Format: absolute path. Use
  `${CMAKE_CURRENT_LIST_DIR}/zephyr-rust-support`.
- Consumed by: `nano_ros_use_board()` step 9 — appended to
  `ZEPHYR_EXTRA_MODULES` so the overlay is picked up automatically.

### Board west fragment (not a `board.cmake` variable)

Each Zephyr consumer board also ships
`packages/boards/nros-board-<name>/west-downstream.yml` (Phase 215.J.3):
an `import:false`-compatible west manifest fragment declaring ONLY
`zephyr-lang-rust` at nano-ros's supported rev. A consumer adds one
`manifest.self.import: - file: <…>/west-downstream.yml` line +
`west update` to fetch the module. This is the one genuinely west-native
piece (module fetch); everything else flows through `nros setup board`.

## Consumers

The schema is consumed at three sites:

- `zephyr/cmake/nano_ros_use_board.cmake` (Phase 215.B, not yet
  landed). The cmake function `nano_ros_use_board(<name>)` resolves
  `BOARD_DIR = ${NROS_REPO_DIR}/packages/boards/nros-board-<name>`,
  `include()`s `${BOARD_DIR}/board.cmake`, then routes each variable as
  documented above (`EXTRA_CONF_FILE` / `DTC_OVERLAY_FILE` / `BOARD` /
  `NANO_ROS_RMW` / cached `NROS_BOARD_RUNNER`). All overrides land
  BEFORE Zephyr's board-resolution phase — the function either
  re-orders (sets variables in the parent scope) or `FATAL_ERROR`s on
  wrong-order invocation.

- `nros board info <name>` (Phase 215.C.3, lives in `nros-cli`, not in
  this superproject). Read-only: parses both `board.cmake` and the
  `[package.metadata.nros.board]` table and prints them side by side,
  flagging any field that disagrees. The same parser backs
  `--check-drift` and the Phase 215.F drift audit.

- Phase 215.F drift audit
  (`packages/testing/nros-tests/tests/phase215_f_manifest_drift.rs`).
  For every `packages/boards/nros-board-*/` that carries BOTH
  `board.cmake` and `[package.metadata.nros.board]`, the audit
  parses each and asserts byte-equal field-by-field for the overlap
  (`zephyr_board`, `toolchain`, `default_rmw`, `runner`, conf/overlay
  paths, and the Phase 215.J provisioning fields `zephyr_line`,
  `requires_rust`, `rust_targets`, `rmw_source`). Bare Rust-only boards
  without `board.cmake` are skipped.

## Adding a new board

1. Drop the board crate at `packages/boards/nros-board-<name>/` with
   the standard layout (`prj.conf` + `boards/<hwv2-id>.{conf,overlay}`
   + Rust skeleton).
2. Author `board.cmake` setting every variable in this doc. Use
   `${CMAKE_CURRENT_LIST_DIR}` for path resolution.
3. Mirror the same facts into `[package.metadata.nros.board]` in the
   crate's `Cargo.toml` (see Phase 215.C.1 in the phase doc).
4. Phase 215.F audit catches any drift between the two faces.
