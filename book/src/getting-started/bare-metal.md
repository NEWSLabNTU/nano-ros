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

> **Prereqs.** Install the `nros` CLI once per machine (`scripts/install.sh`,
> see [Installation](installation.md)), then provision this board.
> `nros setup` fetches a prebuilt bare-metal toolchain
> (`arm-none-eabi-gcc`, `qemu-system-arm`) plus the Rust
> `thumbv7m-none-eabi` target into a shared store — no manual
> cross-compiler install. It provides no zenoh router: that is ROS's
> `rmw_zenohd` ([RFC-0075](../design/rmw.md)), so running a zenoh image
> needs it from a ROS 2 install.

```bash probe=30
# Provision the bare-metal Cortex-M3 board (zenoh RMW is the default):
nros setup mps2-an385-baremetal --rmw zenoh
```

> One key, not two. `qemu-arm-baremetal` and `mps2-an385` were the same
> entry under two spellings until phase-437 W5 collapsed them (RFC-0093:
> a board name states the reach of its build, and this build is true for
> one silicon part). Nothing extra is provisioned for physical hardware —
> no probe/flash tooling. For a real STM32F4 see the
> [out-of-tree worked example](../porting/stm32f4-out-of-tree.md).

## Project layout

Scaffold a single Rust package for the board:

```bash probe=40
nros new bm_talker --platform baremetal --lang rust --rmw zenoh
cd bm_talker
```

```text
bm_talker/
├── system.toml                # WHAT this deploys to — the one file you edit
├── Cargo.toml                 # Rust deps only
├── package.xml                # ROS-style manifest (drives codegen tooling)
├── generated/                 # message bindings `nros sync` writes
├── build/                     # everything `nros sync` / `nros build` generate
└── src/main.rs
```

There is no `.cargo/` to edit. The target triple, the linker script, the
`--gc-sections` flag, the QEMU runner and the `[patch.crates-io]` rows that
resolve nano-ros's crates all come from the board, and `nros sync` writes them
into `build/<image>/nros-cargo.toml` — a generated file `nros build` hands
cargo with `--config` (RFC-0098). The scaffold's image is
`[image.baremetal]`.

The board crate is `nros-board-mps2-an385` (note: no `-freertos`
suffix — this is the bare-metal variant) which provides:

- Cortex-M3 startup + linker script
- LAN9118 driver for smoltcp
- `BoardIdle::wfi()` for cooperative wait

### Direct-exec or RTIC — one crate, two entry shapes

The same crate serves both bare-metal entry models. Both are names of the
*same* board descriptor, so you pick the entry shape in `system.toml` and the
matching Cargo feature — never a different board crate:

| Entry shape | `[image.<id>] board =` | board dep |
|---|---|---|
| direct-exec (inline spin loop) | `"qemu-mps2-an385"` | `nros-board-mps2-an385 = { version = "*", features = ["board-entry"] }` |
| RTIC (framework-owned `#[rtic::app]`, deferred dispatch) | `"rtic-mps2-an385"` | `nros-board-mps2-an385 = { version = "*", features = ["rtic"] }` |

The in-tree RTIC peers (`listener-rtic/`, `service-client-rtic/`,
`action-server-rtic/`, …) differ from their direct-exec siblings in exactly
those two lines.
(This talker no longer writes the row at all: its generated selection package,
described below, derives `board-entry` or `rtic` from the board name.)

The RTIC surface used to be a separate `nros-board-rtic-mps2-an385`
crate; phase-337 W6.a folded it in, because it depended on this crate
and re-declared its `Config` — the two copies had drifted to different
default IPs, which nothing catches until a node fails to reach the
router.

