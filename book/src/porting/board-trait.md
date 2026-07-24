# The `Board` Trait Family

The `Board` trait family is the **porting surface for a new MCU or host target**. It lives in `packages/core/nros-platform/src/board/` and pins the contract every board crate (`nros-board-<board>` or a user-authored crate in a downstream Entry pkg) implements. Phase 212.N introduces this surface; earlier prototypes used `nros-board-common::board_init::*`, and those legacy traits stay as a transition shim until Phase 212.N.7 lands.

> **New board crates: implement `nros-platform::board`, not the legacy shim.**
> The porting surface for new code is the `Board` family described on this
> page (`Board` = `BoardInit` + `BoardPrint` + `BoardExit`, driven by
> `BoardEntry::run`). The `nros-board-common::board_init` trait family is
> transition-legacy — kept only so not-yet-migrated in-tree boards keep
> building — and must not be implemented in new board crates. Convergence
> onto the single `nros-platform::board` family is tracked in issue 0243,
> sequenced with phase-230.

A board impl tells nano-ros four things: *how to initialize hardware*, *how to print a line of text*, *how to terminate*, and (optionally) *how to bring a transport up and gate on the network*. With those four pieces the `BoardEntry::run` driver owns the boot lifecycle, and a user Entry pkg `main.rs` is a ~30 LoC shim.

## Where the trait family sits

```text
nros-platform::board
│
├── Board: BoardInit + BoardPrint + BoardExit     // super-trait (blanket impl)
│
├── TransportBringup: Board                       // mixin — Ethernet / WiFi / serial / CAN / USB CDC / IVC
├── NetworkWait:    Board                         // mixin — carrier / DHCP / link-up gate
│
└── BoardEntry: Board
        fn run<F, E>(setup: F) -> Result<(), E>
        where F: FnOnce(&mut RuntimeCtx) -> Result<(), E>;
```

- **`BoardInit::init_hardware()`** — clock tree, pin mux, peripheral wakes. Runs once on boot before allocation. Panicking here is the same as panicking from `fn main()` — no recovery.
- **`BoardPrint::println(args: core::fmt::Arguments<'_>)`** — emit a line. Boards wrap whatever stdout makes sense: `cortex_m_semihosting::hprintln!`, a vendor printf bridge, a UART writer, `libc::write(STDOUT_FILENO, …)`, or `printk`.
- **`BoardExit::{exit_success, exit_failure}() -> !`** — terminate cleanly (or with failure). QEMU boards call `cortex_m_semihosting::debug::exit`; real hardware resets or halts; POSIX shells `std::process::exit`.
- **`TransportBringup::init_transport()`** — bring the link layer up to L2 (Ethernet frames flow / WiFi associated / UART open at baud). Returns *before* any L3/IP state — that's `NetworkWait`'s job.
- **`NetworkWait::wait_link_up()`** — block until carrier + DHCP/static IP + default route. Only IP-aware transports implement it; CAN-only, serial-only, IVC-only boards skip it.
- **`BoardEntry::run(setup)`** — the boot driver. Implementations live in the family driver crates (`nros-board-{posix,freertos,threadx,…}`); user Entry pkg `main.rs` calls it.

`Board` is itself a blanket-implemented super-trait: any type that carries `BoardInit + BoardPrint + BoardExit` automatically satisfies `Board`. Concrete board crates do *not* `impl Board` directly — they impl the three sub-traits (plus whichever mixins they need).

## The `BoardEntry::run` lifecycle

`BoardEntry::run` owns the full boot → user-closure → exit flow. The exact body lives in the family driver crate (e.g. `nros-board-posix`, `nros-board-freertos`); each family folds its RTOS specifics in, but the *order* is fixed:

