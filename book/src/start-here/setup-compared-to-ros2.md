# Setup Compared to Standard ROS 2

This page is for ROS 2 users who already know the normal desktop flow:
install a ROS distro, create a workspace, run `rosdep`, build with
`colcon`, and select an RMW at runtime with `RMW_IMPLEMENTATION`.

nano-ros keeps the workspace and package vocabulary, but changes the
setup boundary because it targets embedded and RTOS builds.

## Standard ROS 2 Flow

A typical ROS 2 application starts from a distro install:

```bash
source /opt/ros/humble/setup.bash
mkdir -p ~/ros2_ws/src
cd ~/ros2_ws
rosdep install --from-paths src --ignore-src -y
colcon build
source install/setup.bash
```

The middleware implementation is selected at process startup:

```bash
export RMW_IMPLEMENTATION=rmw_cyclonedds_cpp
ros2 run my_pkg my_node
```

That model assumes shared libraries, a hosted OS, and runtime plugin
loading.

## nano-ros Flow

Where standard ROS 2 installs a distro and resolves system packages with
`rosdep`, nano-ros provisions a **per-board toolchain** with one command.
`nros setup` replaces the distro install + `rosdep`: it ships **prebuilt
toolchains per platform per RMW** — the cross-compiler, emulator, RMW host
daemon, and SDK sources for a board are fetched from a pinned index into a
shared store (`${NROS_HOME:-~/.nros}/sdk`). You do not install cross-toolchains by hand,
and you do not need a ROS distro on the machine.

```bash
# 1. Build the in-tree nros CLI (analogous to installing a ROS distro, Phase 218):
./scripts/bootstrap.sh      # builds packages/cli/target/release/nros
source ./activate.sh        # OR: direnv allow / source ./activate.fish

# 2. Provision a board + RMW (analogous to `rosdep install`):
nros setup native --rmw zenoh

# 3. Build and run something (analogous to `colcon build`):
#    `nros sync` generates the message bindings and the build settings
#    the target implies; both live under gitignored directories, so a
#    fresh clone cannot build without this step.
cd examples/native/rust/talker
nros sync
nros build
./build/native/target/debug/talker
```

Step 3 is the one that maps most directly onto ROS 2, and it maps
better than it used to. `nros sync` + `nros build` plays the part
`colcon build` plays: you declare the system once, in a file, and one
command works out what to compile and drives the language toolchain.
What you declare is `system.toml` — the board, the RMW, the domain,
the components — and nothing in `Cargo.toml` or `CMakeLists.txt`
mentions a target (RFC-0098). Changing target is editing the `board =`
line and re-running `nros sync`.

For embedded targets, name the board instead of `native`; `nros setup`
fetches the matching prebuilt cross-toolchain + emulator + SDK:

```bash
nros setup mps2-an385-freertos --rmw zenoh     # arm-none-eabi-gcc, qemu, FreeRTOS+lwIP
nros setup zephyr            --rmw zenoh     # Zephyr west workspace + SDK bits
nros setup qemu-armv7a-nuttx    --rmw zenoh     # arm-none-eabi-gcc, qemu, NuttX
```

Useful flags: `nros setup --list` (every package + version),
`nros setup <board> --dry-run` (resolve + print the plan, fetch nothing),
`nros setup --licenses` (license-gated packages). See
[Supported Boards](../reference/supported-boards.md) for the board list and
[`nros` CLI](../reference/cli.md) for every subcommand.

> Contributors working on nano-ros itself drive the same index through
> `just` — `just <module> setup` calls `nros setup <board>` under the hood,
> so the provisioned toolchains are identical.

## Choosing platform + RMW

Unlike standard ROS 2, RMW and platform are **compile-time** choices —
there is no runtime `RMW_IMPLEMENTATION` switch on embedded targets
(no `dlopen`). What replaces `RMW_IMPLEMENTATION` is two lines in
`system.toml`, and they are the *only* place the choice appears:

```toml
[system]
name = "my_app"
rmw  = "zenoh"                 # zenoh | cyclonedds | xrce

[image.board]
board = "mps2-an385-freertos"  # the platform follows from the board
```

Then `nros sync && nros build`. There is no cache variable to set and
no Cargo feature to name: the board descriptor already knows the
triple, the linker flags and the platform module, and `nros build`
writes them into the build root it generates. A second `[image.*]` row
is a second target from the same sources — `nros build <image-id>`
picks one.

