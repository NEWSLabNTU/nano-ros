//! phase-263 C2c — runtime E2E for the Zephyr (native_sim) C++ workspace EMBEDDED entry:
//! `nano_ros_entry(BOARD zephyr LAUNCH …)` in the C++ workspace. The C++ sibling of the C2d
//! C zephyr entry.
//!
//! The cpp nodes are TYPED (`std_msgs::msg::Int32`), so unlike the raw-pub C nodes they pull
//! the full nros-cpp header surface + the generated std_msgs C++ interfaces. The entry-as-app
//! wiring (find_package(Zephyr) + nano_ros_entry, the C/C++ analog of rust_cargo_application)
//! is reused from C2d; the cpp path additionally needed: an idempotent interface generator
//! (talker + listener both generate std_msgs), the component class-header include propagated
//! to `app`, a hard sizes-header ordering edge on the component libs, `::setvbuf` (not
//! `std::setvbuf` — absent on Zephyr picolibc), and the node-pkg interface link guarded with
//! `if(TARGET)` (the generated interface is whole-archived into `app` on Zephyr).
//!
//! Delivery is observed CROSS-PROCESS (issue 0096): the native_sim `zephyr.exe` runs the
//! `demo_bringup` talker, and a SEPARATE native C listener (`native_entry_robot2`, a
//! language-agnostic `/chatter` subscriber on the wire) receives it through a zenoh router.
//! The router uses the exact port baked into the fixture (17833).
//!
//! The fixture is built by the west lane; this test skips cleanly when `zephyr.exe` is absent.
//!
//! Run with: `cargo nextest run -p nros-tests --test cpp_zephyr_entry_e2e`

use nros_tests::fixtures::{
    ManagedProcess, ZenohRouter, ZephyrPlatform, ZephyrProcess,
    build_native_workspace_c_entry_robot2, build_zephyr_workspace_cpp_entry, require_zenohd,
};
use std::{process::Command, time::Duration};

/// The router port baked into the C++ zephyr workspace entry (the west lane's
/// `-DCONFIG_NROS_ZENOH_LOCATOR="tcp/127.0.0.1:17833"`).
const ZEPHYR_ENTRY_PORT: u16 = 17833;

#[test]
fn cpp_zephyr_workspace_entry_delivers_cross_process() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }

    let entry = build_zephyr_workspace_cpp_entry()
        .unwrap_or_else(|e| nros_tests::skip!("C++ zephyr workspace entry not built (west): {e}"));
    let observer = build_native_workspace_c_entry_robot2()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native listener entry fixture not built: {e}"));

    let router = ZenohRouter::start_on("127.0.0.1", ZEPHYR_ENTRY_PORT).unwrap_or_else(|e| {
        nros_tests::skip!("zenohd failed to start on {ZEPHYR_ENTRY_PORT}: {e}")
    });
    let observer_locator = router.locator();

    let mut obs = {
        let mut cmd = Command::new(&observer);
        cmd.env("NROS_LOCATOR", &observer_locator)
            .env("NROS_SESSION_MODE", "client")
            .env("NROS_ENTRY_SPIN_MS", "60000");
        ManagedProcess::spawn_command(cmd, "native-observer")
            .unwrap_or_else(|e| panic!("spawn observer: {e}"))
    };
    obs.wait_for_output_pattern("Waiting for messages", Duration::from_secs(10))
        .unwrap_or_else(|_| {
            obs.kill();
            panic!("native observer listener never became ready")
        });

    let mut zephyr = ZephyrProcess::start(&entry, ZephyrPlatform::NativeSim)
        .unwrap_or_else(|e| panic!("boot zephyr native_sim: {e}"));

    let out = obs
        .wait_for_output_count(
            nros_tests::output::LISTENER_LOG_PREFIX,
            3,
            Duration::from_secs(90),
        )
        .unwrap_or_else(|_| {
            zephyr.kill();
            obs.kill();
            panic!(
                "native observer never received the C++ Zephyr native_sim entry's /chatter — \
                 the embedded C++ Zephyr LAUNCH-entry runtime delivery did not work"
            )
        });

    zephyr.kill();
    obs.kill();

    let n = nros_tests::count_pattern(&out, nros_tests::output::LISTENER_LOG_PREFIX);
    assert!(n >= 3, "expected ≥3 cross-process deliveries, got {n}");
}
