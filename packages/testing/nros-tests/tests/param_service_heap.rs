//! Issue 2812 — the RTOS allocators' per-node service term, on booted images
//! with five parameter-serving nodes and a live router.
//!
//! The ThreadX byte pool and the FreeRTOS heap_4 default were both measured on
//! one-node images. Every node that serves the parameter services runs six
//! servers, each owning a request/reply buffer pair on that allocator (which on
//! both ports also hands out the task stacks). So each derived default now adds
//! a term per further node (`nros_sizing_descriptor::service_heap`), from the
//! sizing descriptor's `[image] param_service_nodes`.
//!
//! One boot per port asserts:
//!   * the row bakes the port the test's router listens on;
//!   * a NATIVE listener on the host receives `param_a`'s `/chatter` through
//!     `rmw_zenohd`: all five nodes registered and the session is up;
//!   * no allocator-exhaustion line on the console, the board's own or the
//!     RMW adapter's `heap exhausted` for an entity handle;
//!   * the board's allocator peak line leaves bytes FREE. A pool at 0 free
//!     refused something even when nothing said so, which is how the five-node
//!     ThreadX image fails on the flat pool: it still delivers, and
//!     `/param_e`'s parameter servers are never created.
//!
//! Why FIVE: on ThreadX each node past the first measured 34,024 bytes of pool
//! (742,240 peak with one node, 844,312 with four), and the flat one-node pool
//! is 851,968 bytes, so four nodes still fit it with about 7.6 KB to spare and
//! the fifth is the one that does not. FreeRTOS's heap_4 default has hundreds
//! of KiB of headroom at this size, so that boot is the no-regression half.
//!
//! Each image's port is the next slot in its band after the parameter cell's,
//! `port_of(<platform>, Rust, Params) + 1` (offset 71, used by no workload), so
//! it can run beside the one-node images. It is baked into the fixture's
//! `system.toml`.
//!
//! Fixtures: rows `threadx-riscv64-param-five-node` and
//! `freertos-param-five-node`.
//!
//! Run with: `cargo nextest run -p nros-tests --test param_service_heap`

use nros_tests::{
    alloc::port_of,
    count_pattern,
    fixtures::{
        ManagedProcess, QemuProcess, RequireFixture, ZenohRouter, baked_locator,
        build_native_listener, build_param_five_node_freertos,
        build_param_five_node_threadx_riscv64, is_qemu_available, is_qemu_riscv64_available,
        require_zenohd,
    },
    matrix::{Lang, PlatformId, Workload},
    output::{
        FREERTOS_HEAP_PEAK_MARKER, FREERTOS_MALLOC_FAILED, LISTENER_LOG_PREFIX,
        LISTENER_READY_MARKER, RMW_HANDLE_HEAP_EXHAUSTED_MARKER, THREADX_BYTE_POOL_PEAK_MARKER,
        THREADX_BYTE_POOL_REFUSED_MARKER,
    },
};
use std::{path::Path, process::Command, time::Duration};

