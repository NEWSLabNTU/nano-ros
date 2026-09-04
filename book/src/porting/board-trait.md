# The `Board` Trait Family

The `Board` trait family is the **porting surface for a new MCU or host target**. It lives in `packages/platform/nros-platform/src/board/` and pins the contract every board crate (`nros-board-<board>` or a user-authored crate in a downstream Entry pkg) implements. Earlier prototypes used `nros-board-common::board_init::*`; those legacy traits stay as a transition shim while the in-tree boards finish migrating.

> **New board crates: implement `nros-platform::board`, not the legacy shim.**
> The porting surface for new code is the `Board` family described on this
> page (`Board` = `BoardInit` + `BoardPrint` + `BoardExit`, driven by
> `BoardEntry::run`). The `nros-board-common::board_init` trait family is
> transition-legacy — kept only so not-yet-migrated in-tree boards keep
> building — and must not be implemented in new board crates. Convergence
> onto the single `nros-platform::board` family is tracked in issue 0243,
> sequenced with the RTIC/Embassy integration work.

A board impl tells nano-ros four things: *how to initialize hardware*, *how to print a line of text*, *how to terminate*, and (optionally) *how to bring a transport up and gate on the network*. With those four pieces the `BoardEntry::run` driver owns the boot lifecycle, and a user Entry pkg `main.rs` is a ~30 LoC shim.

## Where the trait family sits

```text
nros-platform::board
│
├── Board: BoardInit + BoardPrint + BoardExit     // super-trait (blanket impl)
│
│
└── BoardEntry: Board
        fn run<F, E>(setup: F) -> Result<(), E>
        where F: FnOnce(&mut RuntimeCtx) -> Result<(), E>;
```

- **`BoardInit::init_hardware()`** — clock tree, pin mux, peripheral wakes. Runs once on boot before allocation. Panicking here is the same as panicking from `fn main()` — no recovery.
- **`BoardPrint::println(args: core::fmt::Arguments<'_>)`** — emit a line. Boards wrap whatever stdout makes sense: `cortex_m_semihosting::hprintln!`, a vendor printf bridge, a UART writer, `libc::write(STDOUT_FILENO, …)`, or `printk`.
- **`BoardExit::{exit_success, exit_failure}() -> !`** — terminate cleanly (or with failure). QEMU boards call `cortex_m_semihosting::debug::exit`; real hardware resets or halts; POSIX shells `std::process::exit`.
- **`BoardEntry::run(setup)`** — the boot driver. Implementations live in the family driver crates (`nros-board-{posix,freertos,threadx,…}`); user Entry pkg `main.rs` calls it.

`Board` is itself a blanket-implemented super-trait: any type that carries `BoardInit + BoardPrint + BoardExit` automatically satisfies `Board`. Concrete board crates do *not* `impl Board` directly — they impl the three sub-traits (plus whichever mixins they need).

## The `BoardEntry::run` lifecycle

`BoardEntry::run` owns the full boot → user-closure → exit flow. The exact body lives in the family driver crate (e.g. `nros-board-linux`, `nros-board-freertos`); each family folds its RTOS specifics in, but the *order* is fixed:

1. **`BoardInit::init_hardware()`** — clocks, pinmux, MMIO setup.
2. **Device bring-up — the family crate's job, inside `run`.** Bring the link
   layer to L2 (Ethernet frames flow / WiFi associated / UART open at baud),
   then gate on carrier / DHCP if the board has an IP stack. There is no mixin
   trait for this: see the note below.
