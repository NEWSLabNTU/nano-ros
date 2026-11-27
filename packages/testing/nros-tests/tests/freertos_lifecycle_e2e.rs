//! Issue 2428's embedded acceptance: a managed node on FreeRTOS QEMU (zenoh)
//! publishes nothing while `unconfigured` or `inactive`, and publishes once
//! `active`, in C, C++ and Rust.
//!
//! Each image of `bins/lifecycle-freertos-mps2` runs one managed talker under
//! `/talker` that boots unconfigured (`[lifecycle] autostart = "none"`). The
//! component holds no lifecycle code, so what is checked is the runtime's
//! publisher gate (phase-554 W6). `bins/lifecycle-driver`, a native nano-ros
//! lifecycle client, drives `configure`, `activate` and `deactivate` through
//! the node's own REP-2002 services and counts `/chatter` in each state. No
//! ROS 2 install is involved, so this is a baked cell every tier can run.

use std::{process::Command, time::Duration};

use nros_tests::{
    TestResult,
    alloc::port_of,
    fixtures::{
        ManagedProcess, QemuProcess, RequireFixture, ZenohRouter, build_lifecycle_driver,
        build_lifecycle_freertos_mps2, lane::require_platform_in_lane, or_unmet,
    },
    matrix::{Lang, PlatformId, Workload},
    output::{LC_DONE, LC_PHASE},
    wait_for_port,
};
use rstest::rstest;

/// One `LC_PHASE` line, parsed.
#[derive(Debug)]
struct Phase {
    ok: bool,
    state: String,
    rx: u32,
}

/// The driver's line for `phase`, or a panic naming what it printed.
fn phase(output: &str, phase: &str) -> Phase {
    let line = output
        .lines()
        .filter_map(|l| l.split(LC_PHASE).nth(1))
        .find(|l| l.split_whitespace().next() == Some(phase))
        .unwrap_or_else(|| panic!("the driver reported no `{phase}` step:\n{output}"));
    let field = |key: &str| -> &str {
        line.split_whitespace()
            .find_map(|w| w.strip_prefix(key))
            .unwrap_or_else(|| panic!("`{key}` missing from `{line}`"))
    };
    Phase {
        ok: field("ok=") == "true",
        state: field("state=").to_owned(),
        rx: field("rx=").parse().unwrap_or(u32::MAX),
    }
}

fn lang_of(lang: &str) -> Lang {
    match lang {
        "c" => Lang::C,
        "cpp" => Lang::Cpp,
        _ => Lang::Rust,
    }
}

#[rstest]
#[case::c("c")]
#[case::cpp("cpp")]
#[case::rust("rust")]
fn a_managed_freertos_node_is_silent_until_active(#[case] lang: &str) -> TestResult<()> {
    require_platform_in_lane(&[PlatformId::FreertosMps2], "the FreeRTOS lifecycle cell");
    let image = build_lifecycle_freertos_mps2(lang)
        .require("lifecycle-freertos-mps2 -- run `just build freertos`");
    let driver = build_lifecycle_driver().require("lifecycle-driver -- run `just build linux`");

    // The image's baked locator dials its cell's port on QEMU's SLIRP host.
    let port = port_of(PlatformId::FreertosMps2, lang_of(lang), Workload::Lifecycle);
    let _router = or_unmet(ZenohRouter::start_slirp(port));
    assert!(
        wait_for_port(port, Duration::from_secs(5)),
        "zenohd not reachable on {port}"
    );

    let mut guest = QemuProcess::start_mps2_an385_networked_measured(image)
        .unwrap_or_else(|e| panic!("start QEMU for the {lang} lifecycle image: {e:?}"));

    let mut cmd = Command::new(driver);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", format!("tcp/127.0.0.1:{port}"))
        .env("NROS_SESSION_MODE", "client");
    let mut drv = ManagedProcess::spawn_image(cmd, "lifecycle-driver")
        .unwrap_or_else(|e| panic!("spawn lifecycle-driver: {e}"));
    let output = drv
        .wait_for_output_count(LC_DONE, 1, Duration::from_secs(180))
        .unwrap_or_else(|out| {
            guest.kill();
            panic!("[{lang}] the driver never finished:\n{out:?}")
        });
    drv.kill();
    guest.kill();
    eprintln!("[{lang}] driver:\n{output}");

    let boot = phase(&output, "boot");
    assert_eq!(boot.state, "unconfigured", "[{lang}] boot: {boot:?}");
    assert_eq!(
        boot.rx, 0,
        "[{lang}] an unconfigured node published: {boot:?}"
    );

    let configure = phase(&output, "configure");
    assert!(configure.ok, "[{lang}] configure refused: {configure:?}");
    assert_eq!(
        configure.state, "inactive",
        "[{lang}] after configure: {configure:?}"
    );
    assert_eq!(
        configure.rx, 0,
        "[{lang}] an inactive node published: {configure:?}"
    );

    let activate = phase(&output, "activate");
    assert!(activate.ok, "[{lang}] activate refused: {activate:?}");
    assert_eq!(
        activate.state, "active",
        "[{lang}] after activate: {activate:?}"
    );
    assert!(
        activate.rx >= 2,
        "[{lang}] an active 1 Hz talker published {} times in 4 s: {activate:?}",
        activate.rx
    );

    let deactivate = phase(&output, "deactivate");
    assert!(deactivate.ok, "[{lang}] deactivate refused: {deactivate:?}");
    assert_eq!(
        deactivate.state, "inactive",
        "[{lang}] after deactivate: {deactivate:?}"
    );
    assert_eq!(
        deactivate.rx, 0,
        "[{lang}] a deactivated node kept publishing: {deactivate:?}"
    );
    Ok(())
}

/// Each image's baked locator dials its cell's port. Static: reads the
/// manifest, so a wrong port is red in every lane, not only the one that
/// boots the image.
#[test]
fn lifecycle_rows_dial_their_cell_ports() {
    for (id, lang) in [
        ("freertos-c-lifecycle-mps2", Lang::C),
        ("freertos-cpp-lifecycle-mps2", Lang::Cpp),
        ("freertos-rust-lifecycle-mps2", Lang::Rust),
    ] {
        nros_tests::fixtures::baked_locator::assert_row_dials_port(
            id,
            port_of(PlatformId::FreertosMps2, lang, Workload::Lifecycle),
        );
    }
}

/// Runtime cells `matrix::consumer_of` assigns to this binary. Fixture-free.
#[test]
fn cases_bound_to_matrix_cells() {
    use nros_tests::matrix::{
        Consumer, Kind as K, Lang as L, PlatformId as P, Rmw as R, Workload as W,
        assert_consumer_cases,
    };
    assert_consumer_cases(
        Consumer::FreertosLifecycle,
        &[
            (P::FreertosMps2, L::C, R::Zenoh, W::Lifecycle, K::Workspace),
            (
                P::FreertosMps2,
                L::Cpp,
                R::Zenoh,
                W::Lifecycle,
                K::Workspace,
            ),
            (
                P::FreertosMps2,
                L::Rust,
                R::Zenoh,
                W::Lifecycle,
                K::Workspace,
            ),
        ],
    );
}
