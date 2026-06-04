//! Mixed-Priority RTIC Listener Example for nros on QEMU MPS2-AN385
//!
//! Subscribes to `std_msgs/Int32` on `/chatter` using RTIC v2's
//! hardware-scheduled async tasks with LAN9118 Ethernet networking.
//!
//! This example demonstrates `ffi-sync` with mixed RTIC priorities:
//! - `net_poll` runs at priority 1 (low) — drives transport I/O
//! - `listen` runs at priority 2 (high) — can preempt `net_poll`
//!
//! Without `ffi-sync`, the higher-priority `listen` task could preempt
//! `net_poll` mid-`spin_once(0)`, corrupting zenoh-pico's global state.
//! With `ffi-sync`, `critical_section::with()` masks interrupts during
//! FFI calls, so the preempting task waits until the section exits.
//!
//! # Running
//!
//! ```bash
//! nros generate-rust
//! cargo run --release
//! ```

#![no_std]
#![no_main]

use panic_semihosting as _;

use nros::prelude::*;
use nros_board_mps2_an385::{Config, println};
use std_msgs::msg::Int32;

use rtic_monotonics::systick::prelude::*;

systick_monotonic!(Mono, 1000);

// Phase 213.E.1 — zenoh locator overridable at build time via `NROS_LOCATOR`
// env-var (compile-time, keeps `#![no_std]` clean). Falls back to the QEMU
// slirp fixture default. MAC/IP/gateway tuples stay literal for now —
// board-internal smoltcp tuning, not user-facing config.
// TODO(213.E later): move MAC/IP/gateway to
// [package.metadata.nros.deploy.<target>] once macro/board-crate plumbing
// lands.
const LOCATOR: &str = match option_env!("NROS_LOCATOR") {
    Some(s) => s,
    None => "tcp/10.0.2.2:7450",
};

// Type aliases for RTIC Local struct annotations
type NrosExecutor = Executor;
type NrosSubscription = Subscription<Int32>;

#[rtic::app(device = mps2_an385_pac, dispatchers = [UARTRX0, UARTTX0])]
mod app {
    use super::*;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        executor: NrosExecutor,
        subscription: NrosSubscription,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let config = Config {
            mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
            ip: [10, 0, 2, 11],
            prefix: 24,
            gateway: [10, 0, 2, 2],
            zenoh_locator: LOCATOR,
            domain_id: 0,
        };
        nros_board_mps2_an385::init_hardware(&config);

        Mono::start(cx.core.SYST, 25_000_000);

        // Phase 127.D — release CPU between busy-wait poll iterations
        // (Executor::open + sleep_ms) now that SysTick IRQ is armed.
        // Lets QEMU's main loop service slirp / LAN9118 I/O when two
        // MPS2 guests share the host.
        nros_board_mps2_an385::enable_wfi_idle();

        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("listener");
        // Phase 104.A — bare-metal callers explicitly register the RMW
        // backend before `Executor::open`. POSIX hosts auto-register via
        // `.init_array`; this target doesn't walk that section.
        nros_rmw_zenoh::register().expect("Failed to register RMW backend");
        let mut executor = Executor::open(&exec_config).unwrap();
        let mut node = executor.create_node("listener").unwrap();
        let subscription = node.create_subscription::<Int32>("/chatter").unwrap();

        net_poll::spawn().unwrap();
        listen::spawn().unwrap();

        (
            Shared {},
            Local {
                executor,
                subscription,
            },
        )
    }

    /// Drive transport I/O — equivalent to rclcpp spin_some().
    ///
    /// Each `spin_once(0)` call processes one round of network I/O.
    /// The 10 ms RTIC yield lets QEMU's I/O loop service the TAP device
    /// (host → LAN9118 RX FIFO path only runs during WFI).
    ///
    /// Runs at priority 1 (low) — can be preempted by `listen` (priority 2).
    #[task(local = [executor], priority = 1)]
    async fn net_poll(cx: net_poll::Context) {
        loop {
            cx.local
                .executor
                .spin_once(core::time::Duration::from_millis(0));
            Mono::delay(10.millis()).await;
        }
    }

    /// Poll for incoming messages forever.
    ///
    /// Runs at priority 2 (high) — can preempt `net_poll`. The `ffi-sync`
    /// feature ensures `try_recv()` waits for any in-progress FFI critical
    /// section in `net_poll` before accessing zenoh-pico state.
    #[task(local = [subscription], priority = 2)]
    async fn listen(cx: listen::Context) {
        println!("Waiting for messages on /chatter...");

        loop {
            // Skip transient transport errors (e.g. a non-CDR sample from
            // the zenoh discovery channel arriving on this subscription's
            // buffer before the first publisher sample lands).
            if let Ok(Some(msg)) = cx.local.subscription.try_recv() {
                println!("Received: {}", msg.data);
            }

            Mono::delay(1.millis()).await;
        }
    }
}
