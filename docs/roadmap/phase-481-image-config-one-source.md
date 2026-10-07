# Phase 481 -- image configuration has one source: `system.toml` on every road

**Status (2026-10-07). W0 DONE, PAUSED before W1 -- W0 measured D11's leaf
helper unable to locate nano-ros on a plain `west build` (the book's road); a
revised mechanism (a Zephyr `module_ext_root` hook, measured working on both
roads) is proposed in RFC-0098 D11 and needs a decision before W1-W2.** Implements
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

#### W0 results (measured 2026-10-07, Zephyr 3.7, native_sim/native/64, distrobox `ros2`)

**Verdict: the fragment works; the LOCATOR does not.** Every Kconfig bullet
holds on all three targets, but a helper the leaf's `CMakeLists.txt` includes
before `find_package(Zephyr)` cannot find nano-ros on the book's own road (a
plain `west build` with nano-ros as a west project). Per this item's
acceptance, W1 did not start; the RFC carries a proposed revision (D11, "W0
revision") and this phase is paused on it.

Setup. The probe knob is `CONFIG_NROS_EXECUTOR_MAX_CBS` (`int`, default `-1` =
derive; the leaves state none, so a base build has `-1` and no cargo env row).
The stub rendered `CONFIG_…=<v>` from a marker comment in `system.toml` into
`<build>/nros/…image.conf`, appended it to `EXTRA_CONF_FILE`, and put
`system.toml` on `CMAKE_CONFIGURE_DEPENDS`. "Cargo read" is the value nros-node's
build script wrote to `OUT_DIR/nros_node_config.rs` (`pub const MAX_CBS`) —
reached by `cmake -E env NROS_EXECUTOR_MAX_CBS=…` on the C/C++ lane and by
`nros_zephyr_build::knob()`'s `$DOTCONFIG` read on the zephyr-lang-rust lane.
Targets: `examples/zephyr/c/talker` and `examples/zephyr/rust/talker` (zenoh,
the `just zephyr build-one` west line), and `examples/workspaces/c`
`demo_bringup:zephyr` (`build-ws-c-entry-zenoh`, the stub hand-inserted into the
GENERATED `build/zephyr-zenoh/zephyr_entry/CMakeLists.txt`). All edits were
reverted afterwards; nothing here is committed.

| bullet | C leaf | Rust leaf | workspace image |
|---|---|---|---|
| fragment `13` lands in `.config` and the cargo read | `.config` 13; `build.ninja` `NROS_EXECUTOR_MAX_CBS=13` (source `kconfig`); `MAX_CBS = 13` | `.config` 13; `MAX_CBS = 13` (`$DOTCONFIG`) | `.config` 13; env 13; `MAX_CBS = 13` |
| leaf `prj.conf` says `9` | 13 — fragment merges after `CONF_FILE` | 13 | 13 (bringup `prj.conf` 9) |
| `-DCONFIG_NROS_EXECUTOR_MAX_CBS=17` | 17 | 17 | 17 |
| exported `NROS_EXECUTOR_MAX_CBS=21` | `.config` 13, env 21 (`environment wins`), `MAX_CBS = 21` | exported at `ninja` time: `MAX_CBS = 21`; unexported: back to 13 | `.config` 13, env 21 |
| `system.toml` edited, plain `ninja`, no wipe | `Re-running CMake` once; `.config` 11; `MAX_CBS = 11`; second `ninja` a no-op | same, 13 → 11 | the bringup's model goes STALE first (`nros model-path: … input hash changed system.toml — run nros sync`) — this road's driver is `nros build`, which syncs; after `nros sync`, `ninja` re-ran cmake and gave 13 |
| helper locates nano-ros pre-`find_package(Zephyr)` | fixture road: yes (`$ENV{NROS_REPO_DIR}`, `-DZEPHYR_EXTRA_MODULES` both set) | fixture road: yes | yes — the generated application names absolute paths, the generator knows the root |
| … on a plain `west build`, nano-ros a west project | **NO** (below) | **NO** | n/a (always generated) |

**The failing road, measured.** A scratch west topdir (`zephyr/` hard-linked
from the 3.7 workspace, the modules symlinked) whose manifest lists nano-ros as
a PROJECT at `modules/nano-ros`; `west list` and `zephyr_module.py` both report
`"nros":"…/modules/nano-ros"`. With `NROS_REPO_DIR` unset and no
`-DZEPHYR_EXTRA_MODULES` — the book's `integration-zephyr.md` flow — the C leaf
configured with the stub fails at the include:

```
CMake Error at CMakeLists.txt:12 (include):
  include could not find requested file:
    /tmp/w0/stub/nros_leaf_stub.cmake
```

and the rest of the same configure succeeds, so the road itself works. What the
leaf can see before `find_package(Zephyr)`: west's cmake line is
`-DWEST_PYTHON=… -B… -GNinja -DBOARD=… -DCONF_FILE=… -S…` — no module path —
and module discovery runs INSIDE `find_package(Zephyr)` (`zephyr_module.cmake`).
The third locator does not rescue it: `nros sdk-root` falls back to "this
toolchain's own `share/nano-ros`", which in a released `nros` is a SECOND copy of
nano-ros, not the west project the module is built from — issue 1258/1387's
two-checkouts class (cmake from one tree, module from another); the dev CLI in
the box has none and errors.

**Two alternatives, measured on the same road.**

- **Partial load** — `find_package(Zephyr COMPONENTS zephyr_default:zephyr_module)`,
  include from `${ZEPHYR_NROS_MODULE_DIR}`, then the full `find_package(Zephyr)`.
  The include works (`ZEPHYR_NROS_MODULE_DIR=…/modules/nano-ros`), but the second
  `find_package` prints "Loading Zephyr default modules (Zephyr base (cached))"
  and loads nothing more: no Kconfig, no `.config`, and the configure dies in
  `nros_feature_set: unknown PLATFORM 'zephyr'`. **Fails.**
- **A Zephyr `module_ext_root` hook** — `zephyr/module.yml` `settings:
  module_ext_root: <dir>`, whose `<dir>/modules/modules.cmake` Zephyr includes
  from `zephyr_module.cmake` after module discovery and BEFORE
  `configuration_files`/`kconfig`, in the application's own directory scope.
  The stub there (keyed on `APPLICATION_SOURCE_DIR/system.toml`, no-op without
  one) on the west-project road: C leaf `.config` 11 over `prj.conf` 9, env 11,
  `zephyr.exe` built; `system.toml` 11 → 15 and plain `ninja`: one `Re-running
  CMake`, 15; `-DCONFIG_…=17` → 17; exported 21 → env 21, `.config` 15. Rust leaf:
  `.config` 11, `MAX_CBS = 11`, `zephyr.exe` built. On the fixture road (module
  via `-DZEPHYR_EXTRA_MODULES`) the same hook gave 15 on the C leaf. The hook
  printed its own path as `…/modules/nano-ros/…` on one road and the checkout on
  the other: it is always the tree the module is. **Works, with no leaf edit.**

**Three corrections to D11's merge-order sentence**, all from Zephyr 3.7's
`kconfig.cmake` and the runs above:

1. A `-DCONFIG_*` value is STICKY: Zephyr stores it as `CLI_CONFIG_*` in the
   cache and re-applies it on every later configure until `-U`'d (measured: a
   `-D…=17` from a failed configure still decided the next one).
