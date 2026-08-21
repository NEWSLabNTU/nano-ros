# Integrating nano-ros into a Vendored Tree

This page is for the engineer whose company has its own source tree —
a BSP, possibly a forked RTOS, an existing build system — and needs
nano-ros to live *inside* it: pinned, reproducible, buildable offline,
upgradable on their schedule. It collects the contracts the other
chapters mention in passing into one place.

nano-ros is **source-only**: nothing is on crates.io, there is no
install prefix, and no binary distribution. Vendoring the repository
is not a workaround — it is the supported consumption model.

## Pinning the checkout

Vendor the repo as a git submodule (or subtree) and pin a release tag:

```bash
git submodule add https://github.com/NEWSLabNTU/nano-ros.git third_party/nano-ros
git -C third_party/nano-ros checkout nros-v0.5.0
git add third_party/nano-ros
```

Rules that keep the pin trustworthy:

- **Pin tags (`nros-v<X.Y.Z>`), not `main`.** A moving branch unships
  the pairing you tested.
- **Move the pin forward only.** A rewind silently drops whatever the
  skipped commits fixed, and two hex strings in a diff cannot be
  ordered by eye.
- **nano-ros's own submodules are decisions, not lags.** The Cyclone
  DDS fork tracks the Cyclone that ROS ships; zenoh-pico is pinned at
  a wire-stable version. Never "update to latest" inside the vendored
  checkout — take what the tag pinned.

Initialize nano-ros's submodules **non-recursively** — the launch
toolchain's third-level runtime submodules are never built by
nano-ros:

```bash
git -C third_party/nano-ros submodule update --init packages/cli/third-party/play_launch
```

For build-related submodules you normally do nothing: the configure
step populates the ones your `(platform, RMW)` pair needs (see
Bootstrap below), or you pre-seed them explicitly for offline builds.

## What a fresh vendored checkout needs, in order

Four things, and the order matters:

1. **The `nros` CLI.** `./scripts/bootstrap.sh` inside the checkout
   builds `packages/cli/target/release/nros`. Nothing else works
   without it.
2. **An activated environment** for interactive use:
   `source third_party/nano-ros/activate.sh` wires `nros` and the
   SDK-store tools onto `PATH` and exports `nano_ros_ROOT`. CI can
   skip this and pass paths explicitly (`-Dnano_ros_ROOT=…`, absolute
   CLI path).
3. **Provisioned toolchains.** `nros setup <board> --rmw <rmw>`
   fetches the cross-compiler, emulator, and SDK sources for exactly
   that pair into the shared store at `${NROS_HOME:-~/.nros}/sdk`.
   `--dry-run` prints the plan first. (The zenoh *router* is
   deliberately not provisioned — it comes from a ROS 2 install,
   `ros2 run rmw_zenoh_cpp rmw_zenohd`.)
4. **Message codegen.** `NROS_REPO_DIR=<checkout> nros sync` from your
   workspace root — see the next section for why this is not optional.

## The Rust side: `nros sync` is mandatory, and your config is inline

Two things in every Rust leaf are **gitignored and generated**: the
`generated/` message crates and the cargo patch machinery that
resolves registry-style names (`nros = { version = "*" }`) to the
vendored sources. A fresh clone cannot build a Rust package until
`nros sync` has run — the failure otherwise is cargo dying during
manifest parse with an error that never names sync.

For an **out-of-tree consumer** (your workspace, outside the nano-ros
checkout), `nros sync` writes the `[patch.crates-io]` table **inline
into your leaf's `.cargo/config.toml`, with absolute paths, and no
`include` line**. This is deliberate:

- The in-repo examples use a *relative* `include =
  ["…/nros-patch.toml"]` pointing at a central gitignored file. That
  shape has three fragile preconditions (cargo ≥ 1.93, a correct
  relative path, the central file existing) and its failure modes are
  confusing. Outside the checkout you get the whole table inline, so
  the only failure mode is loud.
- **Never copy an in-repo example's `.cargo/config.toml` into your
  tree.** You would be copying the relative `include` line, which
  cannot resolve from your directory. Copy the *package* and run
  `nros sync` — it writes the correct config for that location.
- A **moved checkout invalidates the absolute paths** — re-run
  `nros sync` after relocating either the workspace or the vendored
  nano-ros.

Message dependencies stay path deps pinned `0.0.0`
(`std_msgs = { path = "generated/std_msgs" }` after sync); never
registry-name a message crate — a bare `std_msgs = "*"` can resolve
against the public crates.io.

## The CMake side

Two entry points, both source-backed:

- **`find_package(nano_ros REQUIRED)`** — the ament-shaped entry
  (`nano_rosConfig.cmake` at the checkout root). Located via
  `nano_ros_ROOT`, which `activate.sh` exports; CI passes
  `-Dnano_ros_ROOT=<checkout>` explicitly.
