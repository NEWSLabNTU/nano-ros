# Importing a Board Crate

This chapter is for **consumers** of a nano-ros board crate -- vendors and downstream applications (the Autoware Safety Island archetype) who want to bring a board into their own Zephyr app with as little glue as possible. If you are *writing* a new board crate, see [The `Board` Trait Family](board-trait.md) and [Custom Board Package](custom-board.md) instead.

## Goal

Consume a nano-ros board crate from a downstream Zephyr application with a **single CMake call**. The consumer's build script SHOULD NOT carry any of:

- a hand-curated `EXTRA_CONF_FILE` listing the board crate's `prj.conf` and per-board Kconfig snippets
- a hand-curated `DTC_OVERLAY_FILE`
- a hardcoded `BOARD=<id>` string
- a hardcoded `-DNANO_ROS_RMW=<rmw>` define
- a hand-rolled FVP / qemu launch shell

All of those are owned by the board crate. The consumer's `CMakeLists.txt` declares which board it wants; nano-ros layers the rest in.

## Prereqs

Before importing a board crate, the consumer needs:

1. **A Zephyr 3.7+ workspace managed by west.** Zephyr 3.7 is the floor (the official in-tree `zephyr-lang-rust` module did not exist below it, so nothing earlier can link the nano-ros Rust staticlib); newer LTS lines work too.
2. **All gated SDK packages for the target board.** Run `nros doctor --board <name>` to check; missing items can be installed with `nros setup <board>` (or `nros setup --tool <t>` for a single dependency). See Phase 191 / `nros setup` in the [build commands reference](../reference/build-commands.md).
3. **nano-ros listed as a project in your `west.yml`**, and nano-ros's Zephyr module exported on `ZEPHYR_EXTRA_MODULES` so Zephyr picks up its Kconfig + DTS roots.

A minimal `west.yml` fragment:

```yaml
manifest:
  remotes:
    - name: newslab
      url-base: https://github.com/NEWSLabNTU
  projects:
    - name: nano-ros
      remote: newslab
      revision: main
      path: deps/nano-ros
      import: false
  self:
    west-commands: deps/nano-ros/scripts/west-commands.yml
```

And the matching environment:

```sh
export ZEPHYR_EXTRA_MODULES="$PWD/deps/nano-ros/zephyr"
```

## The one-call pattern

A consumer `CMakeLists.txt` should look like this and nothing more:

```cmake
cmake_minimum_required(VERSION 3.20)

# Single import call. Layers in BOARD, EXTRA_CONF_FILE, DTC_OVERLAY_FILE,
# NANO_ROS_RMW default, and the runner hint.
nano_ros_use_board(fvp-aemv8r-smp)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(my_app LANGUAGES CXX)

target_sources(app PRIVATE src/main.cpp)
```

**Call order matters.** `nano_ros_use_board()` MUST precede `find_package(Zephyr ...)`. Zephyr reads `BOARD`, `EXTRA_CONF_FILE`, and `DTC_OVERLAY_FILE` during its `find_package` call; setting them after that point has no effect. The pattern above is the canonical shape -- copy it verbatim and only fill in the board name.

`nano_ros_use_board()` is shipped by the nano-ros Zephyr module (Phase 215.B). It is available the moment `ZEPHYR_EXTRA_MODULES` includes `deps/nano-ros/zephyr`; no extra `include()` is required.

## What the call layers in