2. `*.conf` files dropped in the build dir's TOP level merge LAST, after
   `-DCONFIG_*` (`file(GLOB ${APPLICATION_BINARY_DIR}/*.conf)`); the rendered
   fragment must therefore live in a SUBDIRECTORY (`nros/`), as the stub's did.
3. When `EXTRA_CONF_FILE` is ALSO a cache value from the command line (the
   workspace road: `nros build` passes `-DEXTRA_CONF_FILE=<image conf>`), a
   value appended to the local variable merges BEFORE it (`zephyr_get(… MERGE
   REVERSE)`): measured order `prj.conf`, board conf, the fragment, then
   `prj-zenoh.conf`. So on that road an `[image] conf` file can override the
   rendered fragment; the generated application must place the fragment itself
   (it already writes pre-Zephyr lines) or `nros build` must order it last.

### W1 -- render the image into each road's form

> **Blocked on the D11 revision (W0).** The helper bullet below names the
> mechanism W0 measured failing on a plain `west build`; under the proposed
> revision the same rendering is reached from the module's `module_ext_root`
> hook, with no line in the leaf. The renderer, the `--config` carrier and the
> image selection are unaffected.

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
  suffix: a short hash of the image's resolved configuration when non-empty.
  (Deploy keys are NOT in it: they reach each image's own generated entry,
  never the shared runtime builds -- corrected while implementing.) Every reader of a workspace image's build
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

**LANDED (2026-10-07).** What was built, and measured:

- `cmake_coordinate` appends `-cfg<10 hex>` over
  `leaf_settings::image_block_layers` (transport implication, then
  `[image.<id>] env`) -- the same function the cargo workspace road now uses,
  which deleted that road's second hand-written copy of the transport rule.
  An image stating nothing keeps its exact path.
- `nros build`'s CMake arm writes `<root>/nros-image-env.toml` with
  issue 1712's writer, and the generated root sets `NROS_IMAGE_ENV_CONFIG`
  before `find_package(nano_ros)`, so every Corrosion import and own-command
  lane gets `--config` (1712's carrier, unchanged).
- `ResolvedBuild::cmake_build_dir` exposes the derivation; `nros image-facts`
  prints it (`cmake_build_dir=` / `NROS_IMAGE_CMAKE_BUILD_DIR`).
- The fixture manifest's `build_subdir` for every GENERATED workspace row is
  now CHECKED against `cmake_coordinate` (CLI unit test
  `every_generated_workspace_fixture_row_names_its_cmake_build_dir`;
  negative control: a wrong row fails naming both paths), so a `-cfg<hash>`
  literal is verified rather than trusted.
- Measured on `fixtures/image_config_ws` (two native images, one coordinate),
  built through `workspace-fixtures-build.sh`: `plain` ->
  `build/posix-zenoh-native/`, `max_handles=4 info_enabled=1`; `warn` ->
  `build/posix-zenoh-native-cfga50aa93c83/`, `max_handles=13 info_enabled=0
  warn_enabled=1` (test `image_config_workspace.rs`). On a copy of
  `examples/workspaces/c`: an exported `NROS_LOG_MAX_LEVEL=error` beats the
  image's `warn` (nros-log `MAX_LEVEL` 4 vs 3), unsetting it returns to 3, and
  an unchanged rebuild compiles nothing.
- Not done here: a Zephyr workspace image (W1's fragment), and the issue-1712
  warning for a non-top-level package's own `system.toml` stays (such a
  package states nothing a workspace image owns).

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
