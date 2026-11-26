//! Large-message capability probe (phase 574 W2).
//!
//! One program for every RMW: the image (`system.toml`) picks the backend and
//! the size knobs it needs. `MODE=talker` publishes `COUNT` samples of
//! `PAYLOAD_SIZE` data bytes; `MODE=listener` receives them and checks every
//! byte. Both print one summary line the test reads:
//!
//! - talker: `LARGE_PUB_DONE: size=N sent=K failed=F first_error=...`
//! - listener: `LARGE_RECV_DONE: size=N received=K valid=V invalid=I errors=E first_error=...`
//!
//! The sample is a `std_msgs/UInt8MultiArray` in XCDR1 little-endian, built
//! and checked as bytes (`publish_serialized` / `take_serialized`), so every
//! backend carries a sample its type support accepts (Cyclone checks the
//! stream against the type), and no typed field caps the size.
//!
//! Data layout (`N` = `PAYLOAD_SIZE`, at least 16):
//!   `[0..4]` sequence number, `[4..8]` `N`, both u32 LE;
//!   `[8..N]` `(i * 7 + seq) & 0xFF` for data index `i`, so a sample that
//!   arrives under another sequence number, or shifted, fails the check.

use nros::prelude::*;
use std::time::{Duration, Instant};
use std_msgs::msg::UInt8MultiArray;

/// The CDR bytes before the `data` sequence's elements: the encapsulation
/// header, `layout.dim` (an empty sequence), `layout.data_offset`, and the
/// `data` sequence's length.
const PREFIX: usize = 16;

