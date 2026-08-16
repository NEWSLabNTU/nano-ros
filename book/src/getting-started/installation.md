# Installation

nano-ros is distributed as **source**, vendored into the consumer's
project tree (git submodule, `west` manifest, ESP-IDF component, etc.)
and built in-tree via `add_subdirectory(nano-ros)`. There is no binary
tarball, no system-wide install step, no `find_package(NanoRos)`, and
no prebuilt `nros` CLI.

## Host prerequisites

Everything toolchain-shaped — rustup, cross-compilers, emulators, the
zenoh router — is provisioned by the steps below. The host itself only
needs the basics: git, curl, a C toolchain, `pkg-config`, python3.

**Debian / Ubuntu:**

```sh probe=10 distro=debian
sudo apt-get update
sudo apt-get install -y git curl ca-certificates build-essential \
    pkg-config python3
```

**Fedora / RHEL:**

```sh probe=10 distro=fedora
sudo dnf install -y git curl ca-certificates gcc gcc-c++ make \
    pkgconf-pkg-config python3
```

**Arch:**

```sh probe=10 distro=arch
sudo pacman -S --needed git curl base-devel python
```

(`base-devel` is a package *group*, and it covers `pkg-config`;
`ca-certificates` ships with the base system.)

**macOS:** `xcode-select --install`, plus `brew install pkg-config`.

These few packages are the only ones you install by hand. Per-board OS
dependencies later on are declared in the index and printed for *your*
package manager — apt, dnf, pacman or brew — by:

```sh
nros setup --system          # what is missing + the exact install command
```

That command needs the `nros` CLI, which is why the basics above come
first: they are what it takes to clone the repo and build the CLI.

## The whole flow, end to end

Four steps take a bare machine to a building project; each is detailed
in a section below.

```sh probe=20
# 1. Pull the source at a pinned version (or `main` for latest):
git clone --branch nros-v0.5.0 https://github.com/NEWSLabNTU/nano-ros.git
cd nano-ros

# 2. Build the `nros` CLI from source (installs rustup if needed):
./scripts/bootstrap.sh

# 3. Wire the workspace env into this (and every future) shell:
source ./activate.sh          # or: direnv allow / source ./activate.fish

# 4. Provision the toolchains + RMW daemon for your target:
nros setup <board> --rmw <zenoh|xrce|cyclonedds>    # e.g. `native`
```

