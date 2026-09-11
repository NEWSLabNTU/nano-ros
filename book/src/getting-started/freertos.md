# FreeRTOS (QEMU MPS2-AN385)

Single-node starter on FreeRTOS + lwIP, cross-compiled for Cortex-M3
and booted in QEMU MPS2-AN385. Slirp networking; no host TAP /
bridge / sudo. Rust, C, and C++ talkers all live in-tree.

> **Prereqs.** `nros setup mps2-an385-freertos` is the single command
> that prepares your machine for this board. It fetches a prebuilt
> toolchain set into the shared store at `~/.nros/sdk` — the
> `arm-none-eabi-gcc` cross-compiler, the patched
> `qemu-system-arm` emulator, and the FreeRTOS kernel + lwIP sources.
> You do **not** hand-install a cross-toolchain. The zenoh path DOES
> need a ROS 2 install for the router (`ros2 run rmw_zenoh_cpp
> rmw_zenohd`); the xrce path needs only the Micro-XRCE-DDS agent,
> which `nros setup` installs.

## Setup

Build the in-tree `nros` CLI (Phase 218):

```bash
./scripts/bootstrap.sh      # builds packages/cli/target/release/nros
source ./activate.sh        # OR: direnv allow / source ./activate.fish
```

Then provision the board (`--rmw` defaults to `zenoh`; pick `xrce`
or `cyclonedds` to match the example you intend to run):

```bash
nros setup mps2-an385-freertos --rmw zenoh
```

This fetches the cross-compiler, the patched `qemu-system-arm`, and the
FreeRTOS + lwIP sources into `${NROS_HOME:-~/.nros}/sdk` (plus the
Micro-XRCE-DDS agent when you pick `--rmw xrce`). The zenoh router is
NOT installed — it comes from your ROS 2 install (RFC-0075).

## Project layout

Each language uses the standard nano-ros canonical example shape —
standalone Cargo (Rust) or CMake (C / C++) project under
`examples/mps2-an385-freertos/<lang>/<example>/`.

```text
examples/mps2-an385-freertos/
├── rust/talker/             # Cargo package
│   ├── system.toml                 # WHAT this deploys to — the one file you edit
│   ├── Cargo.toml                  # Rust deps only; nothing here names a board
│   ├── package.xml
│   ├── generated/                  # generated message bindings (gitignored)
│   ├── build/                      # everything `nros sync` generates (gitignored)
│   └── src/lib.rs                  # the component class; nros::main! generates the entry
├── c/talker/                 # CMake project, add_subdirectory consumption
│   ├── system.toml                 # the same file, same schema
│   ├── CMakeLists.txt              # targets only
│   ├── package.xml
│   └── src/Talker.c
└── cpp/talker/               # CMake C++14 project
    ├── system.toml
    ├── CMakeLists.txt
    ├── package.xml
    └── src/Talker.cpp
```

All three languages state their deployment in the same `system.toml`, and
nothing else does. The Rust leaf has no `.cargo/` to edit — the
`thumbv7m-none-eabi` triple, the `-Tmps2_an385.ld --nmagic --gc-sections` link
group, the QEMU runner and the `[patch.crates-io]` rows all come from the board
and are written by `nros sync` into
`build/mps2-an385-freertos/nros-cargo.toml` (RFC-0098). The C / C++ leaves have
no `<nano_ros deploy=… board=… rmw=…/>` tuple in their `package.xml`:
`find_package(nano_ros)` reads `system.toml` instead, and derives the platform
from the board.

The Rust `Cargo.toml` pulls the FreeRTOS board crate
(`nros-board-mps2-an385-freertos`), which wraps the kernel + lwIP +
LAN9118 driver build. The C / C++ `CMakeLists.txt` follows the
canonical `add_subdirectory(<repo-root>) +
nano_ros_link_rmw(<target> RMW zenoh)` pattern.

## Configure

