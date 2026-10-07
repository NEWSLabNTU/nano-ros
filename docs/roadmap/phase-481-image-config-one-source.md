# Phase 481 -- image configuration has one source: `system.toml` on every road

**Status (2026-10-07). PLANNED -- nothing implemented.** Implements
[RFC-0098](../design/0098-generated-leaf-build-config.md) D10-D12 (and the
pointer it adds to [RFC-0049](../design/0049-hierarchical-platform-board-config.md)).
Closes issue [1721](../issues/1721-image-env-west-and-workspace-cmake-roads.md).

**Prior:** issue 1712 (archived -- a standalone C/C++ leaf's `[image.<id>] env`
reaches its cargo builds through an unforced `--config` `[env]` file; that
carrier is reused here), phase-445 / RFC-0098 D1-D9 (a leaf states its board
once and its build settings are generated), phase-479 W5 and issue 1037 (the
`[knobs.log]` tenant -- the knobs most likely to differ per image).

## Why

An image's configuration is stated in TWO places on the Zephyr road and in
ZERO usable places on the workspace cmake road.

- **Zephyr.** `system.toml` carries `[image.<id>] env` and `rmw`; the leaf's
  conf files carry the same facts as `CONFIG_NROS_*`. Measured on `main`
  2026-10-07: 135 conf files across 62 example dirs hold 299 `CONFIG_NROS_*`
  lines, in four kinds:

  | kind | symbols (lines) | home |
  |---|---|---|
  | RMW choice | `RMW_ZENOH` 31, `RMW_CYCLONEDDS` 21, `RMW_XRCE` 19 | derived from the image's `rmw` |
  | language API | `CPP_API` 21, `C_API` 15, `RUST_API` 14 | derived from the package |
  | deploy endpoint | `XRCE_AGENT_ADDR` / `_PORT` 19 each | `[image.<id>]` deploy keys |
  | knobs | `XRCE_MAX_*` 18x3, `ZEPHYR_HEAP_SIZE` 5, `ZENOH_SOCKET_TIMEOUT_MS` 5, `MAX_QUERYABLES`, `EXECUTOR_MAX_CBS`, `ZENOH_READ_PRIORITY` 1 each | `[image.<id>] env`, or `[board.knobs]` for a hardware budget; some pools are derivable (RFC-0100) |

  The two sources already disagree: `examples/zephyr/c/talker/system.toml`
  says `rmw = "zenoh"` while the same leaf builds XRCE and Cyclone variants
  from `prj-xrce.conf` / `prj-cyclonedds.conf`. And the image's `env` reaches
  nothing on this road (issue 1721).
- **Workspace cmake.** One configure per coordinate builds `nros-c` /
  `nros-cpp` once for every image on it, so a per-image knob has no build to
  attach to and is dropped with a warning.

## Decisions (from RFC-0098)

- **D10** -- `system.toml` is the single source of an image's knobs, RMW and
  deploy endpoint; each road renders them into its native form.
- **D11** -- on Zephyr that form is a generated Kconfig fragment appended to
  `EXTRA_CONF_FILE` by a nano-ros helper the leaf's `CMakeLists.txt` calls
  before `find_package(Zephyr)`; a gate keeps nano-ros knob symbols out of leaf
  conf files; knobs with no Kconfig symbol ride the 1712 `--config` file.
- **D12** -- a workspace image whose resolved configuration differs gets its
  own configure (`build/<coord>-<hash>/`); identical configurations still share
  `build/<coord>/`.

## Work items

### W0 -- measure the Zephyr carrier before building it

On one Zephyr C leaf, one Zephyr Rust leaf and one workspace Zephyr image,
hand-place a fragment and a pre-`find_package(Zephyr)` helper stub, and
measure:

- the fragment's value lands in `.config` and in the cargo build script's
  `$DOTCONFIG` read (C/C++ lane and the zephyr-lang-rust lane);
- a leaf `prj.conf` value for the same symbol is overridden (it merges
  earlier), and a `-DCONFIG_*` command-line value still wins (it merges later);
- an exported environment variable still wins in `_nros_resolve_knob`;
- editing `system.toml` re-runs Kconfig on an incremental `west build`
  (configure dependency), with no `rm -rf`;
- the helper can locate nano-ros before Zephyr has loaded the module
  (`NROS_REPO_DIR` / the installed path) on the fixture road AND a user's plain
  `west build`.

**Acceptance:** each bullet answered with a measured build, written into this
doc. If the helper cannot locate nano-ros pre-`find_package(Zephyr)` on some
road, D11's mechanism is revised in the RFC before W1 starts.

### W1 -- render the image into each road's form

- `nros ws leaf-system --kconfig-out <path>` writes the Zephyr fragment from
  the image: `[image.<id>] env` rows that have a Kconfig counterpart (through
  `nros_zephyr_build::KCONFIG_PAIRS`, never a second table), the RMW choice
  from the image's `rmw`, the language API from the package, and the deploy
  endpoint. Write-if-changed; deleted when the image states nothing.
