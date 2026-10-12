# Phase 484 -- one owner per fact: resource locations and configuration

**Status (2026-10-12).** W0, W1, W3, W4 (but W4b-2), W6 and W7a/W7b landed; W2
has four leftovers; W5a in progress (slices 1-6 and `[board.knobs]` →
`[knobs]` done); W5b (folded in from phase-481) measured and built — M1 red
so choices/endpoint/domain stay in the fragment; W8 waits on a release. Implements
[RFC-0103](../design/0103-one-owner-per-fact.md). Tracks issues
[1767](../issues/archived/1767-resource-location-resolved-many-ways.md) and
[1768](../issues/archived/1768-build-wiring-census-misses-link-script-macro.md).

**Not this phase:** the Zephyr leaf `.conf` migration, the image Kconfig
fragment renderer, per-image workspace configures and issues 1721/1757 —
phase-481 built and archived them (2026-10-11). W5b, which RFC-0103 D2b first
gave 481, came back here when 481 closed without it (see W5).

## Why

Two censuses (2026-10-09/10, RFC-0103 §1): 13 mechanisms locate a resource and
13 resources are located two or more ways; ~30 configuration places state the
same fact in up to 10 of them, and one knob carries up to three names. Every
arm is correct under the configuration its author tested, so every gate is
green; they diverge exactly where a second checkout, an override or an
installed user differs.

## Work items

Each wave deletes what it replaces in the same change (issue 0196: a rule
whose old spelling survives is two rules).

### W0 -- measure before building

1. **Zephyr enforcement (RFC-0103 §6 Q1).** Can the generated image
   configuration make a leaf `.conf` assignment of a nano-ros knob a Kconfig
   ERROR rather than a gate finding? Measure on Zephyr 3.7 and 4.4: a
   promptless symbol assigned in `prj.conf`; a generated Kconfig file sourced
   before the module's definitions supplying the default; `-DCONFIG_*` on a
   promptless symbol; a conditional prompt (`prompt "…" if !NROS_IMAGE_MANAGED`)
   for projects without `system.toml`.
2. **Store growth (§6 Q2).** Size of each `[source.*]` tree, pin churn over
   six months, what RFC-0095 D11's reclaim covers, and the cost of a second
   pin under a shared object store.
3. **Index location outside a checkout (§6 Q3).** How a user project reaches
   nano-ros crates today and which index-location option works for plain
   `cargo build`, a checkout and a linked worktree.

**Acceptance:** each answer is written into RFC-0103 with the commands that
produced it, and W3/W5's design is revised to match.

#### W0 results (2026-10-10)

