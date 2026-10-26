# Integrating nano-ros into a Vendored Tree

This page is for the engineer whose company has its own source tree —
a BSP, possibly a forked RTOS, an existing build system — and needs
nano-ros to be part of it: pinned, reproducible, buildable offline,
upgradable on their schedule. It collects the contracts the other
chapters mention in passing into one place.

nano-ros is distributed as a **release**: one archive holding the `nros`
CLI and the library source it builds against (the *SDK root*). Nothing
is on crates.io and there is no prebuilt library. So "vendoring" nano-ros
means pinning and mirroring a release, not copying its repository into
your tree: your tree holds your packages and their `system.toml`, and
the release supplies everything else.

## Pinning the release

Install the exact release your tree is tested against:

```bash
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh \
    | sh -s -- --version 0.6.0-nros4
export PATH="$HOME/.nros/bin:$PATH"
nros pin 0.6.0-nros4          # record it in the project's nros-toolchain.toml
```

Rules that keep the pin trustworthy:

- **Pin a version, never "latest".** A moving target unships the
  pairing you tested.
- **Move the pin forward only.** `nros pin <newer>` reports the codegen
  delta before it writes anything, so an upgrade that invalidates
  generated bindings warns first.
- **A release's toolchain pins are decisions, not lags.** Its
  `nros-sdk-index.toml` fixes the cross-compilers, the Cyclone DDS that
  ROS ships, a wire-stable zenoh-pico. Take what the release pinned.

## What a fresh machine needs, in order

Four things, and the order matters:

1. **The `nros` CLI**, installed as above. Nothing else works without it.
2. **`~/.nros/bin` on `PATH`** for interactive use. CI can skip this and
   call `$NROS_HOME/bin/nros` by absolute path.
3. **Provisioned toolchains.** `nros setup <board> --rmw <rmw>`
   fetches the cross-compiler, emulator, and SDK sources for exactly
   that pair into the shared store at `${NROS_HOME:-~/.nros}/sdk`.
   `--dry-run` prints the plan first. (The zenoh *router* is
   deliberately not provisioned — it comes from a ROS 2 install,
   `ros2 run rmw_zenoh_cpp rmw_zenohd`.)
4. **`nros sync`.** `nros sync` from your workspace root. It produces
   the message bindings *and* the build settings each `[image.*]`
   implies — see the next section for why this is not optional.

## The Rust side: `nros sync` is mandatory, and nothing it writes is in your source tree

Two things a Rust package needs are **generated, not authored**: the
`generated/` message crates, and every build setting the target implies
— the triple, the runner, the link flags, the resolved `[env]`, and the
`[patch.crates-io]` table that resolves registry-style names
(`nros = { version = "*" }`) to the release's SDK root. You state the
target once, as a board name in the package's `system.toml`, and
`nros sync` derives the rest (RFC-0098):

```toml
[system]
name = "my_app"
rmw  = "cyclonedds"

[image.linux]
board = "linux"
```

`nros sync` writes those settings to `build/<image-id>/nros-cargo.toml`
— one file per `[image.*]` row, under `build/`, never beside your
package — and `nros build` hands it to cargo:

```bash
nros sync
nros build            # or: nros build <image-id> for one image
```

A CI job that drives cargo itself passes the same file:

```bash
cargo build --manifest-path <pkg>/Cargo.toml \
            --config <pkg>/build/<image-id>/nros-cargo.toml
```

This is what makes a tree cheap to move: there is no `.cargo/config.toml`
to copy, no `include` line whose relative path cannot resolve from your
directory, and no table of absolute paths under version control to
invalidate. After relocating the workspace or installing another
release, re-run `nros sync`; it rewrites what is under `build/` and
touches nothing you track. Switching a package to another board is
editing its one `board =` line and syncing again.

A build that runs before sync fails at the first preflight with a line
naming `nros sync`, rather than deep inside cargo.

Message dependencies are path deps that `nros sync` writes for you from
`package.xml`'s `<depend>`s (`std_msgs = { path = "generated/std_msgs",
default-features = false }  # nros-managed`); never registry-name a message
crate — a bare `std_msgs = "*"` can resolve against the public crates.io.