Then point **your project's manifest** at the checkout — Pattern A/B
below for CMake (the `add_subdirectory` / `-DNANO_ROS_ROOT` guard), or
the [Rust-only consumers](#rust-only-consumers) path dependency. See
[Pinning a version](#pinning-a-version) for how the tag relates to
`nros version`.

> **`activate.sh` is the after-install step, NOT a prereq.** Sourcing
> `activate.sh` (or `activate.fish`, or `direnv allow`) only wires the
> workspace env into a new shell — it presumes the `nros` binary
> already exists at `packages/cli/target/release/nros`. The bootstrap
> front door below is what produces that binary; the activate file is
> what makes it findable in every subsequent shell.

> See [build-as-subdirectory.md](build-as-subdirectory.md) for the
> canonical user incantation (4-line `CMakeLists.txt`). This page
> walks the surrounding workspace + per-target setup choices.

## Do I need ROS 2 installed?

**Building, no. Running two zenoh nodes, yes — or use Cyclone.** `nros
sync` generates the message bindings from the interface sources vendored
in `packages/cli/interfaces/`, so nothing about setup, codegen or
compilation needs ROS 2.

Running is where it splits, and the reason is the **router**. zenoh-pico
connects in client mode, so any two-process example needs a zenoh router
— and since [RFC-0075](../design/rmw.md) that router is
`rmw_zenoh_cpp/rmw_zenohd`, which ships with ROS 2. nano-ros no longer
vendors one. So on a host with no ROS 2:

* **`--rmw cyclonedds` works standalone** — Cyclone is in-process, there
  is no daemon to start. This is the ROS-less path.
* **`--rmw zenoh` (the default) has no router.** Either install
  `ros-<distro>-rmw-zenoh-cpp`, or point `NROS_RMW_ZENOHD` at a
  `rmw_zenohd` you obtained another way.

| Task | Needs a ROS 2 install? |
|---|---|
| `nros setup`, `nros sync`, message codegen | **No** — interface sources are vendored |
| Building any node (Rust / C / C++), any RMW | **No** |
| Embedded targets (Zephyr, FreeRTOS, NuttX, ThreadX) | **No** |
| Cyclone DDS backend, build *and* run | **No** — `idlc` comes from the index dist, and there is no daemon |
| **Running a multi-process zenoh example** | **Yes** — the router is ROS's `rmw_zenohd` (or set `NROS_RMW_ZENOHD`) |
| Verifying with `ros2 topic echo` / `ros2 node list` | **Yes** |
| Interop tests against `rmw_zenoh_cpp` / a real ROS 2 graph | **Yes** |
| Contributor lanes that bridge to ROS 2 (`just test-all` interop cells) | **Yes** |

When you source `activate.sh` without ROS 2 present it says so:

```
activate.sh: /opt/ros/humble/setup.bash not found — ROS-dependent recipes
  will fail (interop tests, `ros2` CLI verification). The setup, codegen
  and first-node flows do not need it.
```

That line is informational on a getting-started host. If you *do* want
the ROS 2 side and your distro has no Humble packages (Arch, for
example), a container is the usual answer — nano-ros itself stays on the
host.

## Pattern A: nano-ros lives inside your ROS 2 workspace

The recommended layout is to clone nano-ros into your workspace's
`src/` directory alongside your packages:

```
~/ros2_ws/
├── src/
│   ├── nano-ros/             # <-- this repo
│   ├── pkg_a/                # your package(s)
│   ├── pkg_b/
│   └── …
└── (build/, install/, log/ — generated by colcon)
```

`colcon build` discovers nano-ros + each user package via `package.xml`,
builds them in dependency order, and shares one nano-ros build across
every consuming package. Users never run `cmake` manually — the per-user
package's `CMakeLists.txt` does `add_subdirectory(../nano-ros nano_ros)`
under the hood.

A complete working example lives at
[`examples/templates/multi-package-workspace/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/multi-package-workspace).

## Pattern B: nano-ros as a third-party subdirectory

For C/C++ projects that don't use colcon at all:

```
~/my_project/
├── CMakeLists.txt
├── src/main.c
└── third_party/
    └── nano-ros/             # git submodule
```

The top-level `CMakeLists.txt`:

```cmake
cmake_minimum_required(VERSION 3.22)
project(my_app LANGUAGES C)

# `NROS_RMW` is the user-facing cache var (overridable via
# `-DNROS_RMW=<rmw>`); forward it to `NANO_ROS_RMW`, the var the
# nano-ros add_subdirectory reads. Matches the canonical example
# shape in examples/native/c/talker/CMakeLists.txt.
set(NANO_ROS_PLATFORM posix)
set(NROS_RMW "zenoh" CACHE STRING
    "Active RMW (zenoh|xrce|cyclonedds) — selects the backend linked into my_app.")
set(NANO_ROS_RMW "${NROS_RMW}")
add_subdirectory(third_party/nano-ros nano_ros)

add_executable(my_app src/main.c)
target_link_libraries(my_app PRIVATE NanoRos::NanoRos)
nros_platform_link_app(my_app)
```

`nros_platform_link_app` transitively wires the selected RMW backend
on POSIX — no explicit `nano_ros_link_rmw()` call is needed. That is
the entire consumption shape. See
[build-as-subdirectory.md](build-as-subdirectory.md) for the full
walkthrough.

## Provision your toolchain with `nros setup`

`nros setup` is the single command that prepares a machine to build
nano-ros for a given board. The cross-compiler, emulator, RMW host
daemon, and any SDK sources a board needs are resolved from a pinned
index and placed in a shared store (`~/.nros/sdk`). You do **not**
install cross-toolchains by hand, and you do not need ROS 2 on the
machine.

Packages are **prebuilt where the index has a binary for your host, and
built from source otherwise**. Source builds are normal, not an error —
but they cost real time and disk, so `nros setup` announces them before
it starts:

```
nros setup: 1 package(s) have no prebuilt for linux-x86_64 — BUILDING FROM SOURCE: <name>
  Expect minutes (tens of minutes for a large recipe) and hundreds of MB under …
```

Source recipes build with the **workspace's** pinned Rust channel, so a
recipe carrying its own pin does not make rustup fetch a second
toolchain behind your back. `nros setup <board> --dry-run` prints the
whole plan, prebuilt vs source, without fetching anything — on the
`native` board it is two source packages (`zenoh-pico` and `mbedtls`,
both vendored submodules) and no daemon at all.

### 1. Get the `nros` CLI onto PATH

nano-ros is a **source distribution** — there is no prebuilt `nros`
download. One front door from a fresh checkout (`just` is NOT a
prereq; rustup is installed on demand):

> Sourcing `activate.sh` on a host without `just` prints one line about
> RTOS SDK path defaults (`FREERTOS_DIR`, `NUTTX_DIR`, `IDF_PATH`, …)
> not being loaded. Harmless here — nothing in the native flow reads
> them. Embedded builds and every `just` recipe do need it:
> `cargo install just`.

```sh
git clone https://github.com/NEWSLabNTU/nano-ros.git
cd nano-ros
./scripts/bootstrap.sh
```

Builds the in-tree `nros` CLI **from source** at
`packages/cli/target/release/nros` and leaves it on PATH for this
shell.

Already have cargo? The equivalent by hand (same build, same binary):
```sh
git submodule update --init packages/cli/third-party/play_launch
cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros
export PATH="$PWD/packages/cli/target/release:$PATH"
```

Both produce the per-checkout binary at
`packages/cli/target/release/nros`. **One checkout = one CLI version =
one runtime ABI** — no global install, no `~/.nros/bin` PATH skew
across worktrees.

### 2. Activate the workspace (every subsequent shell)

The bootstrap path you ran in §1 left `nros` on PATH for *that* shell
only. Every new shell needs the workspace env wired in. Pick whichever
fits your shell — all three wire the same env exports + PATH entries
from the Phase 218.C SSoT `activate.sh`:

```bash
direnv allow                  # auto-activates on `cd nano-ros` (recommended)
source ./activate.sh          # bash / zsh, one-shot per shell
source ./activate.fish        # fish, one-shot per shell
```

The activate file also sources `/opt/ros/humble/setup.bash` if
present (required by `nros generate-rust` + cyclonedds codegen +
rmw_zenoh interop tests) and exports `NROS_REPO_DIR`.

### 3. Provision a board (+ RMW)

```bash
nros setup <board> --rmw <zenoh|xrce|cyclonedds>
```

`nros setup` resolves the board's package set **union** the RMW's host
packages and fetches them all — prebuilt cross-toolchain + emulator +
RMW daemon + board SDK sources. `--rmw` defaults to `zenoh`.

| Command | Provisions |
|---|---|
| `nros setup native` | host build; zenoh-pico + mbedtls sources (**no router — see above**) |
| `nros setup native --rmw xrce` | host build; the Micro-XRCE-DDS agent |
| `nros setup native --rmw cyclonedds` | host build; Cyclone DDS runtime + `idlc` |
| `nros setup qemu-arm-freertos` | `arm-none-eabi-gcc`, patched `qemu-system-arm`, FreeRTOS + lwIP sources |
| `nros setup qemu-arm-nuttx` | `arm-none-eabi-gcc`, qemu, NuttX sources |
| `nros setup qemu-riscv64-threadx` | `riscv64-*-gcc`, qemu, ThreadX/NetX sources |
| `nros setup threadx-linux` | ThreadX POSIX-sim sources |
| `nros setup qemu-esp32-baremetal` | the ESP32-C3 QEMU + bare-metal (esp-hal) toolchain bits |
| `nros setup zephyr` | the Zephyr west workspace + Zephyr SDK bits |
| `nros setup mps2-an385` / `stm32f4` / `qemu-arm-baremetal` | bare-metal `arm-none-eabi-gcc` + qemu |

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
> ([RFC-0075](../design/rmw.md)). Start it with the repo helper, which
> resolves it out of `/opt/ros` and configures it:
>
> ```bash
> just native zenohd     # leave running for the whole session
> ```
>
> It is not configured on the command line — `rmw_zenohd` ignores argv
> and reads `ZENOH_CONFIG_OVERRIDE`, which is what the helper sets. To
> pick a different endpoint, set `ZENOH_LOCATOR` before invoking it.
>
> Without a router the talker blocks forever on `Executor::open` with no
> output. Default ports: `tcp/127.0.0.1:7447` on POSIX,
> `tcp/10.0.2.2:7451` on QEMU FreeRTOS (Slirp forwards to host).
> Mismatch = silent hang; see
> [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

After provisioning, follow the per-platform starter page (FreeRTOS,
Zephyr, NuttX, …) for the board's build + run steps.

## Rust-only consumers

nano-ros is source-only — nothing is published to crates.io
(decision 2026-05-14). The full `nros` crate can't be published
because it depends transitively on C/C++ submodules (zenoh-pico,
mbedtls); `nros-core` isn't carved out for crates.io either, to
avoid a hybrid distribution model with version drift between the
crates.io snapshot and in-repo HEAD.

Rust packages consume nano-ros one of two ways:

- **Registry-style names + patch redirection** (what every shipped
  example uses): declare `nros = { version = "*" }` (+ msg crates) and
  let `nros sync` write the `# nros-managed` `[patch.crates-io]` block
  into `.cargo/config.toml`, resolving the names into the checkout
  (`NROS_REPO_DIR` names it). Copy the project anywhere; re-run
  `nros sync` to re-point.
- **Explicit path dependency** on the in-workspace checkout:

```toml
[dependencies]
nros = { path = "src/nano-ros/packages/api/nros",
         default-features = false,
         features = ["std", "rmw-cffi", "platform-posix", "ros-humble"] }
```

Each Rust package carries its own `.cargo/config.toml` patch entries
when needed — see
[`examples/templates/multi-package-workspace/src/pkg_rust_publisher/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/multi-package-workspace/src/pkg_rust_publisher)
for the pattern.

## Pinning a version

Consumers pin the **source tree**, not a package: clone (or submodule)
at a `nros-v<X.Y.Z>` git tag —

```sh
git clone --branch nros-v0.5.0 https://github.com/NEWSLabNTU/nano-ros.git
# or, as a submodule in your project:
git submodule add -b main https://github.com/NEWSLabNTU/nano-ros third_party/nano-ros
git -C third_party/nano-ros checkout nros-v0.5.0
```

The tag names the **bundle version**: the `nros` CLI built from that
tree prints the same number (`nros version` → `nros 0.5.0`; the
`scripts/check-version-lockstep.sh` gate + `bootstrap.sh shell-doctor`
enforce binary↔tree agreement), and the tree's committed
`nros-sdk-index.toml` pins the exact toolchain/SDK versions
`nros setup` will provision for it. One tag therefore fixes the whole
surface: runtime source, CLI behavior, and provisioned toolchains.
Upgrading = checking out a newer tag and re-running
`./scripts/bootstrap.sh` (rebuilds the CLI to match) + `nros setup`
(reconciles the SDK store against the new index; already-current
packages are no-ops). See
[versioning](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/development/versioning.md)
for how the bundle number is chosen.

## Contributor setup (working on nano-ros itself)

Contributors clone the repo and drive everything through `just`. The
`just` setup recipes are thin wrappers over the same `nros setup` index —
`just <module> setup` calls `nros setup <board>` under the hood, so the
toolchains a contributor gets are identical to a user's.

```bash
git clone https://github.com/NEWSLabNTU/nano-ros.git
cd nano-ros
./scripts/bootstrap.sh base   # path A from §1 above; gets rustup + just + nros
source ./activate.sh          # OR: direnv allow / source ./activate.fish
just setup all                # provision every supported board's SDK/toolchain
```

Diagnose missing tools (read-only):

```bash
just doctor tier=all
```

Provision one module:

```bash
just freertos setup           # → nros setup qemu-arm-freertos
just nuttx setup              # → nros setup qemu-arm-nuttx
just threadx_linux setup      # → nros setup threadx-linux
```

## Docker environment

For a containerized environment with QEMU 7.2+:

```bash
just docker build
just docker shell      # Interactive shell with all tools
just docker test-qemu  # Run QEMU tests in container
```

## Migrating from a pre-140 checkout

If you were on a nano-ros version that still had `just install-local`,
see
[migration-install-local-removal.md](../release/migration-install-local-removal.md)
for the one-page rewrite.

## Next Steps

- [First Node — Rust](./first-node-rust.md) — build + run a Rust talker on Linux
- [First Node — C](./first-node-c.md) — build + run a C talker on Linux
- [First Node — C++](./first-node-cpp.md) — build + run a C++ talker on Linux
- [Build as Subdirectory](build-as-subdirectory.md) — CMake consumption walkthrough
- [C API](../reference/c-api.md) — API entry points and CMake integration
- [`examples/templates/multi-package-workspace/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/templates/multi-package-workspace) — full mixed C / C++ / Rust example
