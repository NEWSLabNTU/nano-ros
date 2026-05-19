# FreeRTOS (QEMU MPS2-AN385)

Single-node starter on FreeRTOS + lwIP, cross-compiled for Cortex-M3
and booted in QEMU MPS2-AN385. Slirp networking; no host TAP /
bridge / sudo. Rust, C, and C++ talkers all live in-tree.

> **Prereqs.** Clone with `just setup tier=default` already run. Need
> `arm-none-eabi-gcc` + `qemu-system-arm` on `PATH`. `just freertos
> setup` fetches the FreeRTOS kernel + lwIP sources under
> `third-party/freertos/`.

## Project layout

Each language uses the standard nano-ros canonical example shape —
standalone Cargo (Rust) or CMake (C / C++) project under
`examples/qemu-arm-freertos/<lang>/zenoh/<example>/`.

```text
examples/qemu-arm-freertos/
├── rust/zenoh/talker/             # Cargo package, cross-compile target = thumbv7m-none-eabi
│   ├── Cargo.toml
│   ├── .cargo/config.toml          # target + QEMU runner
│   ├── config.toml                 # network + zenoh locator + scheduling
│   ├── package.xml
│   ├── generated/                  # codegen output (gitignored)
│   └── src/main.rs
├── c/zenoh/talker/                 # CMake project, add_subdirectory consumption
│   ├── CMakeLists.txt
│   ├── config.toml
│   ├── package.xml
│   └── src/main.c
└── cpp/zenoh/talker/               # CMake C++14 project
    ├── CMakeLists.txt
    ├── config.toml
    ├── package.xml
    └── src/main.cpp
```

The Rust `Cargo.toml` pulls the FreeRTOS board crate
(`nros-board-mps2-an385-freertos`) which wraps the kernel + lwIP +
LAN9118 driver build. The C / C++ `CMakeLists.txt` follows the
canonical `add_subdirectory(<repo-root>) +
nano_ros_link_rmw(<target> RMW zenoh)` pattern with
`NANO_ROS_BOARD = mps2-an385-freertos`.

## Configure

Network + Zenoh + scheduling live in `config.toml` (parsed by the
board crate at boot):

```toml
[network]
ip       = "10.0.2.21"
mac      = "02:00:00:00:00:01"
gateway  = "10.0.2.2"          # QEMU Slirp gateway = host loopback
netmask  = "255.255.255.0"

[zenoh]
locator   = "tcp/10.0.2.2:7447"   # host's zenohd reached via Slirp
domain_id = 0

[scheduling]
app_priority           = 12
app_stack_bytes        = 262144
zenoh_read_priority    = 16
zenoh_lease_priority   = 16
poll_priority          = 16
poll_interval_ms       = 5
```

The `10.0.2.0/24` subnet is QEMU Slirp's default; `10.0.2.2` is the
Slirp gateway that forwards to host loopback. No TAP, no sudo.

## Build

```bash
# Rust:
cd examples/qemu-arm-freertos/rust/zenoh/talker
cargo build --release

# C / C++ — use the cross-toolchain CMake invocation:
just freertos build-fixtures        # builds all 12 (Rust + C + C++) at once
# Or single-example:
toolchain="$(pwd)/cmake/toolchain/arm-freertos-armcm3.cmake"
codegen="$(pwd)/packages/codegen/packages/target/release/nros-codegen"
cd examples/qemu-arm-freertos/c/zenoh/talker
cmake -B build -DCMAKE_TOOLCHAIN_FILE="$toolchain" \
              -DCMAKE_BUILD_TYPE=Release \
              -D_NANO_ROS_CODEGEN_TOOL="$codegen"
cmake --build build --parallel
```

First Rust build pulls + cross-compiles deps (~5 min). C / C++ build
also compiles FreeRTOS kernel + lwIP — first run ~3 min.

## Run

```bash
# 1. Start zenohd on the host (Slirp forwards 10.0.2.2:7447 → host:7447):
just zenohd                               # or: ./build/zenohd/zenohd

# 2. Boot the talker in QEMU:
cd examples/qemu-arm-freertos/rust/zenoh/talker
cargo run --release
# Or for C / C++:
qemu-system-arm -cpu cortex-m3 -machine mps2-an385 \
                -nographic -semihosting-config enable=on,target=native \
                -nic socket,model=lan9118,listen=:6666 \
                -kernel ./build/c_talker

# 3. Verify from stock ROS 2:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32
```

QEMU exits via Ctrl-A x.

For batch testing without manual QEMU launches: `just freertos
test` runs every E2E (pub/sub, service, action) against a temporary
in-test zenohd.

## GitHub source

Canonical, copy-out:

- Rust: [`examples/qemu-arm-freertos/rust/zenoh/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-arm-freertos/rust/zenoh/talker)
- C: [`examples/qemu-arm-freertos/c/zenoh/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-arm-freertos/c/zenoh/talker)
- C++: [`examples/qemu-arm-freertos/cpp/zenoh/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-arm-freertos/cpp/zenoh/talker)

## Next

- Subscriber: peer `listener/` directory next to each talker.
- Services + actions: peer `service-*/` and `action-*/` directories.
- Real hardware: same code runs on STM32F4-Discovery /
  NXP-LPC55S69 / TI-MSP432 with a different board crate + linker
  script; see the [Bare-metal Cortex-M3](./bare-metal.md) page for
  the no-RTOS variant.
- RTOS-specific debugging: [FreeRTOS LAN9118
  Debugging](../internals/freertos-lan9118-debugging.md).