- Rows with no Kconfig symbol go to the 1712 `--config` file, which the Zephyr
  cargo commands (`nros_cargo_build.cmake` and `rust_cargo_application`) gain.
- The helper (`cmake/zephyr/NanoRosLeafConfig.cmake` or similar) renders,
  appends to `EXTRA_CONF_FILE` preserving any user value, and registers
  `system.toml` as a configure dependency.
- An image is SELECTED by `-DNROS_IMAGE=<id>` when a leaf has several (e.g.
  one per RMW); one image needs no selection.

**Acceptance:** unit tests for the renderer (each row kind, the empty case, a
row with no Kconfig symbol); W0's measurements repeated through the real
helper.

### W2 -- the gate

`check-leaf-conf-nros-knobs`: a tracked conf file under `examples/` (leaves,
workspace images, templates) may not assign a nano-ros knob symbol -- the RMW
choice, the language API, the deploy endpoint, or any symbol in
`KCONFIG_PAIRS`. The refusal names the `system.toml` line to write. The shared
board fragments under `cmake/zephyr/` are the BOARD layer and out of scope;
the per-run `-DCONFIG_*` values the fixture scripts pass are command-line,
not files, and out of scope. Negative control in its self-test.

**Acceptance:** the gate fails on today's tree with the 299-line inventory
above (so it is ratcheted or lands with W3), and passes after W3.

### W3 -- migrate the Zephyr examples

- The 18 single-package Zephyr leaves (`examples/zephyr/{c,cpp,rust}/*`), the
  workspace Zephyr images (`examples/workspaces/*/src/demo_bringup/boards/*`)
  and `examples/templates/zephyr-byo` move their nano-ros symbols into
  `system.toml`. A leaf built for several RMWs states one image per RMW
  (`[image.zephyr_zenoh]`, `[image.zephyr_xrce]`, ...) instead of a
  `prj-<rmw>.conf` per RMW; Zephyr-native lines an RMW needs (`CONFIG_NET_TCP`
  for XRCE) stay in a conf file the helper selects from the image's `rmw`.
- The fixture drivers (`zephyr-fixture-leaves.sh`, `zephyr-fixture-run-one.sh`,
  `just zephyr`'s `build-one`) select an image instead of assembling a
  per-RMW `CONF_FILE`; the fixture rows and `matrix::CELLS` are unchanged in
  coordinate.
- Fix the eight Zephyr C/C++ leaves whose `[[component]] name` is the literal
  string `"${NROS_CYCLONE_IDLC}"` (a phase-445 W3b substitution slip in
  `examples/zephyr/{c,cpp}/{talker,listener,service-client,service-server}/system.toml`).
- Pool sizes the declarations already answer (`XRCE_MAX_*`, `MAX_QUERYABLES`)
  are DELETED rather than moved where the derivation covers them (RFC-0100);
  a row moves only when it is a real per-image choice.

**Acceptance:** W2's gate passes; every migrated Zephyr fixture builds and its
existing runtime test passes on native_sim (and the mps2 rows that run in the
box); `build/**/.config` of one migrated leaf per kind shows the same nano-ros
symbol values as before the migration (a diff, measured).

### W4 -- per-image workspace configures

- `cmake_coordinate` (`packages/cli/nros-cli-core/src/cmd/build.rs`) gains a
  suffix: a short hash of the image's resolved configuration (its `env` rows
  and deploy keys) when non-empty. Every reader of a workspace image's build
  dir resolves it through that one function (issue 1582's one spelling).
- The image's rows reach its configure: the 1712 `--config` file for the cmake
  road, the W1 fragment for a Zephyr workspace image. The warning issue 1712
  added for workspace members is removed.
- `generated_output_collisions` and the fixture locators follow the new path.

**Acceptance:** a workspace with two images on one coordinate -- one stating
`NROS_LOG_MAX_LEVEL = "warn"`, one stating nothing -- builds two configures,
and each binary reports its own ceiling (a test, like
`image_env_cmake_knob.rs`); a workspace whose images state nothing produces
exactly today's `build/<coord>/` paths (the book's paths are unchanged).

### W5 -- docs and close

- Book: the Zephyr getting-started and configuration pages show knobs in
  `system.toml`, not `prj.conf`; the env-var / knob reference names the Zephyr
  rendering; the workspace page explains per-image configures and their cost.
- CLAUDE.md: the Zephyr conf-merge pitfall line (per-leaf `boards/*.conf` are
  never merged; LAST-WINS) gains the rule that nano-ros knobs are not authored
  in conf files at all.
- RFC-0098 status; issue 1721 resolved and archived.

**Acceptance:** `just ci gate` green; tier 2 (`just ci matrix`) green for the
Zephyr cells; issue 1721 archived.

## Not in this phase

- The shared board fragments under `cmake/zephyr/` (`mps2-an385.conf`,
  `native-sim-line-*.conf`, `qemu-cortex-m3.conf`) -- board facts that belong
  in the board descriptors under RFC-0049; moving them is its own change.
- NuttX and ESP-IDF Kconfig front-ends -- same D10 rule, no measured consumer
  today.
