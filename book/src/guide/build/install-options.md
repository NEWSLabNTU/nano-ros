# Install options, pinning, working offline

<!-- type: how-to · level: 3 -->

**Needs first:** [Install](../../start/install.md)

[Install](../../start/install.md) is the short path. This page covers a
specific release, a whole workspace, pinning a project, and building with
no network.

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
url=https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh
# a specific release instead of the newest
curl -fsSL "$url" | sh -s -- --version 0.6.0-nros4
# print what would happen, change nothing
curl -fsSL "$url" | sh -s -- --dry-run
```

`NROS_HOME` moves the install root away from `~/.nros`. `nros version` then
prints the CLI, codegen and library versions, and `nros doctor`, run outside a
workspace, checks the install itself.

### One install for several users, or a read-only one

A build never writes into the install, so one install can serve every account
on a machine or sit read-only in a container image. Each project keeps its
own build caches, a few GB; `NROS_BUILD_ROOT` set to a writable directory
shares them.

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

   ```text
   nros setup <board> --rmw <zenoh|xrce|cyclonedds>
   ```

   Packages are prebuilt where the index has a binary for your host, else
   built from source, announced first. A CMake board also gets Corrosion, and
   a host with no Rust gets rustup and the stable toolchain in `~/.cargo`.

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

`rmw_zenohd` reads only `ZENOH_CONFIG_OVERRIDE`, and installs off `PATH`, in
`<prefix>/lib/rmw_zenoh_cpp/`. nano-ros finds it through `NROS_RMW_ZENOHD`,
then the prefixes you sourced, then `$ROS_DISTRO`, never `PATH`: a `zenohd`
there is usually a different zenoh. With no ROS 2 packages for your distro,
install ROS 2 from source or a container and source its `setup.bash`.

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

Only the newest release is supported and fixes are not backported, so move
a pin deliberately; a pinned release keeps installing, as its assets stay
published
([Release support](../../reference/support-status.md#release-support)).

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
before it writes (`--dry-run` writes nothing), the next `nros build`
regenerates the bindings, and `nros setup <board>` brings the store up to
the new index:

```text
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh \
    | sh -s -- --version <newer>
nros pin <newer>
nros setup <board>
nros build
```

Take the toolchain versions the release pinned: its cross compilers, the
Cyclone DDS ROS ships and its zenoh-pico are chosen together.

## Working offline

Prepare on a machine with a network and the same Ubuntu release and
architecture as the offline one, then carry four things across.

### 1. Carry the host packages

Download them in a plain container of the offline machine's Ubuntu release,
with the ROS 2 apt repository added as
[Install](../../start/install.md#install-ros-2-first) adds it. apt downloads
only what it thinks is missing, so an empty package status tells it nothing
is installed. It then carries Install's host packages, ROS 2, and everything
they need:

```sh probe=310 track=installed variant=offline
mkdir debs
docker run --rm -v "$PWD/debs:/debs" ubuntu:24.04 bash -c '
  set -e
  apt-get update
  apt-get install -y curl ca-certificates
  codename="$(. /etc/os-release && echo "$VERSION_CODENAME")"
  base=https://github.com/ros-infrastructure/ros-apt-source/releases/download
  curl -fL -o /tmp/ros2-apt-source.deb \
    "$base/1.3.0/ros2-apt-source_1.3.0.${codename}_all.deb"
  dpkg -i /tmp/ros2-apt-source.deb
  apt-get update
  : > /tmp/nothing-installed
  apt-get install -y --download-only -o Dir::State::status=/tmp/nothing-installed \
    git curl ca-certificates build-essential \
    cmake pkg-config python3 python3-dev zstd \
    python3-catkin-pkg python3-empy python3-lark python3-yaml \
    ros-jazzy-ros-base ros-jazzy-rmw-zenoh-cpp ros-jazzy-example-interfaces
  cp /var/cache/apt/archives/*.deb /debs/'
```

For Humble, use `ubuntu:22.04` and `ros-humble-…`. For Jazzy on 2026-11-07,
this downloaded 606 packages, 227 MB. On the offline machine,
`sudo apt install ./debs/*.deb` installs them all, ROS 2 included.

The container's release must match the offline machine's. apt takes each
package's newest version on the day, and a carried package older than the
offline machine's own copy would be a downgrade, so download after that
machine's last update.

### 2. Mirror the release asset

A release is tagged `nros-<version>` and
its asset is `nros-<os>-<arch>.tar.zst`, with a `.sha256` beside it. The
installer's dry run prints the exact URL for a version and this host:

```sh probe=320 track=installed variant=offline
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh \
    | sh -s -- --version 0.6.0-nros7 --dry-run
```

```text
nros-install: 0.6.0-nros7 for linux-x86_64
nros-install:   from https://github.com/NEWSLabNTU/nano-ros/releases/download/nros-0.6.0-nros7/nros-linux-x86_64.tar.zst
```

Download that file and its `.sha256`, carry both, and install with
`NROS_INSTALL_URL=file:///path/to/nros-linux-x86_64.tar.zst sh install.sh`.

### 3. Pin the SDK index

A release reads the index it ships with, so the
toolchains it provisions are the ones it was built with.
`nros setup --index <path>` uses another copy you have, and
`NROS_INDEX_URL=<url>` fetches one from a mirror; the index
names every asset's upstream, so a full mirror rehosts those URLs in your
copy.

### 4. Provision and prepare while online

Run `nros setup <board>` (it
stages Corrosion too, or `nros setup --tool corrosion` alone), then
`nros configure` in each workspace. It fetches the crates its images need
into cargo's cache and writes everything generated.

### 5. Carry the stores

`nros setup --offline-dirs` prints the directories
to copy, one per line, each with its reason on stderr:

```sh probe=330 track=installed variant=offline
nros setup --offline-dirs
```

```text
/home/you/.nros
/home/you/.rustup
/home/you/.cargo
```

`~/.nros` (or `NROS_HOME`) holds the CLI, the SDK root and every package
`nros setup` fetched. `~/.rustup` and `~/.cargo` are rustup's homes (or
`$RUSTUP_HOME` and `$CARGO_HOME`): the pinned Rust toolchains and cargo's
registry cache. Without them `nros setup` tries to download rustup again.
Copy them to the same paths on the offline machine.

### 6. Build with `NROS_OFFLINE=1`

`nros` fetches nothing and runs cargo with `CARGO_NET_OFFLINE=true`; a build
missing something stops and names it. `nros build --offline` does the same
for one build.

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
