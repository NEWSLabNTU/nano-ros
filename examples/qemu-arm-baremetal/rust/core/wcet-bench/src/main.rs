//! WCET benchmark for nros on QEMU Cortex-M3
//!
//! Measures cycle counts of core nros operations using the DWT
//! cycle counter. On QEMU the DWT may not increment (reads as 0) —
//! the infrastructure is validated on real hardware (STM32F4).
//!
//! Run with: `just test-qemu-wcet`

#![no_std]
#![no_main]

use cortex_m::peripheral::Peripherals;
use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use panic_semihosting as _;

use builtin_interfaces::msg::Time;
use nros::{
    CdrReader, CdrWriter, Deserialize, NodeConfig, PublisherOptions, SafetyValidator, Serialize,
    StandaloneNode as Node, crc32,
};
use std_msgs::msg::Int32;

const ITERATIONS: u32 = 100;

/// Enable the DWT cycle counter using the cortex-m safe peripheral API.
fn enable_cycle_counter() {
    let mut cp = Peripherals::take().expect("cortex-m peripherals already taken");
    cp.DCB.enable_trace();
    cp.DWT.enable_cycle_counter();
}

/// Read DWT cycle count.
#[inline(always)]
fn cycles() -> u32 {
    cortex_m::peripheral::DWT::cycle_count()
}

/// Statistics tracker for benchmark measurements.
struct Stats {
    min: u32,
    max: u32,
    sum: u64,
    count: u32,
}

impl Stats {
    fn new() -> Self {
        Self {
            min: u32::MAX,
            max: 0,
            sum: 0,
            count: 0,
        }
    }

    fn record(&mut self, value: u32) {
        if value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }
        self.sum += value as u64;
        self.count += 1;
    }

    fn avg(&self) -> u32 {
        if self.count == 0 {
            0
        } else {
            (self.sum / self.count as u64) as u32
        }
    }
}

/// Benchmark: CDR serialize Int32 (single i32 write)
fn bench_serialize_int32() -> Stats {
    let mut stats = Stats::new();
    let msg = Int32 { data: 42 };

    // Warmup
    for _ in 0..10 {
        let mut buf = [0u8; 16];
        let mut w = CdrWriter::new(&mut buf);
        let _ = msg.serialize(&mut w);
    }

    // Measure
    for _ in 0..ITERATIONS {
        let mut buf = [0u8; 16];
        let mut w = CdrWriter::new(&mut buf);
        let start = cycles();
        let _ = msg.serialize(&mut w);
        let elapsed = cycles().wrapping_sub(start);
        stats.record(elapsed);
    }
    stats
}

/// Benchmark: CDR deserialize Int32 (single i32 read)
fn bench_deserialize_int32() -> Stats {
    let mut stats = Stats::new();

    // Prepare serialized data
    let mut buf = [0u8; 16];
    let msg = Int32 { data: 42 };
    let mut w = CdrWriter::new(&mut buf);
    let _ = msg.serialize(&mut w);

    // Warmup
    for _ in 0..10 {
        let mut r = CdrReader::new(&buf);
        let _ = Int32::deserialize(&mut r);
    }

    // Measure
    for _ in 0..ITERATIONS {
        let mut r = CdrReader::new(&buf);
        let start = cycles();
        let _ = Int32::deserialize(&mut r);
        let elapsed = cycles().wrapping_sub(start);
        stats.record(elapsed);
    }
    stats
}

/// Benchmark: CDR serialize Time (two fields: i32 + u32)
fn bench_serialize_time() -> Stats {
    let mut stats = Stats::new();
    let msg = Time {
        sec: 1234,
        nanosec: 567890,
    };

    // Warmup
    for _ in 0..10 {
        let mut buf = [0u8; 16];
        let mut w = CdrWriter::new(&mut buf);
        let _ = msg.serialize(&mut w);
    }

    // Measure
    for _ in 0..ITERATIONS {
        let mut buf = [0u8; 16];
        let mut w = CdrWriter::new(&mut buf);
        let start = cycles();
        let _ = msg.serialize(&mut w);
        let elapsed = cycles().wrapping_sub(start);
        stats.record(elapsed);
    }
    stats
}

