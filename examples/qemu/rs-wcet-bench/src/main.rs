//! WCET benchmark for nano-ros on QEMU Cortex-M3
//!
//! Measures cycle counts of core nano-ros operations using the DWT
//! cycle counter. On QEMU the DWT may not increment (reads as 0) —
//! the infrastructure is validated on real hardware (STM32F4).
//!
//! Run with: `just test-qemu-wcet`

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use panic_semihosting as _;

use builtin_interfaces::msg::Time;
use nano_ros::{
    CdrReader, CdrWriter, Deserialize, NodeConfig, PublisherOptions, Serialize,
    StandaloneNode as Node,
};
use std_msgs::msg::Int32;

const ITERATIONS: u32 = 100;

/// Enable the DWT cycle counter via raw register writes.
/// (Same logic as nano-ros-bsp-qemu::CycleCounter::enable())
fn enable_cycle_counter() {
    unsafe {
        let demcr = 0xE000_EDFC as *mut u32;
        core::ptr::write_volatile(demcr, core::ptr::read_volatile(demcr) | (1 << 24));
        let dwt_ctrl = 0xE000_1000 as *mut u32;
        core::ptr::write_volatile(dwt_ctrl, core::ptr::read_volatile(dwt_ctrl) | 1);
    }
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
    hprintln!("  nano-ros WCET Benchmark (Cortex-M3)");
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
