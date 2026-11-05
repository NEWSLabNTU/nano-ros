# Installation Details

<!-- type: how-to · level: 3 -->

[Installation](../../start/install.md) is the short path. This page is the rest:
what each host package is for, other ways to install, provisioning in depth,
ROS 2 and the RMW daemons, pinning a project to a release, and using nano-ros
from a build that is not a nano-ros workspace.

## Host packages

`cmake` builds the C/C++ quick start and every workspace root. `zstd`
unpacks the release and the prebuilt SDK assets `nros setup` fetches.
`python3-dev` is only used by a from-source build of the launch resolver,
which a release ships prebuilt. The Python packages feed `rosidl_adapter`,
the ROS message-to-IDL step the Cyclone DDS backend uses; empy must stay 3.x.
On Arch, `base-devel` is a package group that covers `pkg-config`, and
`ca-certificates` ships with the base system. Each tool's absence produces an
error naming it.

These are the only packages you install by hand. Per-board OS dependencies
later on are declared in the index and printed for your package manager
(apt, dnf or pacman) by:

```sh
nros setup --system          # what is missing + the exact install command
```

The host *platform* layer is POSIX-clean, but the host *board* is `linux`,
and only Linux is tested. macOS is not supported: the board's CPU-affinity
code uses `cpu_set_t`, which macOS's libc does not define.

## Install options

`install.sh` installs into `~/.nros/sdk/nros/<version>/` and points
`~/.nros/bin/nros` at the newest version installed, so however many versions
accumulate, `nros` means one command. It refuses an asset whose checksum it
cannot verify.

```sh
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh \
    | sh -s -- --version 0.6.0-nros4     # a specific release instead of the newest
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install.sh \
    | sh -s -- --dry-run                 # print what would happen, change nothing
```

`NROS_HOME` moves the install root away from `~/.nros`. The repository's
GitHub Releases page lists every release.

## Provisioning with `nros setup`

`nros setup` is the single command that prepares a machine to build
nano-ros for a given board. The cross-compiler, emulator, RMW host
tools and any SDK sources a board needs are resolved from the index the
release pins and placed in a shared store (`~/.nros/sdk`). You do **not**
install cross-toolchains by hand. It provides no zenoh router — that is
ROS's `rmw_zenohd` — and Cyclone DDS needs no daemon.

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