/// Benchmark: CDR roundtrip Int32 (serialize + deserialize)
fn bench_roundtrip_int32() -> Stats {
    let mut stats = Stats::new();
    let msg = Int32 { data: -99 };

    // Warmup
    for _ in 0..10 {
        let mut buf = [0u8; 16];
        let mut w = CdrWriter::new(&mut buf);
        let _ = msg.serialize(&mut w);
        let mut r = CdrReader::new(&buf);
        let _ = Int32::deserialize(&mut r);
    }

    // Measure
    for _ in 0..ITERATIONS {
        let mut buf = [0u8; 16];
        let mut w = CdrWriter::new(&mut buf);
        let _ = msg.serialize(&mut w);
        let start_deser = cycles(); // measure just the roundtrip
        let _ = Int32::deserialize(&mut CdrReader::new(&buf));
        // We measure both serialize and deserialize together
        let _ = start_deser; // suppress unused warning

        // Redo as a single measurement
        let mut buf2 = [0u8; 16];
        let start = cycles();
        let mut w2 = CdrWriter::new(&mut buf2);
        let _ = msg.serialize(&mut w2);
        let _ = Int32::deserialize(&mut CdrReader::new(&buf2));
        let elapsed = cycles().wrapping_sub(start);
        stats.record(elapsed);
    }
    stats
}

/// Benchmark: CDR serialize with encapsulation header
fn bench_serialize_with_header() -> Stats {
    let mut stats = Stats::new();
    let msg = Int32 { data: 42 };

    // Warmup
    for _ in 0..10 {
        let mut buf = [0u8; 16];
        if let Ok(mut w) = CdrWriter::new_with_header(&mut buf) {
            let _ = msg.serialize(&mut w);
        }
    }

    // Measure
    for _ in 0..ITERATIONS {
        let mut buf = [0u8; 16];
        let start = cycles();
        if let Ok(mut w) = CdrWriter::new_with_header(&mut buf) {
            let _ = msg.serialize(&mut w);
        }
        let elapsed = cycles().wrapping_sub(start);
        stats.record(elapsed);
    }
    stats
}

/// Benchmark: Node::new() creation
fn bench_node_creation() -> Stats {
    let mut stats = Stats::new();

    // Warmup
    for _ in 0..10 {
        let config = NodeConfig::new("bench_node", "/bench");
        let _node = Node::<4, 4>::new(config);
    }

    // Measure
    for _ in 0..ITERATIONS {
        let config = NodeConfig::new("bench_node", "/bench");
        let start = cycles();
        let _node = Node::<4, 4>::new(config);
        let elapsed = cycles().wrapping_sub(start);
        stats.record(elapsed);
    }
    stats
}

/// Benchmark: Node::create_publisher()
fn bench_create_publisher() -> Stats {
    let mut stats = Stats::new();

    // Warmup
    for _ in 0..10 {
        let mut node = Node::<4, 4>::default();
        let _ = node.create_publisher::<Int32>(PublisherOptions::new("/bench_topic"));
    }

    // Measure
    for _ in 0..ITERATIONS {
        let mut node = Node::<4, 4>::default();
        let start = cycles();
        let _ = node.create_publisher::<Int32>(PublisherOptions::new("/bench_topic"));
        let elapsed = cycles().wrapping_sub(start);
        stats.record(elapsed);
    }
    stats
}

/// Benchmark: Node::serialize_message()
fn bench_node_serialize() -> Stats {
    let mut stats = Stats::new();
    let msg = Int32 { data: 42 };

    // Setup a node with a publisher
    let mut node = Node::<4, 4>::default();
    let pub_handle = match node.create_publisher::<Int32>(PublisherOptions::new("/bench_topic")) {
        Ok(h) => h,
        Err(_) => return stats,
    };

    // Warmup
    for _ in 0..10 {
        let _ = node.serialize_message(&pub_handle, &msg);
    }

    // Measure
    for _ in 0..ITERATIONS {
        let start = cycles();
        let _ = node.serialize_message(&pub_handle, &msg);
        let elapsed = cycles().wrapping_sub(start);
        stats.record(elapsed);
    }
    stats
}

/// Benchmark: CRC-32 over N-byte buffer
fn bench_crc32(size: usize) -> Stats {
    let mut stats = Stats::new();
    let buf = [0xA5u8; 1024];
    let data = &buf[..size];

    // Warmup
    for _ in 0..10 {
        let _ = crc32(data);
    }

    // Measure
    for _ in 0..ITERATIONS {
        let start = cycles();
        let _ = crc32(data);
        let elapsed = cycles().wrapping_sub(start);
        stats.record(elapsed);
    }
    stats
}

/// Benchmark: SafetyValidator::validate() with incrementing sequence
fn bench_safety_validate() -> Stats {
    let mut stats = Stats::new();
    let mut validator = SafetyValidator::new();

    // Warmup (also initializes the validator)
    for seq in 0..10i64 {
        let _ = validator.validate(seq, Some(true));
    }

    // Measure
    for i in 0..ITERATIONS {
        let seq = 10 + i as i64;
        let start = cycles();
        let _ = validator.validate(seq, Some(true));
        let elapsed = cycles().wrapping_sub(start);
        stats.record(elapsed);
    }
    stats
}

