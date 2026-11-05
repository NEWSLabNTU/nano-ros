# Zephyr (west module)

<!-- type: how-to · level: 2 -->

> **Support level:** [`native_sim`](../reference/boards/zephyr-native-sim.md), [`mps2_an385`](../reference/boards/mps2-an385-zephyr.md), [the S32K344 safety island](../reference/boards/s32k344-safety-island.md). Each page states what nano-ros can show for the board (where it runs, how often, and its breadth), generated from the board support registry; [Board Support Levels](../reference/board-support-levels.md) explains the words.

Single-node starter on Zephyr via the in-tree `zephyr/`
west module. nano-ros ships as a Zephyr module — `west` discovers it
from your workspace's `west.yml`, drops in a `prj.conf` Kconfig
surface, and the standard `west build` / `west flash` flow takes
care of the rest.

> Changing this in the tree: see [Internals → in-tree examples](../internals/platform/zephyr-in-tree.html).

> **Just want a working starter?** Clone the
> [`nano-ros-zephyr-example`](https://github.com/NEWSLabNTU/nano-ros-zephyr-example)
> repo (`west init -m …`) — a manifest + zenoh talker app pinned to a tested
> Zephyr, with the same quickstart as below baked in. The steps here explain what
> it does so you can adapt it to your own workspace.

> **Prereqs.** Run `nros setup zephyr --rmw <rmw>` once (see
> [Prerequisites](#prerequisites) below) — it provisions the Zephyr
> west workspace + Zephyr SDK bits, the emulator, and your RMW's host
> daemon into the shared store. No hand-installed Zephyr SDK, `west`,
> or cross-toolchain. `nros build` generates the messages from your ROS 2
> install, so its first build of a workspace needs ROS 2 sourced.
> **Python: 3.10+ on Zephyr 3.7 LTS, but ≥ 3.12 on Zephyr 4.x**
> (4.x's `find_package(Python3)` requires 3.12 — see the version
> matrix below). nano-ros's imported west fragment
> `zephyr/west.yml` is a manifest-only file — it does NOT
> pull Zephyr itself; that has to be in your parent manifest
> (`zephyrproject-rtos/zephyr`).

## Which Zephyr? — 3.7 LTS and 4.x both supported

nano-ros consumes as a module on **both** the **3.7 LTS** line (supported
to Jan 2027; the safety-island default) and the current **4.x** rolling
line. You build against **whatever Zephyr your workspace already pins** —
nano-ros adapts. The two lines differ only in *how* you select an RMW and
apply nano-ros's Zephyr patches:

| Capability | Zephyr 3.7 LTS | Zephyr 4.x |
| --- | --- | --- |
| Min Python | 3.10 | **3.12** (`find_package(Python3)`) |
| RMW selection | the image's `rmw` in `system.toml` | the image's `rmw` in `system.toml` (an app with no `system.toml`: the `-S nros-<rmw>` snippet) |
| nano-ros patches | applied during `nros setup zephyr` provisioning | applied during `nros setup zephyr` provisioning |
| Examples as samples / Twister | — | **`samples:` + Twister** (`sample.nano-ros.*`) |
| zenoh (native_sim) | ✅ build + e2e | ✅ build + e2e |
| cyclonedds (native_sim) | ✅ build + e2e | ✅ build · publish · receive · multicast-join *(stable 2-node run pending a tracked `k_mutex` fix)* |
| xrce | ✅ | build path WIP |

native_sim networking uses **NSOS** (host loopback) on both lines — no
TAP/bridge/root. The copy-out, snippet, patch-apply, and dual-line build
flows are exercised in CI; the lanes are in
[Internals → Zephyr in-tree examples](../internals/platform/zephyr-in-tree.html#ci-lanes).

## Project layout

A nano-ros project on Zephyr is a workspace like on every other platform,
beside the west workspace that holds Zephyr. Your code is node packages; the
bringup's `system.toml` names the board. The Zephyr application is generated:
`nros build` writes it under `build/` and runs `west build` on it.

```text
my_zephyr_ws/                          # the west workspace: Zephyr and its modules
├── .west/
├── manifest/west.yml
├── zephyr/                            # cloned by `west update`
└── modules/
c_ws/                                  # the nano-ros workspace
├── src/
│   ├── c_node/                        # your node package
│   └── c_node_bringup/
│       ├── package.xml
│       └── system.toml                # [image.zephyr] board = "zephyr"
└── build/                             # generated, the Zephyr application among it
```

[Quick start](#quick-start-a-talker-on-native_sim-from-nothing) builds exactly
this. An application that owns its west build and has no bringup (its own
`CMakeLists.txt` and `prj.conf`, with nano-ros added as one more module) is
the subject of its own chapter,
[A Zephyr App You Build with West](zephyr-west-app.md).

## Prerequisites

`nros setup zephyr` provisions what the index declares for the board — the
**Zephyr SDK** (`[board.zephyr] packages = ["zephyr-sdk"]`) and the **RMW
transport sources** (zenoh-pico + mbedtls for zenoh, the cyclonedds fork) —
from a pinned index into `${NROS_HOME:-~/.nros}/sdk`. The Zephyr sources and
modules themselves are a `west` workspace, which you keep the standard Zephyr
way. It provides **no zenoh router**: that is ROS's `rmw_zenohd`
(see [the RMW design](../extending/rmw-design.md)); Cyclone DDS needs no daemon at all.

1. **Get the `nros` CLI** — install a release with `scripts/install.sh`
   ([Installation](../start/install.md)); it lands in `~/.nros/bin`.
2. **Provision the board + RMW**:
   ```bash
   nros setup zephyr --rmw zenoh       # Zephyr SDK + zenoh-pico + mbedtls
   nros setup --source px4-rs          # workspace cargo-load dep
   ```
3. **Point Zephyr at the SDK** if your shell does not already find it —
   `export ZEPHYR_SDK_INSTALL_DIR="$(nros locate tool zephyr-sdk)"`, which
   prints where `nros setup` installed it (or register it via the SDK's
   `setup.sh -c`).
4. **Source ROS 2 for the first build.** `nros build` generates the message
   bindings from your ROS 2 install's definitions, `std_msgs` included, so
   the types match your ROS 2 peers:
   `source /opt/ros/humble/setup.bash` before the first `nros build`,
   `nros run` or `nros configure` of a workspace. That configure records
   the prefix, so later shells need not source ROS.

The zenoh router must be **running** before an example connects
(`ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false'
ros2 run rmw_zenoh_cpp rmw_zenohd`); for xrce, the Micro-XRCE-DDS agent
`nros setup … --rmw xrce` installed; for cyclonedds, nothing.

## Quick start: a talker on native_sim, from nothing

The sections after this one explain each piece; this is the whole path in
order, for a C talker on `native_sim` (a host executable, so no emulator).
Zephyr's own host tools come first — `west` and the build tools
[Zephyr's Getting Started](https://docs.zephyrproject.org/3.7.0/develop/getting_started/index.html)
lists; `nros setup` installs no system packages. On Debian/Ubuntu:

```bash probe=30 variant=native-sim,native-sim-rust,zephyr-twin
sudo apt-get install -y --no-install-recommends git cmake ninja-build gperf \
    device-tree-compiler python3-pip python3-venv xz-utils file wget
pip3 install --user west
export PATH="$HOME/.local/bin:$PATH"
```

Provision the board and RMW (the Zephyr SDK, zenoh-pico, mbedtls):

```bash probe=32 variant=native-sim,native-sim-rust,zephyr-twin
nros setup zephyr --rmw zenoh
export ZEPHYR_SDK_INSTALL_DIR="$(nros locate tool zephyr-sdk)"
```

A west workspace that pins Zephyr 3.7 and fetches only the modules nano-ros's
boards build with (`nros setup zephyr` lists them; `import: true` would fetch
every vendor HAL, ~2.5 GB more):

```bash probe=34 variant=native-sim,native-sim-rust,zephyr-twin
mkdir -p my_zephyr_ws/manifest && cd my_zephyr_ws
cat > manifest/west.yml <<'WEST_YML'
manifest:
  remotes:
    - name: zephyrproject-rtos
      url-base: https://github.com/zephyrproject-rtos
  projects:
    - name: zephyr
      remote: zephyrproject-rtos
      revision: v3.7.0
      import:
        name-allowlist: [cmsis, picolibc, mbedtls, tinycrypt, mcuboot, net-tools]
    # Rust support: only a Rust app builds with it. Pinned to the revision
    # nano-ros's own west.yml pins for the 3.7 line (it has no release tags).
    - name: zephyr-lang-rust
      remote: zephyrproject-rtos
      revision: 404fcefdbab0f2711cd342a1fab9d2eda52a1ba9
      path: modules/lang/rust
  self:
    path: manifest
WEST_YML
west init -l manifest
west update --narrow -o=--depth=1
pip3 install --user -r zephyr/scripts/requirements-base.txt
export ZEPHYR_BASE="$PWD/zephyr"
```

Scaffold a workspace beside the west workspace: a C talker node package and
its bringup, whose image runs on board `zephyr` (Zephyr's `native_sim/native/64`).
`nros build` writes the Zephyr application for that image under `build/` and
runs `west build` on it, from the west workspace `NROS_ZEPHYR_WORKSPACE` names.
You write no `CMakeLists.txt` and no `prj.conf`.

The board package also applies nano-ros's `nros-nsos` snippet, which routes the
image's sockets to the host's (Zephyr's NSOS) instead of a TAP interface.
Without it `native_sim` tries to create one (`eth_posix: Cannot create zeth`),
which needs privileges a container or an ordinary user does not have, and the
image never reaches the router.

The first build generates the message bindings from your ROS 2 install, so
source it in this shell first. It records the ROS 2 prefix in the workspace,
so later builds and runs of it need no sourced shell.

```bash probe=40 variant=native-sim
source /opt/ros/humble/setup.bash
export NROS_ZEPHYR_WORKSPACE="$PWD"
cd ..
nros new c_ws --board zephyr --template talker --lang c --rmw zenoh
cd c_ws
nros build
```

That wrote two packages, the same shape every platform's scaffold has:

```text
c_ws/
└── src/
    ├── c_node/            # the talker: package.xml, CMakeLists.txt, its C source
    └── c_node_bringup/    # package.xml, and system.toml: [image.zephyr] board = "zephyr"
```

Run it against a router on the host's loopback — `native_sim` uses the host's
sockets, and the image dials `tcp/127.0.0.1:7447` unless the bringup's
`system.toml` states a `locator`:

```bash probe=60 probe-run=60 probe-keep variant=native-sim,native-sim-rust
source /opt/ros/humble/setup.bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

```text probe-expect=60 variant=native-sim,native-sim-rust
Started Zenoh router with id
```

With the router running, run the talker:

```bash probe=70 probe-run=120 variant=native-sim
nros run c_node
```

```text probe-expect=70 variant=native-sim
Publishing: 'Hello World: 1'
Publishing: 'Hello World: 2'
```

**An application you already build with west.** A Zephyr application that
keeps its own `CMakeLists.txt` and `prj.conf` and has no bringup is not a
workspace image: it adds the nano-ros module to a plain `west build`, as
[A Zephyr App You Build with West](zephyr-west-app.md) shows.

**The same talker in Rust.** The scaffold is the same with `--lang rust`, and
so is everything `nros build` does with it. Make it beside the west workspace
above.

zephyr-lang-rust needs three things the C app does not: the Rust target the
Zephyr board implies (`x86_64-unknown-none` for native_sim/64 on an x86_64
host), rustup's `clippy` (it lints every Rust app as part of the build), and
clang and libclang, for the bindings it generates from Zephyr's headers.
`nros setup` in the workspace installs the target and `clippy`, and with
`--sudo` the two packages (without it, it prints the install command).
`nros build` refuses a Rust Zephyr image that lacks any of them, naming the
command.

```bash probe=40 variant=native-sim-rust
source /opt/ros/humble/setup.bash
. "$HOME/.cargo/env"     # the cargo `nros setup` installed (Installation)
export NROS_ZEPHYR_WORKSPACE="$PWD"
cd ..
nros new rs_ws --board zephyr --template talker --lang rust --rmw zenoh
cd rs_ws
nros setup --sudo
nros build
```

With the router above running, run it:

```bash probe=70 probe-run=120 variant=native-sim-rust
nros run rs_node
```

```text probe-expect=70 variant=native-sim-rust
Publishing: 'Hello World: 1'
Publishing: 'Hello World: 2'
```

## Configure

nano-ros's Zephyr module ships in the installed SDK — the SDK root (the
release's `share/nano-ros`; `nros locate nano-ros` prints it) holds its
`zephyr/module.yml`. You do not add
nano-ros to your `west.yml`: `nros build` names the module on the
`west build` line it runs (`-DZEPHYR_EXTRA_MODULES=<sdk root>`), and a
`west build` you run yourself passes the same:

```bash
west build -b native_sim/native/64 apps/my_app -- \
    -DZEPHYR_EXTRA_MODULES="$(nros locate nano-ros)"
```

Your `west.yml` pins Zephyr itself (and whatever modules your
application uses) the standard Zephyr way:

```yaml
manifest:
  remotes:
    - name: zephyr
      url-base: https://github.com/zephyrproject-rtos
  projects:
    - name: zephyr
      remote: zephyr
      revision: v3.7.0          # or your chosen 3.7 LTS / 4.x SHA
      path: zephyr
      import: true               # pulls Zephyr's own modules
```

The transport sources the module compiles (zenoh-pico, mbedtls, the
Cyclone DDS fork) come from `nros setup zephyr --rmw <rmw>`, not from
west.

Then nano-ros's half of the configuration goes in the bringup's
`system.toml`, and Zephyr's own half in `prj.conf` fragments (in a workspace,
under the bringup's `boards/<board>/`, as
[Growing your workspace](../build/growing-your-workspace.md#zephyr-images) shows):

```toml
# src/c_node_bringup/system.toml
[system]
name = "c_node_bringup"
rmw = "zenoh"
domain_id = 0

[image.zephyr]
board = "zephyr"            # the concrete board is west's `-b`
rmw = "zenoh"               # zenoh | xrce | cyclonedds
# locator = "tcp/192.0.2.10:7447"
# [image.zephyr.knobs.zenoh]
# max_publishers = 4
```

```
# prj.conf — Zephyr settings only; never a CONFIG_NROS_* knob here
CONFIG_NROS=y               # enables the module; everything else is the image's
CONFIG_NETWORKING=y
CONFIG_NET_IPV4=y
CONFIG_NET_TCP=y
```

The module renders the image into a Kconfig fragment merged after `prj.conf`:
the RMW from `rmw`, the language API from your app, the endpoint from
`locator`, the domain from `domain_id`, and every `env` knob (any symbol in the
[Kconfig reference](../internals/platform/zephyr-in-tree.html#kconfig-reference), spelled without
`CONFIG_`). A `CONFIG_NROS_*` line in `prj.conf` would be overridden by it, so
it is at best dead. Several images in one file — one per RMW — are picked with
`west build … -- -DNROS_IMAGE=<id>`; see
[Configuring an image](#configuring-an-image-systemtoml-not-prjconf).
An app with NO `system.toml` gets no rendering and is configured in its own
`prj.conf` ([A Zephyr App You Build with West](zephyr-west-app.md#configure)).

`CONFIG_NROS=y` activates the shell, which maps Kconfig values to
`NROS_*` CMake cache vars and `add_subdirectory()`s the root
nano-ros CMake. `NanoRos::NanoRos` is linked into your `app`
library transparently.

## Configuring an image: `system.toml`, not `prj.conf`

An image's nano-ros configuration lives in ONE place, the bringup's
`system.toml`. The nano-ros Zephyr module renders
the selected image into a Kconfig fragment and merges it **last**, after every
conf file. So the `prj*.conf` files hold Zephyr's own settings only (networking,
POSIX, stacks, heaps) and **never a `CONFIG_NROS_*` line**: one there is either
dead (the image overrides it) or a second source that can disagree with the
first. A gate (`check-leaf-conf-nros-knobs`) refuses one in the tree.

What the module renders, and from where:

| You write in `system.toml` | The build gets |
|---|---|
| `[image.<id>] rmw = "xrce"` | `CONFIG_NROS_RMW_XRCE=y` |
| (nothing — the package's language) | `CONFIG_NROS_{C,CPP,RUST}_API=y` |
| `locator = "tcp/…"` (zenoh) | `CONFIG_NROS_ZENOH_LOCATOR` |
| `locator = "udp/<host>:<port>"` (XRCE) | `CONFIG_NROS_XRCE_AGENT_ADDR` + `_PORT` |
| `domain_id = 7` (image, `[defaults]` or `[system]`) | `CONFIG_NROS_DOMAIN_ID` |
| `[image.<id>.knobs.zenoh] max_queryables = 12` | `CONFIG_NROS_MAX_QUERYABLES=12` — a registry knob, spelled as a board's `[knobs]` is (see [Configuration](../build/configuration.md#per-image-build-knobs)) |
| `[image.<id>.knobs.xrce] max_subscribers = 0` | `CONFIG_NROS_XRCE_MAX_SUBSCRIBERS=0` — likewise |
| `env = { NROS_ZEPHYR_TASK_STACK_SIZE = "16384" }` | `CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE=16384` — any other option in the reference below, spelled without `CONFIG_` |
| `os_config = ["prj-xrce.conf"]` | that Zephyr-native fragment, added ahead of your own `EXTRA_CONF_FILE` |

An `env` row with no Kconfig symbol is not lost: it reaches the image's cargo
builds as an `[env]` row instead. A row whose type does not fit
its symbol, or that only exists under another RMW, is refused at configure
time, naming the `system.toml` line.

### One image per RMW

A bringup built for several backends states one image per RMW, and
picks one at build time. From
[`examples/zephyr/c/talker/system.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/zephyr/c/talker/system.toml):

```toml
[system]
name = "nros_zephyr_talker_c"
rmw = "zenoh"
domain_id = 0
default_images = ["zephyr_zenoh"]

[image.zephyr_zenoh]
board = "zephyr"
rmw = "zenoh"
os_config = ["prj-zenoh.conf"]

[image.zephyr_xrce]
board = "zephyr"
rmw = "xrce"
os_config = ["prj-xrce.conf"]
locator = "udp/127.0.0.1:2018"

[image.zephyr_xrce.knobs.xrce]
max_subscribers = 0
max_service_servers = 0
max_service_clients = 0

[image.zephyr_cyclonedds]
board = "zephyr"
rmw = "cyclonedds"
os_config = ["prj-cyclonedds.conf"]
```

```bash
nros build                    # the default_images: zephyr_zenoh
nros build zephyr_xrce        # another image — its own west build dir
```

Each image builds into its own directory,
`build/<platform>-<rmw>/<package>__<image>_west`, so switching images never
reuses another image's cache.

To run west yourself on one image, `nros build zephyr_xrce --dry-run` prints
the `west build` line for that image's generated application and its own
build directory.

Precedence, last wins: the conf files, then a 4.x `-S nros-<rmw>` snippet,
then the image's rendering, then any `-DCONFIG_…` on the command line (how a
test harness passes a per-run port). A snippet naming a DIFFERENT RMW than the
image is refused rather than silently overridden; one that agrees is
redundant. An application with no `system.toml` gets no rendering, so for it
the snippets and Kconfig defaults still choose.

### What each RMW's Zephyr-native fragment needs

The image's `os_config` file carries what the backend needs from Zephyr itself:

- **Zenoh** (the Kconfig default): TCP and POSIX threads for zenoh-pico —
  `CONFIG_NET_TCP=y`, `CONFIG_POSIX_API=y`, and pthread pools big enough for
  it ([`examples/zephyr/rust/talker/prj-zenoh.conf`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/zephyr/rust/talker/prj-zenoh.conf) uses
  `CONFIG_MAX_PTHREAD_MUTEX_COUNT=64`, `CONFIG_MAX_PTHREAD_COND_COUNT=16`).
- **XRCE-DDS**: `CONFIG_NET_TCP=y`, on top of the BSD sockets
  (`CONFIG_NET_SOCKETS=y`) every image's shared `prj.conf` enables. The agent
  endpoint is the image's `locator`.
- **Cyclone DDS**: brokerless RTPS, wire-compatible with stock ROS 2
  (`rmw_cyclonedds_cpp`). Its source is C++, so `CONFIG_CPP=y` even for Rust
  callers; it needs a large stack (`CONFIG_MAIN_STACK_SIZE=524288`), pthread
  pools, and `CONFIG_NET_IPV4_IGMP=y` for SPDP multicast. The bool
  prerequisites (thread-local storage, dynamic threads, `NET_TCP`, …) are
  `select`ed by `CONFIG_NROS_RMW_CYCLONEDDS`. Its `ddsrt_malloc` draws on the
  nano-ros platform heap, 1 MiB by default under Cyclone — raise it with
  `[image.<id>.knobs.zephyr] heap_size = …` on the image, not in the conf.

On `native_sim`, add the NSOS host-socket offload
(`CONFIG_NET_SOCKETS_OFFLOAD=y` + `CONFIG_NET_NATIVE_OFFLOADED_SOCKETS=y`) so
discovery uses host BSD sockets instead of zeth/TAP. See
[`examples/zephyr/rust/talker/prj-cyclonedds.conf`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/zephyr/rust/talker/prj-cyclonedds.conf)
for the full fragment.

## Build

If your workspace is a fresh manifest-only dir (no `.west/`), initialise
it first so `west` knows which `west.yml` is the manifest:

```bash
cd my_zephyr_ws
west init -l .                           # one-time; points west at the
                                         # local west.yml in cwd
west update                              # clones Zephyr + its modules into the workspace
```

(If you started from `west init -m <remote>`, both calls above are
already done — go straight to `west build` below.)

### `nros build` on Zephyr: it hands off, it does not take over

On every platform `nros build` generates the image's entry under `build/` and
runs the platform's tool on it. On Zephyr the entry is a Zephyr application
and the tool is `west build`. What stays yours is the **west workspace**:
`west init` / `west update` populate it, it pins your Zephyr, and nano-ros never
writes into it.

So for a Zephyr image `nros build`:

1. writes the **application** for that image under `build/`, from the image's
   board and the bringup's nodes (or uses `src/<image>_entry/`, if you have
   taken it over with [`nros materialize`](#owning-the-application-nros-materialize));
2. resolves **your Zephyr** — `$ZEPHYR_BASE`, else `$NROS_ZEPHYR_WORKSPACE`,
   else an in-tree `zephyr-workspace/`, else a sibling `../nano-ros-workspace`.
   Note it is a *Zephyr* that is needed, not a `.west/` directory: with
   `ZEPHYR_BASE` set, `west build` runs from anywhere, which is what lets the
   application live under your workspace's `build/`, outside the west workspace;
3. runs `west build` from there with the image's overlays
   (`EXTRA_CONF_FILE`, `APPLICATION_CONFIG_DIR`) already applied.

If it cannot find a workspace it does **not** guess. It prints the command and
where to run it:

```text
$ nros build demo_bringup:zephyr
Error: no west workspace found, so `west build` cannot be run for `zephyr`.

Zephyr differs from the other drivers: YOU own the west workspace and the
application, and `west` only runs inside a workspace …

Run this from your west workspace:

    west build -b native_sim/native/64 …/build/…/demo_bringup__zephyr_west

Point nros at your workspace:

    NROS_ZEPHYR_WORKSPACE=<dir>   # the dir containing zephyr/
```

That is deliberate and it is the same boundary `nros setup --system` draws when
it composes an install command and prints it rather than running `sudo` for you:
where the tool is yours, nano-ros hands you the command instead of assuming it
may act.

**`nros build` is the flow this chapter teaches**: it applies the image's overlays, picks the build
directory and, when it finishes, prints the artifact and the Zephyr command
that runs or flashes it. It never becomes a required layer between you and
west — [a Zephyr app you build with west](zephyr-west-app.md) stays the escape hatch,
for an application that owns its west build and has no bringup.

#### Starting from nothing: `nros new`

`nros new rs_ws --board zephyr --template talker` writes a workspace whose
bringup already has a Zephyr image, as in the quick start. Inside a workspace,
a further node package is board- and RMW-agnostic — it names no platform, which
is what lets the same package be deployed to Linux and to a Cortex-M without
an edit:

```bash
nros new talker_pkg --template talker --to demo_bringup
```

It writes the package and, with `--to`, adds the `[[component]]` row to that
bringup. There is no Zephyr application to scaffold: `nros build` and
`nros run` generate it from the image.

#### Owning the application: `nros materialize`

The generated application takes the image's keys (`board`, `os_config`, the
RMW) and the board package's Kconfig and devicetree overlay, and most changes
are one of those. When a change is none of them, and the application's own
`CMakeLists.txt` or `prj.conf` has to change, take it over:

```bash
nros materialize zephyr
```

That writes the generated application into `src/zephyr_entry/` as a package of
yours, and from then on `nros build` builds that copy and does not regenerate
it. Deleting `src/zephyr_entry/` returns the image to generation. It is the
last resort, for the reasons
[Customising an entry](../build/growing-your-workspace.md#customising-an-entry)
gives.

#### The whole flow, end to end

Four commands, run from your workspace. This is the [`examples/workspaces/rust`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/rust)
transcript, verbatim:

```bash
# 0. tell nros where your west workspace is — the directory containing
#    `zephyr/`. On the command line:
#
#      nros build demo_bringup:zephyr --zephyr-workspace /path/to/zephyr-workspace
#
#    or once for the shell, if you would rather not repeat it:
export NROS_ZEPHYR_WORKSPACE=/path/to/zephyr-workspace

# 1. build. This generates the message bindings the workspace's packages
#    depend on, resolves the application, applies the image's overlays and
#    runs `west build` for you.
nros build demo_bringup:zephyr

# 2. start the router ROS itself uses — the one `rmw_zenoh_cpp` ships, so it
#    cannot drift from the RMW your ROS 2 nodes use. Separate terminal; it
#    keeps running. Listens on tcp/127.0.0.1:7447.
ros2 run rmw_zenoh_cpp rmw_zenohd

# 3. run the image — `nros build` printed this, with its own build dir
#    (build/<coordinate>/<bringup>__<image>_west):
west build -d build/zephyr-zenoh/demo_bringup__zephyr_west -t run
#    (on native_sim that runs ./build/zephyr-zenoh/demo_bringup__zephyr_west/zephyr/zephyr.exe)
```

```text
*** Booting Zephyr OS build v3.7.0 ***
<inf> nros_net_wait: Network ready (NSOS — host kernel sockets)
<inf> rust: rustapp: nros: zephyr workspace entry up (2 nodes)
<inf> rust: talker_pkg: Publishing: 'Hello World: 1'
<inf> rust: talker_pkg: Publishing: 'Hello World: 2'
```

and from a ROS 2 shell on the same router, the nodes are ordinary ROS nodes:

```console
$ export RMW_IMPLEMENTATION=rmw_zenoh_cpp
$ ros2 topic echo /chatter --once
data: 'Hello World: 6'
---
```

Step 2 is not optional and its ORDER does not matter, but the **locator** does:
zenoh-pico is a client, so with no router the image gets

```text
<err> rust: nros: zephyr entry — executor open failed: Transport(ConnectionFailed)
```

which names no port. Both sides default to `tcp/127.0.0.1:7447` — the port
`rmw_zenohd` listens on and a `rmw_zenoh_cpp` node connects to. Override with
the image's `locator = "tcp/…"` in `system.toml` and
`ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:<port>"]'` on the
router, and change **both**.

#### What an image declares

The Zephyr images in `examples/workspaces/*` are the worked examples:

```toml
[image.zephyr]
board = "zephyr"
conf  = ["prj-zenoh.conf"]      # the RMW overlay this app requires
```

None of them sets `entry`: `nros build` generates each image's application.

* **`os_config`** names the app's own Kconfig fragments, in order. They are looked
  for beside the board config dir, then **beside the application**, then beside
  the bringup — the app rung matters because a Zephyr app keeps its
  `prj-<rmw>.conf` next to the `CMakeLists.txt` west builds. These entries
  `FATAL_ERROR` without an RMW overlay, so the image has to say which one.
* **`board`** is a nano-ros board id, which nano-ros resolves to the Zephyr
  board target. See below.
* **`entry`** names the application package. Normally leave it out: an entry
  declares the deploy target it serves — its own `system.toml` in Rust,
  `nano_ros_add_executable(... DEPLOY zephyr)` in `CMakeLists.txt` — and one
  package usually claims a given board. Set it when the derivation cannot
  answer, which happens two ways and says which:
  * **several packages claim the board** — deriving would be a coin flip, so
    `nros build` lists the candidates rather than picking one. Reachable for
    C/C++ entries, which are matched by their `DEPLOY` token;
  * **none does** — `nros build` falls back to the Bringup directory, and you
    see a conf fragment "not found" in a path that is not your application's.
    A hand-written application that picks its board with
    `nano_ros_use_board(...)` inside its own `CMakeLists.txt` is this case: the
    `DEPLOY` scan does not read that call. Usually the better fix
    is to delete the application — `nros build` generates one and hands it the
    board package's own Kconfig and devicetree overlay, which is all
    `nano_ros_use_board` added.

#### If you live in west: `west nros`

Everything above assumes you stand in your nano-ros workspace and name the
Zephyr one. If you work the other way round — inside the west workspace — west
already knows where Zephyr is, so it can tell `nros` for you:

```bash
cd ~/zephyr-workspace
west nros build --workspace ~/my_robot demo_bringup:zephyr
```

No `ZEPHYR_BASE`, no `NROS_ZEPHYR_WORKSPACE`, no `--zephyr-workspace`. Every
argument after `nros` is passed straight through, so anything `nros` accepts
works here unchanged.

It picks up automatically in any workspace whose manifest lists nano-ros —
a checkout of this repository, or a mirror of a release you vendor (see
[Pin and mirror a release](../build/vendored-tree.md)). **With the installed SDK alone
there is no `west nros`:** west reads extension commands from a manifest
project, and the installed SDK root is a versioned directory under `~/.nros`
that no `west.yml` can name. Name the Zephyr workspace to `nros` instead,
which is what `west nros` does for you:

```bash
nros build --zephyr-workspace ~/zephyr-workspace demo_bringup:zephyr
```

`west nros` is the only nano-ros west extension; to run an image, use the
stock `west build -t run`. Two things worth knowing:

* **It only exists inside a west workspace.** West loads extension commands
  from the manifest, so from an unrelated directory you get
  `west: unknown command "nros"`. That is why this is an addition rather than a
  replacement: it swaps which of the two trees you have to name, trading the
  Zephyr path (which differs per machine) for your own workspace path (which
  you chose).
* **It never overwrites a `ZEPHYR_BASE` you set yourself** — an exported one is
  a deliberate choice, and this only fills in a missing answer.

#### Which boards you can name

`board` names a board in nano-ros's catalog, and the board's descriptor
states the Zephyr id `west build -b` receives:

```toml
# packages/boards/zephyr/nros-board.toml
names = ["zephyr"]                        # what your images say

[board.zephyr]
west_board = "native_sim/native/64"       # what west is given
```

So an image writes `board = "zephyr"`, never `board = "native_sim/native/64"`:
the Zephyr spelling is the descriptor's to state, and an image that authors it
is refused with the id to write instead.

Nor is it free-form. A board the catalog does not know is refused with the
list of ones it does, because the descriptor supplies more than the `-b` string
— the platform, the toolchain, the entry kind, the declared capabilities. There
is no safe default for those.

**A board is a package — put it in your workspace.** Drop an `nros-board.toml`
beside a package's `package.xml` and it joins the catalog like any other
package in `src/`:

```text
my_robot/src/
├── talker_pkg/
└── my_board/
    ├── package.xml
    └── nros-board.toml
```

```toml
# src/my_board/nros-board.toml
[[board]]
names = ["my-board"]              # what your images say
west_board = "qemu_cortex_m3"     # what west is given
platform = "zephyr"
toolchain = "stable"
platform_feature = "platform-zephyr"
link_kind = "none"
entry_kind = "zephyr-staticlib"
```

```toml
# src/demo_bringup/system.toml
[image.zephyr]
board = "my-board"
```

No environment variable, nothing outside the tree, nothing copied into the
nano-ros checkout. `west_board` is what lets the friendly name be the one your
workspace uses, which is also what the shipped Zephyr descriptors do (under
`[board.zephyr]`). Leave it out only when your board name already *is* the west
board target and you have no other name for it.

For a board shared by **several** workspaces there is no single workspace to
put it in, and `$NROS_EXTRA_BOARD_PATH` still covers that: PATH-style, naming
directories shaped like `packages/boards/`.

`nros new board <name> --for-platform zephyr` scaffolds the descriptor. Note
this is the *nano-ros* descriptor; an out-of-tree Zephyr board **definition**
(its devicetree and `board.yml`) is contributed the Zephyr way, through your
module's `board_root`.

#### Adding your own drivers, modules and boards

**Everything Zephyr lets you extend, you still extend the Zephyr way.**
`nros build` runs a plain `west build`, so the extension points are untouched.
The first row edits the application's `CMakeLists.txt`, so it needs an
application you own ([`nros materialize`](#owning-the-application-nros-materialize));
the rest do not:

| you want | you do | `nros build` involvement |
| --- | --- | --- |
| an out-of-tree module | `list(APPEND ZEPHYR_EXTRA_MODULES <dir>)` before `find_package(Zephyr)` | none |
| a driver in that module | `zephyr_library()` + `zephyr_library_sources()`, gated on your own Kconfig | none |
| its devicetree binding | `dts/bindings/` + `settings: {dts_root: .}` in the module's `zephyr/module.yml` | none |
| an out-of-tree board / SoC / arch / snippet | the matching `*_root` in the same `module.yml` (`board_root`, `soc_root`, `arch_root`, `snippet_root`, `module_ext_root`) | none |
| turning your Kconfig on for one image | a `os_config` fragment on `[image.*]` | passed as `EXTRA_CONF_FILE` |
| a devicetree overlay for one image | an `.overlay` in the same `os_config` list | passed as `EXTRA_DTC_OVERLAY_FILE` |

Only the last two rows are ours, and they are how an IMAGE differs from its
siblings — the module itself is a property of the application, not of one
image, so it belongs in the app's `CMakeLists.txt` where you would have put it
anyway.

`nros build` passes exactly four things to west of its own —
`APPLICATION_CONFIG_DIR`, `EXTRA_CONF_FILE`, `EXTRA_DTC_OVERLAY_FILE`,
`FILE_SUFFIX`, all Zephyr's own variables — and there is deliberately **no
image key for arbitrary `-D` defines**. Something your application always needs
belongs in its `CMakeLists.txt`, which keeps one place to look.

#### Customizing the west command itself

Anything after `--` goes to west. `west build` has two argument zones, and
`nros build` routes each token to the right one using west's own flag list:

```bash
nros build demo_bringup:zephyr -- --pristine        # a west option
nros build demo_bringup:zephyr -- -t run            # build, then run it
nros build demo_bringup:zephyr -- -DMY_OPT=1        # a cmake option
nros build demo_bringup:zephyr -- -p always -DX=1   # both, in one go
```

which becomes, respectively, the west option zone and the cmake zone:

```text
west build -b <board> <app> -p always -- -DEXTRA_CONF_FILE=… -DX=1
```

`nros build --dry-run` prints that line without running it, so you can always
see exactly what west is being asked to do — and copy it if you would rather
drive west yourself.

**Two flags are refused rather than routed**, because the image already decides
them and a build must not be able to disagree with its own declaration:

```console
$ nros build demo_bringup:zephyr -- --sysbuild
Error: `--sysbuild` is decided by the image, not by the command line.
It comes from the presence of a `sysbuild.conf` beside the application.

Change it there, so one build cannot disagree with the declaration it was
resolved from.
```

The other is `-b`/`--board`, which comes from the image's `board`.

Verified by building one. An out-of-tree module with its own Kconfig, a driver
source gated on it, a devicetree overlay declaring a node and a binding shipped
from the module's `dts_root`, added to [`examples/workspaces/rust`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/rust)'s entry and
enabled through the image:

```toml
[image.zephyr]
os_config = ["prj-zenoh.conf", "prj-user-extra.conf", "user-widget.overlay"]
```

```text
*** Booting Zephyr OS build v3.7.0 ***
user_extra: out-of-tree driver init, widget-id=42
<inf> rust: rustapp: nros: zephyr workspace entry up (2 nodes)
<inf> rust: talker_pkg: Publishing: 'Hello World: 1'
```

The driver's `BUILD_ASSERT(DT_NODE_EXISTS(DT_NODELABEL(user_widget)))` is what
makes that a test rather than a screenshot: without the overlay reaching the
build, it fails to compile instead of quietly doing nothing.

A mistake in your module surfaces as Zephyr's own error, unmediated — a wrong
`kconfig:` path in `module.yml` reports

```text
ERROR: "kconfig" key in …/extra_module/zephyr/module.yml has value
"../Kconfig" which does not point to a valid Kconfig file.
```

which is the message you would get from a plain `west build`, because it is a
plain `west build`.

### Driving west yourself

An application with no bringup, built with a plain `west build` and configured
in its own `prj.conf`, is covered in
[A Zephyr App You Build with West](zephyr-west-app.md): adding the module,
the Kconfig settings that replace `system.toml`, messages, and running it.

## Rust applications

Two things differ for a **Rust** app (C/C++ apps skip this section):

1. **The Rust crate's `[lib]` must be named `rustapp`** (`crate-type =
   ["staticlib"]`) — a `zephyr-lang-rust` contract: its `rust_cargo_application()`
   links `librustapp.a`. The Cargo *package* name is free.

2. **Generate the interface crates and the dependency wiring for YOUR
   layout** before the first `west build`, which does not run nano-ros's
   preparation for a Rust app. From your app dir, run (after the
   [Prerequisites](#prerequisites) `nros setup`, which provides the codegen
   toolchain), in a shell that has sourced ROS 2 (the message definitions
   come from it; this configure records the prefix for later runs):

   ```bash
   source /opt/ros/humble/setup.bash
   nros configure
   ```

   This writes `generated/<pkg>/` (the message crates) and a `.cargo/config.toml`
   whose `[patch.crates-io]` points the `nros-*` crates into the nano-ros SDK
   root and the generated interfaces at `generated/*`.
   `--nano-ros-path <dir>` names another root (the SDK ROOT, the directory holding `nano_rosConfig.cmake`).

## Run

`nros build` ends by printing what to type next, read from the finished build
itself — the emulators Zephyr's board declares (`run_<emu>` targets in
`build.ninja`) and its flash runner (`zephyr/runners.yaml`). For an image in
build directory `<dir>` that is one or both of

```text
west build -d <dir> -t run        # an emulated board: native_sim, QEMU
west flash -d <dir>               # a board with a flash runner
```

Both are Zephyr's own commands; nano-ros adds nothing to them. The router
comes first either way:

```bash
# Start the router (ROS's `rmw_zenohd` — nano-ros ships no router of its
# own) on port 7447, the port your app dials by default (an image with no
# `locator` gets "tcp/127.0.0.1:7447"; change either side to match):
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/0.0.0.0:7447"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd &
```

### Run on native_sim

`board = "zephyr"` is Zephyr's `native_sim/native/64`: the image is a host
executable, and its sockets are the host's (NSOS), so it reaches the router on
loopback with no network setup.

```bash
nros build demo_bringup:zephyr
west build -d build/zephyr-zenoh/demo_bringup__zephyr_west -t run
# — the same as running the executable directly:
./build/zephyr-zenoh/demo_bringup__zephyr_west/zephyr/zephyr.exe
```

(A build you drove with plain `west build`, into west's default `build/`:
`west build -t run`, or `./build/zephyr/zephyr.exe`.)

### Run on QEMU (mps2-an385)

The Cortex-M3 witness is Zephyr's `mps2/an385` under QEMU, with Zephyr's own
IP stack on the emulated SMSC911x. An image names it `board =
"mps2-an385-zephyr"`; `west build -t run` boots it under the QEMU the Zephyr
SDK ships (`nros setup zephyr` installed it). In the Rust workspace above, a
second image is that board. `nros setup board` comes first: it applies
nano-ros's patch set to your west workspace, without which zephyr-lang-rust's
cargo command carries none of nano-ros's settings for the board (among them
the C cross compiler the zenoh-pico crate builds with), and adds the board's
Rust target. The image is one more block in the bringup:

```bash probe=80 variant=native-sim-rust
nros setup board mps2-an385-zephyr --zephyr-workspace "$NROS_ZEPHYR_WORKSPACE"
cat >> src/rs_node_bringup/system.toml <<'TOML'

[image.zephyr_m3]
board = "mps2-an385-zephyr"
TOML
nros setup --sudo
nros build zephyr_m3
```

The guest sits on QEMU's user-mode network, and the board package states it:
its `prj.conf` gives the image `10.0.2.15` (gateway `10.0.2.2`) and the locator
`tcp/10.0.2.2:7447`, and `nros build` applies it, so there is nothing to set.
The Slirp gateway `10.0.2.2` is the host, so the router above is reachable.
An image whose router listens elsewhere states its own `locator` in
`system.toml`.

With two images in the bringup, name the one to run:

```bash probe=90 probe-run=180 variant=native-sim-rust
nros run zephyr_m3
```

```text probe-expect=90 variant=native-sim-rust
Publishing: 'Hello World: 1'
Publishing: 'Hello World: 2'
```

### Verify from stock ROS 2

```bash
# The scaffolded talker publishes std_msgs/String (the talker in
# "A Zephyr App You Build with West" publishes std_msgs/Int32).
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
# Talker publishes best-effort; stock `ros2 topic echo` defaults to
# RELIABLE, so the QoS-mismatched echo silently delivers nothing.
# Force best-effort to receive:
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

The Zephyr boot banner runs first, then the talker fires: it prints
`Publishing: 'Hello World: 1'`, `Publishing: 'Hello World: 2'`, ...
(Rust + C + C++ all count from 1, matching the official ROS 2 demo talker).

**Readiness signal.** On `native_sim`, expect the first publish line
(`Publishing: 'Hello World: 1'`) within 5 seconds of boot; on a QEMU
board (`mps2-an385-zephyr`, or a `qemu_cortex_a9` you built yourself)
expect it within ~15 seconds (QEMU cold boot + Zephyr init). If no publish line
in 30 seconds:

1. Confirm `CONFIG_NROS=y` lit up via `west build -t menuconfig`;
   without it the module shell never `add_subdirectory`'s nano-ros.
2. Check `CONFIG_NETWORKING=y`, `CONFIG_NET_IPV4=y`, `CONFIG_NET_TCP=y`
   in `prj.conf` — Zephyr networking is opt-in.
3. Confirm `zenohd` reachable from the simulated network (Slirp
   needs `10.0.2.2:7447` on QEMU; native_sim uses host loopback).
4. See [Troubleshooting — First 10 Minutes](../get-started/first-10-minutes.md).

**Zephyr 4.x build gotchas.**
- `Could NOT find Python3 ... required is at least "3.12"` — 4.x needs
  Python ≥ 3.12. Provision one without sudo (e.g. `uv venv --python 3.12
  .venv312 && uv pip install west -r zephyr/scripts/requirements.txt`) and
  run west through it (`.venv312/bin/python -m west build ...`), so the
  ROS descriptor-codegen subprocess still uses the system ROS Python.
- `attempt to assign the value ... to the undefined symbol ETH_NATIVE_POSIX`
  — that symbol was renamed `ETH_NATIVE_TAP` in 4.x; the version-aware
  NSOS overlay handles it (**contributors:** the in-tree `just zephyr
  build-one` recipe applies it automatically).

## Flash a real board

The boards above are `native_sim` and QEMU machines, and the ARM FVP is a
model ([its chapter](./arm-fvp.md)). The hardware boards nano-ros ships a
package for are the Nucleo-F767ZI (`board = "nucleo-f767zi-zephyr"`), whose
Renode twin boots the same image ([its chapter](../starters/nucleo-f767zi.md)), and the
Raspberry Pi Pico W (`board = "rpi-pico-w-zephyr"`), whose Wokwi twin does;
the Pico W's own chapter is still to come. Any board Zephyr supports is reached with a
board package that names it (`west_board`, see
[Which boards you can name](#which-boards-you-can-name)), and flashed with the
runner Zephyr's own board definition declares:

```bash
# src/my_board/nros-board.toml names west_board = "nucleo_f767zi" (say);
# in system.toml:  [image.nucleo] board = "my-board"
nros build demo_bringup:nucleo
west flash -d build/zephyr-zenoh/demo_bringup__nucleo_west
```

> **Not replayed.** No probe runs this flash line (a CI runner has no board), so it is written from the board's runner, not measured ([tracking issue](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/archived/2176-unprobed-fvp-sitl-flash-lines.md)).

`nros build` prints that `west flash` line when the build's
`zephyr/runners.yaml` names a flash runner. The runner (J-Link, nrfjprog,
OpenOCD, pyOCD, …) and its probe are Zephyr's and your bench's; nano-ros adds
nothing to them. The board must also have a network nano-ros can use.

## Zephyr 4.x: select the RMW with a snippet

On 4.x, nano-ros ships `west` **snippets** so you pick the RMW on the
build line instead of hand-writing the overlay:

```bash
west build -b native_sim/native/64 -S nros-cyclonedds apps/my_app
#                                   ^^^^^^^^^^^^^^^^^^^  nros-zenoh | nros-cyclonedds | nros-xrce
```

The snippet (shipped via the module's `snippet_root`) carries the
RMW-common Kconfig — equivalent to merging `prj-<rmw>.conf`. It is for an
app with NO `system.toml`: with one, the image's `rmw` decides and merges
after the snippet, so a snippet that agrees is redundant and one that names a
different RMW is refused at configure time. The
`prj-<rmw>.conf` / `-DCONF_FILE` path still works (and is the only option
on 3.7).

## nano-ros patches into your workspace

nano-ros needs a few patches to Zephyr's `native_sim` NSOS driver
(`recvmsg`, IPv4-multicast) so the host-loopback-based examples work.
Both supported lines (3.7 LTS and 4.x) take the **same path** —
`nros setup zephyr` reads `zephyr/patches.yml` and applies each patch
against the workspace's Zephyr tree, sha256-checked. No extra step
on your side beyond the provisioning command (already run during
[Prerequisites](#prerequisites)).

To re-apply (e.g. after a `west update` reset the tree):

```bash
nros setup zephyr --rmw zenoh
```

(Earlier nano-ros revisions documented a `west patch apply` flow on
4.x — that required a workspace-side west extension that doesn't ship
with stock Zephyr. Both lines now use the provisioner. Cyclone-DDS-on-Zephyr patches stay baked into the pinned
cyclonedds submodule, not delivered through `patches.yml`.)

## Start from an example

The copy-out talkers under
[`examples/zephyr/<lang>/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/zephyr)
on GitHub, one per language, are **copy-out clean**, with no reference back
into the nano-ros repository. Each is the same project as the
[quick start](#quick-start-a-talker-on-native_sim-from-nothing)'s: one node
package and a bringup with no launch file of its own, whose launch file is
generated. Their node sources are a starting point to copy into your own node
packages.

The other roles are image rows of the language workspaces,
`examples/workspaces/{rust,c,cpp}`, one bringup per behaviour:
`pubsub_bringup`, `service_bringup` and `action_bringup`. A Zephyr image id is
`zephyr_<role>`, plus `_<rmw>` for any RMW other than zenoh, so

```bash
nros run pubsub_bringup:zephyr_listener_xrce
```

builds and runs the XRCE listener. The release does not ship `examples/`;
`nros new <ws> --board zephyr --template <role> --lang <c|cpp|rust>` writes
any role as a workspace of your own.

Cyclone `idlc` and the descriptor-gen scripts are located via the module's
exported cache vars (`NROS_CYCLONE_IDLC`, `NROS_CYCLONE_SCRIPTS_DIR`,
`NROS_CYCLONE_CMAKE_DIR`); message-package dirs come from `NROS_<PKG>_DIR`
env (defaulting to `/opt/ros/humble/share/<pkg>`). No `/opt/ros` or
repo-relative paths are baked into the example.

See the [`zephyr/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/zephyr)
module dir + its [`Kconfig`](https://github.com/NEWSLabNTU/nano-ros/blob/main/zephyr/Kconfig)
for the canonical in-repo surface.

## GitHub source

- Zephyr module shell:
  [`zephyr/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/zephyr)
- Copy-out talkers:
  [`examples/zephyr/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/zephyr/rust/talker),
  [`examples/zephyr/c/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/zephyr/c/talker),
  [`examples/zephyr/cpp/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/zephyr/cpp/talker)
- The other roles: the `zephyr_*` image rows of the language workspaces'
  `pubsub_bringup`, `service_bringup` and `action_bringup`
- Module manifest:
  [`zephyr/module.yml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/zephyr/module.yml)
- Kconfig surface (canonical — every `CONFIG_NROS*`
  symbol the doc cites lives here):
  [`zephyr/Kconfig`](https://github.com/NEWSLabNTU/nano-ros/blob/main/zephyr/Kconfig)
- Patches applied by `nros setup zephyr` (the `west patch` flow is
  retired):
  [`zephyr/patches.yml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/zephyr/patches.yml)

## Next

- Pick a real board (Nordic, NXP, STM32, …): a board package naming its
  `west_board`, then [Flash a real board](#flash-a-real-board).
- Cyclone DDS on Cortex-A/R: see the DDS section of
  [Choosing an RMW Backend](../communicate/choosing-an-rmw.md) for the
  required Kconfig deltas.
- Build nano-ros's own Zephyr examples without west:
  [Zephyr in-tree examples (contributor)](../internals/platform/zephyr-in-tree.html).

## Design background

- [RFC-0122](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md): behaviour bringups, single-node witnesses, and cadence-backed CI.
  The copy-out talker per platform and language, and the paired examples as
  image rows of behaviour bringups.