Board, RMW, domain and network identity are declared in `system.toml` and
**baked at compile time** — there is no config file on the device. Verbatim
from the in-tree
[`examples/mps2-an385-freertos/rust/talker/system.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/mps2-an385-freertos/rust/talker/system.toml):

```toml
[system]
name = "freertos_rs_talker"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "freertos_rs_talker"
class = "freertos_rs_talker::Talker"
name = "talker"

[image.mps2-an385-freertos]
board = "freertos"
locator = "tcp/10.0.2.2:7800"
ip = "10.0.2.15"
gateway = "10.0.2.2"
```

The C and C++ leaves carry the *same* file, in the same schema — only the
names and the locator differ. Nothing about the deployment lives in
`Cargo.toml`, in `CMakeLists.txt`, or in the `package.xml` `<export>` block
any more.

For a C or C++ leaf, switching board really is editing `[image.<id>] board`
(a cross build still passes its own `-DCMAKE_TOOLCHAIN_FILE`). For the **Rust**
leaf it is two lines, not one: a single-package leaf is its own entry, so it
still names the board crate in `[dependencies]`, and `nros sync` writes its
patch rows from the manifest rather than from the image. Move
`nros-board-mps2-an385-freertos` with the `board =` line or the build fails
inside your own `src/main.rs` — that is
[issue 1305](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/1305-single-package-board-crate-dep-not-generated.md).

Task stacks / priorities come from the board crate's defaults (Cargo
features), not a config file — see the
[Configuration Guide](../user-guide/configuration.md).

The `10.0.2.0/24` subnet is QEMU Slirp's default; `10.0.2.2` is the
Slirp gateway that forwards to host loopback. No TAP, no sudo.

Ports: read your image's own `locator`. The shipped Rust talker dials host
port **7800**; the C and C++ talkers declare a `[system] locator` of their
own.

> **Contributors:** the prebuilt test fixtures bake different ports —
> see [Per-Platform Contributor Lanes](../internals/platform-lanes.md#freertos).

## Build

```bash
# Rust — `nros sync` first, once per checkout location. It writes the
# generated message bindings AND build/<image>/nros-cargo.toml, which
# carries this board's triple, link group and [patch.crates-io] table
# (RFC-0098 D1). Without it cargo builds for the host, or fails while
# PARSING the manifest with an error that never names sync (see below).
cd examples/mps2-an385-freertos/rust/talker
nros sync
nros build          # or: cargo build --config build/<image>/nros-cargo.toml

# C / C++ — the leaf's own CMake, with the cross-toolchain file (the `nros`
# CLI on PATH auto-resolves the codegen tool — no `-D_NANO_ROS_CODEGEN_TOOL=`
# needed, and no `-DNANO_ROS_BOARD=` / `-DNROS_RMW=`: both come from
# system.toml). The toolchain file path is relative to the nano-ros checkout:
toolchain="$NROS_REPO_DIR/cmake/toolchain/arm-freertos-armcm3.cmake"
cd examples/mps2-an385-freertos/c/talker
cmake -B build -DCMAKE_TOOLCHAIN_FILE="$toolchain" \
              -DCMAKE_BUILD_TYPE=Release
cmake --build build --parallel
```

> **Contributors:** the in-tree fixture build lanes for this platform are in
> [Per-Platform Contributor Lanes](../internals/platform-lanes.md#freertos).

First Rust build pulls + cross-compiles deps (~5 min); the ELF lands at
`build/mps2-an385-freertos/target/thumbv7m-none-eabi/debug/talker`. The
C / C++ build also compiles FreeRTOS kernel + lwIP — first run ~3 min.

**Driving cargo yourself** (an IDE, a CI step, `--release`) is supported:
run `nros sync` first, then point cargo at the generated settings file. Run it
from the directory *above* the package, so the package's own `.cargo/` is not
read a second time — phase-445 W6 deletes that directory, after which the
working directory stops mattering:

```bash
cd examples/mps2-an385-freertos/rust
cargo build --manifest-path talker/Cargo.toml \
            --config talker/build/mps2-an385-freertos/nros-cargo.toml
```

If you skipped `nros sync`, the Rust build stops at its preflight with one
line naming what is missing and telling you to run sync in that directory —
not with a cargo error four frames down. The single-package C / C++ builds
above do not need sync at all: their message bindings are a CMake-time output.
See
[Workflow by Platform and Language](../user-guide/workflow-by-platform.md)
for why the requirement is per language rather than per platform.

## Run

```bash
# 1. Start the router (ROS's `rmw_zenohd`) on the host, on port 7800 —
#    the locator the Rust example bakes from its system.toml
#    ([image.mps2-an385-freertos] locator, shown above).
#    Slirp forwards guest 10.0.2.2:<p> → host:<p>.
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7800"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd &

# 2. Boot the talker in QEMU. Invoke qemu-system-arm directly, with the
#    LAN9118 + Slirp wiring the example expects — the board's own cargo
#    runner is bare `-kernel`, so booting through it gives you QEMU
#    without networking:
cd examples/mps2-an385-freertos/rust/talker
qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic \
    -icount shift=auto \
    -semihosting-config enable=on,target=native \
    -kernel build/mps2-an385-freertos/target/thumbv7m-none-eabi/debug/talker \
    -nic user,model=lan9118

# 3. Verify from stock ROS 2:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
# Talker publishes best-effort; stock `ros2 topic echo` defaults to
# RELIABLE, so the QoS-mismatched echo silently delivers nothing.
# Force best-effort to receive:
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

QEMU exits via Ctrl-A x.

> **Contributors:** the in-tree fixture run/test lanes for this platform are in
> [Per-Platform Contributor Lanes](../internals/platform-lanes.md#freertos).

**Readiness signal.** Within ~20 seconds of QEMU boot, the talker
should print `Publishing: 'Hello World: 1'` on its semihosting
stdout — the count starts at 1, matching the official ROS 2 demo
talker. QEMU cold-boot through FreeRTOS init + lwIP DHCP + zenoh
session open typically takes 10–15 s. If no `Publishing:` line in
30 seconds:

1. Confirm the router is running on the host on the port the image
   dials — the `locator` in its `system.toml` (7800 for the Rust
   talker; Slirp forwards `10.0.2.2:7800` → host:7800). Without it the talker retries the
   zenoh handshake until QEMU is killed.
2. Check the talker's early log for `lwIP DHCP timeout` or
   `Failed to open session`.
3. Bridge tip: `ros2 topic echo /chatter` from a stock ROS 2
   install (with `RMW_IMPLEMENTATION=rmw_zenoh_cpp`) confirms
   end-to-end interop.
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

Canonical, copy-out:

- Rust: [`examples/mps2-an385-freertos/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-freertos/rust/talker)
- C: [`examples/mps2-an385-freertos/c/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-freertos/c/talker)
- C++: [`examples/mps2-an385-freertos/cpp/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-freertos/cpp/talker)

## Next

- Subscriber: peer `listener/` directory next to each talker.
- Services + actions: peer `service-*/` and `action-*/` directories.
- Real hardware: same code runs on STM32F4-Discovery /
  NXP-LPC55S69 / TI-MSP432 with a different board crate + linker
  script; see the [Bare-metal Cortex-M3](./bare-metal.md) page for
  the no-RTOS variant.
- Your own board / RTOS: the
  [Board Integration matrix](../concepts/board-integration.md) maps each
  user profile (Cargo-first, vendor-IDE, Zephyr, ESP-IDF, NuttX, niche fork)
  to the shortest bring-up path.
- RTOS-specific debugging: [FreeRTOS LAN9118
  Debugging](../internals/freertos-lan9118-debugging.md).
