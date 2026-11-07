# Install

<!-- type: how-to · level: 1 -->

This page takes a Linux machine to one that builds nano-ros, in about 15
minutes. You install a few host packages, install the `nros` command, and let
`nros setup` fetch the toolchains for your board. There is no nano-ros
checkout and no system-wide install: everything lands under `~/.nros`.

## Before you start

- **A Linux host.** The book's steps are tested on Ubuntu 22.04 and 24.04,
  and the host packages below also cover Fedora and Arch. macOS is not
  supported.
- **ROS 2 with its Zenoh middleware,** installed before step 1, as
  [Install ROS 2 first](#install-ros-2-first) shows.
- **A network connection** for this install. To install on a machine with no
  network, see [Install options](../guide/build/install-options.md#working-offline).

### Install ROS 2 first

The edition follows your Ubuntu release: **Humble on 22.04, Jazzy on 24.04**
(Humble has no 24.04 packages). If ROS 2 is already installed, skip to
[Steps](#steps).

First add the ROS 2 apt repository. The `ros2-apt-source` package holds its
address and signing key; these lines install release 1.3.0 of it for your
Ubuntu release:

```sh probe=4 track=installed variant=fresh-host
sudo apt-get update
sudo apt-get install -y curl ca-certificates software-properties-common
sudo add-apt-repository -y universe
curl -fL -o /tmp/ros2-apt-source.deb \
    "https://github.com/ros-infrastructure/ros-apt-source/releases/download/1.3.0/ros2-apt-source_1.3.0.$(. /etc/os-release && echo "$VERSION_CODENAME")_all.deb"
sudo dpkg -i /tmp/ros2-apt-source.deb
sudo apt-get update
```

The version is written out, so the step asks GitHub for nothing but the file.
A newer release of `ros2-apt-source` works the same way: replace both
`1.3.0`s with its number.

Then install ROS 2 and its zenoh middleware:

```sh probe=5 track=installed variant=fresh-host
sudo apt-get install -y ros-jazzy-ros-base ros-jazzy-rmw-zenoh-cpp   # humble on 22.04
```

nano-ros generates your project's message bindings from this install, and
the Tutorial watches your nodes with its `ros2` command. You source it
before your first build; the Tutorial shows where.

Install it **first** on Ubuntu: step 1's `python3-catkin-pkg` must then
come from the ROS 2 repository. Taken from Ubuntu's own archive before the
repository is added, it blocks ROS's catkin package, and the ROS install
fails with `trying to overwrite '/usr/lib/python3/dist-packages/catkin_pkg/__init__.py'`.
If that already happened, `sudo apt remove python3-catkin-pkg` and run the
ROS 2 install again.

**For a lab of many seats,** prepare one machine or image with this page and
the first board's `nros setup` done, then copy it to each seat. Thirty seats
then download nothing in class, and none of them depends on the network that
day.

## Steps

### 1. Install the host packages

The host needs git, curl, a C toolchain, CMake, Python 3, `zstd`, and the
Python packages that ROS message generation uses. Run the line for your
distribution.

**Debian or Ubuntu:**

```sh probe=10 distro=debian
sudo apt-get update
sudo apt-get install -y git curl ca-certificates build-essential \
    cmake pkg-config python3 python3-dev zstd \
    python3-catkin-pkg python3-empy python3-lark python3-yaml
```

**Fedora:**

```sh probe=10 distro=fedora
sudo dnf install -y git curl ca-certificates gcc gcc-c++ make \
    cmake pkgconf-pkg-config python3 python3-devel python3-pip zstd
pip install --user catkin_pkg 'empy==3.3.4' lark PyYAML
```

**Arch:**

```sh probe=10 distro=arch
sudo pacman -S --needed git curl base-devel cmake python python-pip zstd
pip install --user --break-system-packages catkin_pkg 'empy==3.3.4' lark PyYAML
```

Fedora and Arch ship no packages for the Python side, so the last line
installs it with pip, into your home directory. The command finishes with no
error, and a package that is already installed is left as it is.
[Host packages](../guide/build/install-options.md#host-packages) says what
each package is for, and how to use a virtual environment instead.

### 2. Install the `nros` command

```sh probe=20 track=installed
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh | sh
export PATH="$HOME/.nros/bin:$PATH"   # install.sh prints this line too
```

The installer downloads the newest release for your host, checks its
checksum, and ends with lines like these:

```text
nros-install: installed 0.6.0-nros8
nros-install:   /home/you/.nros/sdk/nros/0.6.0-nros8
nros-install:   /home/you/.nros/bin/nros -> /home/you/.nros/sdk/nros/0.6.0-nros8/bin/nros

nros-install: next →  nros setup <board>    (e.g. `nros setup linux`)
```

Add the `export PATH=…` line to your shell's startup file, such as
`~/.bashrc`, so that `nros` is on your `PATH` in every new terminal.

### 3. Check the install

```sh probe=22 track=installed
nros version
```

It prints the version of the command, of the code it generates, and of the
libraries it was built with:

```text
nros 0.6.0-nros8
codegen version 18
play_launch d9b79f2bf4505a04b5351c83e916aefaadaf805e
ros-launch-manifest v0.2.0
```

Your numbers are those of the release you installed. The first line is the
release, suffix included: the same `0.6.0-nros8` the installer printed and a
project's pin names. A `nros` built from a source checkout has no release, so
it prints `0.6.0 (checkout build, no release version; …)` instead.

### 4. Provision your board

`nros setup` fetches what a board needs: the cross-compiler, the emulator,
the middleware's host tools and the board's sources, plus a Rust toolchain if
the host has none. You do not install a cross-compiler yourself.

```bash probe=25 track=installed
nros setup <board> --rmw <zenoh|xrce|cyclonedds>
```

Replace `<board>` and the middleware with the ones you run first:

- For the first Tutorial chapter, the emulated Cortex-M3 board:
  `mps2-an385-baremetal` with `zenoh`. The chapter runs this step for you.
- For a talker on this machine: `linux` with `cyclonedds`. Cyclone DDS runs
  inside your process, so it needs no router and no agent.
- For any other board, its name from `nros board list`.

When the board is ready, the last line names it:

```text
nros setup: linux ready; locked in /home/you/.nros/nros-sdk.lock
```

A board whose packages are already present prints
`nros setup: linux — all packages already present` instead.

`nros-sdk.lock` records which version of each tool and source the store
holds, so another machine can install the same set. Run outside a project,
as here, `nros setup` writes the store's own lock, `~/.nros/nros-sdk.lock`.
Run inside a project (a directory with a `system.toml`, a workspace, or a
pinned `nros-toolchain.toml`), it writes the project's lock beside them
instead, and that is the one to commit. The nearest project up the path wins.
To move a version a project's lock records, run `nros update`.

## If it doesn't work

- **`nros: command not found`.** This shell does not have `~/.nros/bin` on its
  `PATH`. Run the `export PATH=…` line from step 2, and add it to your shell's
  startup file.
- **The installer stops with a checksum error.** The download was cut short
  or changed. Run the `curl` line again. The installer never installs a
  release whose checksum it cannot check.
- **`nros setup` says it is building from source.** The release has no
  prebuilt package for your host, so `nros setup` builds it. That is normal
  and takes several minutes. `nros setup <board> --dry-run` shows the plan
  before anything is fetched.
- **Something else.** `nros doctor` checks the install: the command, the store
  under `~/.nros`, and the version a project pins. It names what is wrong and
  the command that fixes it.

[Troubleshooting](../reference/troubleshooting-index.md) covers what goes
wrong in the first build and run, such as an unsourced ROS 2 shell.

## Next

- [Choose your path](choose-your-path.md): the route through the book for
  your work.
- [Run a talker on an emulated board](../tutorial/run-a-talker.md): the first
  Tutorial chapter.
- [Nucleo-F767ZI](../starters/nucleo-f767zi.md): a board's first run, in its
  emulated twin and then on the board.
- [Install options](../guide/build/install-options.md): a specific release,
  pinning a project, working offline, what each package is for, and ROS 2's
  role in detail.
