//! XRCE-DDS stress-test binary for large message and throughput testing.
//!
//! Dual-mode binary controlled by `MODE` env var:
//! - `MODE=talker`: publishes raw byte payloads
//! - `MODE=listener`: subscribes and validates payload integrity
//!
//! Environment variables:
//!   XRCE_AGENT_ADDR     — Agent UDP address (default: "127.0.0.1:2019")
//!   MODE                — "talker" or "listener" (default: "talker")
//!   PAYLOAD_SIZE        — Payload size in bytes (default: 64, minimum 16)
//!   PUBLISH_COUNT       — Number of messages to publish (default: 100)
//!   PUBLISH_INTERVAL_MS — Interval between publishes in ms (default: 10)
//!   EXPECTED_COUNT      — Messages to receive before exiting (default: 50)
//!   TIMEOUT_SECS        — Listener timeout in seconds (default: 30)

use nros::{Executor, ExecutorConfig};
use std::time::Instant;

/// Build a test payload with integrity markers.
///
/// Layout:
///   [0..4]   CDR header: 0x00, 0x01, 0x00, 0x00 (little-endian)
///   [4..8]   Sequence number (u32 LE)
///   [8..12]  Total payload size (u32 LE, including CDR header)
///   [12..N]  Fill pattern: byte[i] = ((i - 12) & 0xFF) as u8
fn build_payload(buf: &mut [u8], seq: u32, size: usize) {
    assert!(size >= 16 && size <= buf.len());
    buf[0] = 0x00;
    buf[1] = 0x01;
    buf[2] = 0x00;
    buf[3] = 0x00;
    buf[4..8].copy_from_slice(&seq.to_le_bytes());
    buf[8..12].copy_from_slice(&(size as u32).to_le_bytes());
    for (i, byte) in buf[12..size].iter_mut().enumerate() {
        *byte = (i & 0xFF) as u8;
    }
}

/// Validate a received payload. Returns (seq, valid).
fn validate_payload(data: &[u8], expected_size: usize) -> (u32, bool) {
    if data.len() < 16 {
        return (0, false);
    }
    if data[0] != 0x00 || data[1] != 0x01 || data[2] != 0x00 || data[3] != 0x00 {
        return (0, false);
    }
    let seq = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    let size_marker = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
    if size_marker != expected_size || data.len() != expected_size {
        return (seq, false);
    }
    for (i, &byte) in data[12..].iter().enumerate() {
        if byte != (i & 0xFF) as u8 {
            return (seq, false);
        }
    }
    (seq, true)
}

fn run_talker() {
    let agent_addr =
        std::env::var("XRCE_AGENT_ADDR").unwrap_or_else(|_| "127.0.0.1:2019".to_string());
    let payload_size: usize = std::env::var("PAYLOAD_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(64);
    let publish_count: u32 = std::env::var("PUBLISH_COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    let interval_ms: u64 = std::env::var("PUBLISH_INTERVAL_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);

    let actual_size = payload_size.max(16);

    eprintln!(
        "XRCE Stress Talker: agent={} size={} count={} interval={}ms",
        agent_addr, actual_size, publish_count, interval_ms
    );

    let config = ExecutorConfig::new(&agent_addr).node_name("xrce_stress_talker");
    let mut executor = Executor::open(&config).expect("Failed to open XRCE session");

    let mut node = executor
        .create_node("xrce_stress_talker")
        .expect("Failed to create node");
    let publisher = node
        .create_publisher::<std_msgs::msg::Int32>("/stress_test")
        .expect("Failed to create publisher");

    // Stabilize connection
    executor.spin_once(500);
    std::thread::sleep(std::time::Duration::from_millis(500));
    println!("Publishing...");

    let mut buf = vec![0u8; actual_size];
    let start = Instant::now();

    for seq in 0..publish_count {
        build_payload(&mut buf, seq, actual_size);
        match publisher.publish_raw(&buf[..actual_size]) {
            Ok(()) => {
                println!("Published: seq={} size={}", seq, actual_size);
            }
            Err(e) => {
                eprintln!("Publish error: seq={} size={}: {:?}", seq, actual_size, e);
            }
        }
        executor.spin_once(50);
        if interval_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(interval_ms));
        }
    }

    let elapsed = start.elapsed();
    println!(
        "PUBLISH_DONE: sent={} elapsed_ms={}",
        publish_count,
        elapsed.as_millis()
    );

    let _ = executor.close();
}

fn run_listener() {
    let agent_addr =
        std::env::var("XRCE_AGENT_ADDR").unwrap_or_else(|_| "127.0.0.1:2019".to_string());
    let expected_count: usize = std::env::var("EXPECTED_COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);
    let timeout_secs: u64 = std::env::var("TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30);
    let payload_size: usize = std::env::var("PAYLOAD_SIZE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(64);

    let actual_size = payload_size.max(16);

    eprintln!(
        "XRCE Stress Listener: agent={} expected={} timeout={}s payload_size={}",
        agent_addr, expected_count, timeout_secs, actual_size
    );

    let config = ExecutorConfig::new(&agent_addr).node_name("xrce_stress_listener");
    let mut executor = Executor::open(&config).expect("Failed to open XRCE session");

    let mut node = executor
        .create_node("xrce_stress_listener")
        .expect("Failed to create node");
    let mut subscription = node
        .create_subscription_sized::<std_msgs::msg::Int32, 16384>("/stress_test")
        .expect("Failed to create subscriber");

    println!("Ready: listening");

    let start = Instant::now();
    let timeout = std::time::Duration::from_secs(timeout_secs);
    let mut received: usize = 0;
    let mut valid: usize = 0;
    let mut invalid: usize = 0;

    while received < expected_count && start.elapsed() < timeout {
        executor.spin_once(50);

        match subscription.try_recv_raw() {
            Ok(Some(len)) => {
                let (seq, is_valid) = validate_payload(&subscription.buffer()[..len], actual_size);
                received += 1;
                if is_valid {
                    valid += 1;
                } else {
                    invalid += 1;
                }
                println!("Received: seq={} size={} valid={}", seq, len, is_valid);
            }
            Ok(None) => {}
            Err(e) => {
                eprintln!("Receive error: {:?}", e);
            }
        }
    }

    let elapsed = start.elapsed();
    println!(
        "RECV_DONE: received={} valid={} invalid={} elapsed_ms={}",
        received,
        valid,
        invalid,
        elapsed.as_millis()
    );

    let _ = executor.close();
}

fn main() {
    let mode = std::env::var("MODE").unwrap_or_else(|_| "talker".to_string());
    match mode.as_str() {
        "talker" => run_talker(),
        "listener" => run_listener(),
        other => {
            eprintln!("Unknown MODE={}, expected 'talker' or 'listener'", other);
            std::process::exit(1);
        }
    }
}
