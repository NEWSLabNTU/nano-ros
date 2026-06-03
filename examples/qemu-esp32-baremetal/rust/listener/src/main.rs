//! Simple ESP32-C3 QEMU Listener using nros-board-esp32-qemu
//!
//! Subscribes to typed `std_msgs/Int32` messages on `/chatter`.
//! Compare with qemu-bsp-listener -- this is the ESP32-C3 equivalent.
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
//! ./scripts/esp32/launch-esp32c3.sh --tap tap-qemu1 \
//!     --binary build/esp32-qemu/esp32-qemu-listener.bin
//! ```

#![no_std]
#![no_main]

use esp_backtrace as _;
use nros::prelude::*;
use nros_board_esp32_qemu::{esp_println, prelude::*};
use std_msgs::msg::Int32;

nros_board_esp32_qemu::esp_bootloader_esp_idf::esp_app_desc!();

// Phase 213.E.2 — locator + domain_id are compile-time overridable via
// `NROS_LOCATOR` / `NROS_DOMAIN_ID` env vars (`option_env!` is
// `#![no_std]`-clean and folds to a constant). Matches the E.1 shape.
const LOCATOR: &str = match option_env!("NROS_LOCATOR") {
    Some(s) => s,
    None => "tcp/10.0.2.2:7454",
};
const DOMAIN_ID: u32 = match option_env!("NROS_DOMAIN_ID") {
    // `u32::from_str_radix` is not yet `const`-stable on our MSRV; we
    // accept only single-digit decimals which covers the common
    // ROS_DOMAIN_ID range used in CI (0..=9). Multi-digit values
    // should bake via the platform-side compile-time path until
    // `const_from_str` lands.
    Some(s) if s.len() == 1 => (s.as_bytes()[0] - b'0') as u32,
    _ => 0,
};

#[entry]
fn main() -> ! {
    // Phase 212.M.10 — build-time Config literal supersedes the
    // pre-212 `Config::from_toml(include_str!(...))` sidecar.
    // Transcribed verbatim from the retired `nros.toml`.
    // TODO(213.E.2) — `mac_addr` / `ip` / `gateway` remain hardcoded:
    // they're board-internal smoltcp config and the env-var → byte-
    // array macro plumbing is pending. Override via `NROS_LOCATOR`
    // / `NROS_DOMAIN_ID` for the network-facing knobs.
    let config = Config {
        mac_addr: [0x02, 0x00, 0x00, 0x00, 0x00, 0x02],
        ip: [10, 0, 2, 51],
        prefix: 24,
        gateway: [10, 0, 2, 2],
        zenoh_locator: LOCATOR,
        domain_id: DOMAIN_ID,
    };
    run(config, |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("listener")
            .clock_us(nros_board_esp32_qemu::nros_platform_esp32_qemu::clock::clock_us);
        // Phase 104.A — bare-metal callers explicitly register the RMW
        // backend before `Executor::open`. POSIX hosts auto-register via
        // `.init_array`; this target doesn't walk that section.
        nros_rmw_zenoh::register().expect("Failed to register RMW backend");
        let mut executor = Executor::open(&exec_config)?;
        let nid = executor.node_builder("listener").build()?;

        esp_println::println!("Subscribing to /chatter (std_msgs/Int32)");

        executor
            .node_mut(nid)
            .create_subscription::<Int32, _>("/chatter", |msg: &Int32| {
                esp_println::println!("Received: {}", msg.data);
            })?;

        esp_println::println!("Subscriber declared");
        esp_println::println!("Waiting for messages...");

        // Phase 127.A diagnostic — periodically dump the
        // nros-smoltcp poll counters so the run log proves whether
        // the registered callback fires and how many staged TX
        // bytes the bridge actually pushes into smoltcp socket TX
        // queues. Silent staging accumulation = callback path
        // broken; nonzero `bridge_tx_drained` + 0 wire delivery =
        // OpenETH / smoltcp issue.
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
