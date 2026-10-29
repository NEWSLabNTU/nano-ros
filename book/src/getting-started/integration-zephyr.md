# Zephyr (west module)

Single-node starter on Zephyr via the in-tree `zephyr/`
west module. nano-ros ships as a Zephyr module — `west` discovers it
from your workspace's `west.yml`, drops in a `prj.conf` Kconfig
surface, and the standard `west build` / `west flash` flow takes
care of the rest.

> **Contributor path?** Building nano-ros's own Zephyr examples
> straight from this repository (no west-managed workspace) is
> covered at [Zephyr in-tree examples (contributor)](../internals/zephyr-in-tree.md). The page below is
> the canonical user entry.

> **Just want a working starter?** Clone the
> [`nano-ros-zephyr-example`](https://github.com/NEWSLabNTU/nano-ros-zephyr-example)
> repo (`west init -m …`) — a manifest + zenoh talker app pinned to a tested
> Zephyr, with the same quickstart as below baked in. The steps here explain what
> it does so you can adapt it to your own workspace.

> **Prereqs.** Run `nros setup zephyr --rmw <rmw>` once (see
> [Prerequisites](#prerequisites) below) — it provisions the Zephyr
> west workspace + Zephyr SDK bits, the emulator, and your RMW's host
> daemon into the shared store. No hand-installed Zephyr SDK, `west`,
> or cross-toolchain, and no ROS 2 install required.
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
flows are exercised in CI (contributor lanes `just zephyr ci-both`,
`just zephyr check-copy-out`).

## Project layout

A Zephyr workspace using nano-ros looks like any other Zephyr
project — the **nano-ros module sits beside Zephyr**, your
application sits beside both:

```text
my_zephyr_ws/
├── .west/
├── zephyr/                            # cloned by `west init`
├── modules/
│   └── nano-ros/                      # imported via west.yml
└── apps/
    └── my_app/                        # your application
        ├── CMakeLists.txt
        ├── system.toml                # nano-ros config: RMW, endpoint, knobs
        ├── prj.conf                   # Zephyr's own Kconfig (+ CONFIG_NROS=y)
        ├── west.yml                   # (optional) per-app manifest
        └── src/
            └── main.c                 # nros user code
```

The application `CMakeLists.txt` is a stock Zephyr app — `find_package(Zephyr)`
+ `target_sources`. **No `add_subdirectory(<nano-ros>)`** is needed;
the module shell handles it once `CONFIG_NROS=y` flips on.

A minimal `apps/my_app/` looks like this — the shipped
[`examples/zephyr/c/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/zephyr/c/talker)
quoted directly (only the project/class names are trimmed to `my_app`), so
this page can't drift from the real API surface. It is a **stateful C
component** (a struct + a `configure` function, RFC-0043 / phase-244.C2) —
not a hand-written `main()` — because the Zephyr typed carrier generates
the entry point and calls into the component by identity:

```cmake
# apps/my_app/CMakeLists.txt
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(my_app)

find_package(nano_ros REQUIRED)
find_package(std_msgs REQUIRED)

nano_ros_auto_add_library(talker_lib STATIC src/Talker.c)

nros_components_register_node(talker_lib
    PLUGIN my_app::Talker
    EXECUTABLE talker
    SHAPE configure
    TYPED)
```

```c
// apps/my_app/src/Talker.c
// `talker_configure` creates a publisher on `/chatter` + a timer that
// publishes an Int32 counter each tick, with the C API. `RCLC_C_COMPONENT`
// emits the C-ABI factory/configure the Zephyr typed Entry carrier calls.
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <rclc/rclc.h>
#include <std_msgs/msg/int32.h>

typedef struct {
    rcl_publisher_t pub;
    rcl_timer_t timer;
    int32_t count;
} talker_t;

static void on_tick(rcl_timer_t* timer, int64_t last_call_time) {
    talker_t* self = RCLC_CONTAINER_OF(timer, talker_t, timer);
    (void)last_call_time;
    std_msgs__msg__Int32 msg = {.data = self->count};
    if (rcl_publish(&self->pub, &msg, NULL) == RCL_RET_OK) {
        printf("Published: %d\n", (int)self->count);
    }
    self->count++;
}

static rcl_ret_t talker_configure(rcl_node_t* node, rclc_executor_t* executor, talker_t* self) {
    self->count = 0;
    self->pub = rcl_get_zero_initialized_publisher();
    rcl_ret_t rc = rclc_publisher_init_default(
        &self->pub, node, ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, Int32), "/chatter");
    if (rc != RCL_RET_OK) {
        return rc;
    }
    self->timer = rcl_get_zero_initialized_timer();
    rc = rclc_timer_init_default(&self->timer, NULL, 500000000ULL, on_tick);
    if (rc != RCL_RET_OK) {
        return rc;
    }
    return rclc_executor_add_timer(executor, &self->timer);
}

RCLC_C_COMPONENT(talker_t, talker_configure)
```

See [`examples/templates/zephyr-byo/app/src/Talker.c`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/templates/zephyr-byo/app/src/Talker.c)
for the up-to-date source (this is that BYO starter's talker, only stripped
of its file header comment; the canonical in-tree
[`examples/zephyr/c/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/zephyr/c/talker)
publishes `std_msgs/String` the same way). The component uses the C API only:
`configure` is handed an `rcl_node_t *` and an `rclc_executor_t *` over the
carrier's own node and executor, so it shares both with a C++ component.

Its configuration is in [Configure](#configure) below. You do not choose the
language API: the module renders it from the app, and the typed Zephyr carrier
that drives this component's `configure` is C++, so the `nros-cpp` header
surface is on the app include path even though `Talker.c` itself is C.

## Prerequisites

`nros setup zephyr` provisions what the index declares for the board — the
**Zephyr SDK** (`[board.zephyr] packages = ["zephyr-sdk"]`) and the **RMW
transport sources** (zenoh-pico + mbedtls for zenoh, the cyclonedds fork) —
from a pinned index into `${NROS_HOME:-~/.nros}/sdk`. The Zephyr sources and
modules themselves are a `west` workspace, which you keep the standard Zephyr
way. It provides **no zenoh router**: that is ROS's `rmw_zenohd`
([RFC-0075](../design/rmw.md)); Cyclone DDS needs no daemon at all.

1. **Get the `nros` CLI** — install a release with `scripts/install.sh`
   ([Installation](installation.md)); it lands in `~/.nros/bin`.
2. **Provision the board + RMW**:
   ```bash
   nros setup zephyr --rmw zenoh       # Zephyr SDK + zenoh-pico + mbedtls
   nros setup --source px4-rs          # workspace cargo-load dep
   ```
3. **Point Zephyr at the SDK** if your shell does not already find it —
   `export ZEPHYR_SDK_INSTALL_DIR="$(nros sdk-path zephyr-sdk --require)"` (or
   register it via the SDK's `setup.sh -c`).
4. **Message definitions need no setup.** `find_package(<pkg>)` resolves a
   message package from `NROS_INTERFACE_SEARCH_PATH`, then `AMENT_PREFIX_PATH`,
   then the definitions bundled with the release, so
   `std_msgs` and the other stock packages build with no ROS install.

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

```bash probe=30 variant=native-sim,native-sim-rust
sudo apt-get install -y --no-install-recommends git cmake ninja-build gperf \
    device-tree-compiler python3-pip python3-venv xz-utils file wget
pip3 install --user west
export PATH="$HOME/.local/bin:$PATH"
```

Provision the board and RMW (the Zephyr SDK, zenoh-pico, mbedtls):

```bash probe=32 variant=native-sim,native-sim-rust
nros setup zephyr --rmw zenoh
export ZEPHYR_SDK_INSTALL_DIR="$(nros sdk-path zephyr-sdk --require)"
```

A west workspace that pins Zephyr 3.7 and fetches only the modules nano-ros's
boards build with (`nros setup zephyr` lists them; `import: true` would fetch
every vendor HAL, ~2.5 GB more):

```bash probe=34 variant=native-sim,native-sim-rust
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

Scaffold a C application (a stock Zephyr app: `find_package(Zephyr)`, a typed
talker component, `CONFIG_NROS=y` in `prj.conf`) and build it, naming the
nano-ros module on the line, plus the `native_sim` network overlay the SDK
ships: it routes the app's sockets to the host's (Zephyr's NSOS) instead of a
TAP interface. Without it `native_sim` tries to create one (`eth_posix: Cannot
create zeth`), which needs privileges a container or an ordinary user does not
have, and the image never reaches the router:

```bash probe=40 variant=native-sim
nros new my_app --platform zephyr --lang c
west build -b native_sim/native/64 my_app -- -DZEPHYR_EXTRA_MODULES="$(nros sdk-root)" \
    -DEXTRA_CONF_FILE="$(nros sdk-root)/cmake/zephyr/native-sim-line-3.7.conf"
```

Run it against a router on the host's loopback — `native_sim` uses the host's
sockets, and the image dials `tcp/127.0.0.1:7447` unless `system.toml` states a
`locator`:

```bash probe=60 probe-run=60 probe-keep variant=native-sim,native-sim-rust
source /opt/ros/humble/setup.bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

```text probe-expect=60 variant=native-sim,native-sim-rust
Started Zenoh router with id
```

```bash probe=70 probe-run=120 variant=native-sim
./build/zephyr/zephyr.exe
```

```text probe-expect=70 variant=native-sim
Publishing: 'Hello World: 1'
Publishing: 'Hello World: 2'
...
```

**The same talker in Rust.** A Rust image on Zephyr is a workspace: a node
package plus a Zephyr entry, which `nros new entry` scaffolds (see
[The entry: `nros new entry`](#the-entry-nros-new-entry)). From the west
workspace above, beside it:

zephyr-lang-rust compiles the app for the triple the Zephyr board implies,
`x86_64-unknown-none` for native_sim/64, and `nros setup zephyr` does not
install it yet
([issue 2151](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/2151-zephyr-rust-target-not-provisioned.md)):

```bash probe=40 variant=native-sim-rust
rustup target add x86_64-unknown-none
export NROS_ZEPHYR_WORKSPACE="$PWD"
cd ..
nros new rs_ws --workspace --lang rust --rmw zenoh
cd rs_ws
nros new entry zephyr_entry --platform zephyr
nros sync
nros build demo_bringup:zephyr_entry
```

With the router above running, run the image `nros build` wrote (it prints
this path):

```bash probe=70 probe-run=120 variant=native-sim-rust
./build/zephyr-zenoh/demo_bringup__zephyr_entry_west/zephyr/zephyr.exe
```

The workspace's talker and listener run in the one image:

```text probe-expect=70 variant=native-sim-rust
Publishing: 0
I heard: 0
```

## Configure

nano-ros's Zephyr module ships in the installed SDK — the directory
`nros sdk-root` prints holds its `zephyr/module.yml`. You do not add
nano-ros to your `west.yml`: `nros build` names the module on the
`west build` line it runs (`-DZEPHYR_EXTRA_MODULES=<sdk root>`), and a
`west build` you run yourself passes the same:

```bash
west build -b native_sim/native/64 apps/my_app -- \
    -DZEPHYR_EXTRA_MODULES="$(nros sdk-root)"
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

Then, per application, nano-ros's half of the configuration goes in a
`system.toml` beside the `CMakeLists.txt` (RFC-0098 D10, phase-481), and
Zephyr's own half in `prj.conf`:

```toml
# system.toml
[system]
name = "my_app"
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
[Kconfig reference](../internals/zephyr-in-tree.md#kconfig-reference), spelled without
`CONFIG_`). A `CONFIG_NROS_*` line in `prj.conf` would be overridden by it, so
it is at best dead. Several images in one file — one per RMW — are picked with
`west build … -- -DNROS_IMAGE=<id>`; see
[Configuring an image](#configuring-an-image-systemtoml-not-prjconf).
An app with NO `system.toml` gets no rendering and is configured by Kconfig
defaults and the RMW snippets alone.

`CONFIG_NROS=y` activates the shell, which maps Kconfig values to
`NROS_*` CMake cache vars and `add_subdirectory()`s the root
nano-ros CMake. `NanoRos::NanoRos` is linked into your `app`
library transparently.

## Configuring an image: `system.toml`, not `prj.conf`

An application's nano-ros configuration lives in ONE place, its
`system.toml` (RFC-0098 D10, phase-481). The nano-ros Zephyr module renders
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
| `[image.<id>.knobs.zenoh] max_queryables = 12` | `CONFIG_NROS_MAX_QUERYABLES=12` — a registry knob, spelled as a board's `[knobs]` is (see [Configuration](../user-guide/configuration.md#per-image-build-knobs)) |
| `[image.<id>.knobs.xrce] max_subscribers = 0` | `CONFIG_NROS_XRCE_MAX_SUBSCRIBERS=0` — likewise |
| `env = { NROS_ZEPHYR_TASK_STACK_SIZE = "16384" }` | `CONFIG_NROS_ZEPHYR_TASK_STACK_SIZE=16384` — any other option in the reference below, spelled without `CONFIG_` |
| `os_config = ["prj-xrce.conf"]` | that Zephyr-native fragment, added ahead of your own `EXTRA_CONF_FILE` |

An `env` row with no Kconfig symbol is not lost: it reaches the image's cargo
builds as an `[env]` row instead (issue 1712). A row whose type does not fit
its symbol, or that only exists under another RMW, is refused at configure
time, naming the `system.toml` line.

### One image per RMW

An application built for several backends states one image per RMW, and
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
nros sync
nros build                    # the default_images: zephyr_zenoh
nros build zephyr_xrce        # another image — its own west build dir
```

Each image builds into its own directory,
`build/<platform>-<rmw>/<package>__<image>_west`, so switching images never
reuses another image's cache.

Driving west yourself, the image is a CMake cache variable:
`west build -b native_sim/native/64 my_app -- -DNROS_IMAGE=zephyr_xrce`. Like
any `-D` it sticks: re-running `west build` on the same build directory
WITHOUT it keeps the image it last built, exactly as `-b` keeps the board. Pass
it every time, or use `-p` when switching. A name the `system.toml` does not
declare is refused, listing the ones it does.

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

The other platforms let `nros build` own the build: it generates a cargo or
CMake root from your `[image.*]` declarations and runs the tool. **Zephyr is not
like that, on purpose.**

You own the west workspace and the application. `west init` / `west update`
populate the workspace, your app carries its own `CMakeLists.txt` and
`prj.conf`, and `west` only runs from inside that workspace. None of that is
ours to generate — an app's Kconfig is authored, not derived (RFC-0065 D5).

So for a Zephyr image `nros build`:

1. resolves the **application** — the entry package for that image's board,
   not the bringup;
2. resolves **your Zephyr** — `$ZEPHYR_BASE`, else `$NROS_ZEPHYR_WORKSPACE`,
   else an in-tree `zephyr-workspace/`, else a sibling `../nano-ros-workspace`.
   Note it is a *Zephyr* that is needed, not a `.west/` directory: with
   `ZEPHYR_BASE` set, `west build` runs from anywhere, which is what makes a
   **freestanding application** — one outside the west workspace — work;
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

    west build -b native_sim/native/64 …/src/zephyr_entry

Point nros at your workspace:

    NROS_ZEPHYR_WORKSPACE=<dir>   # the dir containing zephyr/
```

That is deliberate and it is the same boundary `nros setup --system` draws when
it composes an install command and prints it rather than running `sudo` for you:
where the tool is yours, nano-ros hands you the command instead of assuming it
may act.

**`nros build` is the flow this chapter teaches**, for a workspace and for a
single application alike: it applies the image's overlays, picks the build
directory and, when it finishes, prints the artifact and the Zephyr command
that runs or flashes it. It never becomes a required layer between you and
west — [driving west yourself](#driving-west-yourself) stays the escape hatch,
for an application with no `system.toml` or a build you want to steer by hand.

#### Starting from nothing: `nros new node` and `nros new entry`

A node package is board- and RMW-agnostic — it names no platform, which is what
lets the same package be deployed to Linux and to a Cortex-M without an edit:

```bash
nros new node talker_pkg
```

It writes the package and adds the `[[component]]` row to your bringup. (Use
`nros new <name> --platform linux` only for a standalone *runnable project* —
it pins a board crate, so an entry on another platform cannot link it.)

#### The entry: `nros new entry`

You do not have to write the entry package by hand:

```bash
nros new entry zephyr_entry --platform zephyr
```

```text
nros new entry: scaffolded ./src/zephyr_entry (7 file(s))
  declared [image.zephyr_entry] in ./src/demo_bringup/system.toml

Next:
  nros sync
  nros build demo_bringup:zephyr_entry --zephyr-workspace <dir>
```

It writes the package — `Cargo.toml`, `CMakeLists.txt`, `build.rs`,
`src/lib.rs`, `prj.conf`, `prj-zenoh.conf`, `boards/<board>.conf` — and three
things outside it that are easy to miss and invisible until they fail:

* the **`[image.*]` block**, with `board`, `entry` and the `os_config` overlay the
  entry's own `CMakeLists.txt` requires;
* an **`exclude` entry in every enclosing cargo workspace**, because a
  west-built `staticlib` must not be an ordinary cargo member;
* a **path dependency per node package** the bringup declares, since the entry
  links them.

`--board`, `--rmw` and `--bringup` override the defaults
(`native_sim/native/64`, `zenoh`, and the workspace's single bringup).

#### The whole flow, end to end

Five commands, run from your workspace. This is the [`examples/workspaces/rust`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/rust)
transcript, verbatim:

```bash
# 0. tell nros where your west workspace is — the directory containing
#    `zephyr/`. On the command line:
#
#      nros build demo_bringup:zephyr --zephyr-workspace /path/to/zephyr-workspace
#
#    or once for the shell, if you would rather not repeat it:
export NROS_ZEPHYR_WORKSPACE=/path/to/zephyr-workspace

# 1. generate the message bindings this workspace's packages depend on.
#    `nros build` refuses with "this workspace has never been synced" until
#    you do — it will not silently build against absent bindings.
nros sync

# 2. build. This resolves the application, applies the image's overlays and
#    runs `west build` for you.
nros build demo_bringup:zephyr

# 3. start the router ROS itself uses — the one `rmw_zenoh_cpp` ships, so it
#    cannot drift from the RMW your ROS 2 nodes use. Separate terminal; it
#    keeps running. Listens on tcp/127.0.0.1:7447.
ros2 run rmw_zenoh_cpp rmw_zenohd

# 4. run the image — `nros build` printed this, with its own build dir
#    (build/<coordinate>/<bringup>__<image>_west):
west build -d build/zephyr-zenoh/demo_bringup__zephyr_west -t run
#    (on native_sim that runs ./build/zephyr-zenoh/demo_bringup__zephyr_west/zephyr/zephyr.exe)
```

```text
*** Booting Zephyr OS build v3.7.0 ***
<inf> nros_net_wait: Network ready (NSOS — host kernel sockets)
<inf> rust: rustapp: nros: zephyr workspace entry up (2 nodes)
<inf> rust: talker_pkg: talker publishing chatter seq=0
<inf> rust: talker_pkg: talker publishing chatter seq=1
```

and from a ROS 2 shell on the same router, the nodes are ordinary ROS nodes:

```console
$ export RMW_IMPLEMENTATION=rmw_zenoh_cpp
$ ros2 topic echo /chatter --once
data: 6
---
```

Step 3 is not optional and its ORDER does not matter, but the **locator** does:
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

None of them sets `entry`: `nros build` generates each image's application, so
there is nothing to choose between. `entry` is for an application you wrote
by hand, below.

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
    `DEPLOY` scan does not read that call (issue 1517). Usually the better fix
    is to delete the application — `nros build` generates one and hands it the
    board package's own Kconfig and devicetree overlay, which is all
    `nano_ros_use_board` added (issue 1288).

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
[Pin and mirror a release](./vendored-tree.md)). **With the installed SDK alone
there is no `west nros`:** west reads extension commands from a manifest
project, and the installed SDK root is a versioned directory under `~/.nros`
that no `west.yml` can name. Name the Zephyr workspace to `nros` instead,
which is what `west nros` does for you:

```bash
nros build --zephyr-workspace ~/zephyr-workspace demo_bringup:zephyr
```

`west nros` is now the only nano-ros west extension: `west fvp` shared that
route until RFC-0064 revision 5 retired it, since its body was environment
wiring in front of the stock `west build -t run`. Two things worth knowing:

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
is refused with the id to write instead (issue 1519).

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

**Everything Zephyr lets you extend, you still extend the Zephyr way.** The
application `CMakeLists.txt` is yours (RFC-0065 D5) and `nros build` runs
`west build` against it, so the extension points are untouched:

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
<inf> rust: talker_pkg: talker publishing chatter seq=0
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

The escape hatch: an application with no `system.toml`, or a build you want to
steer by hand. The transports + `px4-rs` come from the
[prerequisites](#prerequisites) step. With the Zephyr SDK exported (also
prerequisites), build your app — `nros` on PATH is auto-resolved as the
codegen tool, and the module is the installed SDK root:

```bash
sdk="$(nros sdk-root)"
# native_sim (POSIX, no QEMU). The 3.7 line needs the NSOS line overlay; apply
# the NSOS patches first (see "apply nano-ros's patches" below).
overlay="$sdk/cmake/zephyr/native-sim-line-3.7.conf"
west build -b native_sim/native/64 apps/my_app -- \
    -DZEPHYR_EXTRA_MODULES="$sdk" -DCONF_FILE="prj.conf;$overlay"

# An emulated Cortex-A9 — `qemu_cortex_a9` is a QEMU machine, not a real
# board (no native_sim overlay):
west build -b qemu_cortex_a9 apps/my_app -- -DZEPHYR_EXTRA_MODULES="$sdk"
```

(Verified end-to-end on a fresh BYO west workspace: this builds to
`zephyr.exe` and runs to `Published: 0` against a router started with
`ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd`.
On the Zephyr 4.4 line, `find_package(Python3)` requires ≥ 3.12 and you
select the RMW with `-S nros-zenoh` instead of the overlay.)

For a quick sanity check that the module is wired correctly:

```bash
west build -t menuconfig                 # confirm CONFIG_NROS=y is visible
```

## Rust applications

Two things differ for a **Rust** app (C/C++ apps skip this section):

1. **The Rust crate's `[lib]` must be named `rustapp`** (`crate-type =
   ["staticlib"]`) — a `zephyr-lang-rust` contract: its `rust_cargo_application()`
   links `librustapp.a`. The Cargo *package* name is free.

2. **Generate the interface crates and the dependency wiring for YOUR
   layout.** From your app dir, run (after the [Prerequisites](#prerequisites)
   `nros setup`, which provides the codegen toolchain + message sources):

   ```bash
   nros sync
   ```

   This writes `generated/<pkg>/` (the message crates) and a `.cargo/config.toml`
   whose `[patch.crates-io]` points the `nros-*` crates into the nano-ros SDK
   root and the generated interfaces at `generated/*`. `nros sdk-root
   --explain` prints the root `nros` picks; `--nano-ros-path <dir>` names
   another (the SDK ROOT, the directory holding `nano_rosConfig.cmake`).

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
SDK ships (`nros setup zephyr` installed it):

```bash
# in system.toml:  [image.zephyr_m3] board = "mps2-an385-zephyr"
nros build demo_bringup:zephyr_m3
west build -d build/zephyr-zenoh/demo_bringup__zephyr_m3_west -t run
```

The guest sits on QEMU's user-mode network: give the image an address there
(`10.0.2.15`, gateway `10.0.2.2`) and a `locator = "tcp/10.0.2.2:<port>"`, and
start the router on `0.0.0.0:<port>` so the Slirp gateway reaches it.

### Verify from stock ROS 2

```bash
# Match the message type to the app you booted: the canonical in-tree
# talker publishes std_msgs/String; the raw BYO Talker.c shown above
# publishes std_msgs/Int32.
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
# Talker publishes best-effort; stock `ros2 topic echo` defaults to
# RELIABLE, so the QoS-mismatched echo silently delivers nothing.
# Force best-effort to receive:
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort   # in-tree talker
# ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort  # BYO my_app
```

> **Contributors:** nano-ros's own zephyr talker has a matching in-tree
> recipe for the canonical `native_sim` build path — see
> [Per-Platform Contributor Lanes](../internals/platform-lanes.md#zephyr).

The Zephyr boot banner runs first, then the talker fires: the
canonical in-tree talker prints `Publishing: 'Hello World: 1'`,
`Publishing: 'Hello World: 2'`, ... (Rust + C + C++ all count from
1, matching the official ROS 2 demo talker); the raw BYO `Talker.c`
above prints `Published: 0`, `Published: 1`, ...

**Readiness signal.** On `native_sim`, expect the first publish line
(`Publishing: 'Hello World: 1'` for the in-tree talker;
`Published: 0` for the BYO app) within 5 seconds of boot; on a QEMU
board (`mps2-an385-zephyr`, or a `qemu_cortex_a9` you built yourself)
expect it within ~15 seconds (QEMU cold boot + Zephyr init). If no publish line
in 30 seconds:

1. Confirm `CONFIG_NROS=y` lit up via `west build -t menuconfig`;
   without it the module shell never `add_subdirectory`'s nano-ros.
2. Check `CONFIG_NETWORKING=y`, `CONFIG_NET_IPV4=y`, `CONFIG_NET_TCP=y`
   in `prj.conf` — Zephyr networking is opt-in.
3. Confirm `zenohd` reachable from the simulated network (Slirp
   needs `10.0.2.2:7447` on QEMU; native_sim uses host loopback).
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

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

nano-ros ships no Zephyr hardware board of its own — the boards above are
`native_sim` and QEMU machines, and the ARM FVP is a model
([its chapter](./arm-fvp.md)). Any board Zephyr supports is reached with a
board package that names it (`west_board`, see
[Which boards you can name](#which-boards-you-can-name)), and flashed with the
runner Zephyr's own board definition declares:

```bash
# src/my_board/nros-board.toml names west_board = "nucleo_f767zi" (say);
# in system.toml:  [image.nucleo] board = "my-board"
nros build demo_bringup:nucleo
west flash -d build/zephyr-zenoh/demo_bringup__nucleo_west
```

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
different RMW is refused at configure time (phase-481). The
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
with stock Zephyr. Phase 208.D.7 / E.9 unified both lines on the
provisioner. Cyclone-DDS-on-Zephyr patches stay baked into the pinned
cyclonedds submodule, not delivered through `patches.yml`.)

## Start from an example

The [`examples/zephyr/<lang>/<role>/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/zephyr)
dirs on GitHub are **copy-out clean** — each is a complete application
(`CMakeLists.txt`, `system.toml`, `prj*.conf`, `src/`) with no reference
back into the nano-ros repository, so its files are a starting point to
copy into your own app tree. The release does not ship `examples/`;
`nros new <name> --platform zephyr --lang <c|cpp>` scaffolds a minimal
C or C++ app instead. A Rust app is a workspace: `nros new <ws> --workspace
--lang rust`, then `nros new entry zephyr_entry --platform zephyr` (see
[The entry: `nros new entry`](#the-entry-nros-new-entry)); `nros new --platform
zephyr --lang rust` refuses and prints those commands.

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
- Worked examples:
  [`examples/zephyr/rust/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/zephyr/rust),
  [`examples/zephyr/c/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/zephyr/c),
  [`examples/zephyr/cpp/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/zephyr/cpp)
- Module manifest:
  [`zephyr/module.yml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/zephyr/module.yml)
- Kconfig surface (canonical post-Phase-208.D.7 fold — every `CONFIG_NROS*`
  symbol the doc cites lives here):
  [`zephyr/Kconfig`](https://github.com/NEWSLabNTU/nano-ros/blob/main/zephyr/Kconfig)
- Patches applied by `nros setup zephyr` (the `west patch` flow on this page
  was retired in Phase 208.E.9):
  [`zephyr/patches.yml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/zephyr/patches.yml)

## Next

- Pick a real board (Nordic, NXP, STM32, …): a board package naming its
  `west_board`, then [Flash a real board](#flash-a-real-board).
- Cyclone DDS on Cortex-A/R: see the DDS section of
  [Choosing an RMW Backend](../user-guide/rmw-backends.md) for the
  required Kconfig deltas.
- Build nano-ros's own Zephyr examples without west:
  [Zephyr in-tree examples (contributor)](../internals/zephyr-in-tree.md).
