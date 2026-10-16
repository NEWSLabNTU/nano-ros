//! RFC-0103 D10a (phase-497 W3) — two emulated images on ONE shared network.
//!
//! `examples/workspaces/rust`'s multihost pair, built as `freertos_br_robot1`
//! (talker) and `freertos_br_robot2` (listener), sits on `[network.br]`: the
//! host bridge `qemu-br` (`scripts/qemu/setup-network.sh` makes it,
//! 192.0.3.1/24), each guest at its own static address, both dialing the
//! router on the bridge. The launcher takes each guest's `-nic` from the
//! image's resolved network (`build/nros/net/<image>.toml`) through
//! `nros_board_common::image_net::qemu_nic` — the function `nros ws qemu-nic`
//! uses — so nothing here spells `bridge,br=…`.
//!
//! A host bridge needs root to provision, so no lane claims one
//! (`lane_scope::Capability::HostBridge`): in every lane this test is a
//! deselection that reads NOT VERIFIED. A run that DOES claim it (a person on
//! a bridged host, `NROS_TEST_UNCLAIMED` unset) reaches the probes below and
//! fails when the bridge is absent.

use std::time::Duration;

use nros_tests::{
    count_pattern,
    fixtures::{ZenohRouter, freertos_bridge_pair_image, or_unmet},
    lane_scope,
    matrix::PlatformId,
    qemu::{QemuProcess, image_nic, is_qemu_available},
};

/// The host bridge `[network.br]` attaches to, and its address (the router
/// listens there).
const BRIDGE: &str = "qemu-br";
const BRIDGE_IP: &str = "192.0.3.1";

#[test]
fn freertos_bridge_pair_delivers_over_the_host_bridge() {
    lane_scope::require_host_bridge_claimed();
    if !lane_scope::admits(PlatformId::FreertosMps2) {
        nros_tests::lane_skip!(
            "{}",
            lane_scope::skip_note(PlatformId::FreertosMps2, "rust")
        );
    }
    if !std::path::Path::new(&format!("/sys/class/net/{BRIDGE}")).exists() {
        nros_tests::unmet!(
            "host bridge `{BRIDGE}` is absent — create it with `sudo scripts/qemu/setup-network.sh`, \
             and allow it in /etc/qemu/bridge.conf (`allow {BRIDGE}`)"
        );
    }
    if !is_qemu_available() {
        nros_tests::unmet!("qemu-system-arm not found");
    }

    let (talker_bin, talker_net) = freertos_bridge_pair_image("robot1")
        .unwrap_or_else(|e| nros_tests::unmet!("robot1 fixture: {e}"));
    let (listener_bin, listener_net) = freertos_bridge_pair_image("robot2")
        .unwrap_or_else(|e| nros_tests::unmet!("robot2 fixture: {e}"));
    let talker_nic = image_nic(&talker_net, "lan9118").expect("robot1's -nic");
    let listener_nic = image_nic(&listener_net, "lan9118").expect("robot2's -nic");
    assert!(
        talker_nic.starts_with(&format!("bridge,br={BRIDGE},"))
            && listener_nic.starts_with(&format!("bridge,br={BRIDGE},")),
        "both images must render the bridge attach: {talker_nic} / {listener_nic}"
    );
    assert_ne!(talker_nic, listener_nic, "each image has its own MAC");

    // The router the images bake (the network's `router`, D10d), on the bridge.
    let locator = nros_tests::fixtures::baked_locator::baked_locator_in(
        &nros_tests::project_root(),
        "workspace-rust-freertos-br-robot1",
    )
    .expect("robot1 bakes a locator");
    let port = nros_tests::fixtures::baked_locator::locator_port(&locator)
        .unwrap_or_else(|| panic!("`{locator}` carries no port"));
    let _zenohd = or_unmet(ZenohRouter::start_on(BRIDGE_IP, port));

    let mut listener =
        QemuProcess::start_mps2_an385_on(&listener_bin, &listener_nic).expect("start robot2");
    std::thread::sleep(Duration::from_secs(5));
    let mut talker =
        QemuProcess::start_mps2_an385_on(&talker_bin, &talker_nic).expect("start robot1");

    let listener_output = listener.collect_until(
        nros_tests::output::LISTENER_LOG_PREFIX,
        Duration::from_secs(60),
    );
    let talker_output = talker.collect_until(
        nros_tests::output::TALKER_LOG_PREFIX,
        Duration::from_secs(30),
    );
    talker.kill();
    listener.kill();
    eprintln!("robot2 (listener):\n{listener_output}");
    eprintln!("robot1 (talker):\n{talker_output}");

    assert!(
        count_pattern(&talker_output, nros_tests::output::TALKER_LOG_PREFIX) > 0,
        "robot1 published nothing over the bridge"
    );
    assert!(
        count_pattern(&listener_output, nros_tests::output::LISTENER_LOG_PREFIX) > 0,
        "robot2 received nothing over the bridge"
    );
}
