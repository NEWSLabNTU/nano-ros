---
id: 1712
title: "A C/C++ leaf's `[image.<id>] env` never reaches cargo on the cmake road — the APP rung exists only for Rust leaves"
status: resolved
type: bug
area: [build, config, cmake]
severity: medium
found: 2026-10-06
related: [1037, 1721, rfc-0049, rfc-0098, phase-445, phase-479, issue-0460, issue-0616]
---

## Summary

RFC-0049's APP rung — `[image.<id>] env` in a leaf's `system.toml` — is
delivered on the cargo road only. `nros sync` / `nros build` render it into
`<leaf>/build/<image>/nros-cargo.toml` (`cmd::leaf_settings`, layer 5), and
`nros ws leaf-system` reports that file as `NROS_LEAF_SETTINGS`, "empty for a
leaf that road does not build (a C/C++ leaf, a Zephyr one)". On the cmake road
the cargo builds of `nros-c` / `nros-cpp` receive the BOARD facts
(`cmake -E env NROS_BOARD=… NROS_BOARD_TOML=… NROS_PLATFORM_NAME=…` in
`build.ninja`) and nothing from the image's `env`.

phase-479 W5's commit says the opposite ("Cargo and cmake need no new carrier:
image env and board facts already reach build scripts through `nros-cargo.toml`
`[env]` / `corrosion_set_env_vars`"). The board half is true; the image half is
not.

## Measured (2026-10-06, issue 1037's sweep)

`examples/native/c/talker` copied out to a scratch dir, its `[image.native]`
given `env = { NROS_LOG_MAX_LEVEL = "warn", NROS_LOG_BUFFER_SIZE = "512" }`,
configured with the fixture lane's own arguments and built:

- both `nros-log` build-script outputs: `MAX_LEVEL = 0`, `BUFFER_SIZE = 256` —
  the builtins. The image's statement was dropped.
- `DYNAMIC_LOGGER_CAPACITY = 32` — the `native` board's `[board.knobs.log]`,
  so the board rung DOES arrive.
- the same build with `NROS_LOG_MAX_LEVEL=warn` exported in the shell running
  `cmake --build`: `MAX_LEVEL = 3`. The env rung works; only the image file is
  not read.

So on the cmake road an image can override its board only through the shell
that happens to run the build, which is not a declaration anyone can commit.

## Scope

Every knob a C/C++ image might state per image — not only the logging tenant.
Rust leaves (cargo road) and the workspace roads are unaffected as far as
measured.

## Fix direction

`NanoRosBoardFacts.cmake` already attaches the board facts with
`corrosion_set_env_vars`; the image's `env` rows are one more source for the
same call (`nros ws leaf-system` could print them, or the leaf-settings file
could be read), applied BELOW an exported variable so the ladder's top rung
still wins.

## Resolution (2026-10-06)

**The carrier is a cargo config, not `corrosion_set_env_vars`.** The fix
direction above named that call; measuring it ruled it out. It renders
`cmake -E env K=V cargo …`, and `cmake -E env` overwrites an inherited value
(`K=shell cmake -E env K=cmake sh -c 'echo $K'` prints `cmake`), so an image
row there would outrank the exported variable — the top two rungs inverted.
The cargo road's own mechanism has the right precedence by construction.
Measured with cargo 1.99 on a probe crate whose build script declares
`rerun-if-env-changed`: an `[env]` row in a `--config` file reaches the build
script; an exported value wins over it; and editing or deleting the row re-runs
the build script on the next build.

**What changed.**

- `nros ws leaf-system --image-env-out <path>` writes the image's own layers —
  what `transport` implies, then `[image.<id>] env` — as a one-table cargo
  config (`[env]`, no `force`; write-if-changed; removed when the image states
  nothing) and prints it as `NROS_LEAF_IMAGE_ENV`. The layers come from
  `leaf_settings::image_layers`, the composition the cargo road applies.
- `nano_ros_read_leaf_system` asks for it into `<build>/nros/` and records it
  (GLOBAL `NROS_IMAGE_ENV_CONFIG`) for the TOP-LEVEL leaf of a non-Zephyr
  configure. A Zephyr leaf, or a package inside a larger configure, gets a
  WARNING naming issue 1721 instead of a silent drop.
- `cmake/NanoRosImageEnv.cmake` carries it: `--config=<file>` through
  `corrosion_set_cargo_flags` inside `nros_board_facts_env`, the helper
  `check-board-facts-delivery` already requires of every Corrosion import
  (nros-c, nros-cpp, the zenoh staticlib, the workspace runtime crate, the
  rv-virt ThreadX board); on the two own-command lanes (NuttX FFI driver,
  message FFI glue) as a flag plus a DEPENDS on the file, since those are real
  OUTPUT edges. The gate now also fails an own-command lane that does not call
  `nros_image_env_cargo_flags()` (the two Zephyr lanes are exempt, issue 1721);
  mutation-checked on the NuttX lane.
- The rows join the shared-cargo-directory key (`nros_knob_key_fields` road 3,
  content hash), issue 0616's rule: two images differing only there must not
  share an uplifted archive. No field — no key change — for an image that
  states nothing, and no C/C++ leaf in the tree stated any.

**Measured** on the new fixture `bins/image-env-probe-c` (row
`image-env-probe-c`, `[image.native] env = { NROS_EXECUTOR_MAX_CBS = "13",
NROS_LOG_MAX_LEVEL = "warn" }`), built by `fixtures-build.sh linux c zenoh`,
then driven by plain `cmake --build build-zenoh`:

| step | binary reports |
| --- | --- |
| built | `max_handles=13 info_enabled=0 warn_enabled=1` — both rows reached nros-node/nros-c and nros-log |
| rebuild, nothing changed | same; no crate recompiled |
| `NROS_LOG_MAX_LEVEL=error NROS_EXECUTOR_MAX_CBS=7` exported | `max_handles=7 … warn_enabled=0` — env outranks the image |
| unexported again | back to `13 / 0 / 1` |
| `system.toml` edited to `11` / `info` | `Re-running CMake`, nros-log/-node/-c rebuilt, `max_handles=11 info_enabled=1` |
| `env` removed | `max_handles=4 info_enabled=1` (the builtins — the pre-fix result), no `--config` left in `build.ninja` |
| restored | `13 / 0 / 1` |

`ninja -t query build.ninja` lists `../system.toml` among the configure's
inputs, which is the whole ninja-side edge: Corrosion's cargo targets always
run, and cargo's own `[env]` fingerprint decides what re-runs. No step needed
a wipe. Regression test: `tests/image_env_cmake_knob.rs`.

**Not fixed here — issue 1721.** The Zephyr west road builds its cargo
commands while Zephyr loads its modules, before the application's
`find_package(nano_ros)` reads `system.toml`, and resolves knobs through its
own `_nros_resolve_knob` ladder (env > Kconfig > derived > builtin) baked into
`cmake -E env`; the workspace cmake road (`nros build`, `build/<coord>/`)
serves every image on a coordinate from one configure. Both need a design
decision, not a carrier.

