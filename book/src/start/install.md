# Installation

<!-- type: how-to · level: 1 -->

Three steps take a bare Linux machine to one that builds nano-ros: install a
few host packages, install the `nros` CLI, and let `nros setup` provision the
toolchains for your board.

A release carries everything else. The `nros` CLI provisions toolchains,
scaffolds projects, generates message bindings and drives the build. The
library ships inside the release as source, and `nros build` compiles it into
your project. There is no prebuilt library, no system-wide install, and no
nano-ros clone: your project never names a path into nano-ros.

> Working on nano-ros itself? That needs a checkout and its own build of the
> CLI; see
> [Contributing](../internals/orientation/contributing.html#development-setup).

## Host prerequisites

The host needs only the basics: git, curl, a C toolchain, CMake,
`pkg-config`, Python 3, `zstd`, and the Python packages ROS message
generation uses. `nros setup` provides everything toolchain-shaped later.

**Debian / Ubuntu:**

```sh probe=10 distro=debian
sudo apt-get update
sudo apt-get install -y git curl ca-certificates build-essential \
    cmake pkg-config python3 python3-dev zstd \
    python3-catkin-pkg python3-empy python3-lark python3-yaml
```

**Fedora / RHEL:**

```sh probe=10 distro=fedora
sudo dnf install -y git curl ca-certificates gcc gcc-c++ make \
    cmake pkgconf-pkg-config python3 python3-devel python3-pip zstd
```

**Arch:**

```sh probe=10 distro=arch
sudo pacman -S --needed git curl base-devel cmake python python-pip zstd
```

Fedora and Arch ship no rosidl packages, so install the Python side with pip
(in a venv if your Python is externally managed):

```sh
pip install --user catkin_pkg 'empy==3.3.4' lark PyYAML
```

macOS is not supported. [Installation Details](../guide/build/install-options.md#host-packages)
says what each package is for.

## Install the `nros` CLI

```sh probe=20 track=installed
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh | sh
export PATH="$HOME/.nros/bin:$PATH"   # install.sh prints this line too
```

`install.sh` downloads the newest release for your host, checks its sha256,
and installs it under `~/.nros`. Add the `export PATH=…` line to your shell's
startup file. Check the install:

```sh
nros version        # the CLI and library versions
nros doctor         # SDK paths, toolchains, environment
```

## Provision your board

`nros setup` fetches what a board needs: the cross-compiler, the emulator,
the RMW host tools and the board's SDK sources, plus a Rust toolchain if the
host has none. You do not install cross-toolchains by hand.

```bash probe=25 track=installed
nros setup <board> --rmw <zenoh|xrce|cyclonedds>
```

For a first project on Linux, use Cyclone DDS. It runs in-process, so it
needs no router and no agent:

```sh
nros setup linux --rmw cyclonedds
```

`nros board list` names every board. `nros setup <board> --dry-run` prints
what a board pulls without fetching anything. Inside a workspace, a bare
`nros setup` provisions every image it declares.

## Do I need ROS 2 installed?

**Yes, to build.** `nros build`, `nros run` and `nros configure` generate
your project's message bindings from the message definitions of your ROS 2
install, so the types and their hashes match the ROS 2 nodes you talk to.
Every talker uses `std_msgs`, so install ROS 2 (the book uses Humble) and
source it before the first build of a project:

```sh
source /opt/ros/humble/setup.bash
```

That first configure records the ROS 2 prefix in the project, so later
builds and runs of that project work from a shell that has not sourced ROS.
Without a sourced ROS 2 and without that record, the build stops with
`` `AMENT_PREFIX_PATH` is unset and no configure of … has recorded one to
reuse``; [Troubleshooting](../reference/troubleshooting-index.md#a10-ament_prefix_path-is-unset)
has the fix.

ROS 2 also provides:

- the Zenoh router, ROS's `rmw_zenohd`, which two Zenoh nodes meet through
  (Cyclone DDS needs no router);
- `ros2 topic echo` and `ros2 node list`, to watch your nodes;
- the existing ROS 2 graph your nodes interoperate with.

[Installation Details](../guide/build/install-options.md#ros-2-and-the-zenoh-router)
covers the router and the XRCE agent.

## Next steps

- [First Project](../tutorial/run-a-talker.md): scaffold a talker, run it in QEMU,
  and see it in ROS 2.
- [Installation Details](../guide/build/install-options.md): other install options,
  provisioning in depth, pinning a project to a release, and using nano-ros
  from your own build.
- First Node in [Rust](../tutorial/add-a-second-node.md), [C](../tutorial/add-a-second-node-c.md) or
  [C++](../tutorial/add-a-second-node-cpp.md): a single talker on Linux.
