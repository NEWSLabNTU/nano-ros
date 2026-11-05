# Install

<!-- type: how-to · level: 1 -->

This page takes a Linux machine to one that builds nano-ros, in about 15
minutes. You install a few host packages, install the `nros` command, and let
`nros setup` fetch the toolchains for your board. There is no nano-ros
checkout and no system-wide install: everything lands under `~/.nros`.

## Before you start

- **A Linux host.** The book's steps are tested on Ubuntu 22.04, and the
  host packages below also cover Fedora and Arch. macOS is not supported.
- **ROS 2 Humble, with its Zenoh middleware.** Install it from the
  [ROS 2 documentation](https://docs.ros.org/en/humble/Installation.html),
  then add `ros-humble-rmw-zenoh-cpp`. nano-ros generates your project's
  message bindings from this install, and the Tutorial watches your nodes with
  its `ros2` command. You source it before your first build; the
  Tutorial shows where.
- **A network connection** for this install. To install on a machine with no
  network, see [Install options](../guide/build/install-options.md#working-offline).

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
nros-install: installed 0.6.0
nros-install:   /home/you/.nros/sdk/nros/0.6.0
nros-install:   /home/you/.nros/bin/nros -> /home/you/.nros/sdk/nros/0.6.0/bin/nros

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
nros 0.6.0
codegen version 18
play_launch d9b79f2bf4505a04b5351c83e916aefaadaf805e
ros-launch-manifest v0.2.0
```

Your numbers are those of the release you installed.

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
nros setup: linux ready; locked in /home/you/.nros/…
```

A board whose packages are already present prints
`nros setup: linux — all packages already present` instead.

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
