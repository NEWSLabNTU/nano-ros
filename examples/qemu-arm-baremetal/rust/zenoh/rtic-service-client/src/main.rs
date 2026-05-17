//! RTIC Service Client Example for nros on QEMU MPS2-AN385
//!
//! Calls `AddTwoInts` service using RTIC v2's hardware-scheduled async tasks
//! with LAN9118 Ethernet networking.
//!
//! Uses a single task that owns both the executor and client, calling
//! `spin_once()` between `try_recv()` polls for I/O processing.
//!
//! # Running
//!
//! ```bash
//! cargo nano-ros generate
//! cargo run --release
//! ```

#![no_std]
#![no_main]

use panic_semihosting as _;

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest};
use nros::prelude::*;
use nros_board_mps2_an385::{Config, println};

use rtic_monotonics::systick::prelude::*;

systick_monotonic!(Mono, 1000);

// Type aliases for RTIC Local struct annotations
type NrosExecutor = Executor;
type NrosServiceClient = nros::EmbeddedServiceClient<AddTwoInts>;

#[rtic::app(device = mps2_an385_pac, dispatchers = [UARTRX0])]
mod app {
    use super::*;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        executor: NrosExecutor,
        client: NrosServiceClient,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        let config = Config::from_toml(include_str!("../config.toml"));
        nros_board_mps2_an385::init_hardware(&config);

        Mono::start(cx.core.SYST, 25_000_000);

        // Phase 127.D — release CPU between busy-wait poll iterations
        // (Executor::open + sleep_ms) now that SysTick IRQ is armed.
        // Lets QEMU's main loop service slirp / LAN9118 I/O when two
        // MPS2 guests share the host.
        nros_board_mps2_an385::enable_wfi_idle();

        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("add_client");
        // Phase 104.A — bare-metal callers explicitly register the RMW
        // backend before `Executor::open`. POSIX hosts auto-register via
        // `.init_array`; this target doesn't walk that section.
        nros_rmw_zenoh::register().expect("Failed to register RMW backend");
        let mut executor = Executor::open(&exec_config).unwrap();
        let mut node = executor.create_node("add_client").unwrap();
        let client = node.create_client::<AddTwoInts>("/add_two_ints").unwrap();

        call_service::spawn().unwrap();

        (Shared {}, Local { executor, client })
    }

    /// Drive transport I/O and call service in a single task.
    ///
    /// Each `spin_once(0)` call processes one round of network I/O.
    /// The 10 ms RTIC yield lets QEMU's I/O loop service the TAP device
    /// (host → LAN9118 RX FIFO path only runs during WFI).
    #[task(local = [executor, client], priority = 1)]
    async fn call_service(cx: call_service::Context) {
        // Wait for zenoh session + server queryable discovery.
        for _ in 0..500 {
            cx.local
                .executor
                .spin_once(core::time::Duration::from_millis(0));
            Mono::delay(10.millis()).await;
        }

        println!("Client ready, starting service calls...");

        let test_cases: [(i64, i64); 4] = [(5, 3), (10, 20), (100, 200), (-5, 10)];

        for (a, b) in test_cases {
            let request = AddTwoIntsRequest { a, b };
            println!("Calling: {} + {} = ?", a, b);

            let mut promise = cx.local.client.call(&request).unwrap();

            let mut got_reply = false;
            let mut last_dump = 0u32;
            for _i in 0..3000u32 {
                cx.local
                    .executor
                    .spin_once(core::time::Duration::from_millis(0));
                Mono::delay(10.millis()).await;
                if _i.wrapping_sub(last_dump) >= 100 {
                    last_dump = _i;
                    let (rx, recv) = nros_board_mps2_an385::nros_smoltcp::rx_diagnostics();
                    let (pend, deliv, err) =
                        nros_board_mps2_an385::lan9118_smoltcp::rx_diag_counters();
                    let mut g = [0u32; 15];
                    unsafe { zpico_sys::zpico_get_diag_counters(g.as_mut_ptr()) };
                    println!(
                        "[d] i={} rx={} gst={} gck={} rh={} rd={} set={} | inv={} niu={} big={} to={} pend={}",
                        _i, rx, g[0], g[1], g[3], g[4], g[7],
                        g[10], g[11], g[12], g[13], g[14]
                    );
                }

                // Transient errors (e.g. a non-CDR sample from the zenoh
                // discovery channel arriving on the reply slot before the
                // real reply lands) shouldn't abort the call — the actual
                // reply usually arrives on a later poll. Treat any err
                // the same as `Ok(None)` and let the timeout below handle
                // genuine hangs.
                if let Ok(Some(reply)) = promise.try_recv() {
                    println!("Reply: {} + {} = {}", a, b, reply.sum);
                    got_reply = true;
                    break;
                }
            }
            if !got_reply {
                println!("Service call timed out after 30s");
                nros_board_mps2_an385::exit_failure();
            }

            cx.local
                .executor
                .spin_once(core::time::Duration::from_millis(0));
            Mono::delay(10.millis()).await;
        }

        println!("");
        println!("All service calls completed");
        nros_board_mps2_an385::exit_success();
    }
}
