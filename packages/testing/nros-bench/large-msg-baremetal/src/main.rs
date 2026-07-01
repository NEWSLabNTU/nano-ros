//! QEMU bare-metal large message publish test
//!
//! Tests that publish_raw succeeds for various payload sizes on bare-metal.
//! This is a publish-only test (no E2E — that requires TAP networking).

#![no_std]
#![no_main]

use nros::prelude::*;
use nros_board_mps2_an385::{Config, println, run};
use panic_semihosting as _;

// phase-271 (issue #110) — this bench is a no-alloc `rmw-cffi` bare-metal target,
// so it can't use the `alloc`-only `Executor::open` (which leaks a default
// backing). It registers NO callbacks (manual `spin_once` + `publish_raw`), so it
// carves a tiny per-entry executor backing from its own `static` — no
// workspace-global `NROS_EXECUTOR_MAX_CBS`, and a fraction of the default
// 4-slot / ~74 KB arena.
const EXEC_SIZING: nros::ExecutorSizing = nros::ExecutorSizing {
    cbs: 2,
    sc: 2,
    arena: nros::arena_size_for(2),
};
static mut EXEC_BACKING: [core::mem::MaybeUninit<u64>; EXEC_SIZING.u64_len()] =
    [const { core::mem::MaybeUninit::uninit() }; EXEC_SIZING.u64_len()];

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

#[nros_board_mps2_an385::entry]
fn main() -> ! {
    // Phase 212.M-F.18 — build-time `Config` literal supersedes
    // the pre-212 `Config::from_toml(include_str!(...))` sidecar.
    // Transcribed verbatim from the retired `nros.toml` (ip
    // 10.0.2.10/24, mac 02:00:00:00:00:00, gateway 10.0.2.2,
    // locator tcp/10.0.2.2:7450, domain_id 0). Bench fixtures are
    // static — no runtime parameter sweep — so the `Config { ... }`
    // literal matches the §M.10 native example pattern (ref:
    // commit `e6f4cb346`).
    let config = Config {
        mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x00],
        ip: [10, 0, 2, 10],
        prefix: 24,
        gateway: [10, 0, 2, 2],
        zenoh_locator: "tcp/10.0.2.2:7450",
        domain_id: 0,
    };
    run(config, |config| {
        let exec_config = ExecutorConfig::new(config.zenoh_locator)
            .domain_id(config.domain_id)
            .node_name("large_msg_test");
        // Phase 104.A — bare-metal callers explicitly register the RMW
        // backend before `Executor::open`. POSIX hosts auto-register via
        // `.init_array`; this target doesn't walk that section.
        nros_rmw_zenoh::register().expect("Failed to register RMW backend");
        // SAFETY: `EXEC_BACKING` is a program-lifetime static; `run`'s closure
        // executes once, so this executor is its sole `'static` borrower.
        let backing = unsafe { &mut *core::ptr::addr_of_mut!(EXEC_BACKING) };
        let mut executor = unsafe { Executor::open_in(&exec_config, backing, EXEC_SIZING)? };
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
    })
}
