//! RTIC Service Server Example for nros on QEMU MPS2-AN385
//!
//! Handles `AddTwoInts` service requests using RTIC v2's hardware-scheduled
//! async tasks with LAN9118 Ethernet networking.
//!
//! - `#[init]` calls `board::init_hardware()` and creates nano-ros handles
//! - `net_poll` task drives transport I/O via `spin_once(0)`
//! - `serve` task polls for requests via `handle_request()`
//! - All nano-ros handles are `#[local]` — no locks required
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

use example_interfaces::srv::{AddTwoInts, AddTwoIntsResponse};
use nros::prelude::*;
use nros_board_mps2_an385::{Config, println};

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
    None => "tcp/10.0.2.2:7460",
};

// Type aliases for RTIC Local struct annotations
type NrosExecutor = Executor;
type NrosServiceServer = nros::EmbeddedServiceServer<AddTwoInts>;

#[rtic::app(device = mps2_an385_pac, dispatchers = [UARTRX0, UARTTX0])]
mod app {
    use super::*;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        executor: NrosExecutor,
        service: NrosServiceServer,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let config = Config {
            mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x00],
            ip: [10, 0, 2, 10],
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
            .node_name("add_server");
        // Phase 104.A — bare-metal callers explicitly register the RMW
        // backend before `Executor::open`. POSIX hosts auto-register via
        // `.init_array`; this target doesn't walk that section.
        nros_rmw_zenoh::register().expect("Failed to register RMW backend");
        let mut executor = Executor::open(&exec_config).unwrap();
        let mut node = executor.create_node("add_server").unwrap();
        let service = node.create_service::<AddTwoInts>("/add_two_ints").unwrap();

        net_poll::spawn().unwrap();
        serve::spawn().unwrap();

        (Shared {}, Local { executor, service })
    }

    /// Drive transport I/O — equivalent to rclcpp spin_some().
    ///
    /// Each `spin_once(0)` call processes one round of network I/O.
    /// The 10 ms RTIC yield lets QEMU's I/O loop service the TAP device
    /// (host → LAN9118 RX FIFO path only runs during WFI).
    #[task(local = [executor], priority = 1)]
    async fn net_poll(cx: net_poll::Context) {
        loop {
            cx.local
                .executor
                .spin_once(core::time::Duration::from_millis(0));
            Mono::delay(10.millis()).await;
        }
    }

    /// Poll for and handle service requests.
    #[task(local = [service], priority = 1)]
    async fn serve(cx: serve::Context) {
        // Wait for zenoh session establishment
        Mono::delay(2000.millis()).await;

        println!("Service server ready: /add_two_ints");

        loop {
            // Swallow transient transport errors (e.g. a non-CDR query
            // arriving on the queryable's buffer from the zenoh discovery
            // channel); the real request usually lands on a later poll.
            let _ = cx.local.service.handle_request(|req| {
                let sum = req.a + req.b;
                println!("Handled: {} + {} = {}", req.a, req.b, sum);
                AddTwoIntsResponse { sum }
            });

            Mono::delay(10.millis()).await;
        }
    }
}