The reason this looks like `colcon` rather than like a cross-compile
recipe is deliberate: the decision a ROS 2 user makes at runtime,
nano-ros makes in one declarative file, and everything downstream is
derived rather than restated (RFC-0098).

Multi-RMW bridges (one binary, two or more backends) use
`Executor::open_with_rmw("<name>", ...)` + `node_builder.rmw("<name>")`
— see [Cross-backend Bridges](../user-guide/cross-backend-bridges.md).

## What Stays Familiar

- Workspace layout: one source checkout next to your packages.
- Package metadata: downstream packages still use `package.xml`.
- ROS vocabulary: nodes, publishers, subscriptions, services, actions,
  QoS profiles, parameters, and message packages keep ROS-shaped names.
- `colcon build` still works as a consumer-side build for POSIX C++
  workspaces that already use it; `nros build` is the equivalent that
  also reaches embedded targets, driving `cmake`, `cargo`, `west` or
  `idf.py` for you.
- Interop: POSIX nano-ros nodes can communicate with standard ROS 2
  nodes through compatible RMW backends (Zenoh, Cyclone DDS, XRCE).

## What Changes

- **Source-only.** No binary SDK tarball, no crates.io umbrella crate,
  no Arduino zip / ESP-IDF binary component / GitHub Releases artifact.
  The locked policy is `git clone --branch=v<X.Y.Z>` +
  in-tree build.
- **Per-board provisioning, no `rosdep`.** `nros setup <board> --rmw <rmw>`
  is the single setup command. It fetches toolchains (cross-gcc, emulator)
  and SDK sources for exactly that board+RMW into `~/.nros/sdk` — no
  system-wide package install. Most are prebuilt; a few without a seeded
  asset are built from source — `--dry-run` says which on your host. The
  zenoh router is deliberately NOT provisioned: it comes from a ROS 2
  install (`ros2 run rmw_zenoh_cpp rmw_zenohd`, RFC-0075), so the zenoh
  path needs ROS 2 on the router host; xrce and cyclonedds do not.
  (The `just <module> setup` recipes call the same command for contributors.)
- **Compile-time RMW + platform.** Embedded targets can't `dlopen`, so
  the combination is locked in at build time — stated once as
  `[system] rmw` and `[image.<id>] board` in `system.toml`, and derived
  from there into the build.
- **No install prefix.** Phase 140 removed `just install-local`; there
  is no `cmake --install` step for nano-ros itself — consumers pull it
  into their build via `add_subdirectory(<repo-root>)` or
  `find_package(nano_ros)` from the checkout. (Example packages still
  carry their own `install(TARGETS …)` for colcon compatibility.) The
  integration shells under `integrations/<rtos>/` re-export the same
  root CMake under each RTOS's native package manager.
- **The store only grows, so there are verbs to inspect and shrink it.**
  Provisioning never overwrites: a new version lands beside the old one, which
  is what makes going back to a previous toolchain free. `nros store list`
  prints every entry with its size and when it was last read; `nros store gc
  --older-than 90d` proposes what to reclaim and **removes nothing unless you
  add `--delete`**. It never removes an entry a pin names — it reads
  `nros-sdk-index.toml` / `nros-sdk.lock` from the directory you run it in and
  its ancestors, and says which files it consulted. `nros toolchain uninstall
  <version>` removes one nano-ros toolchain under the same rule, and refuses
  when it cannot find a pin file to check against, rather than assuming nothing
  needs it.
- **Generated bindings in-tree.** Message codegen lands under
  `<your-package>/generated/` (or `OUT_DIR` for Cargo builds), not in
  an installed ROS message library.
- **Configuration is build-time on embedded.** Runtime env vars
  (`ROS_DOMAIN_ID`, `NROS_LOCATOR` — legacy alias `ZENOH_LOCATOR`,
  …) work on POSIX; embedded targets bake the same values from their
  `[image.<id>]` row in `system.toml` (`locator`, `ip`, `gateway`,
  `netmask`) and `[system]` (`rmw`, `domain_id`) — one file, and the
  same one in every language — plus Kconfig on Zephyr.

## Next Step

Continue with [Installation](../getting-started/installation.md), then
run the [ROS 2 Interoperability](../getting-started/ros2-interop.md)
example before moving to a platform-specific guide.