```bash
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
| `nros setup linux` | host build; zenoh-pico + mbedtls sources (**no router: see [ROS 2 and the Zenoh router](#ros-2-and-the-zenoh-router)**) |
| `nros setup linux --rmw xrce` | host build; the Micro-XRCE-DDS agent |
| `nros setup linux --rmw cyclonedds` | host build; Cyclone DDS runtime + `idlc` |
| `nros setup mps2-an385-freertos` | `arm-none-eabi-gcc`, patched `qemu-system-arm`, FreeRTOS + lwIP sources |
| `nros setup qemu-armv7a-nuttx` | `arm-none-eabi-gcc`, qemu, NuttX sources |
| `nros setup rv-virt-threadx` | `riscv64-*-gcc`, qemu, ThreadX/NetX sources |
| `nros setup threadx-linux` | ThreadX POSIX-sim sources |
| `nros setup zephyr` | the Zephyr west workspace + Zephyr SDK bits |
| `nros setup mps2-an385-baremetal` | bare-metal `arm-none-eabi-gcc` + qemu |

### Provision a whole workspace

Once you have a workspace, you do not need to name its boards. Run
`nros setup` with no board at the workspace root (the directory that
holds `src/`), or pass `--workspace <dir>`. `nros setup --dry-run`
prints the plan without fetching anything.

For example, a Rust workspace with a Linux image and a FreeRTOS image,
both on zenoh:

```bash probe=30 track=installed variant=workspace
nros new two_ws --board linux --template talker --lang rust --rmw zenoh
cd two_ws
nros new --image freertos --board mps2-an385-freertos   # adds [image.freertos]
```

One `nros setup` provisions both boards:

```bash probe=40 track=installed variant=workspace
nros setup
```

Then every image builds. The first build generates the message bindings
from your ROS 2 install, so source it in that shell; the build records the
prefix, and later builds of the workspace need no sourced shell:

```bash probe=50 track=installed variant=workspace
source /opt/ros/humble/setup.bash
nros build --all
```

It reads every `system.toml` in the workspace and provisions each
`[image.*]`'s board and RMW in one plan. For a zenoh image it reports
ROS's `rmw_zenohd`: the path it found, or the apt packages that provide
it. It then lists the OS packages the workspace itself names, which are
its build types' host tools (for example `cmake` for `ament_cmake`) and
any `<depend>` that is a system key, and prints the install command.
Add `--sudo` to run that command. `--check` only reports.

Every `<depend>` must resolve before anything is fetched. A name that is
not a package in the workspace, a message package, a system key, a
package your ROS install provides or a rosdep key stops `nros setup`.
The error names the `package.xml` that declares it, which is the same
check `nros build` makes.

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
> C-only image is not refused; it gets a warning.

See [Supported Boards](../../reference/supported-boards.md) for the full
board list and [`nros` CLI](../../reference/cli.md) for every subcommand.

> **Heads-up before your first example.** Every nano-ros example
> (Linux talker, FreeRTOS talker, …) connects to its **RMW host
> daemon** at startup — a zenoh router for zenoh, the Micro-XRCE-DDS
> agent for xrce. **Cyclone DDS is in-process** — no separate daemon —
> so none of this applies if you ran `nros setup … --rmw cyclonedds`.
>
> For **xrce**, `nros setup … --rmw xrce` installs the agent into the
> SDK store (`~/.nros/sdk/<tool>/<version>/bin/`), and you run it in a
> dedicated terminal before launching any example.
>
> For **zenoh**, the router is **not** provisioned by `nros setup`: it is
> ROS 2's `rmw_zenoh_cpp/rmw_zenohd`, so that nano-ros is tested against
> the router a ROS 2 deployment actually runs
> (see [the RMW design](../../extending/rmw-design.md)). Start it with your ROS environment
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
> [Troubleshooting — First 10 Minutes](../../get-started/first-10-minutes.md).

After provisioning, follow the per-platform starter page (FreeRTOS,
Zephyr, NuttX, …) for the board's build + run steps.

## ROS 2 and the Zenoh router

**Building, yes, once per project. Running two zenoh nodes, yes, or use
Cyclone.** `nros build`, `nros run` and `nros configure` generate the message
bindings from the message definitions of the ROS 2 install you sourced, so the
generated layouts and type hashes are the ones your ROS 2 peers use. A
release does not fall back to a copy of its own: that would pick a ROS 2
distro for you without saying so.

Source ROS 2 before the first build, run or configure of a project:

```bash
source /opt/ros/humble/setup.bash
```

That configure records the prefix (`AMENT_PREFIX_PATH`) in the project, and
every later configure of the same project reuses it, so a later shell, an
IDE or a CI step that has not sourced ROS still builds. A project whose
packages depend on no message package at all configures without ROS 2. With
neither a sourced shell nor a record, configure stops and names the setup
step; see
[Troubleshooting](../../get-started/first-10-minutes.md#a10-ament_prefix_path-is-unset).

Running is where it splits, and the reason is the **router**. zenoh-pico
connects in client mode, so any two-process example needs a zenoh router
— and that router is
`rmw_zenoh_cpp/rmw_zenohd`, which ships with ROS 2. nano-ros does not
vendor one. So:

* **`--rmw cyclonedds` needs no router** — Cyclone is in-process, there
  is no daemon to start.
* **`--rmw zenoh` (the default) needs ROS's router.** Either install
  `ros-<distro>-rmw-zenoh-cpp`, or point `NROS_RMW_ZENOHD` at a
  `rmw_zenohd` you obtained another way.

The same sourced setup is what makes the router findable. nano-ros reads `AMENT_PREFIX_PATH` from that, so a ROS installed anywhere works,
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
| `nros setup` | **No** |
| The first `nros build` / `nros run` / `nros configure` of a project with messages | **Yes**, sourced: the bindings are generated from it |
| Later builds and runs of that project | **No** sourced shell: the first configure recorded the prefix |
| Building any node (Rust / C / C++), any RMW, any board | **Yes**, as above: every talker uses `std_msgs` |
| Running a Cyclone DDS example | **No** router: Cyclone is in-process |
| **Running a multi-process zenoh example** | **Yes** — the router is ROS's `rmw_zenohd` (or set `NROS_RMW_ZENOHD`) |
| Verifying with `ros2 topic echo` / `ros2 node list` | **Yes** |
| Interop against `rmw_zenoh_cpp` / a real ROS 2 graph | **Yes** |

If your distro has no ROS 2 packages (Arch, for example), install ROS 2
from source or from a container image, and source its `setup.bash` (or
your overlay's `install/setup.bash`) the same way: nano-ros reads only
`AMENT_PREFIX_PATH`, wherever the install is.

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

The pin is a file, `nros-toolchain.toml`, beside your project — commit it.
With one in a directory, every `nros` you run there (or below it) is the
pinned release: the `nros` on PATH hands the command to the pinned one,
fetching it with that release's own `install.sh` the first time. A project
pinned to an older release than the one you installed, from nothing:

```bash probe=40 track=installed variant=pinned
mkdir pinned_app && cd pinned_app
printf '[toolchain]\nversion = "0.6.0-nros4"\n' > nros-toolchain.toml
```

Ask for the pin. The release that answers is the pinned one, fetched on
first use, and it names its own manifest:

```bash probe=50 probe-run=600 track=installed variant=pinned
nros pin
```

```text probe-expect=50 variant=pinned
sdk/nros/0.6.0-nros4/share/nros/manifest.toml
```

and everything after it builds with that release. The pinned release is
older than this page, so it is driven with the scaffold spelling it shipped
with, `--workspace`.

```bash probe=60 track=installed variant=pinned
nros new pinned_ws --workspace
cd pinned_ws
nros setup linux --rmw cyclonedds
nros build
```

`nros pin <version>` reports the **codegen delta** between the two
releases before it writes anything, so an upgrade that would invalidate
already-generated bindings warns first; the next `nros build`
regenerates them. After installing a newer release, re-run `nros setup <board>`
— it reconciles the SDK store against the new index, and packages that
are already current are no-ops. See
[versioning](https://github.com/NEWSLabNTU/nano-ros/blob/main/book-internals/src/dev-env/versioning.md)
for how the release number is chosen.

## Consuming nano-ros

### Pattern A: your project is a nano-ros workspace

This is the supported shape, in any language. A workspace is a
directory of packages — node packages plus one bringup package — and
it has **no root build file at all**, neither `CMakeLists.txt` nor
`Cargo.toml`:

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

`nros build` resolves the launch files, generates what the board choice
implies and the entry and build root for each `[image.*]` under `build/`,
then hands off to CMake, cargo or west:

```bash
nros build                     # or: nros build linux
```

`nros new my_robot_ws --board linux --template talker` scaffolds that layout
with one node package — see [First Project](../../tutorial/run-a-talker.md) — and
`nros new <pkg> --to <bringup>` adds the next. A POSIX C++ workspace that already
uses colcon can still be built by `colcon build`; the packages keep
their `package.xml` and their own `CMakeLists.txt`.

A single node is the same shape with one node package; `--lang <rust|c|cpp>`
picks its language (the [First Node](../../tutorial/add-a-second-node.md) chapters).
`system.toml` lives only in the bringup, never beside a `Cargo.toml` or
`CMakeLists.txt`.

A complete working example to read lives at
[`examples/templates/multi-package-workspace/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/multi-package-workspace).

