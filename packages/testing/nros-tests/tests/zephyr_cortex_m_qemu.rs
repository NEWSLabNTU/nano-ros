//! Zephyr on QEMU MPS2-AN385 (Cortex-M3) — the phase-337 W2 witness.
//!
//! **Bucket (phase-329 W4): matrix consumer.** These are the two Runtime cells
//! of `PlatformId::ZephyrQemuCortexM` in `matrix::CELLS`, and nothing else.
//!
//! ## Why this platform exists at all
//!
//! Before phase-337 W2, "Zephyr" in this repo meant exactly one configuration:
//! `native_sim/native/64`. That build offloads sockets to the host
//! (`CONFIG_NET_SOCKETS_OFFLOAD=y`), so Zephyr's own IP stack never runs, no
//! ethernet driver is involved, and pointers are the host's 64 bits. RFC-0064
//! calls that a board, not a platform — and the gap is not theoretical. Bringing
//! this witness up cost five real defects, every one invisible to native_sim:
//! a 32-bit `size_t`/`uintptr_t` header conflict, an atomics feature gated on an
//! arch list (which said yes to a target that has native CAS), a Rust staticlib
//! with no allocator or panic handler off the std path, a cmake feature string
//! computed in two places, and a board with no entropy device.
//!
//! So what these tests assert is narrow and deliberate: an image built for a
//! 32-bit Cortex-M3, running Zephyr's IN-KERNEL IP stack over the `eth_smsc911x`
//! driver, opens a zenoh session through SLIRP to a router on the host and
//! publishes.
//!
//! ## Three cells, one per language
//!
//! C, C++ and Rust. This section used to say a Rust cell was impossible here:
//! the pinned `zephyr-lang-rust` could not compile the `zephyr` crate for any
//! board whose devicetree has gpio nodes — a five-argument `GpioPin::new`
//! against a six-argument signature (**issue 0432**), which an385 has and
//! native_sim does not. 0432 was resolved 2026-08-12 by phase-346 W2/W3, and
//! `matrix::CELLS` had declared the Rust cell `Runtime` since phase-346 W3 with
//! nothing running it — the kind of undeclared gap RFC-0051 exists to prevent.
//!
//! ## Prerequisites
//!
//! - `just zephyr build-fixtures` (the west leaves lane builds the
//!   `build-cortex-m-*` leaves — `scripts/build/zephyr-fixture-leaves.sh`)
//! - `qemu-system-arm` with `mps2-an385` machine support
//! - a zenoh router — ROS's `rmw_zenohd`, resolved by `ros_zenohd_path`
//!   (nano-ros ships none; RFC-0075). `just zenohd` starts one.
//!
//! Run with: `cargo nextest run -p nros-tests --test zephyr_cortex_m_qemu`

use nros_tests::{
    alloc::port_of,
    fixtures::{
        QemuProcess, Rmw, ZenohRouter, build_zephyr_cortex_m_example, is_qemu_available,
        is_zenohd_available,
    },
    matrix::{Lang, PlatformId, Workload},
};
use std::time::Duration;

/// The talker needs ~2 s for `net_config` to assign the static address, then a
/// TCP connect to the host router, then the publish loop. 90 s is the same
/// budget the FreeRTOS MPS2 lane uses on the identical machine and NIC.
const BOOT_BUDGET: Duration = Duration::from_secs(90);

/// Budget for a cell that waits on the TALKER line rather than the net line.
///
/// Deliberately far below `BOOT_BUDGET`: the board reaches "Network ready" at
/// ~2.15 s and the talker's timer period is 500 ms, so a healthy image publishes
/// within a few seconds. Waiting 90 s for a line a HALTED image will never print
/// just converts a fast, legible failure into nextest's 60 s per-test timeout,
/// which reports "TIMEOUT" and hides the fault text in the output.
const PUBLISH_BUDGET: Duration = Duration::from_secs(15);

/// Zephyr's `net_config` announcing the static address from
/// `cmake/zephyr/mps2-an385.conf`. This is the platform's distinguishing
/// evidence: it can only be printed by the in-kernel IP stack driving a real
/// ethernet controller, which is precisely what `native_sim`'s offloaded
/// sockets never do.
const NET_STACK_READY_MARKER: &str = "IPv4 address: 10.0.2.15";

