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
./scripts/bootstrap.sh      # builds packages/cli/target/release/nros
source ./activate.sh        # OR: direnv allow / source ./activate.fish

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

```text
examples/mps2-an385-baremetal/rust/talker/
├── system.toml                # WHAT this deploys to — the one file you edit
├── Cargo.toml                 # Rust deps only; nothing here names a board
├── package.xml                # ROS-style manifest (drives codegen tooling)
├── generated/                 # generated message bindings (gitignored)
├── build/                     # everything `nros sync` generates (gitignored)
└── src/                       # lib.rs component class + main.rs entry
```

There is no `.cargo/` to edit. The target triple, the linker script, the
`--gc-sections` flag, the QEMU runner and the `[patch.crates-io]` rows that
resolve nano-ros's crates all come from the board, and `nros sync` writes them
into `build/mps2-an385-baremetal/nros-cargo.toml` — a generated file `nros
build` hands cargo with `--config` (RFC-0098).

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

The RTIC surface used to be a separate `nros-board-rtic-mps2-an385`
crate; phase-337 W6.a folded it in, because it depended on this crate
and re-declared its `Config` — the two copies had drifted to different
default IPs, which nothing catches until a node fails to reach the
router.

Which network the defaults describe is now explicit rather than
implied. `Config::default()` is the bridge plan (`192.0.3.10/24`,
gateway `192.0.3.1`); `Config::qemu_slirp()` is QEMU's user-mode NAT
(`10.0.2.10/24`, gateway `10.0.2.2`) and is what the RTIC entry boots
from. Either way, an image that sets `ip`/`gateway`/`locator` in its
`[image.<id>]` table overrides the default — which every in-tree example
does, and which you should too.

## Configure

The whole deployment statement is `system.toml`, beside the manifest — the
same schema a multi-node workspace's bringup package uses. It is baked at
compile time: `nros::main!()` folds the image's network identity into a
`DeployOverlay` the board's boot `Config` applies. Verbatim from the in-tree
[`examples/mps2-an385-baremetal/rust/talker/system.toml`](https://github.com/NEWSLabNTU/nano-ros/blob/main/examples/mps2-an385-baremetal/rust/talker/system.toml):

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
ip = "10.0.2.10"
gateway = "10.0.2.2"
netmask = "255.255.255.0"
```

`board` picks the silicon and everything that follows from it; `rmw` picks the
backend. Everything the board implies — triple, linker script, runner, cross
compiler, `[patch.crates-io]` — follows the `board =` line on the next
`nros sync`, so no linker flag and no triple ever moves by hand.

One thing does not follow yet. A single-package leaf *is* its own entry, so it
still names its board crate in `[dependencies]`
(`nros-board-mps2-an385 = { … }`), and `nros sync` writes its patch rows from
what the manifest names rather than from what the image names. Change the
`board =` line alone and sync reports success while the build fails inside
your own `src/main.rs` with an unresolved board crate. **On a single-package
Rust leaf the `board =` line and the `[dependencies]` row move together** —
that is
[issue 1305](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/issues/1305-single-package-board-crate-dep-not-generated.md).
In a workspace the entry is generated, so there the board line really is the
only edit.

QEMU Slirp networking — no host TAP / bridge / sudo. The zenoh
default port is 7447; this example dials **10500** (the `locator`
above), so start the router (ROS's `rmw_zenohd`) on that port:

```bash
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:10500"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

or edit the `locator` above to the port you prefer.

## Build

```bash
cd examples/mps2-an385-baremetal/rust/talker
nros sync            # message bindings + build/mps2-an385-baremetal/nros-cargo.toml
nros build           # builds the image `[image.mps2-an385-baremetal]` declares
```

First build (~5 min) cross-compiles all of nano-ros's Rust deps for
`thumbv7m-none-eabi`. Re-builds finish in seconds. The ELF lands at
`build/mps2-an385-baremetal/target/thumbv7m-none-eabi/debug/qemu-bsp-talker` —
under `build/`, never beside your sources.

**Driving cargo yourself** (an IDE, a CI step, `--release`) is supported: run
`nros sync` first, then point cargo at the generated settings file. Run it from
the directory *above* the package, so the package's own `.cargo/` is not read a
second time — phase-445 W6 deletes that directory, after which the working
directory stops mattering:

```bash
cd examples/mps2-an385-baremetal/rust
cargo build --manifest-path talker/Cargo.toml \
            --config talker/build/mps2-an385-baremetal/nros-cargo.toml
```

Skipping `nros sync` is not a mysterious failure: `nros build` stops at its
preflight with one line naming the file it could not find and telling you to
run sync in this directory.

That settings file is regenerated on every `nros sync`; never edit it, and
never add a `.cargo/config.toml` of your own to carry board facts. Your own
Rust-toolchain preferences go where cargo already looks for them — a parent
directory or `$CARGO_HOME`. See
[Workflow by Platform and Language](../user-guide/workflow-by-platform.md).

## Run

```bash
# 1. Bring up the router on the host (Slirp forwards 10.0.2.2:10500 →
#    host 127.0.0.1:10500). This example dials 10500, NOT zenoh's
#    default 7447 — edit `[image.mps2-an385-baremetal] locator` in
#    system.toml if you want 7447:
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:10500"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd &

# 2. Boot the talker in QEMU. Invoke qemu-system-arm directly with the
#    LAN9118 networking wiring the example expects — the board's own
#    runner is bare `-kernel`, so it boots QEMU without networking. The
#    patched qemu-system-arm is provisioned by
#    `nros setup mps2-an385-baremetal` and reaches PATH via activate.sh:
qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic \
    -icount shift=auto \
    -semihosting-config enable=on,target=native \
    -kernel build/mps2-an385-baremetal/target/thumbv7m-none-eabi/debug/qemu-bsp-talker \
    -nic user,model=lan9118
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

1. Router not running — talker spins on smoltcp poll until
   killed.
2. Wrong LAN9118 emulation flag — `qemu-system-arm` needs
   `-nic user,model=lan9118` (or equivalent). The board's runner (in
   the generated `build/mps2-an385-baremetal/nros-cargo.toml`) is bare
   `-kernel`, so booting through it gives you QEMU without networking;
   the direct `qemu-system-arm` invocation shown above carries the
   LAN9118 wiring and is the working invocation for this tutorial. If
   you copy the runner out, mirror those flags.
3. Cooperative spin starvation — if you added a long-running
   callback, the entire executor stalls; bare-metal has no
   preemption.
4. See [Troubleshooting — First 10 Minutes](./troubleshooting-first-10-min.md).

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
  `examples/mps2-an385-baremetal/rust/` tree.
- Wake-callback opt-in: the `wake-callback` (latency-probe) bench
  under `packages/testing/nros-bench/wake-latency-cortex-m3/` shows
  how to feed a backend's transport-notify into the cooperative
  spin loop on bare-metal.
- Real hardware: the same code runs on an STM32F4-Discovery with a board
  crate of your own and a different linker script — see
  [Worked Example — STM32F4 Out of Tree](../porting/stm32f4-out-of-tree.md),
  which takes that board through the customization ladder end to end.
