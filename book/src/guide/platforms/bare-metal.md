# Bare-metal Cortex-M3 (QEMU)

<!-- type: how-to · level: 2 -->

> **Support level:** [`mps2-an385-baremetal`](../../reference/boards/mps2-an385.md). Each page states what nano-ros can show for the board (where it runs, how often, and its breadth), generated from the board support registry; [Board Support Levels](../../reference/board-support-levels.md) explains the words.

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

> **Prereqs.** Install the `nros` CLI once per machine (`scripts/install.sh`,
> see [Installation](../../start/install.md)), then provision this board.
> `nros setup` fetches a prebuilt bare-metal toolchain
> (`arm-none-eabi-gcc`, `qemu-system-arm`) plus the Rust
> `thumbv7m-none-eabi` target into a shared store — no manual
> cross-compiler install. It provides no zenoh router: that is ROS's
> `rmw_zenohd` (see [the RMW design](../../extending/rmw-design.md)), so running a zenoh image
> needs it from a ROS 2 install.

```bash probe=30
# Provision the bare-metal Cortex-M3 board (zenoh RMW is the default):
nros setup mps2-an385-baremetal --rmw zenoh
```

> One key, not two: `qemu-arm-baremetal` is a retired spelling of the same
> entry. A board name states the reach of its build, and this build is true
> for one silicon part. Nothing extra is provisioned for physical hardware —
> no probe/flash tooling. For a real STM32F4 see the
> [out-of-tree worked example](../../extending/stm32f4-out-of-tree.md).

## Project layout

Scaffold a workspace for the board:

```bash probe=40
nros new bm_ws --board mps2-an385-baremetal --template talker --lang rust --rmw zenoh
cd bm_ws
```

```text
bm_ws/
└── src/
    ├── bm_node/                 # node package: package.xml, Cargo.toml (a library), src/lib.rs
    └── bm_node_bringup/         # package.xml and system.toml: the board, the RMW, the node
```

`bm_node` is the talker, a library with no `main()` that names no board. The
bringup's `system.toml` holds one image, `[image.baremetal]`. There is no
`.cargo/` to edit: the target triple, the linker script, the `--gc-sections`
flag, the QEMU runner and the `[patch.crates-io]` rows that resolve nano-ros's
crates all come from the board, and `nros build` writes them under `build/`
together with the generated entry it compiles.

The board crate is `nros-board-mps2-an385` (note: no `-freertos`
suffix — this is the bare-metal variant) which provides:

- Cortex-M3 startup + linker script
- LAN9118 driver for smoltcp
- `BoardIdle::wfi()` for cooperative wait

### The entry shape

The image's `board` line names the board, and the generated entry depends on
the board crate with the feature that shape needs. Pick it there — never a
different board crate, and never an edit to the node package:

| Entry shape | `[image.<id>] board =` | board crate feature the entry enables |
|---|---|---|
| direct-exec (inline spin loop) | `"mps2-an385-baremetal"` | `board-entry` |

Which network the defaults describe is stated by the board: its
`nros-board.toml` says `[board.net] network = "qemu"`, so `Config::default()`
sits on QEMU's own user-mode network (`10.0.2.15/24`, gateway and router
`10.0.2.2`) — the network every QEMU board shares. `Config::qemu_slirp()` is
the same network at a second address (`10.0.2.10`, router port `7450`). Either way, an image that sets its `locator` in its `[image.<id>]`
and its address in a board network file (`[eth0] network = "qemu"`, `host = N`)
table overrides the default — which every in-tree example
does, and which you should too. The address goes in the image's board network
file, named by `os_config` (below).

## Configure

The whole deployment statement is the bringup's `system.toml`. It is baked at
compile time: the generated entry folds the image's network identity into a
`DeployOverlay` the board's boot `Config` applies. With a `locator` and an
address added, `src/bm_node_bringup/system.toml` reads:

```toml
[system]
name = "bm_node"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "bm_node"
class = "bm_node::Talker"
name = "bm_node"
dispatch = "deferred"

[image.baremetal]
board = "mps2-an385-baremetal"
locator = "tcp/10.0.2.2:10500"
os_config = ["board-net.toml"]
```