/// Benchmark: Full safety pipeline (extract attachment, CRC, validate)
///
/// Simulates try_recv_validated: parse seq + CRC from a 37-byte attachment,
/// compute CRC on a 128-byte payload, compare, then validate sequence.
fn bench_safety_full_pipeline() -> Stats {
    let mut stats = Stats::new();

    // Prepare a synthetic 128-byte payload
    let payload = [0x42u8; 128];
    let payload_crc = crc32(&payload);

    // Build a 37-byte attachment: [seq(8) | timestamp(8) | gid(13) | attachment_len(4) | crc(4)]
    let mut attachment = [0u8; 37];
    // seq = 0 at bytes 0..8 (little-endian i64)
    // CRC at bytes 33..37
    attachment[33..37].copy_from_slice(&payload_crc.to_le_bytes());

    let mut validator = SafetyValidator::new();

    // Warmup
    for i in 0u64..10 {
        attachment[0..8].copy_from_slice(&(i as i64).to_le_bytes());
        attachment[33..37].copy_from_slice(&payload_crc.to_le_bytes());
        let seq = i64::from_le_bytes(attachment[0..8].try_into().unwrap());
        let att_crc = u32::from_le_bytes(attachment[33..37].try_into().unwrap());
        let computed_crc = crc32(&payload);
        let crc_ok = Some(att_crc == computed_crc);
        let _ = validator.validate(seq, crc_ok);
    }

    // Measure
    for i in 0..ITERATIONS {
        let seq_val = 10 + i as i64;
        attachment[0..8].copy_from_slice(&seq_val.to_le_bytes());

        let start = cycles();
        // Extract fields from attachment
        let seq = i64::from_le_bytes(attachment[0..8].try_into().unwrap());
        let att_crc = u32::from_le_bytes(attachment[33..37].try_into().unwrap());
        // Compute CRC over payload
        let computed_crc = crc32(&payload);
        let crc_ok = Some(att_crc == computed_crc);
        // Validate
        let _ = validator.validate(seq, crc_ok);
        let elapsed = cycles().wrapping_sub(start);

        stats.record(elapsed);
    }
    stats
}

fn print_result(name: &str, stats: &Stats) {
    hprintln!(
        "  {}: min={} max={} avg={} cycles",
        name,
        stats.min,
        stats.max,
        stats.avg()
    );
}

#[entry]
fn main() -> ! {
    hprintln!("");
    hprintln!("========================================");
    hprintln!("  nros WCET Benchmark (Cortex-M3)");
    hprintln!("========================================");
    hprintln!("");

    enable_cycle_counter();

    let dwt_test = cycles();
    let dwt_active = cycles().wrapping_sub(dwt_test) > 0 || dwt_test > 0;
    if !dwt_active {
        hprintln!("NOTE: DWT cycle counter reads as 0 (QEMU limitation).");
        hprintln!("      Cycle counts will be 0. Validate on real hardware.");
        hprintln!("");
    }

    hprintln!("Iterations per benchmark: {}", ITERATIONS);
    hprintln!("");

    hprintln!("--- CDR Serialization ---");
    print_result("serialize Int32", &bench_serialize_int32());
    print_result("deserialize Int32", &bench_deserialize_int32());
    print_result("serialize Time", &bench_serialize_time());
    print_result("roundtrip Int32", &bench_roundtrip_int32());
    print_result("serialize w/header", &bench_serialize_with_header());

    hprintln!("");
    hprintln!("--- Node API ---");
    print_result("Node::new()", &bench_node_creation());
    print_result("create_publisher()", &bench_create_publisher());
    print_result("serialize_message()", &bench_node_serialize());

    hprintln!("");
    hprintln!("--- Safety E2E ---");
    print_result("crc32 (64B)", &bench_crc32(64));
    print_result("crc32 (256B)", &bench_crc32(256));
    print_result("crc32 (1024B)", &bench_crc32(1024));
    print_result("validate()", &bench_safety_validate());
    print_result("full pipeline (128B)", &bench_safety_full_pipeline());

    hprintln!("");
    hprintln!("========================================");
    hprintln!("  Benchmark complete");
    hprintln!("========================================");
    hprintln!("");
    hprintln!("[PASS]");

    cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_SUCCESS);
    loop {
        cortex_m::asm::wfi();
    }
}