- **`add_subdirectory(third_party/nano-ros)`** — the lower-level
  shape. See [Build as a CMake subdirectory](build-as-subdirectory.md)
  for the cache-variable table.

Either way, at configure time nano-ros's `bootstrap.cmake` runs
`tools/setup.sh` to populate the source submodules your
`(NANO_ROS_PLATFORM, NANO_ROS_RMW)` pair needs, and may FetchContent
Corrosion from the network if none is installed. That is correct on a
developer machine and wrong in hermetic CI — see the next section.

## Air-gapped / mirrored CI

The offline contract, all of it:

1. **`-DNANO_ROS_SKIP_BOOTSTRAP=ON`.** Disables the configure-time
   submodule fetch and any network FetchContent. You now own
   pre-seeding.
2. **Pre-seed the submodules** your platform/RMW pair needs in your
   mirror (e.g. `zpico-sys/zenoh-pico`, the FreeRTOS kernel + lwIP,
   the Cyclone fork — `tools/setup.sh` derives the per-target list
   from `nros-sdk-index.toml`, which is the single manifest of what
   comes from where).
3. **Pre-install Corrosion**: `nros setup --tool corrosion` stages it
   into the SDK store, where the configure finds it without the
   network. Verify the configure line prints
   `nano-ros: Corrosion <ver> via <origin>` — never infer the version
   from having run an installer; a stale store entry can shadow a
   fresh one.
4. **Cache the SDK store.** Everything `nros setup` fetches lands
   under `${NROS_HOME:-~/.nros}/sdk`; point `NROS_HOME` at a shared
   or cached location and the store is reusable across jobs and
   machines of the same host triple.
5. **Mirror the asset sources.** `nros-sdk-index.toml` names every
   asset's upstream (`[tool.*]`, `[source.*]` sections). For a full
   mirror, rehost those URLs and patch the index in your vendored
   checkout — that patch is part of the carried diff below, and the
   index is versioned with the tree so the patch rebases cleanly.

## The patch set you carry

Be clear-eyed: some extensions live **inside** the vendored checkout.
There is no out-of-tree escape hatch for them today, so a vendored
tree carries a small local diff and rebases it across tag bumps:

| You add… | Files inside the checkout |
| --- | --- |
| A **platform** (your RTOS) | `packages/platform/nros-platform/Cargo.toml` (feature + optional dep), `nros-platform/src/resolve.rs` (type alias), `cmake/platform/nano-ros-<name>.cmake` |
| A **board** the tooling can see | **none needed since `NROS_EXTRA_BOARD_PATH`**: point it (env, or CMake cache var for `nano_ros_use_board()`) at a directory of board crates in YOUR tree — same layout as `packages/boards/`. Board keys stay global (a name under two roots errors). Copying into `packages/boards/` remains the fallback |
| A **mirrored SDK index** | `nros-sdk-index.toml` URL edits |

An application, a message package, a board *overlay crate*, and a
custom transport all live entirely in **your** tree — no diff.

Workflow for the diff: keep a branch in your nano-ros fork
(`company/nros-v0.5.0-patches`), and on upgrade rebase it onto the new
tag rather than merging. The registration edits are a few lines each;
conflicts are rare and loud.

## Upgrading the pin

```bash
git -C third_party/nano-ros fetch origin --tags
git -C third_party/nano-ros rebase --onto nros-v0.6.0 nros-v0.5.0 company/patches
# build + test, then:
git add third_party/nano-ros
```

After every pin move, in order: rebuild the `nros` CLI
(`./scripts/bootstrap.sh`), re-run `nros sync` in each consuming
workspace (the sync output is keyed to the CLI), then rebuild. Stale
sync output after an upgrade produces build errors that look like
missing packages, not like staleness.

## Checklist

- [ ] Submodule pinned to an `nros-v` tag, forward-only
- [ ] `play_launch` submodule initialized non-recursively
- [ ] CI passes `-Dnano_ros_ROOT=` / `NROS_REPO_DIR=` explicitly (no
      reliance on an activated shell)
- [ ] `nros sync` runs in CI before any Rust build; no `.cargo/config.toml`
      copied from in-repo examples
- [ ] Air-gapped: `NANO_ROS_SKIP_BOOTSTRAP=ON`, submodules pre-seeded,
      Corrosion staged, `NROS_HOME` cached
- [ ] Local patch set (platform/board/index edits) lives on a fork
      branch, rebased per tag
- [ ] Router comes from the ROS 2 install on the host that runs it —
      nothing in the vendored tree provides one

## Related

- [Build as a CMake subdirectory](build-as-subdirectory.md) — cache
  variables, link targets
- [How Integration Works](how-integration-works.md) — per-RTOS build
  hosting
- [Vendor Overlay Board Crate](../porting/vendor-overlay.md) — board
  glue that lives in *your* tree
- [Worked Example — STM32F4 Out of Tree](../porting/stm32f4-out-of-tree.md)
- [Install](installation.md) — the interactive first-time flow this
  page assumes you are automating
