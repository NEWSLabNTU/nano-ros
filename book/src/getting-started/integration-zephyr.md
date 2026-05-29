# Zephyr (west module)

Single-node starter on Zephyr via the in-tree `integrations/zephyr/`
west module. nano-ros ships as a Zephyr module — `west` discovers it
from your workspace's `west.yml`, drops in a `prj.conf` Kconfig
surface, and the standard `west build` / `west flash` flow takes
care of the rest.

> **Contributor path?** Building nano-ros's own Zephyr examples
> straight from this repository (no west-managed workspace) is
> covered at [Zephyr (contributor)](./zephyr.md). The page below is
> the canonical user entry.

> **Prereqs.** Run `nros setup zephyr --rmw <rmw>` once (see
> [Prerequisites](#prerequisites) below) — it provisions the Zephyr
> west workspace + Zephyr SDK bits, the emulator, and your RMW's host
> daemon into the shared store. No hand-installed Zephyr SDK, `west`,
> or cross-toolchain, and no ROS 2 install required.
> **Python: 3.10+ on Zephyr 3.7 LTS, but ≥ 3.12 on Zephyr 4.x**
> (4.x's `find_package(Python3)` requires 3.12 — see the version
> matrix below). nano-ros's imported west fragment
> `integrations/zephyr/west.yml` is a manifest-only file — it does NOT
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
| nano-ros patches | applied during `nros setup zephyr` provisioning | **`west patch apply`** (`zephyr/patches.yml`) — *or* the provisioning step |
| Examples as samples / Twister | — | **`samples:` + Twister** (`sample.nano-ros.*`) |
| zenoh (native_sim) | ✅ build + e2e | ✅ build + e2e |
| cyclonedds (native_sim) | ✅ build + e2e | ✅ build · publish · receive · multicast-join *(stable 2-node run pending a tracked `k_mutex` fix)* |
| xrce | ✅ | build path WIP |

native_sim networking uses **NSOS** (host loopback) on both lines — no
TAP/bridge/root. The copy-out, snippet, `west patch`, and dual-line build
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

## Prerequisites

`nros setup` is the single canonical command to prepare a machine to build
nano-ros for a board. It ships prebuilt toolchains per platform per RMW — the
cross-compiler, emulator, RMW host daemon, and SDK sources (including the Zephyr
west workspace + Zephyr SDK bits) are fetched from a pinned index into a shared
store at `~/.nros/sdk`. You do **not** hand-install a cross-toolchain, and you do
**not** need ROS 2 installed.

1. **Install the `nros` CLI** (once per machine):
   ```bash
   curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install-nros.sh | sh
   export PATH="$HOME/.nros/bin:$PATH"
   ```
2. **Provision the Zephyr board** (+ the RMW you'll use):
   ```bash
   nros setup zephyr --rmw zenoh         # --rmw defaults to zenoh; xrce | cyclonedds also valid
   ```
   This provisions the Zephyr west workspace + Zephyr SDK bits, the emulator,
   and the RMW host daemon (`zenohd` for zenoh, the Micro-XRCE-DDS agent for
   xrce). The module's interface codegen also shells out to `nros` at configure
   time.

The RMW host daemon installed by the step above must be **running** before any
example: `zenohd` for zenoh, the Micro-XRCE-DDS agent for xrce.

## Configure

Add nano-ros to your workspace `west.yml`:

```yaml
manifest:
  remotes:
    - name: nano-ros
      url-base: https://github.com/NEWSLabNTU
  projects:
    - name: nano-ros
      remote: nano-ros
      path: modules/nano-ros
      import:
        file: integrations/zephyr/west.yml      # pulls Zephyr + nano-ros deps
```

Then per-application `prj.conf`:

```
CONFIG_NROS=y
CONFIG_NROS_RMW="zenoh"                 # zenoh | xrce | cyclonedds
CONFIG_NROS_ROS_EDITION="humble"        # humble | iron

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

```bash
west update                              # clones nano-ros + Zephyr into the workspace
```

The RMW transport sources nano-ros links against (zenoh-pico, the cyclonedds
fork, …) are provided by `nros setup zephyr --rmw <rmw>` from the
[prerequisites](#prerequisites). One extra source — the nano-ros cargo build
loads the whole workspace, which path-deps `px4-sitl-tests` — so also run
`nros setup --source px4-rs` once from the nano-ros checkout (a small source, not
a PX4 build); without it the `nros-c` cargo build fails `failed to get
px4-sitl-tests`. (Verified end-to-end on a fresh BYO west workspace: this builds
`c/talker` to `zephyr.exe` and runs to `Published: 1` against `zenohd`.)

```bash
west build -b qemu_cortex_a9 apps/my_app
# native_sim alternative (POSIX, no QEMU):
west build -b native_sim/native/64 apps/my_app
```

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
# QEMU Cortex-A9:
west build -t run

# native_sim:
./build/zephyr/zephyr.exe

# Verify from stock ROS 2 in another terminal:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32
```

The Zephyr boot banner runs first, then nano-ros prints
`Published: 1`, `Published: 2`, ... as the talker fires.

**Readiness signal.** On `native_sim`, expect `Published: 1`
within 5 seconds of `./build/zephyr/zephyr.exe`; on `qemu_cortex_a9`
expect it within ~15 seconds (QEMU cold boot + Zephyr init). If
no `Published:` line in 30 seconds:

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

## Zephyr 4.x: apply nano-ros's patches with `west patch`

nano-ros needs a few patches to Zephyr's `native_sim` NSOS driver
(`recvmsg`, IPv4-multicast). On 4.x these are delivered the standard
way — `zephyr/patches.yml` consumed by `west patch`:

```bash
west update
west patch apply        # applies nano-ros's zephyr/patches.yml (checksummed)
# ... build as above ...
west patch clean        # roll back if needed
```

`west patch` is **4.x-only**; on 3.7 the same patches are applied during
provisioning by `nros setup zephyr`. (Cyclone-DDS-on-Zephyr patches
are baked into the pinned cyclonedds submodule, not delivered via
`west patch` — see `integrations/zephyr/README.md`.)

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

See [`integrations/zephyr/README.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/integrations/zephyr/README.md)
for the in-repo quick reference.

## GitHub source

- Zephyr module shell:
  [`integrations/zephyr/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/integrations/zephyr)
- Worked examples:
  [`examples/zephyr/rust/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/zephyr/rust),
  [`examples/zephyr/c/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/zephyr/c),
  [`examples/zephyr/cpp/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/zephyr/cpp)
- Module manifest:
  [`integrations/zephyr/module.yml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/integrations/zephyr/module.yml)

## Next

- Pick a real board (Nordic, NXP, STM32, …): swap `-b <board>` and
  add a board-specific overlay to your `prj.conf`.
- Cyclone DDS on Cortex-A/R: see the DDS section of
  [Choosing an RMW Backend](../user-guide/rmw-backends.md) for the
  required Kconfig deltas.
- Build nano-ros's own Zephyr examples without west:
  [Zephyr (contributor)](./zephyr.md).