| Source | Effect |
| --- | --- |
| `BOARD` | Set to `NROS_BOARD_ZEPHYR_ID` (from the board crate's `board.cmake`) if the user did not pass `-DBOARD=...` on the command line. |
| `EXTRA_CONF_FILE` | The board crate's `prj.conf` and any per-board Kconfig fragments (e.g. an HWv2 snippet) are appended. Any consumer-supplied `EXTRA_CONF_FILE` is preserved and layered AFTER the board's. |
| `DTC_OVERLAY_FILE` | The board crate's per-board DTS overlay is appended. Consumer overlays are preserved and layered after. |
| `NANO_ROS_RMW` | Defaulted to `NROS_BOARD_DEFAULT_RMW` (from `board.cmake`) when the consumer did not pass `-DNANO_ROS_RMW=...`. |
| `NROS_BOARD_RUNNER` | Cached for `west fvp run` (or another runner extension command) to pick up the right simulator binary / target-launcher. |

The values themselves come from a single source of truth -- the board crate's `board.cmake` -- which Phase 215.F's drift audit keeps in sync with the crate's `Cargo.toml` metadata.

## Per-app overrides

The one-call pattern still gives the consumer escape hatches:

- **Pin a different Zephyr board id.** Pass `-DBOARD=<other>` on the CMake / `west build` command line; `nano_ros_use_board()` notices the user value and emits a warning rather than clobbering it.
- **Pick a different RMW.** Pass `-DNANO_ROS_RMW=<rmw>` (`zenoh`, `xrce`, or `cyclonedds` -- see [RMW backends](../internals/rmw-backends.md)). The board crate's default is used only when nothing is set.
- **Layer extra Kconfig / DTS.** Set `EXTRA_CONF_FILE` and `DTC_OVERLAY_FILE` *after* the call to `nano_ros_use_board()`. They will be applied on top of the board crate's contributions, not in place of them.

```cmake
nano_ros_use_board(fvp-aemv8r-smp)

# Extra app-specific Kconfig layered on top of the board's defaults.
list(APPEND EXTRA_CONF_FILE "${CMAKE_CURRENT_SOURCE_DIR}/boards/fvp-aemv8r-smp.conf")

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(my_app LANGUAGES CXX)
```

## Running

For boards whose runner is an FVP (the AEMv8-R archetype, Phase 214):

```sh
west build -d build
west fvp run -d build
```

`west fvp run` consults `NROS_BOARD_RUNNER` and the Phase 214.A resolver to locate the simulator binary, applies the board's launch arguments, wires UART to stdout, and exits cleanly on Ctrl-C.

For any other runner the Zephyr-native command works unchanged:

```sh
west build -d build -t run
```

There is no need for a hand-written `build.sh` or `run_fvp.sh` shell wrapper.

## Inspecting the manifest

To see exactly what `nano_ros_use_board()` will layer in for a given board:

```sh
nros board info fvp-aemv8r-smp
```

This prints the resolved Zephyr board id, the `prj.conf` + Kconfig fragment list, the DTS overlay, the default RMW, and the runner hint -- everything the call would set.

To audit that the `board.cmake` mirror matches the canonical `Cargo.toml` metadata:

```sh
nros board info fvp-aemv8r-smp --check-drift
```

This exits 0 when the two agree and emits a field-by-field diff (with a non-zero exit code) when they don't. The drift audit is the same one CI runs for every `packages/boards/nros-board-*` shipping a `board.cmake`.

## Anti-patterns

Things that LOOK reasonable but the one-call pattern obviates:

- **Don't hand-list the board's `prj.conf` in `EXTRA_CONF_FILE`.** It is already there; doing it again either duplicates or fights the layering order.
- **Don't hardcode `BOARD=<id>` in a `build.sh`.** The call sets it; hardcoding it short-circuits the `nros board info` inspection and the drift audit.
- **Don't carry your own copy of `boards/<id>.conf` or `boards/<id>.overlay` mirroring the board crate's.** Vendor a delta only -- the base ships in the crate.
- **Don't reimplement the FVP runner as a shell script.** `west fvp run` (Phase 214) covers `FVP_BaseR_AEMv8R` and the other supported simulators with the right CLI flags, the same way `west build` covers compilation.
- **Don't `include()` files from `deps/nano-ros/cmake/` directly.** The public surface is `nano_ros_use_board()`; anything else is internal and may move.

## Migrating an existing hand-glued consumer

Most downstream consumers (the ASI archetype is the canonical example) carry years of accumulated glue. The migration is mechanical:

1. **Find the per-board Kconfig and overlay entries in the current `CMakeLists.txt` / `build.sh`.** Anything listing the *board crate's* `prj.conf`, board overlay, or HWv2 snippet -- delete it. Keep only entries that point at the *consumer's* own deltas.
2. **Find any hardcoded `BOARD=<id>` string** (in CMake `set(BOARD ...)`, in `west build -b <id>`, or in `build.sh`). Delete it. `nano_ros_use_board()` will set it from `board.cmake`.
3. **Replace `find_package(Zephyr) + ... + manual FVP launch` with the canonical pattern** above -- one `nano_ros_use_board()` call, then `find_package(Zephyr)`, then `project()`, then `target_sources(app ...)`. Replace the FVP shell script call with `west fvp run -d build`.
4. **Keep only app-specific deltas in `boards/<id>.conf` / `boards/<id>.overlay`.** For the ASI archetype this typically means Autoware-msg sizing (large topic / participant memory) and the application's own GPIO map -- everything generic moves into the board crate.

After migration, the consumer's CMake should be roughly twenty lines, with `nano_ros_use_board()` as the only nano-ros-specific call.

## Cross-references

- [The `Board` Trait Family](board-trait.md) -- for *implementers* of a new board crate (Phase 212.N.8). This chapter is the consumer-side dual.
- [Custom Board Package](custom-board.md) -- the full board-crate authoring guide.
- [Vendor Overlay Board Crate](vendor-overlay.md) -- the lighter "I just want to override one field" path.
- [Build commands reference](../reference/build-commands.md) -- `nros setup`, `nros doctor`, `nros board info` (Phase 191 SDK provisioning, Phase 215.G inspector).
- [RMW backends](../internals/rmw-backends.md) -- the menu of values for `-DNANO_ROS_RMW=...`.
