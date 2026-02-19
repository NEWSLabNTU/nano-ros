//! XRCE-DDS large message publish test.
//!
//! Publishes raw byte payloads of increasing sizes to verify both the
//! non-fragmented fast path and the fragmented stream fallback introduced
//! in Phase 40.3.
//!
//! Environment variables:
//!   XRCE_AGENT_ADDR  — Agent UDP address (default: "127.0.0.1:2019")
//!   XRCE_DOMAIN_ID   — ROS domain ID (default: 0)

use nros::{Executor, ExecutorConfig};
use std_msgs::msg::Int32;

fn main() {
    let agent_addr =
        std::env::var("XRCE_AGENT_ADDR").unwrap_or_else(|_| "127.0.0.1:2019".to_string());
    let domain_id: u32 = std::env::var("XRCE_DOMAIN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    eprintln!(
        "XRCE Large Msg Test: agent={}, domain={}",
        agent_addr, domain_id
    );

    // Open session
    let config = ExecutorConfig::new(&agent_addr)
        .domain_id(domain_id)
        .node_name("xrce_large_msg");
    let mut executor = Executor::<_, 0, 0>::open(&config).expect("Failed to open XRCE session");
    eprintln!("Session created");

    // Create publisher (Int32 type — we'll publish raw bytes)
    let mut node = executor
        .create_node("xrce_large_msg")
        .expect("Failed to create node");
    let publisher = node
        .create_publisher::<Int32>("/large_msg_test")
        .expect("Failed to create publisher");
    eprintln!("Publisher created on /large_msg_test");

    // Test payloads of increasing sizes.
    // With posix MTU=4096 and stream history=4, a single stream slot is ~4096 bytes.
    // Messages larger than one slot (minus XRCE headers) exercise the fragmented path.
    let test_sizes: &[usize] = &[64, 512, 1024, 2048, 3072, 4096, 6144, 8192, 12288];

    let mut passed = 0usize;
    let mut failed = 0usize;

    for &size in test_sizes {
        // Build a payload: 4-byte CDR header + payload bytes.
        // We use a simple pattern so it's recognizable in wireshark.
        let mut payload = vec![0u8; size];
        // CDR encapsulation header (little-endian, no options)
        if size >= 4 {
            payload[0] = 0x00;
            payload[1] = 0x01;
            payload[2] = 0x00;
            payload[3] = 0x00;
        }
        // Fill rest with a repeating pattern
        for (i, byte) in payload.iter_mut().enumerate().skip(4) {
            *byte = (i & 0xFF) as u8;
        }

        // Flush any pending output before each test
        executor.spin_once(50);

        match publisher.publish_raw(&payload) {
            Ok(()) => {
                println!("PASS: publish_raw size={} succeeded", size);
                passed += 1;
            }
            Err(e) => {
                println!("FAIL: publish_raw size={} error: {:?}", size, e);
                failed += 1;
            }
        }

        // Drive session to flush the message through the transport
        executor.spin_once(200);
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    println!(
        "Results: {} passed, {} failed out of {} sizes",
        passed,
        failed,
        test_sizes.len()
    );

    if failed == 0 {
        println!("ALL PASSED");
    } else {
        println!("SOME FAILED");
        std::process::exit(1);
    }

    let _ = executor.close();
}
