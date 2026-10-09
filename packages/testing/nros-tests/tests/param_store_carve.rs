//! Issue 1706 / phase-382 W3' — the parameter store CARVED from the executor
//! backing, on booted RTOS images with a live router.
//!
//! phase-382 W3' moved the store out of the heap: when the build can see that
//! an image builds a store (here, the contract declares the parameter, so
//! `nros sync` writes `[params] store = "declared"`), the executor backing
//! carries a store region and the executor logs
//! `parameter store: N slots (B B) carved from the executor backing`. Zephyr,
//! FreeRTOS and threadx-linux were measured booted when that landed;
//! rv-virt-threadx and NuttX were not, because no fixture produced a bootable
//! image of either that declares a parameter. These two cells are that image.
//!
//! Each case asserts, on one boot:
//!   * the carve line is on the console (the store did not take the heap road),
//!   * no `parameter store refused` line,
//!   * a NATIVE listener on the host receives the image's `/chatter` through
//!     `rmw_zenohd` — the store sits beside a working session.
//!
//! The carve line also proves the board's log path: on rv-virt-threadx a
//! session entry did not register the platform log writer until issue 1706, so
//! every `nros_log` record, this one included, was dropped.
//!
//! Fixtures: rows `nuttx-param-store` (`just nuttx build-fixtures-arm`) and
//! `threadx-riscv64-param-store` (`just threadx_riscv64 build-fixture-extras`).
//!
//! Run with: `cargo nextest run -p nros-tests --test param_store_carve`

use nros_tests::{
    alloc::port_of,
    count_pattern,
    fixtures::{
        ManagedProcess, QemuProcess, RequireFixture, ZenohRouter, build_native_listener,
        build_param_store_nuttx_qemu_arm, build_param_store_threadx_riscv64, is_qemu_available,
        is_qemu_riscv64_available, require_zenohd,
    },
    matrix::{Lang, PlatformId, Workload},
    output::{
        LISTENER_LOG_PREFIX, LISTENER_READY_MARKER, PARAM_STORE_CARVED_MARKER,
        PARAM_STORE_REFUSED_MARKER,
    },
};
use std::{path::Path, process::Command, time::Duration};

/// Boot `image` with `start`, observe it through a native listener on the
/// router at `port`, and assert the carve line and delivery.
fn carve_and_deliver(
    platform: PlatformId,
    image: &Path,
    start: fn(&Path) -> nros_tests::TestResult<QemuProcess>,
) {
    let port = port_of(platform, Lang::Rust, Workload::Params);
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
        ManagedProcess::spawn_command(cmd, "native-listener")
            .unwrap_or_else(|e| panic!("spawn native listener: {e}"))
    };
    obs.wait_for_output_pattern(LISTENER_READY_MARKER, Duration::from_secs(10))
        .unwrap_or_else(|_| {
            obs.kill();
            panic!("native listener never became ready")
        });

    let mut qemu = start(image).unwrap_or_else(|e| panic!("boot {platform:?} QEMU: {e}"));
    // Delivery first: it bounds the boot. The store is built at the first
    // parameter declaration, inside the node's `register` — before the first
    // publish — so by the time samples arrive its line is already sitting in
    // the console pipe, and the collect below only reads it back.
    let delivered = obs.wait_for_output_count(LISTENER_LOG_PREFIX, 3, Duration::from_secs(60));
    let console = qemu.collect_until(PARAM_STORE_CARVED_MARKER, Duration::from_secs(5));
    qemu.kill();
    obs.kill();

    assert!(
        console.contains(PARAM_STORE_CARVED_MARKER),
        "{platform:?}: no `{PARAM_STORE_CARVED_MARKER}` line — the store took the heap road \
         (or the board dropped the `nros_log` record). Console:\n{console}"
    );
    assert!(
        !console.contains(PARAM_STORE_REFUSED_MARKER),
        "{platform:?}: the parameter store was refused. Console:\n{console}"
    );
    let out = delivered.unwrap_or_else(|e| {
        panic!(
            "{platform:?}: the native listener did not receive 3 `/chatter` samples from the \
             carved-store image ({e}). Console:\n{console}"
        )
    });
    let n = count_pattern(&out, LISTENER_LOG_PREFIX);
    assert!(n >= 3, "{platform:?}: expected >= 3 deliveries, got {n}");
}

#[test]
fn nuttx_arm_carves_the_parameter_store_and_delivers() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::NuttxArm],
        "the NuttX parameter-store image",
    );
    require_zenohd();
    if !is_qemu_available() {
        nros_tests::unmet!("qemu-system-arm not found");
    }
    let image = build_param_store_nuttx_qemu_arm()
        .map(|p| p.to_path_buf())
        .require("NuttX parameter-store image");
    carve_and_deliver(PlatformId::NuttxArm, &image, |p| {
        QemuProcess::start_nuttx_virt(p, true)
    });
}

#[test]
fn threadx_riscv64_carves_the_parameter_store_and_delivers() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::ThreadxRiscv64],
        "the rv-virt-threadx parameter-store image",
    );
    require_zenohd();
    if !is_qemu_riscv64_available() {
        nros_tests::unmet!("qemu-system-riscv64 not found");
    }
    let image = build_param_store_threadx_riscv64()
        .map(|p| p.to_path_buf())
        .require("rv-virt-threadx parameter-store image");
    carve_and_deliver(PlatformId::ThreadxRiscv64, &image, |p| {
        QemuProcess::start_riscv64_virt(p, 0)
    });
}
