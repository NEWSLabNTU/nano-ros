//! Zephyr on the Nucleo-F767ZI, in its Renode twin (phase-524 W4c).
//!
//! **Bucket (phase-329 W4): matrix consumer.** The one Runtime cell of
//! `PlatformId::ZephyrNucleoF767zi` in `matrix::CELLS`.
//!
//! RFC-0115 D4 "Breadth" (ruling 2026-11-03): a board with an emulator twin
//! earns its registry status from a Runtime cell on its OWN platform, not from
//! borrowing the QEMU witness and not from the weekly probe track. This is that
//! cell, and it is a smoke cell (D4: boots, link up, one message), nothing
//! more: the C talker's serial image (`[image.nucleo_serial]` of
//! `examples/zephyr/c/talker`, the image the `renode-nucleo` probe track
//! builds) boots in Renode's `nucleo_f767zi` platform, its zenoh link on
//! USART3 reaches a host router through a pty, and a host listener hears it.
//!
//! The boot goes through `nros_tests::renode` (which drives Renode with the
//! probe track's own `scripts/probe/twin/renode.sh`), so this cell and the
//! weekly track boot the twin the same way.
//!
//! ## Prerequisites
//!
//! - `just zephyr build-fixtures` (the west leaves lane builds
//!   `build-nucleo-f767zi-c-talker-serial`; the board's `hal_stm32` module
//!   comes from `west.yml`)
//! - Renode 1.17.0: `nros setup --tool renode` (or `$NROS_RENODE`)
//! - ROS's `rmw_zenohd` (nano-ros ships none; RFC-0075)
//!
//! Run with: `cargo nextest run -p nros-tests --test zephyr_renode_twin`

use nros_tests::{
    alloc::{aux_port, port_of},
    fixtures::{
        ManagedProcess, RequireFixture, ZenohRouter, build_native_listener,
        build_zephyr_nucleo_c_serial_talker, is_zenohd_available,
    },
    matrix::{Lang, PlatformId, Workload},
    output::{LISTENER_LOG_PREFIX, LISTENER_READY_MARKER, TALKER_LOG_PREFIX},
    renode::{RenodeTwin, require_renode},
};
use std::{process::Command, time::Duration};

/// The twin probe track whose board facts (machine, link UART, console UART)
/// this cell boots with: `[[probe]] track = "renode-nucleo"` in
/// `packages/boards/board-support.toml`, whose `twin` is this board.
const TWIN_TRACK: &str = "renode-nucleo";

/// From `start` to the listener's first sample. Renode emulates the M7 at a
/// fraction of real time and the serial link opens its session at 115200
/// baud; the probe track allows 120 s for the same run.
const HEAR_BUDGET: Duration = Duration::from_secs(120);

#[test]
fn zephyr_nucleo_renode_c_zenoh_serial_pubsub_e2e() {
    // Issue 1900 -- the lane first, before any host probe.
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::ZephyrNucleoF767zi],
        "the Nucleo-F767ZI Renode smoke cell",
    );
    let renode = require_renode();
    if !is_zenohd_available() {
        nros_tests::unmet!("zenohd not found -- run `just setup` / source ./activate.sh");
    }

    let elf = build_zephyr_nucleo_c_serial_talker().unwrap_or_else(|e| {
        panic!(
            "zephyr/c/talker for nucleo_f767zi (image nucleo_serial) did not resolve. \
             The lane gate already asserted this fixture is built, so this is a real \
             failure -- read the verdict before rebuilding: {e:?}"
        )
    });
    let listener = build_native_listener()
        .map(|p| p.to_path_buf())
        .require("the native Rust listener (the host peer)");

    let port = port_of(PlatformId::ZephyrNucleoF767zi, Lang::C, Workload::Pubsub);
    let monitor = aux_port(PlatformId::ZephyrNucleoF767zi, 0);
    let run = std::env::temp_dir().join(format!("nros-renode-twin-{}", std::process::id()));

    // Paused: the router opens the link pty before the image runs.
    let mut twin = RenodeTwin::up(&renode, TWIN_TRACK, &elf, &run, monitor)
        .unwrap_or_else(|e| panic!("Renode did not come up for {}: {e}", elf.display()));
    let _router = ZenohRouter::start_with_serial(port, &twin.link()).unwrap_or_else(|e| {
        panic!(
            "failed to start zenohd on tcp {port} + serial {}: {e:?}",
            twin.link().display()
        )
    });

    // Listener first, so its subscription is live before the image publishes.
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

    twin.start()
        .unwrap_or_else(|e| panic!("Renode did not start the simulation: {e}"));

    let heard = obs.wait_for_output_count(LISTENER_LOG_PREFIX, 1, HEAR_BUDGET);
    let console = twin.wait_console(TALKER_LOG_PREFIX, 1, Duration::from_secs(5));
    obs.kill();
    twin.down();
    eprintln!("Nucleo-F767ZI (Renode) console:\n{console}");

    let heard = heard.unwrap_or_else(|_| {
        panic!(
            "the host listener heard nothing from the Nucleo-F767ZI twin over the serial \
             link in {HEAR_BUDGET:?}.\nConsole:\n{console}\nRenode log:\n{}",
            twin.renode_log()
        )
    });
    // Boots: the image's own console says it published.
    nros_tests::output::assert_talker(&console, 1);
    // Link up, one message: the host listener received it through the router.
    assert!(
        nros_tests::count_pattern(&heard, LISTENER_LOG_PREFIX) >= 1,
        "listener output holds no sample:\n{heard}"
    );
}

/// RFC-0051 section 7c -- the cases this file runs are exactly the Runtime cells
/// `matrix::consumer_of` assigns to it. Fixture-free.
#[test]
fn cases_bound_to_matrix_cells() {
    use nros_tests::matrix::{
        Consumer, Kind as K, Lang as L, PlatformId as P, Rmw as R, Workload as W,
        assert_consumer_cases,
    };
    assert_consumer_cases(
        Consumer::ZephyrRenodeTwin,
        &[(P::ZephyrNucleoF767zi, L::C, R::Zenoh, W::Pubsub, K::Example)],
    );
}
