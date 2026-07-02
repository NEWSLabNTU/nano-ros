# Zephyr (west module)

Single-node starter on Zephyr via the in-tree `zephyr/`
west module. nano-ros ships as a Zephyr module — `west` discovers it
from your workspace's `west.yml`, drops in a `prj.conf` Kconfig
surface, and the standard `west build` / `west flash` flow takes
care of the rest.

> **Contributor path?** Building nano-ros's own Zephyr examples
> straight from this repository (no west-managed workspace) is
> covered at [Zephyr (contributor)](./zephyr.md). The page below is
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
| RMW selection | `prj-<rmw>.conf` overlay (`-DCONF_FILE=...`) | **`-S nros-<rmw>` snippet** (or the overlay) |
| nano-ros patches | applied during `nros setup zephyr` provisioning | applied during `nros setup zephyr` provisioning |
| Examples as samples / Twister | — | **`samples:` + Twister** (`sample.nano-ros.*`) |
| zenoh (native_sim) | ✅ build + e2e | ✅ build + e2e |
| cyclonedds (native_sim) | ✅ build + e2e | ✅ build · publish · receive · multicast-join *(stable 2-node run pending a tracked `k_mutex` fix)* |
| xrce | ✅ | build path WIP |

native_sim networking uses **NSOS** (host loopback) on both lines — no
TAP/bridge/root. The copy-out, snippet, patch-apply, and dual-line build
flows are exercised in CI (`just zephyr ci-both`, `just zephyr check-copy-out`).

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
        ├── prj.conf                   # Kconfig — selects nros + RMW
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

set(NANO_ROS_PLATFORM zephyr)
include("${NROS_REPO_DIR}/cmake/NanoRosNodeRegister.cmake")

nano_ros_node_register(
    NAME      talker
    CLASS     my_app::Talker
    LANGUAGE  C
    TYPED
    SOURCES   src/Talker.c
    DEPLOY    zephyr)
```

```c
// apps/my_app/src/Talker.c
// `talker_configure` creates a raw publisher on `/chatter` + a timer that
// publishes a CDR-encoded Int32 counter each tick. `NROS_C_COMPONENT` emits
// the C-ABI factory/configure the Zephyr typed Entry carrier calls.
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>

#include <nros/component.h>

typedef struct {
    _Alignas(8) uint8_t pub[NROS_C_PUBLISHER_STORAGE_SIZE];
    int32_t count;
} talker_t;

static void write_u32_le(uint8_t* p, uint32_t v) {
    p[0] = (uint8_t)v;
    p[1] = (uint8_t)(v >> 8);
    p[2] = (uint8_t)(v >> 16);
    p[3] = (uint8_t)(v >> 24);
}

static void on_tick(void* ctx) {
    talker_t* self = (talker_t*)ctx;
    /* std_msgs/Int32 CDR: 4-byte encapsulation header (CDR_LE) + int32 data. */
    uint8_t buf[8];
    buf[0] = 0x00;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x00;
    write_u32_le(buf + 4, (uint32_t)self->count);
    if (nros_cpp_publish_raw(self->pub, buf, sizeof(buf)) == 0) {
        printf("Published: %d\n", (int)self->count);
    }
    self->count++;
}

static nros_ret_t talker_configure(const nros_cpp_node_t* node, void* executor, talker_t* self) {
    self->count = 0;
    int32_t rc = nros_cpp_publisher_create(node, "/chatter", "std_msgs::msg::dds_::Int32_", "",
                                           nros_c_qos_default(), self->pub);
    if (rc != 0) {
        return rc;
    }
    size_t timer_handle;
    return nros_cpp_timer_create(executor, /*period_ms=*/500, on_tick, self, &timer_handle);
}

