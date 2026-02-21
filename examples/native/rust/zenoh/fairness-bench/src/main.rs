//! Executor Fairness Benchmark (Phase 37.4)
//!
//! Measures executor behavior under asymmetric loads to quantify message loss
//! and evaluate fairness across subscriptions and services.
//!
//! Uses separate processes for publishers/clients since zenoh-pico does not
//! deliver self-published messages back to the same session. The main process
//! runs the executor with subscriptions/services; child processes publish
//! and send service requests.
//!
//! # Usage
//!
//! ```bash
//! # Start zenoh router first:
//! zenohd --listen tcp/127.0.0.1:7447
//!
//! # Run the benchmark:
//! RUST_LOG=warn cargo run --release
//! ```

use example_interfaces::srv::{AddTwoInts, AddTwoIntsRequest, AddTwoIntsResponse};
use nros::prelude::*;
use std_msgs::msg::Int32;

use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ═══════════════════════════════════════════════════════════════════════════
// STATISTICS
// ═══════════════════════════════════════════════════════════════════════════

struct LatencyStats {
    samples: Vec<Duration>,
}

impl LatencyStats {
    fn new() -> Self {
        Self {
            samples: Vec::with_capacity(2048),
        }
    }

    fn record(&mut self, d: Duration) {
        self.samples.push(d);
    }

    fn count(&self) -> usize {
        self.samples.len()
    }

    fn percentile(&self, p: f64) -> Duration {
        if self.samples.is_empty() {
            return Duration::ZERO;
        }
        let mut sorted = self.samples.clone();
        sorted.sort();
        let idx = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
        sorted[idx.min(sorted.len() - 1)]
    }

