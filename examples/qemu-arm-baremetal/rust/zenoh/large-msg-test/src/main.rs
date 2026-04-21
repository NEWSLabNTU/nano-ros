//! QEMU bare-metal large message publish test
//!
//! Tests that publish_raw succeeds for various payload sizes on bare-metal.
//! This is a publish-only test (no E2E — that requires TAP networking).

#![no_std]
#![no_main]

use nros::prelude::*;
use nros_mps2_an385::{Config, println, run};
use panic_semihosting as _;

/// Build a test payload with integrity markers.
fn build_payload(buf: &mut [u8], seq: u32, size: usize) {
    // CDR header (little-endian)
    buf[0] = 0x00;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x00;
    // Sequence number
    buf[4] = (seq & 0xFF) as u8;
    buf[5] = ((seq >> 8) & 0xFF) as u8;
    buf[6] = ((seq >> 16) & 0xFF) as u8;
    buf[7] = ((seq >> 24) & 0xFF) as u8;
    // Total size marker
    let size_bytes = (size as u32).to_le_bytes();
    buf[8] = size_bytes[0];
    buf[9] = size_bytes[1];
    buf[10] = size_bytes[2];
    buf[11] = size_bytes[3];
    // Fill pattern
    let mut i = 12;
    while i < size {
        buf[i] = ((i - 12) & 0xFF) as u8;
        i += 1;
    }
}

#[nros_mps2_an385::entry]
fn main() -> ! {
    run(
        Config::from_toml(include_str!("../config.toml")),
        |config| {
            let exec_config = ExecutorConfig::new(config.zenoh_locator)
                .domain_id(config.domain_id)
                .node_name("large_msg_test");
            let mut executor = Executor::open(&exec_config)?;
            let mut node = executor.create_node("large_msg_test")?;

            println!("Large message publish test");
            println!("=========================");

            let publisher = node.create_publisher::<std_msgs::msg::Int32>("/large_msg_test")?;

            // Poll to establish connection
            for _ in 0..50 {
                executor.spin_once(core::time::Duration::from_millis(10));
            }

            let test_sizes: &[usize] = &[64, 128, 256, 512, 768, 1024];
            let mut buf = [0u8; 1024];
            let mut passed = 0u32;
            let mut failed = 0u32;

            for (seq, &size) in test_sizes.iter().enumerate() {
                build_payload(&mut buf, seq as u32, size);
                match publisher.publish_raw(&buf[..size]) {
                    Ok(()) => {
                        println!("[PASS] publish size={}", size);
                        passed += 1;
                    }
                    Err(e) => {
                        println!("[FAIL] publish size={}: {:?}", size, e);
                        failed += 1;
                    }
                }
                // Allow network processing between publishes
                for _ in 0..10 {
                    executor.spin_once(core::time::Duration::from_millis(10));
                }
            }

            println!("");
            if failed == 0 {
                println!("All tests passed ({} sizes)", passed);
            } else {
                println!("FAILED: {} passed, {} failed", passed, failed);
            }

            Ok::<(), NodeError>(())
        },
    )
}