/// One cell's body. `lang` selects the example tree AND the router port — the
/// port comes from the matrix allocator rather than a literal, so a future
/// platform-index change moves the bake and the test together instead of
/// leaving one of them behind.
/// `wait_marker` is per-cell because the two log orderings genuinely differ, and
/// the difference is not cosmetic — it decides when QEMU is killed.
///
/// For C/C++ the talker's first `Publishing:` reaches the stream BEFORE the
/// boot banner (Zephyr's logging subsystem flushes on its own schedule), so
/// waiting on the talker line returns at ~0.1 s with `net_config` still
/// unflushed and the driver assertion then fails a healthy run. Those cells wait
/// on the NET line, making the accumulated output a superset.
///
/// Rust flushes the other way round: `net_config` lands first and the publishes
/// follow. Waiting on the net line there kills the guest ~immediately after
/// "Network ready", BEFORE the 500 ms timer has fired — which reads exactly like
/// issue 0531's dead-clock symptom and is not it. That cell waits on the TALKER
/// line, so its accumulated output is the superset instead.
fn run_pubsub_cell(lang: &str, matrix_lang: Lang, wait_marker: &str, budget: Duration) {
    if !is_qemu_available() {
        nros_tests::skip!("qemu-system-arm not found");
    }
    if !is_zenohd_available() {
        nros_tests::skip!("zenohd not found — run `just setup` / source ./activate.sh");
    }

    // issue 0807 — this used to `skip!` on ANY resolver error, with a message
    // that said "not prebuilt" whatever the cause. Two things were wrong with
    // that. Since issue 0584 an absent IN-LANE fixture is a hard failure, not a
    // skip: the run is gated, so the build already promised these exist.
    // And the resolver's other verdict is STALE, which the skip relabelled as
    // missing — so a stale image reported as "not prebuilt", was filtered out
    // as a setup condition, and the cell silently stopped running. It sat that
    // way through five consecutive stale verdicts (issue 0806).
    //
    // Out-of-lane is already handled INSIDE the resolver, which raises its own
    // `[SKIPPED]`, so anything that reaches this `Err` is a genuine failure and
    // must be loud.
    let binary = build_zephyr_cortex_m_example(lang, "talker", Rmw::Zenoh).unwrap_or_else(|e| {
        panic!(
            "zephyr/{lang}/talker for mps2_an385 did not resolve. The lane gate \
             already asserted this fixture is built, so this is a real failure, \
             not a setup condition — read the verdict before rebuilding: {e:?}"
        )
    });

    // Bind 0.0.0.0, not loopback: the guest reaches the host through SLIRP's
    // 10.0.2.2 gateway, and a loopback-only listener leaves those SYNs
    // unreachable. The image has the matching locator baked in at build time
    // (`CONFIG_NROS_ZENOH_LOCATOR`), because a Cortex-M image has no env to read
    // one from — so this port and the fixture's must agree, and both derive from
    // `port_of`.
    let port = port_of(PlatformId::ZephyrQemuCortexM, matrix_lang, Workload::Pubsub);
    let _router = ZenohRouter::start_slirp(port)
        .unwrap_or_else(|e| panic!("failed to start zenohd on {port} for the SLIRP guest: {e:?}"));

    let mut qemu = QemuProcess::start_mps2_an385_networked(&binary)
        .expect("spawn Zephyr Cortex-M zenoh talker");

    // Wait on the DRIVER's line, not the talker's, even though the talker's is
    // what the cell is nominally about. The console is muxed and Zephyr's
    // logging subsystem flushes on its own schedule, so the first
    // `Publishing:` reliably lands in the stream BEFORE the boot banner — a
    // wait on the talker pattern returns at ~0.1 s with the `net_config` lines
    // still unflushed, and the driver assertion below then fails on a run that
    // is in fact perfectly healthy. Waiting for the later-flushed line makes the
    // accumulated output a superset, so both assertions see what they need.
    let output = qemu.collect_until(wait_marker, budget);
    qemu.kill();

    eprintln!("Zephyr mps2_an385 {lang} zenoh talker output:\n{output}");

    // Asserted separately from the publishes on purpose: this line is the
    // evidence that Zephyr's OWN stack and the smsc911x driver ran — the whole
    // reason this platform is distinct from native_sim. A talker that published
    // without it would mean the witness had quietly stopped being one.
    assert!(
        output.contains(NET_STACK_READY_MARKER),
        "Zephyr's in-kernel net stack never assigned the static address — the \
         eth_smsc911x driver did not come up.\nOutput:\n{output}"
    );
    nros_tests::output::assert_talker(&output, 1);
}

#[test]
fn zephyr_cortex_m_c_zenoh_pubsub_e2e() {
    run_pubsub_cell("c", Lang::C, NET_STACK_READY_MARKER, BOOT_BUDGET);
}

#[test]
fn zephyr_cortex_m_cpp_zenoh_pubsub_e2e() {
    run_pubsub_cell("cpp", Lang::Cpp, NET_STACK_READY_MARKER, BOOT_BUDGET);
}

/// The Rust cell of this board, which `matrix::CELLS` has declared `Runtime`
/// since phase-346 W3 while no test ran it — a Runtime row nothing executes is
/// the lie RFC-0051 exists to prevent.
///
/// It is also the only way to CONFIRM issue 0531 today. That bug —
/// `k_cycle_get_64()` returning a silent permanent 0 below 60 MHz, so every
/// periodic callback dies while subscriptions keep working — was fixed by
/// RFC-0073 (`platform.c` now falls back to the tick clock) but never verified
/// on hardware, because verification needs a sub-60 MHz board and this is it at
/// 12 MHz. The talker publishes from a 500 ms `nros_cpp_timer_create`, so
/// `assert_talker` IS the clock assertion: a dead clock produces no publishes.
///
/// The C and C++ cells cannot supply that evidence — issue 0552 has both
/// branching to `PC=0` right after net init on this same board, which kills the
/// image before any timer runs. Rust is unaffected there, which is what makes
/// this cell the witness.
#[test]
fn zephyr_cortex_m_rust_zenoh_pubsub_e2e() {
    run_pubsub_cell(
        "rust",
        Lang::Rust,
        nros_tests::output::TALKER_READY_MARKER,
        PUBLISH_BUDGET,
    );
}