NROS_C_COMPONENT(talker_t, talker_configure)
```

See [`examples/zephyr/c/talker/src/Talker.c`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/zephyr/c/talker/src/Talker.c)
for the up-to-date source (this is a verbatim copy, only stripped of its file
header comment). Note the `nros_cpp_*` symbol prefix: those are C-ABI
functions from `nros-cpp` (the `cpp` is a namespace prefix, not C++
linkage) — a C component links against them directly, and shares the same
executor + node as a C++ component would.

`prj.conf` is the one shown in [Configure](#configure) below — note it needs
**both** `CONFIG_NROS_C_API=y` and `CONFIG_NROS_CPP_API=y`: the typed
Zephyr carrier that drives this component's `configure` is C++, so the
`nros-cpp` header surface must be on the app include path even though
`Talker.c` itself is C.

## Prerequisites

`nros setup` provisions the parts nano-ros owns — the **RMW host daemon**
(`zenohd` / Micro-XRCE-DDS agent) and the **RMW transport submodules**
(zenoh-pico + mbedtls for zenoh, the cyclonedds fork) — from a pinned index into
`${NROS_HOME:-~/.nros}/sdk` / the nano-ros checkout. It does **not** replace Zephyr's own SDK,
and interface codegen still needs the ROS message definitions.

1. **Build the in-tree `nros` CLI** (Phase 218, from the nano-ros checkout):
   ```bash
   source ./activate.sh        # OR: direnv allow / source ./activate.fish
   just setup-cli              # builds packages/cli/target/release/nros
   ```
2. **Provision the RMW (daemon + transports)** from the nano-ros checkout:
   ```bash
   ( cd modules/nano-ros && nros setup zephyr --rmw zenoh )  # zenohd + zenoh-pico + mbedtls
   ( cd modules/nano-ros && nros setup --source px4-rs )     # workspace cargo-load dep
   ```
3. **Install the Zephyr SDK** the standard Zephyr way (`nros setup` does *not*
   provide it) and expose it — `export ZEPHYR_SDK_INSTALL_DIR=/path/to/zephyr-sdk-<ver>`
   (or register it via the SDK's `setup.sh -c`).
4. **Message definitions for codegen.** The interface codegen resolves a
   package's `msg/*.msg` from `NROS_<PKG>_DIR` (e.g.
   `export NROS_STD_MSGS_DIR=/opt/ros/humble/share/std_msgs`) — point it at a ROS
   install or any dir holding the `.msg` files.

The RMW host daemon must be **running** before an example connects (`zenohd -l
tcp/127.0.0.1:7456` for zenoh; the Micro-XRCE-DDS agent for xrce).

## Configure

Add nano-ros (and a Zephyr pin — `zephyr/west.yml` is manifest-only;
it does **not** pull Zephyr itself) to your workspace `west.yml`:

```yaml
manifest:
  remotes:
    - name: nano-ros
      url-base: https://github.com/NEWSLabNTU
    - name: zephyr
      url-base: https://github.com/zephyrproject-rtos
  projects:
    - name: zephyr
      remote: zephyr
      revision: v3.7.0          # or your chosen 3.7 LTS / 4.x SHA
      path: zephyr
      import: true               # pulls Zephyr's own modules
    - name: nano-ros
      remote: nano-ros
      revision: main             # required — repo's default branch is
                                 # `main`; west defaults to `master`
                                 # otherwise and the fetch fails.
      path: modules/nano-ros
      import:
        file: zephyr/west.yml    # pulls nano-ros's transport deps
```

Then per-application `prj.conf`:

```
CONFIG_NROS=y
CONFIG_NROS_C_API=y
CONFIG_NROS_RMW_ZENOH=y                 # bool per RMW: NROS_RMW_{ZENOH,XRCE,CYCLONEDDS}
# (ROS edition is a build-time Cargo feature, NOT a Kconfig symbol — do not set
#  CONFIG_NROS_ROS_EDITION; Zephyr aborts on the undefined symbol.)

# Required for any networked RMW on QEMU / native_sim:
CONFIG_NETWORKING=y
CONFIG_NET_IPV4=y
CONFIG_NET_TCP=y
```

`CONFIG_NROS=y` activates the shell, which maps Kconfig values to
`NANO_ROS_*` CMake cache vars and `add_subdirectory()`s the root
nano-ros CMake. `NanoRos::NanoRos` is linked into your `app`
library transparently.

## Build

If your workspace is a fresh manifest-only dir (no `.west/`), initialise
it first so `west` knows which `west.yml` is the manifest:

```bash
cd my_zephyr_ws
west init -l .                           # one-time; points west at the
                                         # local west.yml in cwd
west update                              # clones nano-ros + Zephyr into the workspace
```

(If you started from `west init -m <remote>`, both calls above are
already done — go straight to `west build` below.)

The transports + `px4-rs` come from the [prerequisites](#prerequisites) step
(`west update` clones nano-ros but **not** its submodules). With the Zephyr SDK +
`NROS_STD_MSGS_DIR` exported (also prerequisites), build your app — `nros` on
PATH is auto-resolved as the codegen tool:

```bash
# native_sim (POSIX, no QEMU). The 3.7 line needs the NSOS line overlay; apply
# the NSOS patches first (see "apply nano-ros's patches" below).
overlay="$PWD/modules/nano-ros/cmake/zephyr/native-sim-line-3.7.conf"
west build -b native_sim/native/64 apps/my_app -- -DCONF_FILE="prj.conf;$overlay"

# A real board, e.g. Cortex-A9 (no native_sim overlay):
west build -b qemu_cortex_a9 apps/my_app
```

(Verified end-to-end on a fresh BYO west workspace: this builds to `zephyr.exe`
and runs to `Published: 0` against `zenohd -l tcp/127.0.0.1:7456`. On the 4.4
line, `find_package(Python3)` requires ≥ 3.12 and you select the RMW with
`-S nros-zenoh` instead of the overlay.)

For a quick sanity check that the module is wired correctly:

```bash
west build -t menuconfig                 # confirm CONFIG_NROS=y is visible
```

## Rust applications

Two things differ for a **Rust** app (C/C++ apps skip this section):

1. **The Rust crate's `[lib]` must be named `rustapp`** (`crate-type =
   ["staticlib"]`) — a `zephyr-lang-rust` contract: its `rust_cargo_application()`
   links `librustapp.a`. The Cargo *package* name is free.

2. **Generate the interface crates + the `[patch.crates-io]` wiring for YOUR
   layout** — do **not** copy an in-repo example's `.cargo/config.toml`: its
   `../../../../packages/core/...` paths are repo-relative and break in a
   copied-out app. From your app dir, run (after the [Prerequisites](#prerequisites)
   `nros setup`, which provides the codegen toolchain + message sources):

   ```bash
   nros generate-rust --generate-config \
       --nano-ros-path "$PWD/../../modules/nano-ros/packages/core"
   ```

   This writes `generated/<pkg>/` (the message crates) and a `.cargo/config.toml`
   whose `[patch.crates-io]` points the `nros-*` crates at your
   `modules/nano-ros/packages/core/*` and the generated interfaces at
   `generated/*`. Adjust `--nano-ros-path` to your workspace's
   `modules/nano-ros/packages/core` (the dir holding `nros-core`, `nros-node`, …).
   The example apps' committed `.cargo/config.toml` is for the in-tree build only.

## Run

```bash
# 1. Start zenohd on the host. The in-tree just recipe runs the
#    pinned in-tree zenohd on the zephyr fixture port (7456) — the
#    same port the example apps' `nros.toml` / Kconfig defaults pick
#    up:
just zephyr zenohd &
#    Or directly:
#    zenohd --listen tcp/0.0.0.0:7456 --no-multicast-scouting

# 2. Boot the app. nano-ros's own in-tree zephyr talker has a
#    matching just recipe for the canonical `native_sim` build path:
just zephyr talker
#    For a BYO west workspace + your own app:
# QEMU Cortex-A9:
west build -t run
# native_sim:
./build/zephyr/zephyr.exe

# 3. Verify from stock ROS 2 in another terminal:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
# Talker publishes best-effort; stock `ros2 topic echo` defaults to
# RELIABLE, so the QoS-mismatched echo silently delivers nothing.
# Force best-effort to receive:
ros2 topic echo /chatter std_msgs/msg/Int32 --qos-reliability best_effort
```

The Zephyr boot banner runs first, then nano-ros prints
`Published: 0`, `Published: 1`, ... as the talker fires — Rust + C +
C++ all start at 0 (Phase 208.D.9).

**Readiness signal.** On `native_sim`, expect `Published: 0`
within 5 seconds of `just zephyr talker` (or
`./build/zephyr/zephyr.exe`); on `qemu_cortex_a9` expect it within
~15 seconds (QEMU cold boot + Zephyr init). If no `Published:` line
in 30 seconds:

1. Confirm `CONFIG_NROS=y` lit up via `west build -t menuconfig`;
   without it the module shell never `add_subdirectory`'s nano-ros.
2. Check `CONFIG_NETWORKING=y`, `CONFIG_NET_IPV4=y`, `CONFIG_NET_TCP=y`
   in `prj.conf` — Zephyr networking is opt-in.
3. Confirm `zenohd` reachable from the simulated network (Slirp
   needs `10.0.2.2:7456` on QEMU; native_sim uses host loopback).
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

**Zephyr 4.x build gotchas.**
- `Could NOT find Python3 ... required is at least "3.12"` — 4.x needs
  Python ≥ 3.12. Provision one without sudo (e.g. `uv venv --python 3.12
  .venv312 && uv pip install west -r zephyr/scripts/requirements.txt`) and
  run west through it (`.venv312/bin/python -m west build ...`), so the
  ROS descriptor-codegen subprocess still uses the system ROS Python.
- `attempt to assign the value ... to the undefined symbol ETH_NATIVE_POSIX`
  — that symbol was renamed `ETH_NATIVE_TAP` in 4.x; the version-aware
  NSOS overlay handles it (`just zephyr build-one` does this automatically).

## Zephyr 4.x: select the RMW with a snippet

On 4.x, nano-ros ships `west` **snippets** so you pick the RMW on the
build line instead of hand-writing the overlay:

```bash
west build -b native_sim/native/64 -S nros-cyclonedds apps/my_app
#                                   ^^^^^^^^^^^^^^^^^^^  nros-zenoh | nros-cyclonedds | nros-xrce
```

The snippet (shipped via the module's `snippet_root`) carries the
RMW-common Kconfig — equivalent to merging `prj-<rmw>.conf`. The
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

## Copy out an example as your starting point

The `examples/zephyr/<lang>/<role>/` dirs are **copy-out clean** — copy one
into your own app tree and it builds against the nano-ros module with no
reference back into the nano-ros repo:

```bash
cp -r modules/nano-ros/examples/zephyr/c/talker apps/my_app
# cyclonedds examples need the host idlc + the ROS message dirs:
export NROS_STD_MSGS_DIR=/opt/ros/humble/share/std_msgs   # PKG_DIR contract
west build -b native_sim/native/64 -S nros-zenoh apps/my_app
```

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

- Pick a real board (Nordic, NXP, STM32, …): swap `-b <board>` and
  add a board-specific overlay to your `prj.conf`.
- Cyclone DDS on Cortex-A/R: see the DDS section of
  [Choosing an RMW Backend](../user-guide/rmw-backends.md) for the
  required Kconfig deltas.
- Build nano-ros's own Zephyr examples without west:
  [Zephyr (contributor)](./zephyr.md).