    fn print_summary(&self, label: &str) {
        if self.samples.is_empty() {
            println!("  {label}: no samples");
            return;
        }
        println!(
            "  {label}: n={}, p50={:.2}ms, p95={:.2}ms, p99={:.2}ms",
            self.count(),
            self.percentile(50.0).as_secs_f64() * 1000.0,
            self.percentile(95.0).as_secs_f64() * 1000.0,
            self.percentile(99.0).as_secs_f64() * 1000.0,
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SUBPROCESS HELPER
// ═══════════════════════════════════════════════════════════════════════════

fn spawn_publisher(scenario: &str) -> Child {
    let rust_log = std::env::var("RUST_LOG").unwrap_or_default();
    Command::new(std::env::current_exe().unwrap())
        .args(["--publish", scenario])
        .env("RUST_LOG", rust_log)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("Failed to spawn publisher subprocess")
}

// ═══════════════════════════════════════════════════════════════════════════
// PUBLISHER MODES (run in child process)
// ═══════════════════════════════════════════════════════════════════════════

/// Publish /bench1/fast at 100Hz and /bench1/slow at 10Hz for 20s.
fn publish_scenario_1() {
    let config = ExecutorConfig::from_env().node_name("pub1");
    let mut executor = Executor::<_, 4, 4096>::open(&config).expect("Failed to open session");
    let mut node = executor.create_node("pub1").expect("Node");

    let fast_pub = node
        .create_publisher::<Int32>("/bench1/fast")
        .expect("Fast publisher");

    let slow_pub = node
        .create_publisher::<Int32>("/bench1/slow")
        .expect("Slow publisher");

    // Wait for subscriber session to register
    std::thread::sleep(Duration::from_secs(1));

    let start = Instant::now();
    let mut fast_seq = 0i32;
    let mut slow_seq = 0i32;
    let mut last_fast = Instant::now();
    let mut last_slow = Instant::now();

    while start.elapsed() < Duration::from_secs(20) {
        let now = Instant::now();
        if now.duration_since(last_fast) >= Duration::from_millis(10) {
            let _ = fast_pub.publish(&Int32 { data: fast_seq });
            fast_seq = fast_seq.wrapping_add(1);
            last_fast = now;
        }
        if now.duration_since(last_slow) >= Duration::from_millis(100) {
            let _ = slow_pub.publish(&Int32 { data: slow_seq });
            slow_seq = slow_seq.wrapping_add(1);
            last_slow = now;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// Send 100 service requests to /bench2/add using Promise pattern.
fn client_scenario_2() {
    let config = ExecutorConfig::from_env().node_name("client2");
    let mut executor = Executor::<_, 4, 4096>::open(&config).expect("Failed to open session");
    let mut node = executor.create_node("client2").expect("Node");

    let mut client = node
        .create_client::<AddTwoInts>("/bench2/add")
        .expect("Service client");

    // Wait for server to register with zenohd
    std::thread::sleep(Duration::from_secs(3));

    for i in 0..100i64 {
        let request = AddTwoIntsRequest { a: i, b: i + 1 };
        if let Ok(mut promise) = client.call(&request) {
            let start = Instant::now();
            loop {
                executor.spin_once(10);
                match promise.try_recv() {
                    Ok(Some(_)) => break,
                    Ok(None) => {
                        if start.elapsed() > Duration::from_secs(5) {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Publish 2 topics at 50Hz + service requests at 10Hz for 20s.
fn publish_scenario_3() {
    let config = ExecutorConfig::from_env().node_name("pub3");
    let mut executor = Executor::<_, 4, 4096>::open(&config).expect("Failed to open session");
    let mut node = executor.create_node("pub3").expect("Node");

    let pub_a = node
        .create_publisher::<Int32>("/bench3/topic_a")
        .expect("Publisher A");

    let pub_b = node
        .create_publisher::<Int32>("/bench3/topic_b")
        .expect("Publisher B");

    let mut client = node
        .create_client::<AddTwoInts>("/bench3/add")
        .expect("Service client");

    std::thread::sleep(Duration::from_secs(1));

    let start = Instant::now();
    let mut seq_a = 0i32;
    let mut seq_b = 0i32;
    let mut svc_seq = 0i64;
    let mut last_pub = Instant::now();
    let mut last_svc = Instant::now();

    while start.elapsed() < Duration::from_secs(20) {
        let now = Instant::now();
        if now.duration_since(last_pub) >= Duration::from_millis(20) {
            let _ = pub_a.publish(&Int32 { data: seq_a });
            seq_a = seq_a.wrapping_add(1);
            let _ = pub_b.publish(&Int32 { data: seq_b });
            seq_b = seq_b.wrapping_add(1);
            last_pub = now;
        }
        if now.duration_since(last_svc) >= Duration::from_millis(100) {
            let req = AddTwoIntsRequest {
                a: svc_seq,
                b: svc_seq + 1,
            };
            if let Ok(mut promise) = client.call(&req) {
                let call_start = Instant::now();
                loop {
                    executor.spin_once(10);
                    match promise.try_recv() {
                        Ok(Some(_)) => break,
                        Ok(None) => {
                            if call_start.elapsed() > Duration::from_secs(5) {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
            svc_seq += 1;
            last_svc = now;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// SCENARIO 1: ASYMMETRIC SUBSCRIPTION RATES
// ═══════════════════════════════════════════════════════════════════════════

fn scenario_1_asymmetric_subscriptions() {
    println!("\n========================================================================");
    println!("Scenario 1: Asymmetric Subscription Rates");
    println!("  /bench1/fast at 100 Hz, /bench1/slow at 10 Hz");
    println!("  Executor spin_one_period_timed() at 10ms interval (100 Hz)");
    println!("========================================================================\n");

    const DURATION_SECS: u64 = 10;
    const SPIN_PERIOD: Duration = Duration::from_millis(10);

    let fast_received = Arc::new(AtomicU64::new(0));
    let slow_received = Arc::new(AtomicU64::new(0));
    let fast_cb = fast_received.clone();
    let slow_cb = slow_received.clone();

    let fast_intervals = Arc::new(Mutex::new(LatencyStats::new()));
    let slow_intervals = Arc::new(Mutex::new(LatencyStats::new()));
    let fast_last = Arc::new(Mutex::new(Instant::now()));
    let slow_last = Arc::new(Mutex::new(Instant::now()));
    let fi_cb = fast_intervals.clone();
    let si_cb = slow_intervals.clone();
    let fl_cb = fast_last.clone();
    let sl_cb = slow_last.clone();

    let config = ExecutorConfig::from_env().node_name("sub1");
    let mut executor = Executor::<_, 8, 8192>::open(&config).expect("Failed to open session");

    executor
        .add_subscription::<Int32, _>("/bench1/fast", move |_msg: &Int32| {
            let n = fast_cb.fetch_add(1, Ordering::Relaxed) + 1;
            let now = Instant::now();
            let mut last = fl_cb.lock().unwrap();
            if n > 1 {
                fi_cb.lock().unwrap().record(now.duration_since(*last));
            }
            *last = now;
        })
        .expect("Fast subscription");

    executor
        .add_subscription::<Int32, _>("/bench1/slow", move |_msg: &Int32| {
            let n = slow_cb.fetch_add(1, Ordering::Relaxed) + 1;
            let now = Instant::now();
            let mut last = sl_cb.lock().unwrap();
            if n > 1 {
                si_cb.lock().unwrap().record(now.duration_since(*last));
            }
            *last = now;
        })
        .expect("Slow subscription");

    println!("  Waiting for subscription propagation (2s)...");
    std::thread::sleep(Duration::from_secs(2));

    let mut child = spawn_publisher("1");
    println!("  Publisher started (PID {})", child.id());

    // Wait for publisher to establish session and start publishing
    std::thread::sleep(Duration::from_secs(3));

    println!("  Running measurement ({DURATION_SECS}s)...");
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(DURATION_SECS) {
        let _ = executor.spin_one_period_timed(SPIN_PERIOD);
    }

    let _ = child.kill();
    let _ = child.wait();

    let fast_recv = fast_received.load(Ordering::Relaxed);
    let slow_recv = slow_received.load(Ordering::Relaxed);
    let expected_fast = 1000u64;
    let expected_slow = 100u64;

    println!("\nResults:");
    println!(
        "  /bench1/fast: expected~{expected_fast}, received={fast_recv}, \
         loss~{:.1}%",
        (1.0 - fast_recv as f64 / expected_fast as f64).max(0.0) * 100.0
    );
    println!(
        "  /bench1/slow: expected~{expected_slow}, received={slow_recv}, \
         loss~{:.1}%",
        (1.0 - slow_recv as f64 / expected_slow as f64).max(0.0) * 100.0
    );

    println!("\nInter-callback intervals:");
    fast_intervals.lock().unwrap().print_summary("/bench1/fast");
    slow_intervals.lock().unwrap().print_summary("/bench1/slow");

    println!("\nAnalysis:");
    println!("  Single-slot buffer: each spin_once() delivers at most 1 msg per topic.");
    println!("  Both topics get equal per-spin opportunity — no starvation.");
    println!("  Loss comes from publish rate exceeding spin rate, not from starvation.");
}

// ═══════════════════════════════════════════════════════════════════════════
// SCENARIO 2: SERVICE REQUEST BURST
// ═══════════════════════════════════════════════════════════════════════════

fn scenario_2_service_burst() {
    println!("\n========================================================================");
    println!("Scenario 2: Service Request Burst");
    println!("  100 requests from external client, server at 10ms spin");
    println!("========================================================================\n");

    let handled = Arc::new(AtomicU64::new(0));
    let handled_cb = handled.clone();

    let config = ExecutorConfig::from_env().node_name("server2");
    let mut executor = Executor::<_, 8, 8192>::open(&config).expect("Failed to open session");

    executor
        .add_service::<AddTwoInts, _>("/bench2/add", move |req: &AddTwoIntsRequest| {
            handled_cb.fetch_add(1, Ordering::Relaxed);
            AddTwoIntsResponse { sum: req.a + req.b }
        })
        .expect("Service");

    println!("  Waiting for service registration (2s)...");
    std::thread::sleep(Duration::from_secs(2));

    let mut child = spawn_publisher("2");
    println!("  Client started (PID {})", child.id());

    // Client waits 3s then sends 100 requests at 10ms = ~4s. Spin for 15s total.
    println!("  Running executor (15s)...");
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(15) {
        let _ = executor.spin_one_period_timed(Duration::from_millis(10));
    }

    let _ = child.kill();
    let _ = child.wait();

    let total_handled = handled.load(Ordering::Relaxed);
    println!("\nResults:");
    println!("  Requests sent: 100");
    println!("  Server handled: {total_handled}");
    println!(
        "  Success rate: {:.1}%",
        total_handled as f64 / 100.0 * 100.0
    );

    println!("\nAnalysis:");
    println!("  Blocking call() serializes requests — each completes before the next.");
    println!("  Single-slot buffer: 1 request at a time, no backlog possible.");
}

// ═══════════════════════════════════════════════════════════════════════════
// SCENARIO 3: MIXED SUBSCRIPTION + SERVICE LOAD
// ═══════════════════════════════════════════════════════════════════════════

fn scenario_3_mixed_load() {
    println!("\n========================================================================");
    println!("Scenario 3: Mixed Subscription + Service Load");
    println!("  2 topics at 50 Hz + service at 10 Hz, executor at 10ms spin");
    println!("========================================================================\n");

    const DURATION_SECS: u64 = 10;
    const SPIN_PERIOD: Duration = Duration::from_millis(10);

    let a_received = Arc::new(AtomicU64::new(0));
    let b_received = Arc::new(AtomicU64::new(0));
    let svc_handled = Arc::new(AtomicU64::new(0));
    let a_cb = a_received.clone();
    let b_cb = b_received.clone();
    let svc_cb = svc_handled.clone();

    let config = ExecutorConfig::from_env().node_name("bench3");
    let mut executor = Executor::<_, 8, 8192>::open(&config).expect("Failed to open session");

    executor
        .add_subscription::<Int32, _>("/bench3/topic_a", move |_msg: &Int32| {
            a_cb.fetch_add(1, Ordering::Relaxed);
        })
        .expect("Sub A");

    executor
        .add_subscription::<Int32, _>("/bench3/topic_b", move |_msg: &Int32| {
            b_cb.fetch_add(1, Ordering::Relaxed);
        })
        .expect("Sub B");

    executor
        .add_service::<AddTwoInts, _>("/bench3/add", move |req: &AddTwoIntsRequest| {
            svc_cb.fetch_add(1, Ordering::Relaxed);
            AddTwoIntsResponse { sum: req.a + req.b }
        })
        .expect("Service");

    println!("  Waiting for propagation (2s)...");
    std::thread::sleep(Duration::from_secs(2));

    let mut child = spawn_publisher("3");
    println!("  Publisher+client started (PID {})", child.id());

    std::thread::sleep(Duration::from_secs(3));

    println!("  Running measurement ({DURATION_SECS}s)...");
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(DURATION_SECS) {
        let _ = executor.spin_one_period_timed(SPIN_PERIOD);
    }

    let _ = child.kill();
    let _ = child.wait();

    let a_recv = a_received.load(Ordering::Relaxed);
    let b_recv = b_received.load(Ordering::Relaxed);
    let svc_count = svc_handled.load(Ordering::Relaxed);
    let expected_per_topic = 500u64;
    let expected_svc = 100u64;

    println!("\nResults:");
    println!(
        "  /bench3/topic_a: expected~{expected_per_topic}, received={a_recv}, \
         loss~{:.1}%",
        (1.0 - a_recv as f64 / expected_per_topic as f64).max(0.0) * 100.0
    );
    println!(
        "  /bench3/topic_b: expected~{expected_per_topic}, received={b_recv}, \
         loss~{:.1}%",
        (1.0 - b_recv as f64 / expected_per_topic as f64).max(0.0) * 100.0
    );
    println!("  Service: expected~{expected_svc}, handled={svc_count}");

    println!("\nAnalysis:");
    println!("  Both topics should have similar delivery rates (fair per spin).");
    println!("  Service uses blocking call() — serialized, low loss expected.");
    println!("  No component starves another — all get equal per-spin opportunity.");
}

// ═══════════════════════════════════════════════════════════════════════════
// MAIN
// ═══════════════════════════════════════════════════════════════════════════

fn main() {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();

    // Publisher/client subprocess mode
    if args.len() > 2 && args[1] == "--publish" {
        match args[2].as_str() {
            "1" => publish_scenario_1(),
            "2" => client_scenario_2(),
            "3" => publish_scenario_3(),
            other => eprintln!("Unknown scenario: {other}"),
        }
        return;
    }

    // Main benchmark mode
    println!("========================================================================");
    println!("  nros Executor Fairness Benchmark (Phase 37.4)");
    println!("========================================================================");
    println!();
    println!("Architecture: single-slot buffer per subscription/service.");
    println!("Each spin_once() processes at most 1 message per subscription,");
    println!("1 request per service. Message loss comes from overwrite, not starvation.");
    println!();
    println!("Uses separate processes for publishers (zenoh-pico single-session limit).");
    println!();

    scenario_1_asymmetric_subscriptions();

    // Brief pause between scenarios for zenoh session cleanup
    println!("\n  (2s pause between scenarios...)");
    std::thread::sleep(Duration::from_secs(2));

    scenario_2_service_burst();

    println!("\n  (2s pause between scenarios...)");
    std::thread::sleep(Duration::from_secs(2));

    scenario_3_mixed_load();

    println!("\n========================================================================");
    println!("  Benchmark Complete");
    println!("========================================================================");
}
