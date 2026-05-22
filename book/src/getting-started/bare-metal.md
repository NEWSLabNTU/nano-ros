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

> **Prereqs.** Clone with `just setup` already run.
> Need `qemu-system-arm` + the Rust `thumbv7m-none-eabi` target.

## Project layout

```text
examples/qemu-arm-baremetal/rust/talker/
├── Cargo.toml
├── .cargo/config.toml         # target = thumbv7m-none-eabi
│                              # runner = qemu-system-arm ... -kernel
├── config.toml                # network + zenoh
├── package.xml
├── generated/                 # codegen output (gitignored)
└── src/main.rs                # #[entry] fn main() -> !
```

The board crate is `nros-board-mps2-an385` (note: no `-freertos`
suffix — this is the bare-metal variant) which provides:

- Cortex-M3 startup + linker script
- LAN9118 driver for smoltcp
- `BoardIdle::wfi()` for cooperative wait

## Configure

Mirror of the in-tree
[`config.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/qemu-arm-baremetal/rust/talker/config.toml):

```toml
[network]
ip       = "10.0.2.10"
mac      = "02:00:00:00:00:00"
gateway  = "10.0.2.2"
prefix   = 24                       # NOT `netmask` — Config::from_toml
                                    # parses `prefix` (CIDR) only.

[zenoh]
locator   = "tcp/10.0.2.2:7450"     # bare-metal test-fixture port
domain_id = 0
```

QEMU Slirp networking — no host TAP / bridge / sudo. The
`zenohd` default port is 7447; this example expects **7450** so
start the router with `zenohd run --listen tcp/127.0.0.1:7450`
(or edit `config.toml` to match `zenohd`'s 7447 default).

## Build

```bash
cd examples/qemu-arm-baremetal/rust/talker
cargo build --release
```

First build (~5 min) cross-compiles all of nano-ros's Rust deps for
`thumbv7m-none-eabi`. Re-builds finish in seconds.

## Run

```bash
# 1. Bring up zenohd on the host (Slirp forwards 10.0.2.2:7447):
just zenohd run

# 2. Boot the talker in QEMU. The .cargo/config.toml runner does:
cd examples/qemu-arm-baremetal/rust/talker
cargo run --release
# Expected serial-over-semihosting output:
#   nros Bare-Metal Cortex-M3 Talker
#   Published: 1
#   Published: 2
#   ...

# 3. Verify from stock ROS 2:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32
```

QEMU exits via Ctrl-A x.

**Readiness signal.** Within ~15 seconds of QEMU boot (no RTOS
init delay, but smoltcp + zenoh handshake still takes a few
seconds), expect `Published: 1` on semihosting stdout. If no
`Published:` line:

1. `zenohd` not running — talker spins on smoltcp poll until
   killed.
2. Wrong LAN9118 emulation flag — `qemu-system-arm` needs
   `-nic socket,model=lan9118,…` or equivalent; the runner in
   `.cargo/config.toml` already supplies it.
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
  different board crate (`nros-board-stm32f4-nucleo`) and a
  different linker script.