and the image's address sits in the board network file it names,
`board-net.toml` beside `system.toml` in the bringup (the board package owns
the device, so its address lives in the board's own file):

```toml
[eth0]
network = "qemu"
host = 10
```

`board` picks the silicon and everything that follows from it; `rmw` picks the
backend. Everything the board implies — triple, linker script, runner, cross
compiler, `[patch.crates-io]`, the board crate and its features — follows the
`board =` line on the next `nros build`, so no linker flag and no triple ever
moves by hand. The node package's `Cargo.toml` names no board crate, RMW
backend or panic handler: those are the generated entry's dependencies. So
changing `board =` to `"mps2-an385-freertos"` and building again builds the
FreeRTOS image with no other edit.

QEMU Slirp networking — no host TAP / bridge / sudo. The zenoh
default port is 7447; this example dials **10500** (the `locator`
above), so start the router (ROS's `rmw_zenohd`) on that port:

```bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:10500"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

or edit the `locator` above to the port you prefer.

## Build

The build generates the message bindings from your ROS 2 install, so source
it in this shell first. This first build records the ROS 2 prefix in the
workspace, so later builds and runs of it need no sourced shell.

In the workspace root (where the scaffold step left you):

```bash probe=50
source /opt/ros/humble/setup.bash
nros build           # builds the image `[image.baremetal]` declares
```

`nros build` prepares the build first (message bindings, the generated entry
for `[image.baremetal]`, the cargo settings the board implies), then runs
cargo. There is no step to run before it.

First build (~5 min) cross-compiles all of nano-ros's Rust deps for
`thumbv7m-none-eabi`. Re-builds finish in seconds. The ELF lands under
`build/`, never beside your sources; `nros build` prints its path and a `run:`
line when it finishes.

**Driving cargo yourself** (an IDE, a CI step, `--release`) is supported:
`nros build --dry-run` prints the exact cargo line, against the generated
entry under `build/`. To write that entry and its settings without building,
before you open the workspace in an IDE, run:

```bash
nros configure
```

What it writes is
regenerated on every build; never edit it, and never add a `.cargo/config.toml`
of your own to carry board facts. Your own Rust-toolchain preferences go where
cargo already looks for them — a parent directory or `$CARGO_HOME`. See
[When `nros build` drives the build](../../targets/how-integration-works.md#when-nros-build-drives-the-build).

## Run on QEMU

Three terminals: the router, the board, and a stock ROS 2 peer.

**1. The router** — ROS's `rmw_zenohd`, on the port your image's `locator`
names (Slirp forwards the guest's `10.0.2.2:<p>` to the host's `<p>`). The
scaffold states no `locator`, so the board dials `10.0.2.2:7447`; the example
above dials 10500. It must listen on `0.0.0.0` for the Slirp gateway:

```bash probe=60 probe-run=60 probe-keep
source /opt/ros/humble/setup.bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/0.0.0.0:7447"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

```text probe-expect=60
Started Zenoh router with id
```

**2. The board** — from the workspace root:

```bash probe=70 probe-run=120 probe-keep
nros run bm_node
```

`nros run` rebuilds if anything changed, then boots the talker in QEMU with the
board's runner: the patched `qemu-system-arm` that `nros setup
mps2-an385-baremetal` provisioned into the SDK store, the LAN9118 on Slirp, and
`-icount shift=auto`, which keeps the board's timer in step with Slirp's I/O.
It streams the semihosting console until you press Ctrl-C.

Expected serial-over-semihosting output:

```text probe-expect=70
Publishing: 'Hello World: 1'
Publishing: 'Hello World: 2'
...
```

**3. A stock ROS 2 peer.** The talker publishes best-effort; stock `ros2 topic
echo` defaults to RELIABLE, so the QoS-mismatched echo silently delivers
nothing. Force best-effort to receive:

```bash probe=80 probe-run=90
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
```

```text probe-expect=80
data: 'Hello World:
```

Ctrl-C stops `nros run` and QEMU with it.

**Readiness signal.** Within ~15 seconds of QEMU boot (no RTOS
init delay, but smoltcp + zenoh handshake still takes a few
seconds), expect `Publishing: 'Hello World: 1'` on semihosting
stdout — the count starts at 1, matching the official ROS 2 demo
talker. If no `Publishing:` line:

1. Router not running — talker spins on smoltcp poll until
   killed.
2. No network on the guest — if you boot the ELF with your own QEMU
   line, an explicit `-nic`/`-netdev` that does not reach the LAN9118
   (or `-nic none`, `-nodefaults`) leaves it unwired. The board's
   runner passes `-nic user,model=lan9118`.
3. Cooperative spin starvation — if you added a long-running
   callback, the entire executor stalls; bare-metal has no
   preemption.
4. See [Troubleshooting — First 10 Minutes](../../get-started/first-10-minutes.md).

## Flash a real board

Nothing in-tree is flashed: `mps2-an385` is a QEMU machine, and `nros setup`
provisions no probe or flash tooling for it. A real Cortex-M board is a board
crate of your own — [Worked Example — STM32F4 Out of Tree](../../extending/stm32f4-out-of-tree.md)
takes one through the whole ladder — and its ELF goes to the board with the
probe tool its vendor supports. Add an image for it to the bringup
(`[image.stm32f4] board = "<your board id>"`); once that board package declares
its flash runner, `nros flash` names the image:

```bash
nros flash bm_node_bringup:stm32f4
# or by hand, on the ELF path `nros build` printed:
#   probe-rs run --chip STM32F407VGTx <elf>
#   openocd -f interface/stlink.cfg -f target/stm32f4x.cfg \
#       -c "program <elf> verify reset exit"
```

> **Not replayed.** No probe runs this flash line (a CI runner has no board), so it is written from the board's runner, not measured ([tracking issue](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/archived/2176-unprobed-fvp-sitl-flash-lines.md)).

The chip, interface and triple above are the STM32F4-Discovery's; yours come
from your board.

## GitHub source

- Bare-metal talker, the copy-out example for this board and the same
  project as the one this page builds:
  [`examples/mps2-an385-baremetal/rust/talker/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-baremetal/rust/talker)
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

- Subscriber / service / action: the bare-metal image rows of the Rust
  language workspace, `examples/workspaces/rust`, under `pubsub_bringup`,
  `service_bringup` and `action_bringup`. The serial talker and listener are
  image rows there too, stating `transport = "serial"` over the same
  packages.
- Wake-callback opt-in: the `wake-callback` (latency-probe) bench
  under `packages/testing/nros-bench/wake-latency-cortex-m3/` shows
  how to feed a backend's transport-notify into the cooperative
  spin loop on bare-metal.
- Real hardware: the same code runs on an STM32F4-Discovery with a board
  crate of your own and a different linker script — see
  [Worked Example — STM32F4 Out of Tree](../../extending/stm32f4-out-of-tree.md),
  which takes that board through the customization ladder end to end.

## Design background

- [RFC-0122](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0122-behaviour-bringups-road-witnesses-cadence-backed-ci.md): behaviour bringups, single-node witnesses, and cadence-backed CI.
  The copy-out talker per platform and language, and the paired examples,
  variants and configurations as image rows of behaviour bringups.
