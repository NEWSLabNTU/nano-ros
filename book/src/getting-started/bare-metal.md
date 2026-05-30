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
# Install the nros CLI once per machine:
curl -fsSL https://raw.githubusercontent.com/NEWSLabNTU/nano-ros/main/scripts/install-nros.sh | sh
export PATH="$HOME/.nros/bin:$PATH"

# Provision the bare-metal Cortex-M3 board (zenoh RMW is the default):
nros setup qemu-arm-baremetal --rmw zenoh
```

> Real-board variants exist too: `nros setup mps2-an385` and
> `nros setup stm32f4` provision the same bare-metal toolchain for
> physical hardware.

## Project layout

```text
examples/qemu-arm-baremetal/rust/talker/
├── Cargo.toml
├── .cargo/config.toml         # target = thumbv7m-none-eabi
│                              # runner = qemu-system-arm ... -kernel
├── nros.toml                  # [node] + [[transport]] (locator inside)
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
[`nros.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/qemu-arm-baremetal/rust/talker/nros.toml):

```toml
[node]
domain_id = 0

# Single ethernet transport running a zenoh session. `ip` is CIDR
# (address/prefix); the locator rides the transport.
[[transport]]
kind    = "ethernet"
ip      = "10.0.2.10/24"            # CIDR — address + prefix in one
mac     = "02:00:00:00:00:00"
gateway = "10.0.2.2"
rmw     = "zenoh"
locator = "tcp/10.0.2.2:7450"       # bare-metal test-fixture port
```

QEMU Slirp networking — no host TAP / bridge / sudo. The bare-metal
fixture port is **7450** (NOT zenohd's default 7447); start the
router with `zenohd --listen tcp/127.0.0.1:7450` or edit
`nros.toml` to match the port your zenohd actually listens on.

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
#    127.0.0.1:7450). The bare-metal test-fixture port is 7450, NOT
#    zenohd's default 7447 — edit `nros.toml` if you want 7447 instead.
just qemu zenohd                  # or: zenohd --listen tcp/127.0.0.1:7450

# 2. Boot the talker in QEMU. `just qemu talker` runs the example
#    via cargo's `.cargo/config.toml` runner (= qemu-system-arm with
#    the right -machine / -cpu / -kernel flags).
just qemu talker
# Expected serial-over-semihosting output:
#   nros QEMU Platform
#   Published: 0
#   Published: 1
#   ...

# 3. Verify from stock ROS 2:
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/Int32
```

QEMU exits via Ctrl-A x.

**Readiness signal.** Within ~15 seconds of QEMU boot (no RTOS
init delay, but smoltcp + zenoh handshake still takes a few
seconds), expect `Published: 0` on semihosting stdout. If no
`Published:` line:

1. `zenohd` not running — talker spins on smoltcp poll until
   killed.
2. QEMU NIC mismatch — the `.cargo/config.toml` runner relies on
   QEMU's default NIC for the MPS2-AN385 machine (the runner does
   NOT add a `-nic` flag). If you invoke `qemu-system-arm`
   directly with a `-nic socket,…` form, the talker can't reach
   Slirp on `10.0.2.2`. Use `-nic user,model=lan9118` (Slirp) for
   a direct invocation.
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