3. Open the executor, build a [`RuntimeCtx`](#runtimectx) with overlay knobs from the launch file / CLI, and invoke `setup(&mut runtime)`. The codegen-emitted `run_plan(runtime)` body is what `setup` ultimately calls.
4. Spin the executor to completion (or termination signal).
5. **`BoardExit::exit_success()`** on `Ok`, **`BoardExit::exit_failure()`** on `Err` or any failed init step.

`run` returns `Result<(), E>` rather than `!` so unit tests can drive it in a hosted process without `exit()` killing the test harness — but production boards still call `exit_*` from inside `run`'s body after spin returns, so in practice the caller's `Ok(())` arm is unreachable on a real target.

The `setup` callback is the only place user code runs inside `run`. Everything else is family-crate boilerplate.

> ### Why there is no `TransportBringup` / `NetworkWait` mixin
>
> Earlier revisions of this page documented two mixin traits and a family-crate
> blanket impl that would call them:
>
> ```rust,ignore
> impl<B: Board + TransportBringup + NetworkWait> BoardEntry for B { fn run … }
> ```
>
> **That could not be built, and the traits were removed in phase-206 W4**
> (issue 1067). Two reasons, both structural:
>
> * The blanket impl **overlaps** the direct `BoardEntry` impls that twelve
>   board crates already carry. Rust's coherence rules do not allow both.
> * "Skipped if the board doesn't impl the mixin" **is not expressible** — Rust
>   has no way to call a method only when the type happens to implement a trait.
>   The order was written as if specialization existed.
>
> Measured before removal: `TransportBringup` had **zero** implementations and
> **zero** call sites; `NetworkWait` had one implementation and no callers,
> because the one place that would have called it — the `nros::main!` Zephyr arm
> — routed around it deliberately (`ZephyrBoard::wait_link_up` calls `static
> inline` Zephyr headers with no link symbol, so the native_sim link failed).
>
> Devices are still brought up: **inside `BoardEntry::run`**, or the family
> helper it delegates to. That is the contract, and it is the one that runs.

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
| Ethernet (smoltcp / lwIP / NetX BSD) | driver up, then DHCP/link gate | both, in `run` |
| WiFi (ESP32) | same shape — association is L2, DHCP is L3 | both, in `run` |
| Serial UART only | open at baud | no IP, so no link gate |
| CAN / USB CDC / IVC | link layer only | no IP |
| Bridged-net (threadx-linux veth) | host kernel owns IP | probe the bridge in `run` |
| Native (host) | None | Host OS owns everything; the family crate's `run` skips both mixins |

Boards with multiple transports compose via an internal helper (e.g. a `MultiTransport` newtype) rather than blanket impls — each transport's bringup is sequential and order-sensitive (`init_link` before `link_up`, sockets only after link).

## Worked example — porting a new board

Suppose you're adding `nros-board-acme-cortex-m4-eth`, a Cortex-M4 with a UART for `println` and an MII-attached PHY routed through smoltcp. The crate sits at `packages/boards/nros-board-acme-cortex-m4-eth/` and depends on `nros-platform`, the family crate (`nros-board-freertos` if FreeRTOS is the RTOS), the matching `packages/drivers/<phy>-smoltcp` MAC driver, and a vendor HAL crate.

```rust,ignore
// packages/boards/nros-board-acme-cortex-m4-eth/src/lib.rs
#![no_std]

use nros_platform::board::{
    BoardEntry, BoardExit, BoardInit, BoardPrint,
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

// Device bring-up lives in the BoardEntry::run body — usually by delegating
// to the family helper, which is where the ORDER for that RTOS is fixed:
impl BoardEntry for AcmeCortexM4Eth {
    fn run<F, E>(setup: F) -> Result<(), E>
    where
        F: FnOnce(&mut RuntimeCtx<'_>) -> Result<(), E>,
        E: core::fmt::Debug,
    {
        let cfg = Config::default();          // MAC / IP / netmask / gateway
        nros_board_freertos::run_entry::<Self, F, E>(cfg, setup)
    }
}
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

The family crate is where the `BoardEntry::run` *body* actually lives. The kernel families with a driver crate:

- `nros-board-linux` — native host; the reach is `linux`, not `posix` — `apply_tier_affinity` calls `sched_setaffinity` with `cpu_set_t`, which libc does not define for macOS. `init_transport`/`wait_link_up` no-ops.
- `nros-board-freertos` — FreeRTOS-Kernel + lwIP; `run` spawns the executor task, hands DHCP to lwIP's hook.
- `nros-board-threadx` — ThreadX + NetX BSD; same shape over NetX.
- `nros-board-nuttx` — NuttX POSIX layer; `init_transport` shells `ifup`-style logic.
- `nros-board-zephyr` — carve-out: Kconfig + DTS own BSP; the crate exposes an inherent `wait_link_up` over `<zephyr/net/net_if.h>`. The Rust staticlib cannot take over `main` on Zephyr.
- `nros-board-esp-idf` — ESP-IDF component shape; WiFi association lives in `init_transport`, IP lease in `wait_link_up`.
- Direct-exec (Cortex-M / RV32, no RTOS) has **no family crate**: each board
  implements `BoardEntry::run` itself with a single-thread `zp_read` loop. A
  `nros-board-bare-metal` family driver was written for this shape and no board
  ever opted into it — 135 of its 161 lines were doc comment — so a cleanup
  deleted it. `nros-board-mps2-an385` is the worked reference.

> **Current state:** the trait surface lives in `nros-platform`; family driver crates and per-board shims are migrating onto it. Some in-tree boards under `packages/boards/nros-board-*` still ride the legacy `nros-board-common::board_init::*` traits — same conceptual shape, different module path.

## Cross-references

- **Workspace shape + how an Entry pkg consumes a board** → [Role reference](../user-guide/component-and-entry-pkg.md).
- **Multi-node composition root** → [`docs/design/0024-multi-node-workspace-layout.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0024-multi-node-workspace-layout.md).
- **Why the C ABI looks the way it does** → [Canonical Platform C ABI](../internals/platform-c-abi.md).
- **Platform trait set vs Board trait set** — these are *different* traits with different roles. `Platform*` (clock / alloc / sockets / threading) sits below the RMW; `Board*` sits above the platform and owns the boot lifecycle. A bare-metal board crate typically depends on both: a `nros-platform-*` impl for the platform traits and a `nros-board-*` impl for the board traits.