1. **`BoardInit::init_hardware()`** — clocks, pinmux, MMIO setup.
2. **`TransportBringup::init_transport()`** — driver up at L2 (skipped if the board doesn't impl the mixin).
3. **`NetworkWait::wait_link_up()`** — DHCP / carrier (skipped if the board doesn't impl the mixin).
4. Open the executor, build a [`RuntimeCtx`](#runtimectx) with overlay knobs from the launch file / CLI, and invoke `setup(&mut runtime)`. The codegen-emitted `run_plan(runtime)` body is what `setup` ultimately calls.
5. Spin the executor to completion (or termination signal).
6. **`BoardExit::exit_success()`** on `Ok`, **`BoardExit::exit_failure()`** on `Err` or any failed init step.

`run` returns `Result<(), E>` rather than `!` so unit tests can drive it in a hosted process without `exit()` killing the test harness — but production boards still call `exit_*` from inside `run`'s body after spin returns, so in practice the caller's `Ok(())` arm is unreachable on a real target.

The `setup` callback is the only place user code runs inside `run`. Everything else is family-crate boilerplate.

## `RuntimeCtx`

`RuntimeCtx<'a>` is the per-invocation overlay context the `setup` callback receives:

```rust
pub struct RuntimeCtx<'a> {
    pub params:  &'a [(&'a str, &'a str)],   // <param name=… value=…/> + -p name:=value
    pub remaps:  &'a [(&'a str, &'a str)],   // topic/service/action renames
    pub env:     &'a [(&'a str, &'a str)],   // env-style key/value (rarely set on embedded)
}
```

Slice-of-tuples, `no_std`-safe, no allocation. Codegen owns the storage and passes a `&mut RuntimeCtx<'_>` whose backing slices live in `static`s — `RuntimeCtx::EMPTY` is a const placeholder for launch-less single-node examples or unit tests.

## Picking your transport mixins

What you implement on the transport axis depends on what link layers your board exposes:

| Board transport class | Implement | Notes |
|---|---|---|
| Ethernet (smoltcp / lwIP / NetX BSD) | `TransportBringup` + `NetworkWait` | Both — driver up, then DHCP/link gate |
| WiFi (ESP32) | `TransportBringup` + `NetworkWait` | Same shape — association is L2, DHCP is L3 |
| Serial UART only | `TransportBringup` | No IP, so no `NetworkWait` |
| CAN / USB CDC / IVC | `TransportBringup` | Link-layer only |
| Bridged-net (threadx-linux veth) | `TransportBringup` + `NetworkWait` | Host kernel owns IP — `wait_link_up` just probes the bridge |
| Native POSIX | None | Host OS owns everything; the family crate's `run` skips both mixins |

Boards with multiple transports compose via an internal helper (e.g. a `MultiTransport` newtype) rather than blanket impls — each transport's bringup is sequential and order-sensitive (`init_link` before `link_up`, sockets only after link).

## Worked example — porting a new board

Suppose you're adding `nros-board-acme-cortex-m4-eth`, a Cortex-M4 with a UART for `println` and an MII-attached PHY routed through smoltcp. The crate sits at `packages/boards/nros-board-acme-cortex-m4-eth/` and depends on `nros-platform`, the family crate (`nros-board-freertos` if FreeRTOS is the RTOS), the matching `packages/drivers/<phy>-smoltcp` MAC driver, and a vendor HAL crate.

```rust,ignore
// packages/boards/nros-board-acme-cortex-m4-eth/src/lib.rs
#![no_std]

use nros_platform::board::{
    BoardEntry, BoardExit, BoardInit, BoardPrint,
    NetworkWait, TransportBringup,
    NetworkError, TransportError, RuntimeCtx,
};

pub struct AcmeCortexM4Eth;

impl BoardInit for AcmeCortexM4Eth {
    fn init_hardware() {
        acme_hal::clocks::init_hse_192mhz();
        acme_hal::pinmux::route_uart2();
        acme_hal::pinmux::route_eth_mii();
        acme_hal::eth::release_phy_reset();
    }
}

impl BoardPrint for AcmeCortexM4Eth {
    fn println(args: core::fmt::Arguments<'_>) {
        // 256-byte stack staging buffer is enough for our log lines;
        // pick whatever your UART driver wants.
        let mut buf = heapless::String::<256>::new();
        let _ = core::fmt::write(&mut buf, args);
        let _ = buf.push('\n');
        acme_hal::uart2::write_bytes(buf.as_bytes());
    }
}

impl BoardExit for AcmeCortexM4Eth {
    fn exit_success() -> ! { acme_hal::system::reset() }
    fn exit_failure() -> ! { acme_hal::system::halt_with_blinkenlight() }
}

impl TransportBringup for AcmeCortexM4Eth {
    fn init_transport() -> Result<(), TransportError> {
        // Brings the MAC up to L2; smoltcp owns the IP stack and joins
        // it in NetworkWait.
        acme_phy_smoltcp::init().map_err(|_| TransportError::DriverInit)?;
        acme_phy_smoltcp::wait_link(core::time::Duration::from_secs(5))
            .map_err(|_| TransportError::LinkDown)
    }
}

impl NetworkWait for AcmeCortexM4Eth {
    fn wait_link_up() -> Result<(), NetworkError> {
        acme_phy_smoltcp::dhcp_acquire(core::time::Duration::from_secs(10))
            .map_err(|_| NetworkError::DhcpTimeout)
    }
}

// BoardEntry comes from the family crate's blanket impl:
//   impl<B: Board + TransportBringup + NetworkWait> BoardEntry for B { fn run … }
// The family crate provides the FreeRTOS-shaped run body; you do not
// hand-write a BoardEntry impl unless your target is exotic enough to
// step outside the family.
```

That's the whole board crate. A downstream Entry pkg consumes it as:

```rust,ignore
// pkgs/robot_acme_entry/src/main.rs
use nros_board_acme_cortex_m4_eth::AcmeCortexM4Eth;
use nros_platform::board::BoardEntry;

include!(concat!(env!("OUT_DIR"), "/run_plan.rs"));   // codegen-emitted

fn main() {
    let _ = <AcmeCortexM4Eth as BoardEntry>::run(|runtime| {
        run_plan(runtime)
    });
}
```

See the [Role reference](../user-guide/component-and-entry-pkg.md) for the Entry-pkg surface.

## Family driver crates

The family crate is where the `BoardEntry::run` *body* actually lives. Tier-1 families targeted by Phase 212.N.2:

- `nros-board-posix` — native host (Linux / *BSD); `init_transport`/`wait_link_up` no-ops.
- `nros-board-freertos` — FreeRTOS-Kernel + lwIP; `run` spawns the executor task, hands DHCP to lwIP's hook.
- `nros-board-threadx` — ThreadX + NetX BSD; same shape over NetX.
- `nros-board-nuttx` — NuttX POSIX layer; `init_transport` shells `ifup`-style logic.
- `nros-board-zephyr` — carve-out: Kconfig + DTS own BSP, family crate impls only `NetworkWait` over `<zephyr/net/net_if.h>`. The Rust staticlib cannot take over `main` on Zephyr.
- `nros-board-esp-idf` — ESP-IDF component shape; WiFi association lives in `init_transport`, IP lease in `wait_link_up`.
- `nros-board-bare-metal` — Cortex-M / RV32, no RTOS; minimal `run` body with a single-thread `zp_read` loop.

> **Current state:** as of Phase 212.N.1 the trait surface lives in `nros-platform`; the family driver crates land in N.2 and the per-board shims in N.3. Until then, see `packages/boards/nros-board-*` for the in-tree boards that still ride the legacy `nros-board-common::board_init::*` traits — same conceptual shape, different module path.

## Cross-references

- **Workspace shape + how an Entry pkg consumes a board** → [Role reference](../user-guide/component-and-entry-pkg.md).
- **Multi-node composition root** → [`docs/design/0024-multi-node-workspace-layout.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0024-multi-node-workspace-layout.md).
- **Why the C ABI looks the way it does** → [Canonical Platform C ABI](../internals/platform-c-abi.md).
- **Platform trait set vs Board trait set** — these are *different* traits with different roles. `Platform*` (clock / alloc / sockets / threading) sits below the RMW; `Board*` sits above the platform and owns the boot lifecycle. A bare-metal board crate typically depends on both: a `nros-platform-*` impl for the platform traits and a `nros-board-*` impl for the board traits.
