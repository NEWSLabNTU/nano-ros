//! RTIC Action Client Example for nros on QEMU MPS2-AN385
//!
//! Sends a Fibonacci goal and receives feedback/result using RTIC v2's
//! hardware-scheduled async tasks with LAN9118 Ethernet networking.
//!
//! - `#[init]` calls `board::init_hardware()` and creates nano-ros handles
//! - `net_poll` task drives transport I/O via `spin_once(0)`
//! - `action_call` task uses `try_recv()` loops for goal acceptance, feedback,
//!   and result (RTIC cannot use `Promise::wait()` or `FeedbackStream::wait_next()`)
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

use example_interfaces::action::{Fibonacci, FibonacciGoal};
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
    None => "tcp/10.0.2.2:7470",
};

// Type aliases for RTIC Local struct annotations
type NrosExecutor = Executor;
type NrosActionClient = nros::ActionClient<Fibonacci>;

#[rtic::app(device = mps2_an385_pac, dispatchers = [UARTRX0, UARTTX0])]
mod app {
    use super::*;

    #[shared]
    struct Shared {}

    #[local]
    struct Local {
        executor: NrosExecutor,
        client: NrosActionClient,
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
            .node_name("fibonacci_client");
        // Phase 104.A — bare-metal callers explicitly register the RMW
        // backend before `Executor::open`. POSIX hosts auto-register via
        // `.init_array`; this target doesn't walk that section.
        nros_rmw_zenoh::register().expect("Failed to register RMW backend");
        let mut executor = Executor::open(&exec_config).unwrap();
        let mut node = executor.create_node("fibonacci_client").unwrap();
        let client = node
            .create_action_client::<Fibonacci>("/fibonacci")
            .unwrap();

        net_poll::spawn().unwrap();
        action_call::spawn().unwrap();

        (Shared {}, Local { executor, client })
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

    /// Send a goal, receive feedback, and get the result.
    ///
    /// Uses try_recv() loops throughout — Promise::wait() and
    /// FeedbackStream::wait_next() are NOT usable since the executor
    /// is #[local] to the net_poll task.
    #[task(local = [client], priority = 1)]
    async fn action_call(cx: action_call::Context) {
        // Wait for zenoh session and server
        Mono::delay(3000.millis()).await;

        let goal = FibonacciGoal { order: 5 };
        println!("Sending goal: order={}", goal.order);

        let (goal_id, mut promise) = cx.local.client.send_goal(&goal).unwrap();

        // Poll for goal acceptance (~10s timeout)
        let mut timeout = 1000u32;
        let accepted = loop {
            if let Ok(Some(accepted)) = promise.try_recv() {
                break Some(accepted);
            }
            if timeout == 0 {
                break None;
            }
            timeout -= 1;
            Mono::delay(10.millis()).await;
        };

        match accepted {
            Some(true) => println!("Goal accepted"),
            Some(false) => {
                println!("Goal rejected");
                nros_board_mps2_an385::exit_failure();
            }
            None => {
                println!("Timeout waiting for goal acceptance");
                nros_board_mps2_an385::exit_failure();
            }
        }

        // Receive feedback via try_recv_feedback() loop
        let mut feedback_count = 0u32;
        for _ in 0..300 {
            if let Ok(Some((id, feedback))) = cx.local.client.try_recv_feedback()
                && id.uuid == goal_id.uuid
            {
                feedback_count += 1;
                println!(
                    "Feedback #{}: len={}",
                    feedback_count,
                    feedback.sequence.len()
                );
                if feedback.sequence.len() as i32 > goal.order {
                    break;
                }
            }
            Mono::delay(10.millis()).await;
        }

        println!("");
        println!("Got {} feedback messages", feedback_count);
        nros_board_mps2_an385::exit_success();
    }
}