1. **Zephyr enforcement — YES, by Kconfig itself, if the module defines its
   knob symbols with NO prompt** (RFC-0103 D2a). Zephyr 3.7, native_sim, a
   scratch module using the same `module_ext_root` hook as phase-481, 15
   configure-only probes. A promptless symbol assigned in `prj.conf` or via
   `-DCONFIG_*` fails configure (`is not directly user-configurable`); a
   CONDITIONAL prompt only warns and silently keeps the generated value; a
   prompt the hook ADDS when there is no `system.toml` restores today's
   behaviour for guest projects. Values ride as Kconfig `default`s from a
   generated file sourced first, not as a conf fragment. Zephyr 4.4
   (native_sim, host toolchain): identical — guest `prj.conf` 9 / `-DCONFIG` 17
   honoured, managed 13, managed + `prj.conf` or `-DCONFIG` a configure error;
   a reconfigure without a wipe follows the generated file (13 → 11 → 5).
   Probe scripts: a scratch `app/` + module with `module_ext_root`, not
   committed (reproduce from D2a's description).
2. **Store growth.** ~2.26 G of source trees; 113 distinct pins in six months,
   86 of them zenoh-pico and cyclonedds; ~0.4–0.5 G/month at one tree per pin,
   9× less with `git archive` + hardlinking unchanged files (38 zenoh-pico
   pins: 24.6 M vs ~228 M). `nuttx-kernel`, `nuttx-apps` and `px4-autopilot`
   build in-tree, so they are not store-eligible. Reclaim needs
   `Category::Sources` + `PinRule::Source`; today `sources/` is `Other` and
   never collected.
3. **Index location.** Not a gap: every SDK root (checkout or staged install)
   carries the index at its top and every nros crate compiles from inside
   one, so the existing `CARGO_MANIFEST_DIR` walk stays.

Two corrections to the RFC's first draft came out of W0: the store category is
the existing `sources/` (`location = "store"`, already used by rosidl), not a
new `src/`; and the index carries no sha for submodule rows, so every
`[source.*]` row gains `ref` (W3).

### W1 -- roots and the resolver

- `NROS_HOME` is the only store variable: `NROS_STORE` and `NROS_SDK_STORE`
  readers move to it (CLI `store_root`, `activate.sh`,
  `NanoRosCrossToolchain.cmake`, `NanoRosCorrosion.cmake`,
  `nros_build_paths::riscv64::sdk_store`, `riscv64-toolchain.sh`); a set
  retired variable is an error naming its replacement.
- `NROS_REPO_DIR` is the only root variable and `nros-sdk-index.toml` the only
  marker: `_nros_resolve_root`, `zephyr/CMakeLists.txt`,
  `cmake-incremental.sh`, `CHECKOUT_MARKER` and the lowercase
  `nano_ros_ROOT` export converge.
- Index resource rows gain `env` and `kind` (`source` / `tool` / `external` /
  `user-source`).
- `nros_build_paths::locate(resource)` implements RFC-0103 D3's ladder over
  today's rungs (store, checkout); `nros locate <resource> [--why]
  [--format cmake|sh|json]` wraps it; `nros locate artifact <kind> …` for D7's
  path functions.

**Acceptance:** `nros locate --why` for every index row on a checkout, a linked
worktree and a store-only host prints the rung that answered; a unit test per
rung; a retired variable set fails with its replacement named.

**LANDED (2026-10-10), in three PRs.**

- **Store root** (#1854). `NROS_HOME` only; `NROS_STORE` / `NROS_SDK_STORE`
  refused with the replacement named, in the launcher, the CLI, every build
  script (`nros_build_paths::store`), cmake (`nros_store_root()`,
  `cmake/NanoRosStoreRoot.cmake`) and shell (`scripts/lib/store-root.sh`).
  Five readers that ignored `NROS_HOME` fixed on the way. Gate
  `check-retired-store-vars`.
- **Repo root** (#1859). One marker, `nros_build_paths::CHECKOUT_MARKER`
  (`packages/core/nros-core/Cargo.toml`) — corrected from the RFC's first
  draft, which named `nros-sdk-index.toml`: ~25 sites already used the
  Cargo marker and only two walks the index, and both files sit at every SDK
  root, so the cheaper convergence was the right one. The launcher aliases the
  constant; cmake, the CLI's two private walks and `cmake-incremental.sh`
  use it. One environment name, `NROS_REPO_DIR`; `$ENV{NANO_ROS_ROOT}` is
  refused (`-DNANO_ROS_ROOT` stays as the explicit argument), and cmake's
  order now matches the CLI's (enclosing checkout before the environment).
- **The resolver.** `[source.*]` rows gain `env`; `kind` is NOT a column —
  the table (`[source.*]` / `[tool.*]`) already says it, and a second
  statement of it would be a second home. `nros_build_paths::locate` is the
  one ladder (rows read by a dependency-free reader in build scripts and by the
  CLI's serde model); `nros locate <name>… [--all] [--why] [--format
  path|sh|cmake]` wraps it, plus the `store` / `nano-ros` roots. A build
  script's `locate::source(name)` watches the answer's content, never the
  variable (issue 0491). `nros locate artifact` moves to W6 with the layout it
  names.

Deferred to the wave that owns them: the lowercase `nano_ros_ROOT` export
(W2 — it is an export), `NROS_BIN` as a second name for the CLI in two cmake
files (W7).

### W2 -- tooling stops exporting locations

- The 21 path exports in `just/sdk-env.just` and the path exports in
  `activate.sh` are deleted; `activate.sh` puts `nros` on PATH only.
- cmake and west configure call `nros locate --format cmake` once into
  `<build>/nros/locations.cmake` (normal variables, never `CACHE`), with the
  index and each answer's freshness input on `CMAKE_CONFIGURE_DEPENDS`. The
  cache→ENV→default pattern (11 files) and the Zephyr literal Cyclone path go.
- `just` recipes and `scripts/` call `nros locate` (the literal
  `packages/cli/target/release/nros`, `${NUTTX_DIR:-…}` re-defaults, literal
  zenoh-pico paths, `build/qemu`).
- Gate: no unrouted location (RFC-0103 D9) on every road.

**Acceptance:** `env -i HOME=$HOME PATH=… just ci gate` green in a fresh linked
worktree with no exported SDK variable; `check-inherited-checkout-paths` has
nothing left to re-root on the shell side; the gate's negative control
catches a re-added literal.

**Progress (2026-10-10).** Split by what kind of variable each export is:

- **W2a — first-party dirs (#1861).** The eight dirs a build script compiles
  from THIS checkout (`NROS_PLATFORM_*`, `NROS_LAN9118_LWIP_DIR`,
  `NROS_VIRTIO_NET_NETX_DIR`, `NROS_{C,CPP}_INCLUDE`) are no longer exported;
  every reader already defaulted to the in-repo path.
- **W2b — board facts.** `FREERTOS_PORT`, `FREERTOS_CONFIG_DIR`,
  `THREADX_CONFIG_DIR`, `NETX_CONFIG_DIR` are stated by each board's
  `nros-board.toml` (`force = true`, RFC-0103 D3) and no longer exported; the
  repo-wide default was one board's answer. Three roads carry them: the
  generated cargo config (`nros build`), `nros ws board-facts` → Corrosion
  env (cmake), and `nros ws board-facts` for a cargo leaf with no
  `system.toml` (test bins, benches), which now DERIVES its board from the one
  board crate it depends on instead of answering nothing. The generic
  `nros-board-{freertos,threadx}` crates skip their C compile when no board
  stated the facts (a workspace-wide `cargo check`), as they already did for an
  absent submodule. Found on the way: cmake memoised board facts in the CACHE
  with no freshness input, so a descriptor edit never reached an existing
  build dir; the memo is now per configure. Measured with every retired
  export unset: FreeRTOS + ThreadX-linux single-package Rust, workspace Rust,
  workspace C, and the s32z270 (Cortex-R) workspace C++ row build.

- **W2c — source trees.** No source-tree path reaches the user's shell:
  `scripts/sdk-env.sh` skips every index `[source.*] env` name (derived from
  the index, not listed), and `check-activate-shells` asserts both halves —
  the rest of the SSoT survives activation (issue 0451), these do NOT
  (mutation-tested). Every reader locates the tree instead: Rust through
  `nros_build_paths::locate::{source,try_source}` (generic boards, the
  zenoh-pico runner, NuttX ffi/link, the named `*_dir()` resolvers,
  cyclonedds-sys); platform descriptors through a new `{source:<name>}` token
  (the tree variables leave `required_env`); cmake through
  `nros_locate_var()` in `cmake/NanoRosLocate.cmake` — one
  `nros locate --all --format cmake` per configure, normal variables, and a
  stale module-written CACHE entry is dropped. `nros_validate_vars` asks it
  before `$ENV{}`. `just` recipes keep their recipe-scoped values from
  `just/sdk-env.just`: those are computed per run from the recipe's own
  checkout and inherited by nothing else, which is the distinction RFC-0103
  D4 draws. Found on the way: the FreeRTOS Cyclone include block hard-coded
  `portable/GCC/ARM_CM3` for every board (the s32z270 Cortex-R52 image
  compiled ddsrt against the Cortex-M3 `portmacro.h`); it reads the board's
  `FREERTOS_PORT` now.

**W2 leftovers (2026-10-12).** Three of the four are gone:

- **Tonbandgeraet** is `[source.tonbandgeraet]` (a checkout source; upstream
  has no release, so the pin is the version). `nros_build_paths::tband_dir`
  locates it, and the `TBAND_DIR` export is deleted.
- **The RISC-V recipe** no longer exports or `-D`s `THREADX_CONFIG_DIR` /
  `NETX_CONFIG_DIR`; the board descriptor states them `force = true`. Removing
  them exposed issue 1820: the generated cargo config DROPPED that `force`
  whenever the cargo-leaf road restated a board fact, so a shell still carrying
  a pre-W2b export built the riscv64 image against threadx-linux's
  `tx_user.h`. Those recipe exports had been hiding it. Fixed in the renderer.
  `_run-qemu` keeps its env: it runs a plain `cargo build` the descriptor does
  not reach.
- **`FREERTOS_PORT`** leaves the two fixture rows. The FreeRTOS overlays now
  SET their port through one helper, `nros_freertos_board_port`, used by all
  four boards. It never reads `ENV{FREERTOS_PORT}` and says so when the
  environment disagrees. It refuses a disagreeing `-D` on a one-port board,
  and takes it on S32Z270 (`OVERRIDABLE`: hardware uses NXP's port). The rows'
  comment ("`activate.sh` exports `GCC/ARM_CM3` repo-wide") had been stale
  since W2b; a stale SHELL still carries that export, and the board now
  ignores it.

**W2d -- the binary knows its own root (RFC-0103 D6, amended 2026-10-12).**
`activate.sh` exported `NROS_REPO_DIR` and `nano_ros_ROOT`, and checked
before deleting them they were not dead: a project OUTSIDE the checkout, built
with the in-tree `nros`, found the SDK root only through `$NROS_REPO_DIR`
(the ladder's last rung, a release's `share/nano-ros`, does not exist beside
an in-tree binary), and `find_package(nano_ros)` there relied on the exported
hint. Decided with the user: a contributor's process should match a user's,
and a user never exports a root. So:

- the ladder's last rung asks the BINARY where its root is: a release's
  `share/nano-ros` beside it, an in-tree build's checkout above it;
  `$NROS_REPO_DIR` leaves the ladder;
- cmake modules locate the root from their own path, never `$ENV{}`;
- `activate.sh` / `activate.fish` export neither name;
- the book's raw-`cmake` instructions use `nros build` or the preset
  `nros init` writes, which already passes `-Dnano_ros_ROOT`.

No chicken-and-egg: building the CLI is a `cargo build` whose scripts find the
checkout from `CARGO_MANIFEST_DIR`.

### W3 -- source trees into the store

- Every `[source.*]` row gains `ref = "<sha>"`; a gate holds gitlink ==
  `ref`; `nros-submodule-pins.toml` retires.
- `nros setup --source <name>` materialises
  `$NROS_HOME/sources/<name>/<version>+<sha8>/` by `git archive`, hardlinking
  files unchanged from the nearest pin, read-only. `nuttx-kernel`,
  `nuttx-apps`, `px4-autopilot` stay checkout sources (built in-tree).
- Reclaim: `Category::Sources`, `PinRule::Source`, `.nros-provenance`, liveness
  from installed toolchains' indexes plus the checkout index.
- The local-edit rung: an initialized submodule whose HEAD differs from the
  pin, or is dirty, outranks the store, and the build says so.
- Project `[sources]` / `[tools]` in `system.toml`, project-relative.
- The 81 `system.toml` `sdk = {env:…}` rows and the raw
  `cmd/board_facts.rs` interpolation go; `{env:}` tokens in
  `nros-platform.toml` resolve through `locate`.
- Board-owned config dirs (`THREADX_CONFIG_DIR` for the RISC-V board) become
  board-relative descriptor paths; `force = true` is removed.

**W3a — the pin has one owner.** All 14 submodule
rows state `git` + `ref`, written by `check-source-refs.py --write` from
`.gitmodules` and the staged gitlink, and the gate `check-source-refs` (fast
line) refuses any disagreement — the comparison issue 0602 lacked when it
removed these keys. `SdkIndex::validate` and `verify-index.py` require both on
a submodule row. An installed SDK root reads its pins from its OWN index copy
(`sdk_store::recorded_pin`, behind a `.git` absence test, version-locked to the
release rather than to a fetched index); `stage-sdk-root.sh` no longer writes
`nros-submodule-pins.toml` and instead runs `check-source-refs.py --rev HEAD`
on the commit it stages; `nros-rmw-provision.cmake` recognises an installed
root by the same absence. `check-release-manifest` R6 holds the three together
and refuses the pins file's return. Cost: a submodule bump is now a two-line
change, and the gate prints the one command that makes it.

**W3c-1 — the store mechanism (no row flipped yet).** The ladder gains the
local-edit rung (`locate::checkout_edit`: a store row's checkout copy whose
HEAD is not `ref`, or with tracked edits, outranks the store, and the build
prints a `cargo:warning` saying so) and keys a submodule row's store copy
`<version>+<sha8>` (`locate::store_key`; a clone row's `ref` is the upstream
release its version names, so it keeps `<version>`). `nros setup --source
<name>` on a submodule row with `location = "store"` materialises it
(`sdk_store::materialise_store_source`): `git archive <ref>` from the
checkout's submodule when it has the commit, else a store-wide bare mirror
`fetch/git/<name>.git` fetched by commit; files whose `(mode, blob, path)` a
sibling pin shares are hardlinked to it; files read-only; `.nros-provenance`
(`kind = "archive"`, `ref`, `git`) and `.nros-tree` written; built in a temp
dir and renamed into place. `store.rs` gains `Category::Sources` and
`PinRule::Source { name, key }` from the index's store rows. `sdk_store::
locate_row` is the one `SourcePackage` → ladder conversion, so the path setup
writes and the path builds read are one derivation. The git-environment
helper moved to `nros-build-paths/src/git_env.rs` (it ships in an SDK root;
`packages/cli` does not) so build-script git spawns clear it too. Nothing
changes for any build until a row says `location = "store"`, and a store miss
still falls to the checkout.

**W3c-2 — FreeRTOS, lwIP, ThreadX and NetX Duo are store-first.** Their rows
say `location = "store"`; `nros setup --source <name>` materialises them
(measured: 19 M, 9.6 M, 113 M, 152 M). Readers moved onto the ladder: the
`threadx-linux` and riscv64 board build scripts (`locate::source`), the
ThreadX/FreeRTOS `just` recipes and C-port smoke tests (`nros_locate_source`
in `scripts/build/cargo.sh`, passing `-D<VAR>=` to the smoke CMakeLists). `THREADX_EXTRA_INCLUDES` takes a path RELATIVE to
the located kernel, so the riscv64 descriptor no longer spells
`${workspace}/third-party/threadx/kernel/…`. `just/sdk-env.just` stops
exporting the four trees (an env value is rung 2 and would outrank the store).
New gate `check-store-source-readers` refuses a store-first row's `dest` in
code outside a reasoned allowlist (no-CLI cmake fallbacks, messages, dev
audits) and reports stale entries.

**W3c-3 — zenoh-pico and mbedtls are store-first.** `nros-zpico-build`
locates both (`zenoh_pico_dir()`; its `rerun-if-changed` lines name the
located tree), and the Zephyr module (`zephyr/cmake/nros_rmw_zenoh.cmake`)
asks the same ladder through a new by-NAME cmake helper,
`nros_locate_source(<out> <name>)`, so the cargo and west lanes compile one
tree. The zenoh-pico row has NO `env` override any more: `ZENOH_PICO_DIR`
already meant the install prefix of a user's own zenoh-pico
(`system-zenohpico`), and one name with two meanings is D6's defect; a
contributor overrides the tree by editing the submodule (the local-edit rung).
The audit gates that read zenoh-pico's sources (capability-conditionals,
zenoh-config-template, zenoh-source-manifest, c-array-guard-probe,
zenoh-feature-off-compile, gen-zpico-config-keys, gen-support-status, the
rustdoc preconditions, the workspace-fixture probe, isotp-c) ask
`scripts/lib/source_locate.py` (the Python twin of `nros_locate_source`), so
a worktree that never initialised the submodule runs them against the store
copy. Measured: 6.0 M + 30 M.

**W3c-4 — micro-XRCE-DDS-Client and micro-CDR are store-first.** The
`nros-rmw-xrce-cffi` build script binds both through `vendored(&xrce_sys,
"<name>")` (located, falling back to the checkout directory), and the
`nros-rmw-xrce` CMakeLists locates them with `nros_locate_source` before its
checkout fallback (normal variables, no longer CACHE, so a pin move is not
outlived by a cached path). `check-xrce-vendored-versions` accepts the located
binding and reads each tree's version from the located copy; the source and
config manifest gates read the located trees. Measured: the native Rust, C and
C++ XRCE workspace fixtures compile both trees from the store only.

**Which rows stay checkout sources, and why (decided in W3c):**
`nuttx-kernel`, `nuttx-apps`, `px4-autopilot` — their builds write INTO the
tree (D5). `cyclonedds-src` — the Zephyr lane PATCHES it in place
(`scripts/zephyr/cyclonedds-zephyr-*-patch.sh`), which a read-only shared
tree must never take; it can join the store once those patches are commits on
the fork instead. `nuttx-libc`, `px4-rs` — they are cargo `path =`
dependencies, resolved when cargo PARSES manifests, before any build script or
ladder runs; `px4-rs` is even reached from the root workspace's member list.
Moving either needs a generated `[patch]` row pointing at the store, which is
the RFC-0098 generated-config road, not this one.

**W3d — gc liveness and project `[sources]`.** `nros store gc` adds the SDK
index of every INSTALLED toolchain (`toolchains/<v>/` and `sdk/nros/<v>/`,
`share/nano-ros/nros-sdk-index.toml`) to its pin set
(`store::installed_toolchain_indexes`), so a materialised tree — whose
`+sha8` key no project pin file names — is live while an installed toolchain
needs it, wherever gc runs from; an index this CLI cannot parse (a newer
toolchain) falls back to the every-string reader, over-protecting.
`system.toml` gains `[sources] <row> = "<path>"` (project-relative), rung 3 of
the ladder (`Rung::Project`, `locate::project_source`): build scripts reach
it through the generated config's existing `NROS_WORKSPACE_ROOT` (the file is
watched by content), `nros locate` through `--project`, `NROS_WORKSPACE_ROOT`
or the nearest `system.toml` above the working directory, and `nros ws
board-facts` refuses a key that names no `[source.*]` row. The image-build
carrier the RFC names for it (`resolved.toml [locations]`) is W6.

**Acceptance, measured 2026-10-10 (W3c-4 tip).** A fresh `git worktree` with
NONE of the 20 submodules initialised: `just setup-cli` builds, `nros locate`
answers the store for all eight store-first rows, and the root workspace
loads (`cargo metadata`). The launch resolver needs
`packages/cli/third-party/play_launch` — the CLI's own code, not a
`[source.*]` row — so that one submodule was initialised (non-recursive); with
it, `workspace-rust-native`, `workspace-c-native` and
`workspace-rust-native-xrce` build with every vendored C tree from the store.
Not covered by that run: the zephyr west lane (checked only by a cmake probe of
`nros_locate_source`), and the checkout-only rows above, which still need their
submodules where a build uses them.

**W3e — project `[tools]`.** The `[tool.*]` twin of `[sources]`:
`system.toml` `[tools] <tool> = "<prefix>"` (project-relative) answers `nros
sdk-path <tool>` before the store's pinned prefix, so every bridge that asks —
cmake, `just`, shell — gets the project's toolchain; a name the index does not
pin is still refused, and `nros ws board-facts` refuses an unknown key. Build
scripts that resolve a tool themselves (`nros_build_paths::riscv64`) keep their
own `NROS_RISCV64_PREFIX` override; folding them onto the ladder is W7.

W3 follow-up (found by a fresh worktree, shipped with W4a): `just
setup-worktree` and the `ci gate` provisioning preflight
(`_require-worktree-provisioning`, issue 1513) still demanded the zenoh-pico
and XRCE SUBMODULES. Both now ask `scripts/lib/source_locate.py
--store-row-for <path>`: a store-first row counts as provisioned when the
ladder locates it, and `setup-worktree` materialises it with `nros setup
--source` once a CLI exists (the submodule stays the route before one does).

W3 is complete as scoped: every store-eligible row is store-first, the rest are
checkout sources for a recorded reason, and the acceptance run above passed.

**Acceptance:** an agent worktree with NO submodule initialized builds tier 1
from the store; editing a zenoh-pico submodule commit makes the next build use
and announce the checkout; an installed (no-checkout) project builds a
FreeRTOS image.

### W4 -- selections have one home

- Board, domain id, locator, IP and transport are stated in `system.toml`
  (`[image.<id>]`, `[system]`) and defaulted by the board's `[net]`; the board
  `config.rs` network literals and cmake `NROS_APP_CONFIG` defaults move there.
- Retired as inputs: `NROS_BOARD`, `nros::main!(board=)`, `-DNANO_ROS_BOARD`,
  `NROS_DOMAIN_ID`, `CONFIG_NROS_CYCLONE_DOMAIN_ID` (as a separate input),
  `ZENOH_LOCATOR`; inert `rmw-*` cargo features and `CMakeLists --features
  rmw-*`. `ROS_DOMAIN_ID` / `NROS_LOCATOR` stay as runtime overrides.
- Board alias matching becomes case-insensitive; duplicate-casing aliases go.

**Acceptance:** the second-home gate (RFC-0103 D9) holds at zero for these
facts; one image per road shows the same domain/locator in its
`resolved.toml` and in what it boots with (Zephyr, closes issue 1550).

#### W4 census (2026-10-11)

- `NROS_BOARD` is already a CARRIER only: `nros ws board-facts` writes it
  from the resolved `[image.<id>] board`; no user-facing producer remains.
- `nros::main!(board = …)` — 0 production uses (2 test fixtures), but it is
  the one route for an out-of-tree board type not in `BOARD_PATHS`; retiring
  it needs the descriptor to name the board's Rust path first.
- `NANO_ROS_BOARD` (79 non-comment) keys cmake BOARD-MODULE FILE names
  (`cmake/board/nano-ros-board-<x>.cmake`), which are not always a descriptor
  name — map module ↔ descriptor before it can be derived.
- Domain id and locator: `system.toml` already states both (bridged to cmake
  as `NANO_ROS_LEAF_*`), but 7 board crates bake them with
  `option_env!("NROS_DOMAIN_ID"/"NROS_LOCATOR")` and the Zephyr lane through
  Kconfig — one system.toml-fed carrier per road is the work.
- No board descriptor has a `[net]` table: network defaults live in 7 board
  `config.rs` files and 4 C-struct generators. `[net]` is NEW schema (with a
  per-role IP story for talker/listener pairs), not a migration.
- 29 `rmw-*` cargo features are inert (`[]`, ungated); the threadx-linux ones
  are still passed by fixture rows that duplicate the bare rows, so removing
  them means retiring those rows and their artifact locators together.

**W4a — mechanical.** Board NAMES match
case-insensitively everywhere a name is compared (`answers_to`,
`resolve_deploy`'s directory and platform rungs, `nros board`, `nros setup`'s
preset lookup, `nros-platform-config`'s `[[board]]` lookup, `board_path_for`,
`template_projects.py`); the five case-duplicate alias sets are gone
(`freeRTOS`/`FreeRTOS`, `AN536`, `NuttX`, `NuttX-riscv`, `S32Z270`), and a
unit test over the real catalog refuses a new one. Framework ids
(`west_board`, NuttX `board_config`, PlatformIO `board`) stay exact — they are
the downstream ecosystem's spellings. The dead `NROS_CYCLONE_DOMAIN_ID=`
compile definition is gone (nothing compiled read it); retiring the
`CONFIG_NROS_CYCLONE_DOMAIN_ID` symbol itself is a Kconfig change and waits
for W5's coordination with phase-481.

**W4b-1 — `[board.net]`, the two ThreadX boards.** A board descriptor may
state `[board.net]` (`ip`, `netmask`, `gateway`, `mac`), typed in
`nros_platform_config::BoardNet` (`deny_unknown_fields`, validated by the
CLI's descriptor loader too). `nros_board_common::board_net` resolves it over
the platform default (`BaseConfig::default`, RFC-0103 D1's platform < board)
and is the one producer of every copy: the crate's `Config::default()` reads
generated `BOARD_NET_*` constants, its build script's C `NROS_APP_CONFIG`
takes `ResolvedNet::c_network_initializer()`, and the cmake board module asks
`nros board net <board> --format c-network` (with `--ip-last` / `--mac-last`
for an image's own identity on the board's subnet, which the riscv64 fixtures
pass). Done for `threadx-linux` (states nothing: it IS the platform default)
and `threadx-qemu-riscv64` (`10.0.2.40`, `52:54:00:12:34:56`, gateway
`10.0.2.2`); the `listener()` presets derive from the default instead of
restating it. Measured: the Rust + C threadx-linux and C riscv64 workspace
fixtures build, and every generated `.network` block equals the literal it
replaced. Gate `check-board-net-one-home` (ratchet over migrated boards).
The LOCATOR stayed out: on both boards the Rust and C fallbacks name
different routers (issue 1793).

**W4b-2 — one default network for every QEMU board (re-scoped 2026-10-12,
RFC-0103 D10).** Checking the remaining boards before migrating them found
that `[board.net]` as "the board's identity" was the wrong model. The QEMU
boards each ran a DIFFERENT address plan, and two of them ran two:

- esp32-qemu's and NuttX's `Config` presets said `192.0.3.10`, while their
  entry paths said `10.0.2.50` and `10.0.2.30` (QEMU's default SLIRP subnet);
- threadx-riscv64 said `10.0.2.40`;
- FreeRTOS said `192.0.3.10`, with SLIRP configured to match;
- `mps2-an385` added a third network behind a `docker` feature.

A bridge, a board on several networks and a multi-NIC board cannot be one
identity per board. So D10 gives each fact its owner:

- the board states **interfaces**;
- the project states **networks**;
- the image states its **address**.

This wave does the part every board needs now:

- every QEMU board joins one default network, `qemu`: QEMU's own SLIRP plan
  (`10.0.2.0/24`, gateway/host `10.0.2.2`, guest `10.0.2.15`), named by
  `[board.net] network = "qemu"` rather than restated, so no launcher passes
  `net=`/`host=` (the first draft of D10 said `192.0.3.0/24`; measured, that
  would have moved 139 files to spare 47, and every QEMU board except the
  FreeRTOS C/C++ images already ran 10.0.2.0/24);
- a board's router default is derived from its gateway
  (`tcp/<gateway>:7447`, `board_net_locator!(port)` for another port);
- the per-image last-octet override stays.

The `[network.*]` table, bridge/TAP launch, multi-NIC and the collision
refusal are [phase-497](phase-497-networks-have-owners.md).

Landed:

- **All QEMU boards** (esp32-qemu, mps2-an385 bare-metal, mps2-an385-freertos,
  mps3-an536-freertos, both NuttX entries, threadx-riscv64) state
  `network = "qemu"`. Their `Config::default()` and entry defaults read
  generated `BOARD_NET_*` (`board_net::emit_for_crate`, or
  `emit_for_named_board` for the generic FreeRTOS crate, which serves several
  boards and finds its descriptor through `NROS_BOARD_TOML`). Two boards had
  disagreed with themselves: esp32's `Config` said `192.0.3.10` while its
  entry said `10.0.2.50`, and NuttX's presets said `192.0.3.10` while its
  entry pushed `10.0.2.30`. esp32 keeps `.50` as its stated default, and NuttX
  takes the network's `.15`.
- The FreeRTOS QEMU launcher that reconfigured SLIRP
  (`start_mps2_an385_freertos_slirp`) is gone; one MPS2 launcher remains. 22
  fixture locators moved to `10.0.2.2`. The entry-locator resolver dropped
  its FreeRTOS literal for the board-derived rung, so issue 0946's FreeRTOS
  disagreement is gone.
- The descriptors of s32z270 (hardware) and threadx-linux (host bridge) are
  untouched: they keep the platform default, `192.0.3.0/24`.
- `mps2-an385`'s `docker` profile keeps its literals until phase-497 makes it
  a `[network.docker]` row; that board is therefore not on
  `check-board-net-one-home`'s list (esp32-qemu and nuttx-qemu are).

For FreeRTOS in particular:

- `nros_board_net_c` (`cmake/NanoRosBoardNet.cmake`) is the one cmake spelling.
  It replaces the two ThreadX modules' inline blocks, and `nros board net`
  gained `--format cmake` so the configure depends on the descriptor the CLI
  names.
- `templates/freertos_app_config.c.in` takes its `.network` from that helper.
- The cargo-road C emitter resolves the overlay's own `[board.net]`.
- The generic crate's `listener()` derives from the default.
- Measured equal to the literals they replaced (`.11`, MAC `0x0b`, gateway
  `.1`).

**W4c — the inert threadx-linux `rmw-*` features.** The six role crates under
`examples/threadx-linux/rust/` declared `rmw-zenoh`/`rmw-xrce`/`rmw-cyclonedds`
as `[]` (no forwarding since #60 T5), kept only so six
`--no-default-features --features rmw-zenoh` fixture rows still resolved. Those
rows duplicated the bare rows beside them — the RMW is the bringup's — and
built the same image into a second group dir that no test read (the one
resolver that could, `build_threadx_linux_rust_example_rmw`, had no caller).
Rows, features and resolver are gone. The Zephyr leaves carry the same inert
set and wait for phase-481, which owns those leaves; the three PX4
companions' `rmw-xrce = []` is likewise a no-op that `just/px4.just` still
passes, left for the W8 retirement pass with the recipe.

**W4d — a board names its cmake module.** The cmake road's spelling of a
board is a board-module FILE stem (`cmake/board/nano-ros-board-<x>.cmake`),
and `nros build` used to hand cmake the alias the image authored — so
`board = "freertos"` (an alias of the mps2-an385 FreeRTOS board, whose module
is `mps2-an385-freertos`) configured against a module that does not exist.
Each `[[board]]` now states `cmake_module = "<x>"` (10 boards, 10 modules);
the generated cmake root emits the descriptor's module, falling back to the
authored name only for a board that states none. Gate
`check-board-cmake-module`: every module file claimed by exactly one board,
every claim backed by a file. What it does not do yet: a hand-written leaf
`CMakeLists.txt` still sets its own `NROS_BOARD`, which is the next retirement
(it can now be derived from the leaf's `system.toml` board through this map).

**W4g — an out-of-tree board reaches Form 1.** `nros::main!` maps a board
name to its Rust type through the static `BOARD_PATHS` table alone (it cannot
read descriptors at expansion), so a board not in that table had exactly one
route, `nros::main!(board = X)` — which also dropped the board's framework
(Form 2 hard-wires `OwnedSpin`). A board crate now states its own type in
`[package.metadata.nros.board] type = "::my_board::MyBoard"`, beside the
`framework` it already stated; `nros_build::emit_board_framework()` hands it to
expansion as `NROS_BOARD_TYPE` over the same build-script channel (issue
0415's — a proc macro's own env/file reads are invisible to cargo), and Form 1
takes it when the table has no key. Form 2 stays, deprecated, for W8.

**W4e — the locator has one home on the cargo road.** Measured first, which
reversed the plan: the census line above ("one system.toml-fed carrier per road
is the work") was already true for cargo images — `nros::main!` bakes the
image's `system.toml` locator/domain into its boot config, and every board
applies it OVER the env-fed default (`BootConfig::over_board_defaults`). A new
`nros-cargo.toml [env]` row would have been a SECOND carrier, so it was not
written. What was a second home instead: seven NuttX fixture rows restating
their leaf's own locator as `env = { NROS_LOCATOR, NROS_DOMAIN_ID }`. They are
gone, and `check-image-locator-bake` now (a) reads single-package leaf rows,
not only bringup images, (b) refuses a row value equal to the image's (rule 4),
(c) classifies `baremetal` and `threadx-riscv64`, and (d) names the two
no-router e2e bins that dial nothing on purpose. Not measured here: a NuttX
QEMU run of the seven images (the env and the bake held the same value, so the
binaries' locator strings cannot tell the carriers apart either way).

*Correction (issue 1836).* "Every board applies it" was false for NuttX, and
the unmeasured run is where it showed. `over_board_defaults` makes the BOARD's
locator win; the image's reaches a board only through its deploy-overlay merge
(`BaseConfig::apply_overlay`), and `nros-board-nuttx` had none — it read
`option_env!("NROS_LOCATOR")` and nothing else. With the row env gone, every
NuttX Rust image dialled the empty locator, scouted, and failed
`Executor::open` without putting a packet on the wire. The NuttX board now
applies the overlay like the other five families, and
`check-image-locator-bake` verifies that every family's crates do instead of
assuming it.

**W4f — a leaf's cmake board is derived, not restated.** `nros ws
leaf-system` prints `NROS_LEAF_CMAKE_BOARD` (the board's `cmake_module`,
W4d), and both cmake roads take it: the find_package road sets
`NANO_ROS_BOARD` from it instead of from the authored alias, and the
`add_subdirectory` road (the six rv-virt-threadx Rust leaves) now derives it in
the root `CMakeLists.txt` from the leaf's `system.toml` — the six
`set(NANO_ROS_BOARD rv-virt-threadx)` lines are gone. The rv-virt board module
takes the leaf's locator from that same read (one `leaf-system` call per
configure, not two). Measured: the talker leaf configures, bakes
`tcp/10.0.2.2:9400` from its `system.toml`, and links.

### W5 -- one canonical name per knob

Split by RFC-0103 D2b into a non-Zephyr half (W5a) and the Zephyr hook (W5b).
W5b was first given to phase-481; 481 archived on 2026-10-11 without it, and
it was folded back here the next day (user decision).

**Census (2026-10-11).** 95 knob env names — 49 on the board/platform ladder
(`[board.knobs]` settable), 46 env-only — plus 5 alias spellings; 37 are
`ZPICO_*`. Every reader keys on the env NAME (`nros_zephyr_build::knob`,
`platform_config::resolve_*`), so the registry slots in at the key. The
TOML-key → env map is 11 hand `*_env_key` match functions in
`platform_config.rs` (the tx knobs are literals); `KCONFIG_PAIRS` holds the 20
env ↔ Kconfig rows whose names differ. Four readers bypass the ladder entirely
(`nros-smoltcp`'s `env_usize_compat`, `nros-build-helpers::shared::env_usize`,
`nros-rmw-cyclonedds-sys`'s `KNOBS`, the FreeRTOS heap/stack reads). `config-knob-census.py` misses ~19
spellings because it only matches a literal first argument.

- **W5a (this phase)** — the knob registry: one committed row per knob
  (id, group, type, RMW condition, unit, builtin, range, prompt, help, and
  the Kconfig symbol it reads). Env spelling `NROS_<ID>`; a knob of one RMW is
  `NROS_<RMW>_<X>` (`ZPICO_MAX_PUBLISHERS` → `NROS_ZENOH_MAX_PUBLISHERS`); the
  old spellings become legacy rows accepted with a deprecation until W8. The
  Kconfig symbol defaults to `CONFIG_NROS_<ID>` and is named where it differs,
  so none is renamed (RFC-0103 D2b says why the id cannot be the Kconfig
  stem). `KCONFIG_PAIRS` and the `*_env_key` functions are derived from it,
  then deleted; the four bypass readers go through the ladder; the census
  reads the registry instead of regexes; a gate holds the module
  `zephyr/Kconfig` knob section equal to what the registry generates (prompts
  still present — 481 removes them).
- **W5b (this phase, after W5a)** — measure RFC-0103 D2b's M1–M4; the hook
  writes `Kconfig.from-system-toml`, the module's knob section goes
  prompt-less, the fragment drops its knob rows, `-DNROS_OVERRIDE_<ID>`
  replaces the fixture drivers' knob `-DCONFIG_*`. Choices, endpoint and domain
  stay in the fragment unless M1/M2 are green. **Done 2026-10-12** — see
  "W5b — M1–M4 measured" and "W5b — built" below.

  **Checked against the tree on 2026-10-12, before folding in.** 481 built
  the conf-FRAGMENT renderer (`nros ws leaf-system --kconfig-out`, the
  `module_ext_root` hook) and the snippet agreement check; nothing names
  `Kconfig.from-system-toml` or `NROS_OVERRIDE_`, and `zephyr/Kconfig` still
  defines all 124 `NROS_*` symbols with prompts — so none of W5b exists and
  D2b's premises still hold. One thing moved under it: #1973 makes the
  fragment leave out any symbol the command line sets with `-DCONFIG_*`, so a
  fixture's `-DCONFIG_NROS_DOMAIN_ID` beats `system.toml` today instead of
  failing 1550's agreement check. That is the override channel's job done by
  omission, for the fragment's rows only; once knobs become prompt-less
  defaults the `-D` is a Kconfig error again, and the channel is what replaces
  it.
- ~~`[board.knobs]` → `[knobs]` rides W5a.~~ Done: the top-level `[knobs]` is
  the canonical board rung in both parsers (typed, every `[[board]]` in the
  file), the five tracked descriptors moved, `[board.knobs]` is read with the
  registry's deprecation sentence until W8, a file stating both is refused,
  and `no_in_tree_board_descriptor_uses_legacy_board_knobs` keeps it out of
  the tree.

**W5a progress.** Slice 1 (#1961): `nros-zephyr-build/knobs.toml`, 86 rows,
generated into `KNOBS` + a derived `KCONFIG_PAIRS`; `knob()` reads every
spelling and refuses two that disagree. Slice 2: the eleven `*_env_key` match
tables in `nros-platform-config` and the three `ZPICO_TX_*` literals are gone —
each asks `registry_env(<group>, <knob>)` — and every env read on the ladder
goes through `knob_env`, which reads all spellings of the row, so
`ZPICO_FRAG_MAX_SIZE` still reaches the rung `NROS_ZENOH_FRAG_MAX_SIZE` now
names. The tools that used to read those match arms read the registry instead:
the census's ladder set, `check-knob-single-reader` (owners keyed by canonical
name, readers searched under every spelling — a second reader using the OLD
literal is still a second reader), `check-kconfig-knob-forwarding` (credits a
registry ladder row only while `platform_config.rs` still calls
`registry_env(`), and the pool inventory (ladder rows listed under their
canonical name, a legacy literal folded in). Slice 3: the four bypass readers
are on the ladder. `nros-smoltcp`'s `env_usize_compat` and
`nros-build-helpers::shared::env_usize` are gone (`knob(..).rung(..).resolve(..)`;
`NROS_SERVICE_TIMEOUT_MS` reads through `knob()` like its `nros-rmw-zenoh` twin,
still with no rung); `nros-rmw-cyclonedds-sys` forwards its five descriptor
knobs through `knob(..).strict_env().stated()`, keeping "absent leaves the
header default" and "non-numeric stops the build"; the FreeRTOS heap and
app-stack reads take rung 1 through `knob_env` on the no-platform arm too,
where only the ladder arm did. The registry gained the four cyclonedds rows it
lacked (90 rows). One behaviour change, the intended one: `NROS_SMOLTCP_X` and
`ZPICO_SMOLTCP_X` set to different values now stop the build where the old
fallback silently took the first. Slice 4: every in-tree WRITER uses the
canonical spelling — the Zephyr cmake lane resolves and forwards
`NROS_ZENOH_*` (its rung 1 reads every spelling through `_nros_knob_env`, from
a `cmake/NanoRosKnobRegistry.cmake` the generator now writes beside the Rust
table), the cargo-leaf sidecar writes `NROS_ZENOH_MAX_*`, an `[image.<id>] env`
row is carried under its canonical key, and the examples, the multisession lane,
docs/book and `.env.example` follow; C `-DZPICO_*` macro names stay. The gates
that matched a reader by its literal (`kconfig-knob-forwarding`, `knob-ends`,
`c-array-pool-floors`) now credit any spelling of the row. Slice 5: a value
that arrives under a legacy spelling is used and says so, once — a build
script's `cargo:warning=`, cmake's `DEPRECATION`, the CLI's stderr — in one
generated sentence (`DEPRECATION_TEMPLATE`). Slice 6: the 58 knob symbols the
module defines carry their definition in the registry row (`type`, `prompt`,
`kconfig_if`, `kconfig_attrs`, `help`) and `zephyr/Kconfig`'s knob section is
RENDERED from it; `check-zephyr-kconfig-knobs` holds the section equal to the
render and refuses a registry symbol defined outside it. No value moved: the
rows were extracted from the committed Kconfig and a block-by-block compare
found 0 of 58 differing; `check-zephyr-knob-agreement --write` now edits the
registry row and re-renders. **Per-road diff (2026-10-12)**, `origin/main`
(81aa972a7c) against the branch, each road built incrementally in one worktree
and every resolved value dumped with legacy names mapped to canonical (build
script `OUT_DIR` consts/defines, `cargo:rustc-env/cfg`, `CMakeCache`
`NROS_RESOLVED_*`/`NROS_KNOB_SOURCE_*`, the zephyr `.config`, the leaf
`nros-cargo.toml [env]`): cargo leaf `examples/native/rust/talker` 202 rows,
cmake `examples/native/c/talker` 243 rows, west `features`
`zephyr_rust_params` on Zephyr 3.7 `native_sim/native/64` (the image whose
`ZPICO_MAX_QUERYABLES` row moved; 70 resolver + 83 Kconfig + 14,481
build-script rows) 14,634 rows — **0 differences on all three**. Not measured:
`realtime-rust`'s `zephyr_derived` (the moved `ZENOH_READ_PRIORITY` row reaches
the same `CONFIG_NROS_ZENOH_READ_PRIORITY` by `kconfig_key_for`, unit-tested)
and the 4.4 line. Next: `[board.knobs]` → `[knobs]`, then W5b.

**W5b — M1–M4 measured (2026-10-12).** A scratch module with the same
`module_ext_root` hook (copies a chosen file to
`<build>/nros/Kconfig.from-system-toml`; the module `osource`s
`$(KCONFIG_BINARY_DIR)/../nros/Kconfig.from-system-toml` first inside its
`if`), an empty app, `native_sim/native/64`, configure only, values read from
`zephyr/.config`. Each run is `cmake -GNinja -S app -B <b>
-DBOARD=native_sim/native/64 -DZEPHYR_EXTRA_MODULES=<mod> -DCONF_FILE=<conf>
-DMM_GEN=<generated file>` (plus `-DCONFIG_…` for M3), and a re-configure is
`cmake <b>` with a new `-DMM_GEN`. Zephyr 3.7 (SDK 0.16.8) and 4.4 (this host has no SDK 1.0.1,
so `ZEPHYR_TOOLCHAIN_VARIANT=host`, which is enough to configure native_sim and
does not touch Kconfig). Identical on both lines unless noted:

- **M1 — RED.** A prompt-less `choice` is refused before any assignment is
  read: `<choice …> defined without a prompt` and `the choice symbol … has no
  prompt` are warnings, and Zephyr aborts on Kconfig warnings — with or without
  the generated file's `default`. So the RMW / API choices cannot go
  prompt-less, and they stay in phase-481's fragment.
- **M2 — green only with the guard copied.** A `default 13` in the generated
  file OUTSIDE `if NROS_RMW_ZENOH`, for a symbol the module defines only inside
  it: right value with zenoh selected (13), and a STRAY `CONFIG_…=13` with xrce
  selected — the second definition widens the symbol's dependency to `y`. The
  same default wrapped in the same `if` gives 13 with zenoh and no line with
  xrce. The hook therefore renders each knob under its row's `kconfig_if` (and
  its `depends on`). M1 being red, the endpoint and domain stay in the fragment
  too (D2b: they move only if both are green).
- **M3 — green.** `-DCONFIG_<prompt-less>=17` is a hard configure error on 4.4
  as on 3.7 (`is assigned in a configuration file, but is not directly
  user-configurable (has no prompt)`), and so is a conf line. Correction to
  RFC-0103 D2a: Kconfig's message names ONE definition — the module's, not the
  generated file — so the generated file's comments are not where a user is
  sent. The hook refuses both itself first, naming `system.toml` and
  `-DNROS_OVERRIDE_<ID>`; Kconfig's error stays as the backstop for conf files
  the hook does not see (board fragments, snippets).
- **M4 — green.** Prompts declared only in the generated file: a guest
  configures with `prj.conf` 9 → 9 and `-DCONFIG` 17 → 17; re-configures
  without a wipe (conf 9 → 11 → 11); switching the generated file to a
  system.toml default and back follows it (13, then the guest value). What
  menuconfig shows, read with kconfiglib under the exact environment `ninja
  menuconfig` runs with (the symbol's prompt and visibility): visible with the
  prompt for a guest, invisible (visibility 0, no prompt) for a system.toml
  project. One line difference: switching BACK to the guest with an empty conf,
  3.7 keeps the last value (13) where 4.4 returns to the module default (5) —
  a project that deletes its `system.toml` on 3.7 keeps the old values until a
  pristine build.

**W5b — built (2026-10-12).** The 59 registry knob symbols are prompt-less in
`zephyr/Kconfig` (the prompt text survives as a comment above each), and the
knob section `osource`s `$(KCONFIG_BINARY_DIR)/../nros/Kconfig.from-system-toml`
first. The `module_ext_root` hook always writes it: for a project with a
`system.toml`, `nros ws leaf-system --kconfig-defaults-out` renders every knob
under its module definition's `if`/`depends on`, with the image's `[image.<id>]
env` value — or a `-DNROS_OVERRIDE_<ID>=<v>`, provenance "on the command line
(this build only)" — as a `default`; for a guest it copies
`zephyr/Kconfig.knob-prompts`, rendered by `gen-knob-registry.py` beside the
section, so a guest needs no CLI. One list: `knobs.toml` renders the section,
the guest prompts, `KCONFIG_KNOBS` (Rust) and `NROS_KNOB_KCONFIG_*` (cmake);
`check-zephyr-kconfig-knobs` now also holds the prompts file. The fragment
keeps choices, endpoint and domain; its knob rows are gone. The hook refuses,
before Kconfig and naming `system.toml`, the `env` row and the override, both
a `-DCONFIG_<knob>` (cached `CONFIG_`/`CLI_CONFIG_` included) and a knob line in
any conf file the build names, the image's own `conf` or the default
`prj.conf`; `-DNROS_OVERRIDE_*` on a guest is refused, naming `-DCONFIG_*`.
Override sites: the one tree site was the boot-report fixture's
`arena-exhausted.conf` (`CONFIG_NROS_EXECUTOR_ARENA_SIZE=32`), now the row's
`west_overrides = { EXECUTOR_ARENA_SIZE = "32" }` (`fixtures-manifest.py
west-overrides` → `zephyr-fixture-leaves.sh`); no fixture driver passed a knob
`-DCONFIG_*` (their locator / agent-port / domain ones are endpoint and domain,
which stay).

Acceptance, against `examples/zephyr/c/talker` (`zephyr_zenoh`) and a scratch
guest, `native_sim/native/64`, on 3.7 AND 4.4, configure only: plain → OK;
`-DNROS_OVERRIDE_EXECUTOR_ARENA_SIZE=32` → `.config` 32, and `-U` of it on a
re-configure → 0 (the default); `-DCONFIG_NROS_EXECUTOR_ARENA_SIZE=32` →
refused by the hook; a conf file with `CONFIG_NROS_EXECUTOR_ARENA_SIZE=32` →
refused by the hook; guest `prj.conf` 4096 → 4096, re-configure with `-DCONFIG`
8192 → 8192; guest `-DNROS_OVERRIDE_*` → refused. menuconfig (kconfiglib under
`ninja menuconfig`'s own environment): the system.toml project's knobs are
invisible with no prompt, the guest's visible with theirs, on both lines.

Zephyr tier-2 cells (3.7; `NROS_FIXTURE_COORDS=lane-coords-tier2 just zephyr
build-fixtures`, then `just test-select` over the Zephyr tests under the tier-2
coordinates): all 8 tier-2 leaves built — six `zephyr,c,cyclonedds`, the
`zephyr-cortex-m,c,zenoh` talker and the boot-report image, whose `.config`
carries the override's 32. Green: the four `cyclonedds_c` boot smokes, the
cyclonedds C pubsub / service / action e2e, `zephyr_cortex_m_c_zenoh_pubsub_e2e`
and `zephyr_cortex_m_c_boot_report_names_arena_exhaustion` (the override cell).
Not green, neither from W5b: seven native↔Zephyr tests need the in-lane LINUX
talker fixture, which a Zephyr-only build does not make; and
`a_shared_cmake_input_marks_the_image_stale` failed in the parallel run and
PASSES solo — it truncates-and-restores `cmake/NanoRosCodegenCore.cmake` while
other cells resolve the same fixture, and one of them recorded the file in the
`.nros-srcbaseline` at size 0 mid-write.

**Acceptance:** W5a — the registry is the only list (gate); every knob's
env/TOML/Kconfig spelling derives from its id; one build per road shows
identical resolved values before and after (a diff). W5b — M1–M4 recorded
here, on Zephyr 3.7 and 4.4; a `system.toml` project's `prj.conf` knob line
and its `-DCONFIG_<knob>` each refused by Kconfig; a guest (no `system.toml`)
configures and reconfigures with menuconfig showing the prompts; the Zephyr
tier-2 cells green.

### W6 -- generated output under `<build>/nros/`

- `resolved.toml` gains `[locations]` and per-value provenance including
  `transient` for env/CLI overrides.
- `cargo.toml`, `locations.cmake`, `include/`, `models/`, `sizing/` under
  `<build>/nros/`; one path function each, `nros locate artifact` for non-Rust
  consumers; the cmake model-ladder mirror and the shell literals go.
- The config header written by resolve (RFC-0103 D8); until then one
  `OUT_DIR` parser in `nros_build_paths` replaces the three.

**Acceptance:** no consumer outside the path function names an artifact path
(gate); the PX4 `${NANO_ROS_ROOT}/target/...` literal is gone.

**W6a — one reader of cargo's JSON stream.** The three `OUT_DIR` parsers
(`cargo-out-dir-headers.py`, `xrce-cffi-out-dir.py` — whose `<name>#` substring
match also hit any package whose name ended in it — and the pipeline printed in
nros-rmw-xrce's CMakeLists) are one module, `scripts/lib/cargo_out_dir.py`
(`package_matches`, `out_dirs`), held by the new gate `check-cargo-json-reader`.
The RFC placed this interim parser in `nros_build_paths`; its consumers are
cmake and `just` recipes, which reach Python, not a Rust library, so the one
spelling lives on the Python side until D8 retires it.

**W6b — one answer for where a cargo-built artifact lands.**
`nros_build_paths::artifact::Artifact` (`c-archive`, `cpp-archive`,
`c-include`, `cpp-include`, `c-config-header`, `cpp-config-header`) is the
layout under the cargo target dir; the `nros-c`/`nros-cpp` build scripts that
WRITE the headers take their paths from it, and `nros locate artifact <name>
[--target T] [--profile P]` joins it with cargo's own target dir (`cargo
metadata`, so `CARGO_TARGET_DIR` and `[build] target-dir` count). Twins:
cmake `nros_locate_artifact()` (`cmake/NanoRosLocate.cmake`, REQUIRED — an
artifact has no fallback path), shell `nros_locate_artifact`
(`scripts/build/cargo.sh`), make `$(call nros_artifact,…)`. The PX4 module's
`${NANO_ROS_ROOT}/target/...` literals (archive, include dirs, both pairing
headers) and NuttX `Make.defs`' are gone, with `compile-check-fixtures.sh` and
the borrowed-e2e fixture; every default answer is byte-identical to the literal
it replaced (measured: cmake, make and shell). Gate `check-artifact-path-literals`
(a `target/` + artifact name in CODE, outside an allowlist with reasons; one
entry is a `packages/cli` test with no edge to `nros-build-paths`).
**D8 does not hold as written** — the header carries SIZES the build script
extracts from the compiled `nros` rlib (`nros-sizes-build`, a nested cargo
build), which no pre-compile step can know. So the header stays a build
product; what W6b makes single is its LOCATION. Recorded in RFC-0103.

**W6c — `resolved.toml [locations]`.** The resolve phase records, per image,
where every `[source.*]` tree the ladder answers for came from: `path`, `rung`
(`env` / `project` / `local-edit` / `store` / `checkout`) and `transient` (true
for an environment answer, which no file states). One ladder
(`cmd::locate::locate_answer`, the function `nros locate` prints through), with
the workspace as the project. A record, not a carrier — build scripts re-ask the
ladder — so it is outside `digest`. Measured on `examples/workspaces/rust`
(17 images): from a shell that had once activated the MAIN checkout,
`freertos-kernel`, `lwip`, `threadx`, `threadx-netxduo` and the NuttX/PX4 rows
read `rung = "env"` — the inherited `FREERTOS_DIR` & co., re-rooted onto this
worktree's (uninitialised) checkout copy, outranking the store the rest of the
rows came from. That is RFC-0103 D3's precedence doing what it says, and it is
exactly the fact this record exists to make readable. Per-value provenance for
KNOBS (an env override of a derived count) waits for W5: it needs each row's
env spelling, and until knob ids derive it mechanically, a row → env-name table
written here would be one more hand-kept home for that fact.

**W6d — the cmake model-ladder mirror is gone.** `nano_ros_entry()` took
`nros model-path`'s answer and then re-routed it through a ladder of its own
(`$NROS_MODEL_DIR/<name>`, then `${CMAKE_BINARY_DIR}/nros/<name>`).
`model_location` already searches `$NROS_MODEL_DIR`, nothing writes a model to
the second rung, and the re-route ran AFTER the verb's `model_gate`
(phase-460 W1) — so a re-routed model was one the gate had never seen. The
verb's answer is used as given.

### W7 -- leftovers and close

- The three test-bin `config.toml` files move to `system.toml`. (Deferred,
  2026-10-11: the three `logging-smoke-*` bins are BARE images —
  `run_bare(Config::from_toml(include_str!(..)))`, the RFC-0004
  `[node]`/`[[transport]]` schema — and nothing reads a `system.toml` network
  block for an image without `nros::main!`. Moving them needs that reader
  first; until then each file is the one home of its bin's identity, so it is
  not a second home.)
- `NANO_ROS_*` cmake variables converge on `NROS_*` — **decided 2026-10-11
  (user): a compat shim, old names kept as DEPRECATED ALIASES**, no hard
  break. 55 names / ~1,100 sites, of which 11 are user-facing (documented or
  passed as `-D`): `ROOT`, `PLATFORM`, `BOARD`, `RMW`, `FEATURES`,
  `SAFETY_E2E`, `ROS_EDITION`, `GEN_CACHE_DIR`, `SKIP_BOOTSTRAP`,
  `BUILD_CODEGEN` (and `NANO_ROS_DIR` / `nano_ros_ROOT`, which are CMake's own
  `find_package` conventions and stay). One module,
  `cmake/NanoRosDeprecatedNames.cmake`, holds the OLD → NEW table: called at
  every entry point, it adopts an old name a user set (DEPRECATION message
  naming the new one), refuses the two disagreeing, and nothing else in the
  tree reads an old name (gate). Internal-only `NANO_ROS_*` (export/leaf/
  provider/locator plumbing) are renamed outright. Two passes:
  - **W7b-1** — the shim, the user-facing names with no `NROS_*` twin, and
    every internal name.
  - **W7b-2** — the three names that already have a twin with its own
    precedence (`NANO_ROS_RMW`/`NROS_RMW`, `NANO_ROS_BOARD`/`NROS_BOARD`,
    `NANO_ROS_ROOT`/`NROS_REPO_DIR`): reconcile which wins, then merge.
- **W7b-1 landed (2026-10-11).** `cmake/NanoRosDeprecatedNames.cmake`
  (`nros_adopt_deprecated_names()`, called from the root `CMakeLists.txt`,
  `nano_rosConfig.cmake`, `zephyr/CMakeLists.txt` and
  `NanoRosWorkspace.cmake`) aliases `NANO_ROS_{PLATFORM,FEATURES,SAFETY_E2E,
  ROS_EDITION,GEN_CACHE_DIR,SKIP_BOOTSTRAP,BUILD_CODEGEN,CMAKE_DIR}`; every
  tracked use moved to the `NROS_*` name (550 occurrences in 149 files; history
  docs untouched), and the internal `NANO_ROS_{EXPORT,LEAF,PROVIDER,LOCATOR,
  DEFAULT}_*` names were renamed outright — `NANO_ROS_LEAF_*` now shares the
  `NROS_LEAF_*` spelling the CLI's leaf-system rows already used for the same
  facts. Gate `check-deprecated-cmake-names` (only the shim reads an old
  name); cmake test `cmake-deprecated-names` (cache and normal aliases adopted
  with a deprecation, a disagreement refused, the new name alone silent); book
  page `reference/deprecated-names.md`.
- **W7b-2 landed (2026-10-11).** The three twins merged: `NANO_ROS_RMW` →
  `NROS_RMW` and `NANO_ROS_BOARD` → `NROS_BOARD` (always set to the same value
  as their twin), `NANO_ROS_ROOT` / `NANO_ROS_ROOT_DIR` → `NROS_REPO_DIR`
  (RFC-0103 D6's one root name) — all three in the shim table. Two precedence
  fixes came with the merge: `nano_rosConfig.cmake` adopts deprecated names
  BEFORE stating the root (its own directory is the root), and sets
  `NROS_BOARD` from the leaf's export only when the project did not
  (a `-DNANO_ROS_BOARD` used to win; `-DNROS_BOARD` still does). The
  `NANO_ROS_ROOT <path>` KEYWORD of `nano_ros_workspace()`,
  `nano_ros_generate()` and the providers function is kept as a deprecated
  alias of the `NROS_REPO_DIR` keyword. The first rename pass missed every
  `-DNANO_ROS_*` flag (`\b` cannot separate `-D` from `N`) and the gate shared
  the blind spot — 315 flags; both passes and the gate now match after `-D`.
  `NANO_ROS_ROOT` as an ENVIRONMENT variable stays retired and refused.
- **Old-name retirement pass (W8, after the aliases have shipped in one
  release).** Remove the shim's table entries (each old name then becomes a
  configure ERROR naming its replacement, kept one more release before the
  rows go entirely), drop the "deprecated alias" notes from the book, and
  turn the gate from "only the shim reads an old name" into "no tracked file
  names one". Same pass for the other aliases this phase keeps:
  `ZENOH_LOCATOR` (runtime alias of `NROS_LOCATOR`), the `ZPICO_SMOLTCP_*`
  knob aliases, the env-spelled `[image.<id>] env` knob rows W5 keeps until
  W5b lands. And the legacy `[board.knobs]` section spelling: both descriptor
  parsers (`BoardKnobsFile::parse_for_board`, the CLI's `BoardFile::parse`)
  stop accepting it and refuse it naming `[knobs]`.
- ~~Issue 1768 (census classifier + fail on UNCLASSIFIED).~~ Done (W7a): the `linker-script` role knows `link_script!`; gate `build-wiring-classified`.
- `canonical-build-path.md`, the book's configuration and getting-started
  pages, CLAUDE.md pointers.
- RFC-0103 → Stable; issue 1767 archived.

**Acceptance:** `just ci gate` green; tier 2 green for the cells the waves
touched; issue 1767 archived.

## Open issues that name this phase

Open issues whose `related:` names this phase (listed 2026-10-12, so the
link runs both ways). Listing one here records the link, not a claim that
this phase will fix it.

- [issue 0849](../issues/0849-nros-sync-bakes-the-invocation-path-spelling.md) — `nros sync` bakes the invocation's path SPELLING into every leaf patch table, so working through a symlink to the che …
- [issue 0930](../issues/0930-built-qemu-can-be-stale-against-its-pin.md) — The built QEMU can be older than the commit `third-party/qemu/qemu` pins, and nothing says so
- [issue 0934](../issues/0934-config-redundancy-map.md) — The same fact is authored in up to five config surfaces; no SSoT is declared for rmw, board or domain
- [issue 0941](../issues/0941-board-facts-soft-failure-hides-unreachable-site-config.md) — `nros_resolve_board_facts` fails SOFT, so an unreachable site-config block is silent
- [issue 1811](../issues/archived/1811-nightly-ports-read-in-tree-sources-after-store-move.md) — After phase-484 moved source trees into the store, four nightly RTOS port jobs still look for them in-tree or through …
- [issue 1816](../issues/1816-git-grep-error-read-as-no-match.md) — A `git grep` that could not search reads as "no match", so a forbidding gate passes outside a checkout
- [issue 1850](../issues/1850-host-tool-binaries-located-by-literal-path.md) — The in-tree `nros` and `nros-launch-resolve` binaries are located by literal path at ~50 sites (issue 1767 items 8–9)
- [issue 1852](../issues/1852-generated-header-dir-joined-outside-artifact-locator.md) — `<target>/nros-c-generated` is still joined by hand outside `nros locate artifact` (issue 1767 item 12 residue)
