# Installation

You need two things, and a release carries both:

* the **`nros` CLI**, which provisions toolchains, scaffolds projects,
  generates message bindings and drives the build; and
* the **library**, which is distributed as source and compiled into your
  build. It ships inside the release as its *SDK root* — `nros sdk-root`
  prints where — and `nros build` points your project at it. There is no
  prebuilt library and no system-wide install step.

You do not clone nano-ros, and your project never names a path into it.
Your project is its own directory of packages plus a `system.toml`; the
installed SDK is everything else.

> Working on nano-ros itself? That is a different setup — a checkout and
> its own build of the CLI — and it lives in
> [Contributing](../internals/contributing.md#development-setup).

## Host prerequisites

Everything toolchain-shaped — rustup, cross-compilers, emulators, RMW
host tools — is provisioned by `nros setup` below. The zenoh router is the
exception: it is ROS's `rmw_zenohd` (RFC-0075) and `nros setup` provides
none — see [Do I need ROS 2 installed?](#do-i-need-ros-2-installed); Cyclone
DDS needs no daemon. The host itself only
needs the basics: git, curl, a C toolchain, `pkg-config`, python3.

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

```sh
# Fedora ships no rosidl packages — install the codegen Python deps via pip:
pip install --user catkin_pkg 'empy==3.3.4' lark PyYAML
```

**Arch:**

```sh probe=10 distro=arch
sudo pacman -S --needed git curl base-devel cmake python python-pip zstd
```

```sh
# Arch ships no rosidl packages — install the codegen Python deps
# (use a venv or pipx if your Python is externally managed):
pip install --user catkin_pkg 'empy==3.3.4' lark PyYAML
```

(`base-devel` is a package *group*, and it covers `pkg-config`;
`ca-certificates` ships with the base system.)

(`cmake` builds the C/C++ quick start and every workspace root; `zstd`
unpacks the release and the prebuilt SDK assets `nros setup` fetches;
`python3-dev` is only used by a from-source build of the launch resolver,
which a release ships prebuilt. Each tool's absence produces an error
naming it. The Python packages feed `rosidl_adapter`, the ROS
message-to-IDL step the CycloneDDS backend uses — on Fedora/Arch
install `catkin_pkg`, `empy==3.3.4`, `lark` and `PyYAML` via pip or your
package manager; empy must stay 3.x.)

These few packages are the only ones you install by hand. Per-board OS
dependencies later on are declared in the index and printed for *your*
package manager — apt, dnf or pacman — by:

```sh
nros setup --system          # what is missing + the exact install command
```

That command needs the `nros` CLI, which is why the basics above come
first.

## The whole flow, end to end

Four steps take a bare machine to a building project; each is detailed
in a section below.

```sh
# 1. Install the `nros` CLI (and the SDK it carries):
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh | sh

# 2. Put it on PATH (install.sh prints this line):
export PATH="$HOME/.nros/bin:$PATH"

# 3. Provision the toolchains for your target:
nros setup linux --rmw cyclonedds

# 4. Scaffold a project, then build it:
nros new my_robot --workspace
cd my_robot && nros sync && nros build
```

[First Project](./first-project.md) walks steps 3 and 4 end to end.
Your project says what it deploys to with a board name in its
`system.toml`; it does not name a path, a triple or a backend crate in
any build file. [Consuming nano-ros](#consuming-nano-ros) below has the
shape, and [Pinning a version](#pinning-a-version) how to hold a project
to one release.

## Install the `nros` CLI

```sh probe=20 track=installed
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh | sh
export PATH="$HOME/.nros/bin:$PATH"   # install.sh prints this line too
```

`install.sh` downloads the newest release for your host, verifies its
sha256 (and refuses an asset it cannot verify), installs it into
`~/.nros/sdk/nros/<version>/` and points `~/.nros/bin/nros` at the newest
version installed — so however many versions accumulate, `nros` means
one command. Add the `export PATH=…` line to your shell's startup file so
every new shell finds it.

```sh
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh \
    | sh -s -- --version 0.6.0-nros4     # a specific release instead of the newest
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh \
    | sh -s -- --dry-run                 # print what would happen, change nothing
```

`NROS_HOME` moves the install root away from `~/.nros`. The repository's
GitHub Releases page lists every release.

Check the install:

```sh
nros version        # the CLI and library versions
nros sdk-root       # where the library source this CLI builds against lives
nros doctor         # SDK paths, toolchains, environment
```

## Do I need ROS 2 installed?

**Building, no. Running two zenoh nodes, yes — or use Cyclone.** `nros
sync` generates the message bindings from interface sources the release
bundles, so nothing about setup, codegen or compilation needs ROS 2.

Running is where it splits, and the reason is the **router**. zenoh-pico
connects in client mode, so any two-process example needs a zenoh router
— and since [RFC-0075](../design/rmw.md) that router is
`rmw_zenoh_cpp/rmw_zenohd`, which ships with ROS 2. nano-ros does not
vendor one. So on a host with no ROS 2:

* **`--rmw cyclonedds` works standalone** — Cyclone is in-process, there
  is no daemon to start. This is the ROS-less path.
* **`--rmw zenoh` (the default) has no router.** Either install
  `ros-<distro>-rmw-zenoh-cpp`, or point `NROS_RMW_ZENOHD` at a
  `rmw_zenohd` you obtained another way.

If you do have ROS 2, **source its setup first** — that is what makes the router
findable:

```bash
source /opt/ros/<distro>/setup.bash
```

nano-ros reads `AMENT_PREFIX_PATH` from that, so a ROS installed anywhere works,
including one you built from source or a colcon overlay — it is not assumed to be
under `/opt/ros`.

Do not expect `command -v rmw_zenohd` to print anything: `rmw_zenohd` installs
into `<prefix>/lib/rmw_zenoh_cpp/`, and `setup.bash` puts only `bin/` on `PATH`.
ROS's own way to reach it is `ros2 run rmw_zenoh_cpp rmw_zenohd`. So a silent
`command -v` is normal and not a broken install — the
`ros2 run rmw_zenoh_cpp rmw_zenohd` spelling finds it regardless.

**Only what you name is used.** nano-ros needs the router that ships *with your
`rmw_zenoh_cpp`*: the two share a zenoh build, and a mismatch shows up as
"nothing was delivered" rather than as an error. So the search is
`NROS_RMW_ZENOHD`, then the prefixes you sourced, then `$ROS_DISTRO` — and
nothing else. In particular:

* **`PATH` is not searched.** A `zenohd` there — installed from zenoh's own
  instructions, or left behind by an older nano-ros — is usually a different and
  older zenoh. The machine this was written on had two, at v1.4.0 and 1.7.2,
  against ROS's 1.8.0.
* **`/opt/ros/*` is not globbed.** If you have humble and jazzy installed,
  guessing between them is not better than saying nothing. Source the one you
  want, or set `ROS_DISTRO`.

If you do want a specific binary, name it with `NROS_RMW_ZENOHD`; it is used as
given, with a warning if it is not an `rmw_zenoh_cpp` router.

If you once used an older nano-ros, run `nros doctor`: it reports a retired SDK
store entry that is still installed, `zenohd` included, with the command to
remove it.

| Task | Needs a ROS 2 install? |
|---|---|
| `nros setup`, `nros sync`, message codegen | **No** — interface sources are bundled |
| Building any node (Rust / C / C++), any RMW | **No** |
| Embedded targets (Zephyr, FreeRTOS, NuttX, ThreadX) | **No** |
| Cyclone DDS backend, build *and* run | **No** — `idlc` comes from the index dist, and there is no daemon |
| **Running a multi-process zenoh example** | **Yes** — the router is ROS's `rmw_zenohd` (or set `NROS_RMW_ZENOHD`) |
| Verifying with `ros2 topic echo` / `ros2 node list` | **Yes** |
| Interop against `rmw_zenoh_cpp` / a real ROS 2 graph | **Yes** |

If you *do* want the ROS 2 side and your distro has no Humble packages
(Arch, for example), a container is the usual answer — nano-ros itself
stays on the host.

## Consuming nano-ros

### Pattern A: your project is a nano-ros workspace

This is the supported shape, in any language. A workspace is a
directory of packages — node packages plus one bringup package — and
it has **no root build file at all**, neither `CMakeLists.txt` nor
`Cargo.toml` (RFC-0098 D9):

```
~/my_robot/
├── .colcon_workspace          # marks the root
└── src/
    ├── pkg_a/                 # your node package(s)
    ├── pkg_b/
    └── my_robot_bringup/      # system.toml + launch files, no code
```

The bringup's `system.toml` names the board once per deploy target:

```toml
[system]
name = "my_robot"
rmw  = "cyclonedds"

[image.linux]
board = "linux"
```

`nros sync` resolves the launch files and generates what the board
choice implies; `nros build` generates the entry and the build root for
each `[image.*]` under `build/`, then hands off to CMake, cargo or
west:

```bash
nros sync
nros build                     # or: nros build linux
```

`nros new my_robot --workspace` scaffolds exactly that layout — see
[First Project](./first-project.md). A POSIX C++ workspace that already
uses colcon can still be built by `colcon build`; the packages keep
their `package.xml` and their own `CMakeLists.txt`.

A single package works the same way: `nros new my_node --platform linux
--lang <rust|c|cpp>` scaffolds one directory with its own `system.toml`
(the [First Node](./first-node-rust.md) chapters).

A complete working example to read lives at
[`examples/templates/multi-package-workspace/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/multi-package-workspace).

### Pattern B: nano-ros inside an existing CMake tree

For a C/C++ project that is **not** a nano-ros workspace — an existing
CMake tree with its own layout — the platform and backend are cache
variables set before nano-ros is added, because there is no
`system.toml` for the build to read them from. That is the lower-level
shape and it has its own page, with the full variable table:
[Build as a CMake subdirectory](build-as-subdirectory.md). Reach for it
when Pattern A does not fit your tree, not before.

## Provision your toolchain with `nros setup`

`nros setup` is the single command that prepares a machine to build
nano-ros for a given board. The cross-compiler, emulator, RMW host
tools and any SDK sources a board needs are resolved from the index the
release pins and placed in a shared store (`~/.nros/sdk`). You do **not**
install cross-toolchains by hand. It provides no zenoh router — that is
ROS's `rmw_zenohd` (RFC-0075) — and Cyclone DDS needs no daemon.

Packages are **prebuilt where the index has a binary for your host, and
built from source otherwise**. Source builds are normal, not an error —
but they cost real time and disk, so `nros setup` announces them before
it starts:

```
nros setup: 1 package(s) have no prebuilt for linux-x86_64 — BUILDING FROM SOURCE: <name>
  Expect minutes (tens of minutes for a large recipe) and hundreds of MB under …
```

`nros setup <board> --dry-run` prints the whole plan, prebuilt vs
source, without fetching anything.

```bash probe=25 track=installed
nros setup <board> --rmw <zenoh|xrce|cyclonedds>
```

`nros setup` resolves the board's package set **union** the RMW's host
packages and fetches them all — prebuilt cross-toolchain + emulator +
RMW host tools + board SDK sources (no zenoh router: that is ROS's
`rmw_zenohd`). `--rmw` defaults to `zenoh`. For every
board with a CMake build road (all of them except Zephyr, which builds
with `west`) it also installs the pinned **Corrosion**, the CMake-to-cargo
bridge a C/C++ configure needs: a configure never fetches it, and without
it in the SDK store the configure stops and names this command.

| Command | Provisions |
|---|---|
| `nros setup linux` | host build; zenoh-pico + mbedtls sources (**no router — see above**) |
| `nros setup linux --rmw xrce` | host build; the Micro-XRCE-DDS agent |
| `nros setup linux --rmw cyclonedds` | host build; Cyclone DDS runtime + `idlc` |
| `nros setup mps2-an385-freertos` | `arm-none-eabi-gcc`, patched `qemu-system-arm`, FreeRTOS + lwIP sources |
| `nros setup qemu-armv7a-nuttx` | `arm-none-eabi-gcc`, qemu, NuttX sources |
| `nros setup rv-virt-threadx` | `riscv64-*-gcc`, qemu, ThreadX/NetX sources |
| `nros setup threadx-linux` | ThreadX POSIX-sim sources |
| `nros setup zephyr` | the Zephyr west workspace + Zephyr SDK bits |
| `nros setup mps2-an385-baremetal` | bare-metal `arm-none-eabi-gcc` + qemu |

Every board also needs a **Rust toolchain**, whatever language you write:
the runtime your project links is compiled from source when you first
configure. If the host has neither `rustup` nor `rustc` + `cargo`,
`nros setup` installs rustup (a pinned, checksum-verified installer)
into `~/.cargo` with the stable toolchain as the default — where
`cmake` finds it without any PATH change. A host that already has Rust
is left as it is. For `cargo` in a shell afterwards: `. "$HOME/.cargo/env"`.

Useful flags:

```bash
nros setup --list            # every package in the index + its version
nros setup <board> --dry-run # resolve + print the plan, fetch nothing
nros setup --licenses        # license-gated packages + how to install them
nros setup --tool qemu       # one tool by name
nros setup --source freertos-kernel   # one source package by name
```

Each board's exact package set lives in the index; run
`nros setup <board> --dry-run` to see precisely what a board pulls.

> **A distro cross compiler is a fallback with a floor.** When the store
> has no pinned `arm-none-eabi-gcc` (or `riscv-none-elf-gcc`), the CMake
> configure uses one from `PATH` and says so. Below **GCC 11** a **C++**
> image is refused at configure: GCC 10's newlib libstdc++ has no
> `std::this_thread`, which ordinary ROS 2 C++ uses — Ubuntu 22.04's apt
> `gcc-arm-none-eabi` is 10.3. Install the pin with
> `nros setup --tool arm-none-eabi-gcc`, or point
> `-DNROS_ARM_NONE_EABI_PREFIX=<dir>/bin/arm-none-eabi` at a GCC ≥ 11. A
> C-only image is not refused; it gets a warning (issue 1936).

See [Supported Boards](../reference/supported-boards.md) for the full
board list and [`nros` CLI](../reference/cli.md) for every subcommand.

> **Heads-up before your first example.** Every nano-ros example
> (Linux talker, FreeRTOS talker, …) connects to its **RMW host
> daemon** at startup — a zenoh router for zenoh, the Micro-XRCE-DDS
> agent for xrce. **Cyclone DDS is in-process** — no separate daemon —
> so none of this applies if you ran `nros setup … --rmw cyclonedds`.
>
> For **xrce**, `nros setup … --rmw xrce` installs the agent into the
> nros store (`~/.nros/sdk/<tool>/<version>/bin/`), and you run it in a
> dedicated terminal before launching any example.
>
> For **zenoh**, the router is **not** provisioned by `nros setup`: it is
> ROS 2's `rmw_zenoh_cpp/rmw_zenohd`, so that nano-ros is tested against
> the router a ROS 2 deployment actually runs
> ([RFC-0075](../design/rmw.md)). Start it with your ROS environment
> sourced (leave it running for the whole session):
>
> ```bash
> ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' \
>     ros2 run rmw_zenoh_cpp rmw_zenohd
> ```
>
> It is not configured on the command line — `rmw_zenohd` ignores argv
> and reads `ZENOH_CONFIG_OVERRIDE`, as above. To pick a different
> endpoint, change the port in the `listen/endpoints` entry.
>
> Without a router the talker blocks forever on `Executor::open` with no
> output. Default ports: `tcp/127.0.0.1:7447` on POSIX,
> `tcp/10.0.2.2:7447` on QEMU FreeRTOS (Slirp forwards to host).
> Mismatch = silent hang; see
> [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

After provisioning, follow the per-platform starter page (FreeRTOS,
Zephyr, NuttX, …) for the board's build + run steps.

## Rust-only consumers

Nothing of nano-ros is published to crates.io (decision 2026-05-14);
the crates ship as source in the release's SDK root. The full `nros`
crate can't be published because it depends transitively on C/C++
sources (zenoh-pico, mbedtls); `nros-core` isn't carved out for
crates.io either, to avoid a hybrid distribution model with version
drift between a crates.io snapshot and the SDK.

So a Rust package declares registry-style names and lets `nros sync`
redirect them: `nros = { version = "*" }` (+ msg crates), resolved into
the SDK root the `nros` on PATH carries (`--nano-ros-path` names a
different one). The `[patch.crates-io]` block it writes goes into the
generated settings file under `build/`, not into your package —
`nros build` hands that file to cargo with `--config`, and a cargo
command you drive yourself passes the same path. Copy the project
anywhere; re-run `nros sync` to re-point.

A Rust package carries no build settings of its own — no target triple,
no linker flag, no patch table. It states its board in `system.toml`
and `nros sync` derives the rest (RFC-0098); see
[`examples/templates/multi-package-workspace/src/pkg_rust_publisher/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/multi-package-workspace/src/pkg_rust_publisher)
for a package in that shape, and
[Integrating into a Vendored Tree](vendored-tree.md) for the CI spelling.

## Pinning a version

A release fixes the whole surface at once: the CLI, the library source
in its SDK root, and the `nros-sdk-index.toml` that pins every
toolchain and SDK version `nros setup` provisions. So pinning nano-ros
is pinning a release:

```sh
# install a specific release (several can sit side by side in ~/.nros/sdk/nros/):
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh \
    | sh -s -- --version 0.6.0-nros4

# report this project's pin (nros-toolchain.toml), or move it:
nros pin
nros pin 0.6.0-nros4
```

`nros pin <version>` reports the **codegen delta** between the two
releases before it writes anything, so an upgrade that would invalidate
already-generated bindings warns first; re-run `nros sync` after one
that does. After installing a newer release, re-run `nros setup <board>`
— it reconciles the SDK store against the new index, and packages that
are already current are no-ops. See
[versioning](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/development/versioning.md)
for how the release number is chosen.

## Migrating from an older nano-ros

If you used nano-ros from a checkout with the old `install-local`
recipe, see
[migration-install-local-removal.md](../release/migration-install-local-removal.md)
for the one-page rewrite.

## Next Steps

- [First Project](./first-project.md) — the Quick Start continues here:
  scaffold a two-node workspace and run it (no ROS 2 needed)
- [First Node — Rust](./first-node-rust.md) — build + run a Rust talker on Linux
- [First Node — C](./first-node-c.md) — build + run a C talker on Linux
- [First Node — C++](./first-node-cpp.md) — build + run a C++ talker on Linux
- [Build as Subdirectory](build-as-subdirectory.md) — CMake consumption walkthrough
- [C API](../reference/c-api.md) — API entry points and CMake integration
- [`examples/templates/multi-package-workspace/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/multi-package-workspace) — full mixed C / C++ / Rust example