/// Boot `image` with `start` against a router on its row's port, and assert
/// delivery, no `exhausted` line, and a `peak` line with bytes left free.
fn five_nodes_fit(
    platform: PlatformId,
    fixture_id: &str,
    image: &Path,
    start: fn(&Path) -> nros_tests::TestResult<QemuProcess>,
    peak: &str,
    exhausted: &str,
) {
    let port = port_of(platform, Lang::Rust, Workload::Params) + 1;
    baked_locator::assert_row_dials_port(fixture_id, port);
    let listener = build_native_listener()
        .map(|p| p.to_path_buf())
        .require("native listener");

    // 0.0.0.0, so the slirp guest (gateway 10.0.2.2) reaches it.
    let _router = ZenohRouter::start_slirp(port)
        .unwrap_or_else(|e| nros_tests::unmet!("zenohd failed to start on {port}: {e}"));
    let mut obs = {
        let mut cmd = Command::new(&listener);
        cmd.env("NROS_LOCATOR", format!("tcp/127.0.0.1:{port}"))
            .env("RUST_LOG", "info");
        ManagedProcess::spawn_image(cmd, "native-listener")
            .unwrap_or_else(|e| panic!("spawn native listener: {e}"))
    };
    obs.wait_for_output_pattern(LISTENER_READY_MARKER, Duration::from_secs(10))
        .unwrap_or_else(|_| {
            obs.kill();
            panic!("native listener never became ready")
        });

    let mut qemu = start(image).unwrap_or_else(|e| panic!("boot {platform:?} QEMU: {e}"));
    let delivered = obs.wait_for_output_count(LISTENER_LOG_PREFIX, 3, Duration::from_secs(60));
    let console = qemu.collect_until(peak, Duration::from_secs(20));
    qemu.kill();
    obs.kill();

    let line = console
        .lines()
        .find(|l| l.contains(peak))
        .unwrap_or_else(|| {
            panic!("{platform:?}: the board never printed `{peak}`. Console:\n{console}")
        })
        .trim();
    eprintln!("{platform:?}, five parameter nodes: {line}");
    // A pool with nothing left refused an allocation, whether or not anything
    // reported it: on main the ThreadX image delivered while its byte pool sat
    // at 0 free, the parameter servers silently short.
    let free =
        free_bytes(line).unwrap_or_else(|| panic!("{platform:?}: no `(<n> free)` in `{line}`"));
    assert!(
        free > 0,
        "{platform:?}: the allocator ran dry ({line}), so the five nodes' allocations \
         did not all fit. Console:\n{console}"
    );
    assert!(
        !console.contains(RMW_HANDLE_HEAP_EXHAUSTED_MARKER),
        "{platform:?}: an entity handle did not fit the allocator \
         (`{RMW_HANDLE_HEAP_EXHAUSTED_MARKER}`), so a node is short of its services. \
         Console:\n{console}"
    );
    assert!(
        !console.contains(exhausted),
        "{platform:?}: the allocator could not hold the five nodes' allocations \
         (`{exhausted}`). Console:\n{console}"
    );
    let out = delivered.unwrap_or_else(|e| {
        panic!(
            "{platform:?}: the native listener did not receive 3 `/chatter` samples from the \
             five-node image ({e}). Console:\n{console}"
        )
    });
    let n = count_pattern(&out, LISTENER_LOG_PREFIX);
    assert!(n >= 3, "{platform:?}: expected >= 3 deliveries, got {n}");
}

/// The `<n>` of a peak line's `(<n> free)`.
fn free_bytes(line: &str) -> Option<u64> {
    let (head, _) = line.split_once(" free)")?;
    head.rsplit('(').next()?.trim().parse().ok()
}

#[test]
fn threadx_riscv64_five_parameter_nodes_fit_the_derived_byte_pool() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::ThreadxRiscv64],
        "the rv-virt-threadx five-node parameter image",
    );
    require_zenohd();
    if !is_qemu_riscv64_available() {
        nros_tests::unmet!("qemu-system-riscv64 not found");
    }
    let image = build_param_five_node_threadx_riscv64()
        .map(|p| p.to_path_buf())
        .require("rv-virt-threadx five-node parameter image");
    five_nodes_fit(
        PlatformId::ThreadxRiscv64,
        "threadx-riscv64-param-five-node",
        &image,
        |p| QemuProcess::start_riscv64_virt(p, 0),
        THREADX_BYTE_POOL_PEAK_MARKER,
        THREADX_BYTE_POOL_REFUSED_MARKER,
    );
}

#[test]
fn freertos_five_parameter_nodes_fit_the_derived_heap() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::FreertosMps2],
        "the mps2-an385-freertos five-node parameter image",
    );
    require_zenohd();
    if !is_qemu_available() {
        nros_tests::unmet!("qemu-system-arm not found");
    }
    let image = build_param_five_node_freertos()
        .map(|p| p.to_path_buf())
        .require("mps2-an385-freertos five-node parameter image");
    five_nodes_fit(
        PlatformId::FreertosMps2,
        "freertos-param-five-node",
        &image,
        QemuProcess::start_mps2_an385_networked,
        FREERTOS_HEAP_PEAK_MARKER,
        FREERTOS_MALLOC_FAILED,
    );
}
