# Phase 484 -- one owner per fact: resource locations and configuration

**Status (2026-10-10). W0 LANDED (measured); W1-W7 not started.** Implements
[RFC-0103](../design/0103-one-owner-per-fact.md). Tracks issues
[1767](../issues/1767-resource-location-resolved-many-ways.md) and
[1768](../issues/1768-build-wiring-census-misses-link-script-macro.md).

**Not this phase:** phase-481 (in progress elsewhere) owns the Zephyr leaf
`.conf` migration, the image Kconfig fragment renderer, per-image workspace
configures, and issues 1721/1757. W5 here consumes that renderer and must be
coordinated with it; nothing here edits an example `.conf` file.

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

### W5 -- one canonical name per knob

- The Zephyr module defines nano-ros knob symbols with NO prompt and sources
  a hook-generated `<build>/nros/Kconfig.from-system-toml` first: resolved
  values as `default`s when the project has `system.toml`, added prompts when
  it does not (RFC-0103 D2a). Coordinated with phase-481, whose fragment this
  replaces for knobs.

- Each knob gets an id `<group>.<name>`; TOML / env / Kconfig spellings derive
  mechanically. `KCONFIG_PAIRS` and the `ZPICO_*` / `NROS_SMOLTCP_*` user
  names go; backend macros are generated. `[board.knobs]` → `[knobs]`.
- Coordinated with phase-481: its fragment renderer and its `[image.<id>] env`
  rows read the new names; the env-spelled rows stay accepted until 481
  closes.

**Acceptance:** the mechanical-name gate; every knob's three spellings derive
from its id; one build per road shows identical resolved values before and
after (a diff).

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

### W7 -- leftovers and close

- The three test-bin `config.toml` files move to `system.toml`.
- `NANO_ROS_*` cmake variables converge on `NROS_*`.
- Issue 1768 (census classifier + fail on UNCLASSIFIED).
- `canonical-build-path.md`, the book's configuration and getting-started
  pages, CLAUDE.md pointers.
- RFC-0103 → Stable; issue 1767 archived.

**Acceptance:** `just ci gate` green; tier 2 green for the cells the waves
touched; issue 1767 archived.