Which network the defaults describe is stated by the board: its
`nros-board.toml` says `[board.net] network = "qemu"`, so `Config::default()`
sits on QEMU's own user-mode network (`10.0.2.15/24`, gateway and router
`10.0.2.2`) — the network every QEMU board shares. `Config::qemu_slirp()` is
the same network at the RTIC entry's own address (`10.0.2.10`, router port
`7450`). Either way, an image that sets its `locator` in its `[image.<id>]`
and its address in a board network file (`[eth0] network = "qemu"`, `host = N`)
table overrides the default — which every in-tree example
does, and which you should too. The address goes in the image's board network
file, named by `os_config` (below).

## Configure

The whole deployment statement is `system.toml`, beside the manifest — the
same schema a multi-node workspace's bringup package uses. It is baked at
compile time: `nros::main!()` folds the image's network identity into a
`DeployOverlay` the board's boot `Config` applies. For example,
[`examples/mps2-an385-baremetal/rust/talker/system.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/mps2-an385-baremetal/rust/talker/system.toml)
on GitHub:

```toml
[system]
name = "qemu_bsp_talker"
rmw = "zenoh"
domain_id = 0

[[component]]
pkg = "qemu_bsp_talker"
class = "qemu_bsp_talker::Talker"
name = "talker"
dispatch = "deferred"

[image.mps2-an385-baremetal]
board = "qemu-mps2-an385"
locator = "tcp/10.0.2.2:10500"
os_config = ["board-net.toml"]
```

and the image's address sits in the board network file it names,
`board-net.toml` beside `system.toml` (RFC-0109 D4 — the board package owns
the device, so its address lives in the board's own file):

```toml
[eth0]
network = "qemu"
host = 10
```

`board` picks the silicon and everything that follows from it; `rmw` picks the
backend. Everything the board implies — triple, linker script, runner, cross
compiler, `[patch.crates-io]` — follows the `board =` line on the next
`nros sync`, so no linker flag and no triple ever moves by hand.

That includes the board crate. The example's `Cargo.toml` names no board: its
image-only dependencies (the board crate with the features its entry needs,
the RMW backend, the runtime and the panic handler) come from one generated
path dependency,

```toml
[target.'cfg(target_os = "none")'.dependencies]
qemu_bsp_talker_nros_selection = { path = "build/nros/selection" }
```

which `nros sync` and `nros build` write for the image's board, and
`nros::main!` reaches the board through it. So changing `board =` to
`"freertos"` and re-running `nros sync && nros build` builds the FreeRTOS image
with no other edit. The other bare-metal, FreeRTOS, NuttX, ThreadX-Linux and
RTIC Rust examples take the same dependency, and so do the native ones, the
copy-out templates' Rust packages and the C/C++ apps on this board's cargo
link root. A package that names a board crate beside it is
refused at `nros sync`, naming the crate
([issue 1305](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/archived/1305-single-package-board-crate-dep-not-generated.md)).
In a workspace the entry is generated, so there the board line is the only
edit.

QEMU Slirp networking — no host TAP / bridge / sudo. The zenoh
default port is 7447; this example dials **10500** (the `locator`
above), so start the router (ROS's `rmw_zenohd`) on that port:

```bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:10500"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

or edit the `locator` above to the port you prefer.

## Build

In the package directory (where the scaffold step left you):

```bash probe=50
nros sync            # message bindings + build/baremetal/nros-cargo.toml
nros build           # builds the image `[image.baremetal]` declares
```

First build (~5 min) cross-compiles all of nano-ros's Rust deps for
`thumbv7m-none-eabi`. Re-builds finish in seconds. The ELF lands at
`build/baremetal/target/thumbv7m-none-eabi/debug/bm_talker` — under
`build/`, never beside your sources.

After a successful build `nros build` prints that path and a `run:` line —
the board's own QEMU runner with the store's `qemu-system-arm` filled in.
Pasted, it boots the talker, and QEMU's default user-mode network lands on
the board's LAN9118, so it reaches a router on the host. The command under
[Run on QEMU](#run-on-qemu) is the one the test harness boots: it adds
`-icount shift=auto`, which keeps the board's timer in step with Slirp's
I/O, and names the NIC explicitly.

**Driving cargo yourself** (an IDE, a CI step, `--release`) is supported: run
`nros sync` first, then point cargo at the generated settings file, from the
directory above the package (`nros build --dry-run` prints this line):

```bash
cd ..
cargo build --manifest-path bm_talker/Cargo.toml \
            --config bm_talker/build/baremetal/nros-cargo.toml
```

Skipping `nros sync` is not a mysterious failure: `nros build` stops at its
preflight with one line naming the file it could not find and telling you to
run sync in this directory.

That settings file is regenerated on every `nros sync`; never edit it, and
never add a `.cargo/config.toml` of your own to carry board facts. Your own
Rust-toolchain preferences go where cargo already looks for them — a parent
directory or `$CARGO_HOME`. See
[Workflow by Platform and Language](../user-guide/workflow-by-platform.md).

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

**2. The board** — boot the talker in QEMU, with the LAN9118 on Slirp and the
`-icount` the test harness uses (the `run:` line `nros build` printed works
too). The patched `qemu-system-arm` is provisioned by `nros setup
mps2-an385-baremetal` into the SDK store, which `nros sdk-path qemu` prints.
From the package directory:

```bash probe=70 probe-run=120 probe-keep
"$(nros sdk-path qemu)/bin/qemu-system-arm" -cpu cortex-m3 -machine mps2-an385 -nographic \
    -icount shift=auto \
    -semihosting-config enable=on,target=native \
    -kernel build/baremetal/target/thumbv7m-none-eabi/debug/bm_talker \
    -nic user,model=lan9118
```

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

QEMU exits via Ctrl-A x.

**Readiness signal.** Within ~15 seconds of QEMU boot (no RTOS
init delay, but smoltcp + zenoh handshake still takes a few
seconds), expect `Publishing: 'Hello World: 1'` on semihosting
stdout — the count starts at 1, matching the official ROS 2 demo
talker. If no `Publishing:` line:

1. Router not running — talker spins on smoltcp poll until
   killed.
2. No network on the guest — an explicit `-nic`/`-netdev` that does
   not reach the LAN9118 (or `-nic none`, `-nodefaults`) leaves it
   unwired. With no network option at all QEMU's default user-mode
   network attaches to it, which is why the board's bare runner works;
   `-nic user,model=lan9118` says so explicitly.
3. Cooperative spin starvation — if you added a long-running
   callback, the entire executor stalls; bare-metal has no
   preemption.
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

## Flash a real board

Nothing in-tree is flashed: `mps2-an385` is a QEMU machine, and `nros setup`
provisions no probe or flash tooling for it. A real Cortex-M board is a board
crate of your own — [Worked Example — STM32F4 Out of Tree](../porting/stm32f4-out-of-tree.md)
takes one through the whole ladder — and its ELF goes to the board with the
probe tool its vendor supports, which reads the same `build/<image>/target/…`
path:

```bash
nros build
probe-rs run --chip STM32F407VGTx build/<image>/target/thumbv7em-none-eabihf/debug/<bin>
# or OpenOCD:
#   openocd -f interface/stlink.cfg -f target/stm32f4x.cfg \
#       -c "program build/<image>/target/thumbv7em-none-eabihf/debug/<bin> verify reset exit"
```

The chip, interface and triple above are the STM32F4-Discovery's; yours come
from your board.

## GitHub source

- Bare-metal talker:
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

- Subscriber / service / action peers under the same
  [`examples/mps2-an385-baremetal/rust`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/mps2-an385-baremetal/rust) tree.
- Wake-callback opt-in: the `wake-callback` (latency-probe) bench
  under `packages/testing/nros-bench/wake-latency-cortex-m3/` shows
  how to feed a backend's transport-notify into the cooperative
  spin loop on bare-metal.
- Real hardware: the same code runs on an STM32F4-Discovery with a board
  crate of your own and a different linker script — see
  [Worked Example — STM32F4 Out of Tree](../porting/stm32f4-out-of-tree.md),
  which takes that board through the customization ladder end to end.
