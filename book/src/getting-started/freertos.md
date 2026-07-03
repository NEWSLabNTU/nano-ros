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
│   ├── Cargo.toml
│   ├── .cargo/config.toml          # target + QEMU runner
│   ├── nros.toml                   # network + zenoh locator + scheduling
│   ├── package.xml
│   ├── generated/                  # codegen output — build.rs runs
│   │                               #   `nros generate-rust` on first
│   │                               #   `cargo build`; gitignored.
│   └── src/main.rs
├── c/talker/                 # CMake project, add_subdirectory consumption
│   ├── CMakeLists.txt
│   ├── nros.toml
│   ├── package.xml
│   └── src/main.c
└── cpp/talker/               # CMake C++14 project
    ├── CMakeLists.txt
    ├── nros.toml
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

Network + Zenoh + scheduling live in `nros.toml` (parsed by the
board crate at boot). Verbatim from the in-tree
[`examples/qemu-arm-freertos/rust/talker/nros.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/qemu-arm-freertos/rust/talker/nros.toml):

```toml
# nano-ros config (direct mode). See
# docs/design/0004-configuration-and-transports.md.

[node]
domain_id = 0

[[transport]]
kind    = "ethernet"
ip      = "10.0.2.20/24"
mac     = "02:00:00:00:00:00"
gateway = "10.0.2.2"
locator = "tcp/10.0.2.2:7451"

[node.rt]
app_priority = 12
app_stack_bytes = 262144
zenoh_read_priority = 16
zenoh_read_stack_bytes = 5120
zenoh_lease_priority = 16
zenoh_lease_stack_bytes = 5120
poll_priority = 16
poll_interval_ms = 5
```

The `10.0.2.0/24` subnet is QEMU Slirp's default; `10.0.2.2` is the
Slirp gateway that forwards to host loopback. No TAP, no sudo.

Per-language test-fixture ports: Rust → 7451, C → 7551, C++ → 7651.
The talker / listener under each `<lang>/` uses the matching port;
start `zenohd` on the one you intend to test against. The C / C++
example trees ship their own `nros.toml` with the matching port.

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
# 1. Start zenohd on the host (Slirp forwards 10.0.2.2:7451 → host:7451).
#    The just recipe wraps `zenohd --listen tcp/127.0.0.1:7451
#    --no-multicast-scouting` (the D.2 PATH shim resolves zenohd from
#    `~/.nros/sdk/zenohd/<v>/bin/zenohd`):
just freertos zenohd &
# Equivalent, if the recipe isn't available or you want the literal
# invocation (works as long as `zenohd` is on PATH):
zenohd --listen tcp/127.0.0.1:7451 --no-multicast-scouting &

# 2. Boot the talker in QEMU. The just recipe wraps qemu-system-arm
#    with the LAN9118 + Slirp wiring the example expects; works for
#    Rust as well (it builds + boots the in-tree binary):
just freertos talker
# Or, single-language, in the Rust example dir:
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
- RTOS-specific debugging: [FreeRTOS LAN9118
  Debugging](../internals/freertos-lan9118-debugging.md).
