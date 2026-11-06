# Install options, pinning, working offline

<!-- type: how-to · level: 3 -->

**Needs first:** [Install](../../start/install.md)

[Install](../../start/install.md) is the short path. Use this page to install
a specific release, provision a whole workspace, pin a project to a release,
or build on a machine with no network.

A release carries everything a project needs: the `nros` CLI, and the library
as source in its *SDK root*, which `nros build` compiles into your project.
There is no prebuilt library, no system-wide install and no nano-ros clone.
Its `nros-sdk-index.toml` pins every toolchain and SDK version `nros setup`
provisions, so pinning nano-ros is pinning a release.

## Before you start

- Linux. The host board is `linux`; macOS is not supported.
- The host packages [Install](../../start/install.md) lists. Each tool's
  absence produces an error that names it.

## Host packages

`cmake` builds the C and C++ quick start and every generated workspace root.
`zstd` unpacks the release and the prebuilt assets `nros setup` fetches.
`python3-dev` is used only by a from-source build of the launch resolver,
which a release ships prebuilt. The Python packages feed `rosidl_adapter`,
the message-to-IDL step the Cyclone DDS backend uses; empy must stay 3.x.

Fedora and Arch ship no packages for the rosidl Python side, so Install has
them installed with `pip install --user`; Arch's externally managed Python
also needs `--break-system-packages`. To keep them out of your user site,
create a virtual environment (`python -m venv ~/.venvs/nros`), activate it,
run the same `pip install` without `--user`, and activate it in every shell
you build from.

## Install a specific release

`install.sh` installs into `~/.nros/sdk/nros/<version>/` and points
`~/.nros/bin/nros` at the newest version installed. It refuses an asset whose
checksum it cannot verify.

```sh
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh \
    | sh -s -- --version 0.6.0-nros4     # a specific release instead of the newest
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh \
    | sh -s -- --dry-run                 # print what would happen, change nothing
```

`NROS_HOME` moves the install root away from `~/.nros`. `nros version` then
prints the CLI, codegen and library versions, and `nros doctor`, run outside a
workspace, checks the install itself.

## Provisioning with `nros setup`

1. See what a board needs before fetching anything:

   ```bash
   nros setup linux --dry-run
   ```

   ```text
   nros setup: linux (rmw zenoh) needs 3 package(s):
     rust                   already present (rustup, default stable-x86_64-unknown-linux-gnu)
     rust target            no cross target for this board
     …
   (--dry-run: nothing installed)
   ```

2. Provision the board and the RMW's host tools. `--rmw` defaults to
   `zenoh`:

   ```bash
   nros setup <board> --rmw <zenoh|xrce|cyclonedds>
   ```

   Packages are prebuilt where the index has a binary for your host and built
   from source otherwise; `nros setup` announces a source build before it
   starts. For every board with a CMake build it also installs Corrosion. If
   the host has no Rust, it installs rustup into `~/.cargo` with the stable
   toolchain.

### Provision a whole workspace

In a workspace, provision every image at once: run `nros setup` with no board
at the workspace root, or pass `--workspace <dir>`. For a Linux and a FreeRTOS
image:

```bash probe=30 track=installed variant=workspace
nros new two_ws --board linux --template talker --lang rust --rmw zenoh
cd two_ws
nros new --image freertos --board mps2-an385-freertos   # adds [image.freertos]
```

```bash probe=40 track=installed variant=workspace
nros setup
```

It reads every `system.toml`, provisions each image's board and RMW in one
plan, and lists the OS packages the workspace names, with the install command
(`--sudo` runs it, `--check` only reports). Every `<depend>` must resolve
first; one that does not stops it and names its `package.xml`.

Then build every image. The first build generates the message bindings from
your ROS 2 install, so source it in that shell:

```bash probe=50 track=installed variant=workspace
source /opt/ros/humble/setup.bash
nros build --all
```

Other selections: `nros setup --list` lists every package in the index,
`--tool <name>` and `--source <name>` fetch one package, `--licenses` lists
the license-gated ones, and `nros setup --system` prints the OS packages a
board needs for your package manager.

## ROS 2 and the zenoh router

`nros build`, `nros run` and `nros configure` generate the message bindings
from the ROS 2 install you sourced, so the layouts and type hashes match your
ROS 2 peers. The first configure of a project records the prefix
(`AMENT_PREFIX_PATH`), and later shells, IDEs and CI steps reuse it. A
project with no message dependency configures without ROS 2.

Running two zenoh nodes needs a router, and nano-ros uses the one ROS 2
ships, `rmw_zenohd` from `ros-<distro>-rmw-zenoh-cpp`. Cyclone DDS needs no
router, and the XRCE agent comes from `nros setup … --rmw xrce`. Start the
router with ROS 2 sourced:

```bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

`rmw_zenohd` reads no arguments, only `ZENOH_CONFIG_OVERRIDE`. It installs
into `<prefix>/lib/rmw_zenoh_cpp/`, so `command -v rmw_zenohd` printing
nothing is normal. nano-ros finds it through `NROS_RMW_ZENOHD`, then the
prefixes you sourced, then `$ROS_DISTRO`, and never searches `PATH` or globs
`/opt/ros/*`: a `zenohd` elsewhere is usually a different zenoh. On a distro
with no ROS 2 packages, install ROS 2 from source or a container and source
its `setup.bash` the same way.

## Pinning a version

The pin is a file, `nros-toolchain.toml`, beside your project. Commit it.
Every `nros` you run in that directory or below it is then the pinned
release: the `nros` on `PATH` hands the command to it, fetching it with that
release's own `install.sh` the first time.

Write the pin, for a project pinned to an older release than the one you
installed (`nros pin <version>` writes the same file):

```bash probe=40 track=installed variant=pinned
mkdir pinned_app && cd pinned_app
printf '[toolchain]\nversion = "0.6.0-nros4"\n' > nros-toolchain.toml
```

Ask for the pin. The release that answers is the pinned one, fetched on first
use, and it names its own manifest:

```bash probe=50 probe-run=600 track=installed variant=pinned
nros pin
```

```text probe-expect=50 variant=pinned
sdk/nros/0.6.0-nros4/share/nros/manifest.toml
```

With no pin, `nros pin` says so:

```text
pinned: NOTHING — no nros-toolchain.toml at or above …/empty.
    This project floats: `nros build` will write a pin, and until it does, firmware built here is reproducible only by accident.
```

Build with it. The pinned release is older than this page, so it takes the
scaffold spelling it shipped with, `--workspace`:

```bash probe=60 track=installed variant=pinned
nros new pinned_ws --workspace
cd pinned_ws
nros setup linux --rmw cyclonedds
nros build
```

Move the pin forward only. `nros pin <newer>` reports the codegen delta
between the two releases before it writes anything (`--dry-run` writes
nothing), and the next `nros build` regenerates the bindings. Then run
`nros setup <board>` again to bring the SDK store up to the new index:

```bash
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh \
    | sh -s -- --version <newer>
nros pin <newer>
nros setup <board>
nros build
```

Take the toolchain versions the release pinned: its cross compilers, the
Cyclone DDS ROS ships and its zenoh-pico are chosen together.

## Working offline

1. **Mirror the release asset.** `install.sh` takes `NROS_INSTALL_URL=<url>`:
   a mirror, an air-gapped copy or a test server. The asset's `.sha256` must
   sit beside it.
2. **Pin the SDK index.** `nros setup --index <path>` uses a copy you have;
   `NROS_INDEX_URL=<url>` fetches one from a mirror. The index names every
   asset's upstream, so a full mirror rehosts those URLs in your copy.
3. **Stage Corrosion** with `nros setup --tool corrosion`, or the
   `nros setup <board>` you already run.
4. **Prepare each workspace while online.** `nros configure` fetches the
   crates its images need into cargo's cache and writes everything generated.
5. **Carry the stores.** `~/.nros` (or `NROS_HOME`) holds the CLI, the SDK
   root and every package `nros setup` fetched. The Rust toolchain lives in
   `~/.rustup` and `~/.cargo`, with cargo's registry cache; carry those too,
   or `nros setup` tries to download rustup again.
6. **Build with `NROS_OFFLINE=1`.** `nros` then fetches nothing, and runs
   every cargo it starts with `CARGO_NET_OFFLINE=true`. A build that needs
   something missing stops and names it.

## If it doesn't work

- **The first configure stops on `AMENT_PREFIX_PATH`.** Source ROS 2 once
  (`source /opt/ros/humble/setup.bash`) and configure again; see
  [the troubleshooting index](../../reference/troubleshooting-index.md#a10-ament_prefix_path-is-unset).
- **A C++ image is refused for an old compiler.** With no pinned
  `arm-none-eabi-gcc` in the store, the configure uses one from `PATH`, and
  below GCC 11 a C++ image is refused (Ubuntu 22.04's apt package is 10.3).
  Run `nros setup --tool arm-none-eabi-gcc`.
- **A zenoh example hangs at start with no output.** No router is running,
  or it listens on another endpoint. `nros setup` reports where it found
  `rmw_zenohd`, or the apt packages that provide it.
- **`nros doctor` reports a retired store entry.** Remove it with the command
  it prints.

## See also

- [Build inside an existing CMake or vendor tree](existing-cmake-or-vendor-tree.md):
  nano-ros in a tree you already build.
- [Supported boards](../../reference/supported-boards.md) and the
  [`nros` CLI](../../reference/cli.md).
- [Migrating from install-local](../../migrating/from-older-releases.md#move-off-the-install-local-prefix),
  for a project that used nano-ros from a checkout.

## Design background

- [RFC-0075: zenoh router provenance](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0075-zenoh-router-provenance-and-the-unstable-seam.md)
- [RFC-0116: adoption purposes and budgets](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0116-adoption-purposes-and-front-doors.md)
