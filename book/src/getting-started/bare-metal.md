# Bare-metal Cortex-M3 (QEMU)

Single-node starter on **bare-metal** Cortex-M3 (QEMU MPS2-AN385) —
no RTOS, no kernel scheduler. Pure cooperative spin via
`zpico_spin_once`. Rust only. `nros-c` / `nros-cpp` are not
supported on bare-metal targets (they assume a hosted RTOS for
startup / heap / libc); see the
[examples coverage matrix](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/README.md#coverage-matrix)
for the policy.

> **When to use this path:** ultra-constrained Cortex-M0+ / M3 / M4
> targets with no OS scheduler, no `pthread`. If you have FreeRTOS
> or any RTOS, use the [FreeRTOS starter](./freertos.md) instead —
> it's more ergonomic and produces smaller code overall.

> **Prereqs.** Install the `nros` CLI once per machine, then provision
> this board. `nros setup` fetches a prebuilt bare-metal toolchain
> (`arm-none-eabi-gcc`, `qemu-system-arm`, the zenoh router) plus the
> Rust `thumbv7m-none-eabi` target into a shared store — no manual
> cross-compiler install, no ROS 2 needed.

```bash
# Build the in-tree nros CLI (Phase 218):
source ./activate.sh        # OR: direnv allow / source ./activate.fish
just setup-cli              # builds packages/cli/target/release/nros

# Provision the bare-metal Cortex-M3 board (zenoh RMW is the default):
nros setup qemu-arm-baremetal --rmw zenoh
```

> Real-board variants exist too: `nros setup mps2-an385` and
> `nros setup stm32f4` provision the same bare-metal toolchain for
> physical hardware.

## Project layout

```text
examples/qemu-arm-baremetal/rust/talker/
├── Cargo.toml                 # deps + [package.metadata.nros.deploy.qemu-mps2-an385]
├── .cargo/config.toml         # target = thumbv7m-none-eabi
│                              # runner = qemu-system-arm ... -kernel
├── package.xml
├── generated/                 # codegen output — build.rs runs
│                              #   `nros generate-rust` on first
│                              #   `cargo build`; gitignored.
└── src/                       # lib.rs component class + main.rs entry
```

The board crate is `nros-board-mps2-an385` (note: no `-freertos`
suffix — this is the bare-metal variant) which provides:

- Cortex-M3 startup + linker script
- LAN9118 driver for smoltcp
- `BoardIdle::wfi()` for cooperative wait

## Configure

Deploy config lives in the app's `Cargo.toml` and is baked at compile
time — `nros::main!()` folds it into a `DeployOverlay` the board's boot
`Config` applies. Verbatim from the in-tree
[`examples/qemu-arm-baremetal/rust/talker/Cargo.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/qemu-arm-baremetal/rust/talker/Cargo.toml):

```toml
[package.metadata.nros.deploy.qemu-mps2-an385]
locator = "tcp/10.0.2.2:7450"
ip      = "10.0.2.10"
gateway = "10.0.2.2"
netmask = "255.255.255.0"
```

QEMU Slirp networking — no host TAP / bridge / sudo. The
`zenohd` default port is 7447; this example expects **7450** so
start the router on that port (`just qemu zenohd` does) or edit the
`locator` above to match `zenohd`'s 7447 default.

## Build

```bash
cd examples/qemu-arm-baremetal/rust/talker
cargo build --release
```

First build (~5 min) cross-compiles all of nano-ros's Rust deps for
`thumbv7m-none-eabi`. Re-builds finish in seconds.

## Run

```bash
# 1. Bring up zenohd on the host (Slirp forwards 10.0.2.2:7450 → host
#    127.0.0.1:7450). The bare-metal port is 7450, NOT zenohd's default
#    7447 — edit the deploy `locator` in Cargo.toml if you want 7447.
#    The just recipe resolves the provisioned zenohd and listens there:
just qemu zenohd &

# 2. Boot the talker in QEMU. The `just qemu talker` recipe wraps
#    qemu-system-arm with the LAN9118 networking wiring the example
#    expects — it's the only working invocation for this tutorial
#    (the example's `.cargo/config.toml` runner is bare `-kernel`,
#    no `-nic socket,model=lan9118,…`, so a plain `cargo run` boots
#    QEMU without networking):
just qemu talker
# Expected serial-over-semihosting output (per src/lib.rs):
#   Publishing: 'Hello World: 1'
#   Publishing: 'Hello World: 2'
#   ...

# 3. Verify from stock ROS 2:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
# Talker publishes best-effort; stock `ros2 topic echo` defaults to
# RELIABLE, so the QoS-mismatched echo silently delivers nothing.
# Force best-effort to receive:
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

QEMU exits via Ctrl-A x.

**Readiness signal.** Within ~15 seconds of QEMU boot (no RTOS
init delay, but smoltcp + zenoh handshake still takes a few
seconds), expect `Publishing: 'Hello World: 1'` on semihosting
stdout — the count starts at 1, matching the official ROS 2 demo
talker. If no `Publishing:` line:

1. `zenohd` not running — talker spins on smoltcp poll until
   killed.
2. Wrong LAN9118 emulation flag — `qemu-system-arm` needs
   `-nic socket,model=lan9118,…` or equivalent. The example's
   `.cargo/config.toml` runner is bare `-kernel` (so a plain `cargo
   run` boots QEMU without networking); the LAN9118 wiring lives in
   the `just qemu talker` recipe (`just/qemu-baremetal.just::talker`),
   which is the working invocation for this tutorial. If you copy
   the runner out, mirror those flags.
3. Cooperative spin starvation — if you added a long-running
   callback, the entire executor stalls; bare-metal has no
   preemption.
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## GitHub source

- Bare-metal talker:
  [`examples/qemu-arm-baremetal/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/qemu-arm-baremetal/rust/talker)
- Board crate:
  [`packages/boards/nros-board-mps2-an385/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/packages/boards/nros-board-mps2-an385)

## Constraints to be aware of

- **No `alloc` by default.** Pure `no_std` + `heapless` for
  bounded collections. If you need `alloc`, opt in via the
  `alloc` feature on your board crate and supply a `#[global_allocator]`.
- **No wake primitive.** Cooperative single-thread spin only; the
  executor's `nros_platform_wake_*` slots return `Unsupported`.
- **No preemption.** A long-running user callback blocks every
  other dispatchable handle until it returns.
- **`nros-c` / `nros-cpp` NOT supported.** These wrappers assume
  hosted-RTOS libc + heap. Pure-Rust API only on this target.

For Cortex-M3 with an RTOS, switch to the
[FreeRTOS](./freertos.md) starter.

## Next

- Subscriber / service / action peers under the same
  `examples/qemu-arm-baremetal/rust/` tree.
- Wake-callback opt-in: the `wake-callback` (latency-probe) bench
  under `packages/testing/nros-bench/wake-latency-cortex-m3/` shows
  how to feed a backend's transport-notify into the cooperative
  spin loop on bare-metal.
- Real hardware: same code runs on STM32F4-Discovery with a
  different board crate (`nros-board-stm32f4`) and a different
  linker script.