## The CMake side

Two entry points, both source-backed:

- **`find_package(nano_ros REQUIRED)`** — the ament-shaped entry
  (`nano_rosConfig.cmake` at the SDK root). Located via
  `-Dnano_ros_ROOT="$(nros sdk-root)"`, which `nros build` and the
  CMakePresets `nros init` writes pass; a raw `cmake` names it (no
  shell exports it).
- **`add_subdirectory(<sdk-root> nano_ros)`**, the path from `nros sdk-root` — the lower-level
  shape. See [Build as a CMake subdirectory](build-as-subdirectory.md)
  for the cache-variable table.

Corrosion is never fetched at configure time: without it in the SDK store
the configure stops and names `nros setup --tool corrosion`. `nros setup
<board>` installs it too, for every board with a CMake road.

## Air-gapped / mirrored CI

The offline contract, all of it:

1. **Mirror the release asset.** `install.sh` takes
   `NROS_INSTALL_URL=<url>` — a mirror, an air-gapped copy, a test
   server. The asset's `.sha256` must sit beside it and is still
   required.
2. **Mirror or pin the SDK index.** `nros setup` reads the index the
   release ships; `NROS_INDEX_URL=<url>` fetches a newer one from a
   mirror, `nros setup --index <path>` uses a copy you already have, and
   `NROS_OFFLINE=1` forbids the fetch. The index names every asset's
   upstream (`[tool.*]`, `[source.*]` sections); for a full mirror,
   rehost those URLs in your copy.
3. **Pre-install Corrosion**: `nros setup --tool corrosion` (or the
   `nros setup <board>` you already run) stages it into the SDK store,
   where the configure finds it without the network. Verify the
   configure line prints `nano-ros: Corrosion <ver> via <origin>` —
   never infer the version from having run an installer.
4. **Cache `NROS_HOME`.** The CLI, the SDK root and everything
   `nros setup` fetches land under `${NROS_HOME:-~/.nros}`; point
   `NROS_HOME` at a shared or cached location and it is reusable across
   jobs and machines of the same host triple.

## What lives in your tree, and what does not

An application, a message package, a board *overlay crate*, and a
custom transport all live entirely in **your** tree. A **board** the
tooling can see does too: point `NROS_EXTRA_BOARD_PATH` (env, or CMake
cache var for `nano_ros_use_board()`) at a directory of board crates in
your tree, laid out like the SDK's `packages/boards/`. Board keys stay
global (a name under two roots errors).

A new **platform** (your own RTOS port) is the one extension that still
edits nano-ros itself (`nros-platform`'s feature list and type alias,
plus a `cmake/platform/` module). That is a fork of nano-ros, built and
maintained the way [Contributing](../internals/contributing.md) describes
— see [Custom Platform](../porting/custom-platform.md) for the edits.

## Upgrading the pin

```bash
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh \
    | sh -s -- --version <newer>
nros pin <newer>              # reports the codegen delta first
nros setup <board>            # reconcile the SDK store with the new index
nros sync                     # in each consuming workspace
nros build
```

Re-run `nros sync` after every pin move: its output is keyed to the
CLI, and stale sync output after an upgrade produces build errors that
look like missing packages, not like staleness.

## Checklist

- [ ] One release version, installed with `--version`, recorded with
      `nros pin`, moved forward only
- [ ] CI drives `nros build` (or passes `-Dnano_ros_ROOT="$(nros sdk-root)"`
      to a raw `cmake`) — no reliance on an interactive shell
- [ ] `nros sync` runs in CI before any Rust build — before `nros build`,
      and before any cargo line that passes `--config
      build/<image-id>/nros-cargo.toml`; no build setting hand-written
      into a package
- [ ] Air-gapped: release asset mirrored (`NROS_INSTALL_URL`), index
      pinned (`--index` / `NROS_OFFLINE`), Corrosion staged,
      `NROS_HOME` cached
- [ ] Your own boards found through `NROS_EXTRA_BOARD_PATH`, not copied
      into the SDK
- [ ] Router comes from the ROS 2 install on the host that runs it —
      nothing in the release provides one

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
