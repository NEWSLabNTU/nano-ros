//! Simple ESP32-C3 QEMU Talker using nros-board-esp32-qemu
//!
//! Publishes typed `std_msgs/Int32` messages on `/chatter`.
//! Compare with qemu-bsp-talker -- this is the ESP32-C3 equivalent.
//!
//! # Building
//!
//! ```bash
//! just build-examples-esp32-qemu
//! ```
//!
//! # Running (requires QEMU with Espressif fork)
//!
//! ```bash
//! ./scripts/esp32/launch-esp32c3.sh --tap tap-qemu0 \
//!     --binary build/esp32-qemu/esp32-qemu-talker.bin
//! ```

#![no_std]
#![no_main]

use esp_backtrace as _;
use nros::prelude::*;
use nros_board_esp32_qemu::{esp_println, prelude::*};
use std_msgs::msg::Int32;

nros_board_esp32_qemu::esp_bootloader_esp_idf::esp_app_desc!();

#[entry]
fn main() -> ! {
    // Phase 212.M.10 — build-time Config literal supersedes the
    // pre-212 `Config::from_toml(include_str!(...))` sidecar.
    // Transcribed verbatim from the retired `nros.toml`.
    let config = Config {
        mac_addr: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
        ip: [10, 0, 2, 50],
        prefix: 24,
        gateway: [10, 0, 2, 2],
        zenoh_locator: "tcp/10.0.2.2:7454",
        domain_id: 0,
    };
    run(config, |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("talker")
            .clock_us(nros_board_esp32_qemu::nros_platform_esp32_qemu::clock::clock_us);
        // Phase 104.A — bare-metal callers explicitly register the RMW
        // backend before `Executor::open`. POSIX hosts auto-register via
        // `.init_array`; this target doesn't walk that section.
        nros_rmw_zenoh::register().expect("Failed to register RMW backend");
        let mut executor = Executor::open(&exec_config)?;
        let publisher = {
            let mut node = executor.create_node("talker")?;
            esp_println::println!("Declaring publisher on /chatter (std_msgs/Int32)");
            node.create_publisher::<Int32>("/chatter")?
        };
        esp_println::println!("Publisher declared");

        esp_println::println!("Publishing messages...");

        let mut count: i32 = 0;
        executor.register_timer(nros::TimerDuration::from_millis(1000), move || {
            match publisher.publish(&Int32 { data: count }) {
                Ok(()) => esp_println::println!("Published: {}", count),
                Err(e) => esp_println::println!("Publish failed: {:?}", e),
            }
            count = count.wrapping_add(1);
        })?;

        // Phase 127.A diagnostic — see listener `main.rs` for the
        // rationale. Lets the test log prove whether the staged TX
        // bytes the talker writes are actually pushed into smoltcp
        // socket TX queues.
        let mut next_dump_ms =
            nros_board_esp32_qemu::nros_platform_esp32_qemu::clock::clock_ms() + 1000;
        loop {
            executor.spin_once(core::time::Duration::from_millis(10));
            let now_ms = nros_board_esp32_qemu::nros_platform_esp32_qemu::clock::clock_ms();
            if now_ms >= next_dump_ms {
                let (do_poll_calls, cb_hits, bridge_polls, tx_drained) =
                    nros_board_esp32_qemu::nros_smoltcp::poll_diagnostics();
                let (cb_registered, cb_sets, cb_lost) =
                    nros_board_esp32_qemu::nros_smoltcp::poll_callback_diagnostics();
                esp_println::println!(
                    "[poll] do_poll={} cb_hits={} bridge_polls={} tx_drained={} cb_registered={} cb_sets={} cb_lost={}",
                    do_poll_calls,
                    cb_hits,
                    bridge_polls,
                    tx_drained,
                    cb_registered,
                    cb_sets,
                    cb_lost,
                );
                next_dump_ms = now_ms + 5000;
            }
        }

        #[allow(unreachable_code)]
        Ok::<(), NodeError>(())
    })
}