### Pattern B: nano-ros inside an existing CMake tree

For a C/C++ project that is **not** a nano-ros workspace — an existing
CMake tree with its own layout — the platform and backend are cache
variables set before nano-ros is added, because there is no
`system.toml` for the build to read them from. That is the lower-level
shape and it has its own page, with the full variable table:
[Build as a CMake subdirectory](../build/cmake-subdirectory.md). Reach for it
when Pattern A does not fit your tree, not before.

### Rust-only consumers

Nothing of nano-ros is published to crates.io (decision 2026-05-14);
the crates ship as source in the release's SDK root. The full `nros`
crate can't be published because it depends transitively on C/C++
sources (zenoh-pico, mbedtls); `nros-core` isn't carved out for
crates.io either, to avoid a hybrid distribution model with version
drift between a crates.io snapshot and the SDK.

So a Rust package declares registry-style names and lets the build
redirect them: `nros = { version = "*" }` (+ msg crates), resolved into
the SDK root the `nros` on PATH carries (`--nano-ros-path` names a
different one). The `[patch.crates-io]` block it writes goes into the
generated settings file under `build/`, not into your package —
`nros build` hands that file to cargo with `--config`, and a cargo
command you drive yourself passes the same path. Copy the project
anywhere; the next `nros build` re-points it.

A Rust package carries no build settings of its own — no target triple,
no linker flag, no patch table. Its bringup's `system.toml` states the
board and `nros build` derives the rest; see
[`examples/templates/multi-package-workspace/src/pkg_rust_publisher/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/multi-package-workspace/src/pkg_rust_publisher)
for a package in that shape, and
[Integrating into a Vendored Tree](existing-cmake-or-vendor-tree.md) for the CI spelling.

## Migrating from an older nano-ros

If you used nano-ros from a checkout with the old `install-local`
recipe, see
[migration-install-local-removal.md](../../migrating/install-local-removal.md)
for the one-page rewrite.