fn env<T: std::str::FromStr>(name: &str, default: T) -> T {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn fill(data: &mut [u8], seq: u32) {
    let n = data.len() as u32;
    data[0..4].copy_from_slice(&seq.to_le_bytes());
    data[4..8].copy_from_slice(&n.to_le_bytes());
    for (i, b) in data[8..].iter_mut().enumerate() {
        *b = ((i as u32).wrapping_mul(7).wrapping_add(seq) & 0xFF) as u8;
    }
}

fn encode(buf: &mut Vec<u8>, size: usize, seq: u32) {
    buf.clear();
    buf.extend_from_slice(&[0x00, 0x01, 0x00, 0x00]);
    buf.extend_from_slice(&0u32.to_le_bytes()); // layout.dim: empty
    buf.extend_from_slice(&0u32.to_le_bytes()); // layout.data_offset
    buf.extend_from_slice(&(size as u32).to_le_bytes());
    buf.resize(PREFIX + size, 0);
    fill(&mut buf[PREFIX..], seq);
}

/// `Ok(seq)` when `bytes` is a whole sample of `size` data bytes whose every
/// byte matches its own sequence number, otherwise why not.
fn check(bytes: &[u8], size: usize) -> Result<u32, String> {
    if bytes.len() != PREFIX + size {
        return Err(format!("length {} (want {})", bytes.len(), PREFIX + size));
    }
    if bytes[0..4] != [0x00, 0x01, 0x00, 0x00] {
        return Err(format!("encapsulation {:02x?}", &bytes[0..4]));
    }
    let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
    if len != size {
        return Err(format!("sequence length {len} (want {size})"));
    }
    let data = &bytes[PREFIX..];
    let seq = u32::from_le_bytes(data[0..4].try_into().unwrap());
    let mut want = vec![0u8; size];
    fill(&mut want, seq);
    match data.iter().zip(&want).position(|(a, b)| a != b) {
        None => Ok(seq),
        Some(i) => Err(format!(
            "seq {seq}: byte {i} is {:#04x}, want {:#04x}",
            data[i], want[i]
        )),
    }
}

fn topic() -> String {
    std::env::var("LARGE_TOPIC").unwrap_or_else(|_| "/large_msg".to_string())
}

fn talker(size: usize) {
    let count: u32 = env("COUNT", 5);
    let interval = Duration::from_millis(env("INTERVAL_MS", 200));
    let ctx = nros::init().expect("nros::init failed");
    let cfg = ctx.config("large_talker");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");
    let mut node = executor.create_node("large_talker").expect("create_node");
    let publisher = node
        // The default profile (reliable): every backend takes it, where
        // `QoSProfile::RELIABLE` also asks for automatic liveliness, which XRCE
        // refuses.
        .create_publisher::<UInt8MultiArray>(&topic())
        .expect("create_publisher");
    // Let discovery settle before the first sample.
    let settle = Instant::now();
    while settle.elapsed() < Duration::from_millis(env("SETTLE_MS", 1500)) {
        executor.spin_once(Duration::from_millis(50));
    }
    println!("Publishing: size={size}");
    let (mut sent, mut failed, mut first_error) = (0u32, 0u32, String::from("none"));
    let mut buf = Vec::with_capacity(PREFIX + size);
    for seq in 0..count {
        encode(&mut buf, size, seq);
        match publisher.publish_serialized(&buf) {
            Ok(()) => sent += 1,
            Err(e) => {
                if failed == 0 {
                    first_error = format!("{e:?}");
                }
                failed += 1;
            }
        }
        let t = Instant::now();
        while t.elapsed() < interval {
            executor.spin_once(Duration::from_millis(20));
        }
    }
    // Drain: a reliable stream may still hold fragments.
    let t = Instant::now();
    while t.elapsed() < Duration::from_millis(env("DRAIN_MS", 2000)) {
        executor.spin_once(Duration::from_millis(20));
    }
    println!("LARGE_PUB_DONE: size={size} sent={sent} failed={failed} first_error={first_error}");
}

fn listener<const RX: usize>(size: usize) {
    let expected: usize = env("COUNT", 5);
    let timeout = Duration::from_secs(env("TIMEOUT_SECS", 30));
    let ctx = nros::init().expect("nros::init failed");
    let cfg = ctx.config("large_listener");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");
    let mut node = executor.create_node("large_listener").expect("create_node");
    let mut sub = node
        .create_polling_subscription_sized::<UInt8MultiArray, RX>(&topic())
        .expect("create_subscription");
    println!("Ready: large-msg listener size={size}");
    let start = Instant::now();
    let (mut received, mut valid, mut invalid, mut errors) = (0usize, 0usize, 0usize, 0usize);
    let mut first_error = String::from("none");
    while received < expected && start.elapsed() < timeout {
        executor.spin_once(Duration::from_millis(20));
        loop {
            match sub.take_serialized() {
                Ok(Some(len)) => {
                    received += 1;
                    match check(&sub.buffer()[..len], size) {
                        Ok(seq) => {
                            valid += 1;
                            println!("Received: seq={seq} len={len} valid=true");
                        }
                        Err(why) => {
                            invalid += 1;
                            println!("Received: len={len} valid=false ({why})");
                            if first_error == "none" {
                                first_error = format!("invalid: {why}");
                            }
                        }
                    }
                    if received >= expected {
                        break;
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    errors += 1;
                    if first_error == "none" {
                        first_error = format!("{e:?}");
                    }
                    break;
                }
            }
        }
    }
    println!(
        "LARGE_RECV_DONE: size={size} received={received} valid={valid} invalid={invalid} \
         errors={errors} first_error={first_error} elapsed_ms={}",
        start.elapsed().as_millis()
    );
}

/// The receive buffer is a const generic held inside the subscription, so the
/// listener takes the smallest class that holds the sample and runs on a
/// thread whose stack holds it.
fn run_listener(size: usize) {
    let need = PREFIX + size;
    let run = move || match need {
        n if n <= 2 << 10 => listener::<{ 2 << 10 }>(size),
        n if n <= 20 << 10 => listener::<{ 20 << 10 }>(size),
        n if n <= 68 << 10 => listener::<{ 68 << 10 }>(size),
        n if n <= 260 << 10 => listener::<{ 260 << 10 }>(size),
        n if n <= (1 << 20) + 4096 => listener::<{ (1 << 20) + 4096 }>(size),
        n if n <= (4 << 20) + 4096 => listener::<{ (4 << 20) + 4096 }>(size),
        _ => panic!("PAYLOAD_SIZE={size} is above the largest receive class (4 MiB)"),
    };
    std::thread::Builder::new()
        .stack_size(4 * need + (8 << 20))
        .spawn(run)
        .expect("spawning the listener")
        .join()
        .expect("the listener panicked");
}

fn main() {
    nros_board_linux::register_linked_rmw();
    let size: usize = env::<usize>("PAYLOAD_SIZE", 1024).max(16);
    match std::env::var("MODE").as_deref() {
        Ok("listener") => run_listener(size),
        Ok("talker") | Err(_) => talker(size),
        Ok(other) => {
            eprintln!("MODE={other}: expected talker or listener");
            std::process::exit(2);
        }
    }
}
