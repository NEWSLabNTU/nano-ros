# FreeRTOS (QEMU MPS2-AN385)

Single-node starter on FreeRTOS + lwIP, cross-compiled for Cortex-M3
and booted in QEMU MPS2-AN385. Slirp networking; no host TAP /
bridge / sudo. Rust, C, and C++ talkers all live in-tree.

> **Prereqs.** `nros setup qemu-arm-freertos` is the single command
> that prepares your machine for this board. It fetches a prebuilt
> toolchain set into the shared store at `~/.nros/sdk` — the
> `arm-none-eabi-gcc` cross-compiler, the patched
> `qemu-system-arm` emulator, the FreeRTOS kernel + lwIP sources,
> and the RMW host daemon. You do **not** hand-install a
> cross-toolchain and you do **not** need a ROS 2 install.

## Setup

Build the in-tree `nros` CLI (Phase 218):

```bash
source ./activate.sh        # OR: direnv allow / source ./activate.fish
just setup-cli              # builds packages/cli/target/release/nros
```

Then provision the board (`--rmw` defaults to `zenoh`; pick `xrce`
or `cyclonedds` to match the example you intend to run):

```bash
nros setup qemu-arm-freertos --rmw zenoh
```

This fetches the cross-compiler, the patched `qemu-system-arm`, the
FreeRTOS + lwIP sources, and the RMW host daemon (`zenohd` for
zenoh, the Micro-XRCE-DDS agent for xrce) into `${NROS_HOME:-~/.nros}/sdk`.

## Project layout

Each language uses the standard nano-ros canonical example shape —
standalone Cargo (Rust) or CMake (C / C++) project under
`examples/qemu-arm-freertos/<lang>/<example>/`.

```text
examples/qemu-arm-freertos/
├── rust/talker/             # Cargo package, cross-compile target = thumbv7m-none-eabi
│   ├── Cargo.toml                  # deps + [package.metadata.nros.deploy.freertos]
│   ├── .cargo/config.toml          # target + QEMU runner
│   ├── package.xml
│   ├── generated/                  # codegen output — build.rs runs
│   │                               #   `nros generate-rust` on first
│   │                               #   `cargo build`; gitignored.
│   └── src/lib.rs                  # the component class; nros::main! generates the entry
├── c/talker/                 # CMake project, add_subdirectory consumption
│   ├── CMakeLists.txt              # targets (deploy tuple in package.xml)
│   ├── package.xml
│   └── src/Talker.c
└── cpp/talker/               # CMake C++14 project
    ├── CMakeLists.txt
    ├── package.xml
    └── src/Talker.cpp
```

The Rust `Cargo.toml` pulls the FreeRTOS board crate
(`nros-board-mps2-an385-freertos`) which wraps the kernel + lwIP +
LAN9118 driver build. The C / C++ `CMakeLists.txt` follows the
canonical `add_subdirectory(<repo-root>) +
nano_ros_link_rmw(<target> RMW zenoh)` pattern with
`NANO_ROS_BOARD = mps2-an385-freertos`.

## Configure

Deploy config (router locator, domain, RMW) is declared in the build
manifest and **baked at compile time** — there is no config file on the
device. Verbatim from the in-tree
[`examples/qemu-arm-freertos/rust/talker/Cargo.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/qemu-arm-freertos/rust/talker/Cargo.toml):

```toml
[package.metadata.nros.deploy.freertos]
board     = "qemu-mps2-an385"
rmw       = "zenoh"
domain_id = 0
locator   = "tcp/10.0.2.2:7447"
```

The C / C++ trees declare the same in their `package.xml` `<export>` tuple
(RFC-0048 §4); the connect locator rides the build config
(`-DNROS_ENTRY_LOCATOR` / the fixture row), not the tuple:

```xml
<export>
  <build_type>ament_cmake</build_type>
  <nano_ros deploy="freertos" board="mps2-an385-freertos" rmw="zenoh"/>
</export>
```

Task stacks / priorities come from the board crate's defaults (Cargo
features), not a config file — see the
[Configuration Guide](../user-guide/configuration.md).

The `10.0.2.0/24` subnet is QEMU Slirp's default; `10.0.2.2` is the
Slirp gateway that forwards to host loopback. No TAP, no sudo.

Ports: the shipped examples dial host port **7447**. The prebuilt
*test fixtures* bake per-language ports instead (Rust → 7451, C → 7551,
C++ → 7651) so suites run in parallel; `just freertos talker` boots the
fixture, so pair it with `just freertos zenohd` (listens on 7451).

## Build

```bash
# Rust:
cd examples/qemu-arm-freertos/rust/talker
cargo build --release

# C / C++ — use the cross-toolchain CMake invocation:
just freertos build-fixtures        # builds every in-tree zenoh +
                                    # DDS example across Rust / C / C++
# Or single-example (the `nros` CLI on PATH auto-resolves the codegen
# tool — no `-D_NANO_ROS_CODEGEN_TOOL=` needed):
toolchain="$(pwd)/cmake/toolchain/arm-freertos-armcm3.cmake"
cd examples/qemu-arm-freertos/c/talker
cmake -B build -DCMAKE_TOOLCHAIN_FILE="$toolchain" \
              -DCMAKE_BUILD_TYPE=Release
cmake --build build --parallel
```

First Rust build pulls + cross-compiles deps (~5 min). C / C++ build
also compiles FreeRTOS kernel + lwIP — first run ~3 min.

## Run

```bash
# 1. Start zenohd on the host. The recipe resolves the provisioned
#    binary (build/zenohd/ or ~/.nros/sdk) and listens on the fixture
#    port 7451 (Slirp forwards guest 10.0.2.2:<p> → host:<p>):
just freertos zenohd &

# 2. Boot the talker fixture in QEMU. The just recipe wraps
#    qemu-system-arm with the LAN9118 + Slirp wiring the example expects:
just freertos talker

# Or run the copy-out example by hand — it dials host port 7447
# (its deploy locator above), so start the router there instead:
just native zenohd &   # listens on 7447
cd examples/qemu-arm-freertos/rust/talker
cargo run --release

# 3. Verify from stock ROS 2:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
# Talker publishes best-effort; stock `ros2 topic echo` defaults to
# RELIABLE, so the QoS-mismatched echo silently delivers nothing.
# Force best-effort to receive:
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

QEMU exits via Ctrl-A x.

For batch testing without manual QEMU launches: `just freertos
test` runs every E2E (pub/sub, service, action) against a temporary
in-test zenohd.

**Readiness signal.** Within ~20 seconds of QEMU boot, the talker
should print `Publishing: 'Hello World: 1'` on its semihosting
stdout — the count starts at 1, matching the official ROS 2 demo
talker. QEMU cold-boot through FreeRTOS init + lwIP DHCP + zenoh
session open typically takes 10–15 s. If no `Publishing:` line in
30 seconds:

1. Confirm `zenohd` is running on the host (Slirp forwards
   `10.0.2.2:7451` → host:7451). Without it the talker retries the
   zenoh handshake until QEMU is killed.
2. Check the talker's early log for `lwIP DHCP timeout` or
   `Failed to open session`.
3. Bridge tip: `ros2 topic echo /chatter` from a stock ROS 2
   install (with `RMW_IMPLEMENTATION=rmw_zenoh_cpp`) confirms
   end-to-end interop.
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

Canonical, copy-out:

- Rust: [`examples/qemu-arm-freertos/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-arm-freertos/rust/talker)
- C: [`examples/qemu-arm-freertos/c/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-arm-freertos/c/talker)
- C++: [`examples/qemu-arm-freertos/cpp/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-arm-freertos/cpp/talker)

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
