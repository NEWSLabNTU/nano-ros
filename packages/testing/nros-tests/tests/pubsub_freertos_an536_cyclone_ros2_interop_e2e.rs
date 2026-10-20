//! phase-441 W6 — a CycloneDDS RTOS image meets a stock ROS 2 peer.
//!
//! Every other board cell in `interop::CELLS` is zenoh, which crosses QEMU's
//! user-mode network by dialling a TCP router. Cyclone discovers by SPDP, and
//! slirp carries no multicast, so this cell is phase-441 W2's measured recipe
//! on a real image instead of on two host processes:
//!
//! | half | what it carries |
//! | --- | --- |
//! | guest | `examples/workspaces/cpp/src/slirp_bringup/rmw/cyclonedds.xml`, baked into the mps3-an536 FreeRTOS C++ talker: unicast only, advertises `127.0.0.1`, fixed participant index, peers with the host's meta port through `10.0.2.2`, a `<Discovery><Tag>` |
//! | QEMU | `-nic user,model=lan9118,hostfwd=…` for the guest's two unicast ports |
//! | host | `rmw_cyclonedds_cpp` + `ros2 topic echo`, on `CycloneSlirpPair::host_xml` (issue 1251) |
//!
//! So the crossing puts lwIP over the emulated LAN9118, a 32-bit Cortex-R52,
//! FreeRTOS and the embedded Cyclone build all in the path, which the
//! `cyclone_slirp_pair` test (two host processes, Linux's stack) does not.
//!
//! ## Why it is held for a minute
//!
//! Issue 0997: on the island's FreeRTOS + Cyclone image the participant
//! announced SPDP once, its timed-event tree emptied, and every peer expired
//! its lease about 10 s later. A cell that stops at the first sample passes on
//! exactly that image. This one keeps the echo running for [`HOLD`] and
//! requires samples still arriving at least [`LATE_SAMPLE_GAP`] after the
//! first: the talker publishes its counter at 1 Hz, so the counter values
//! received say how long delivery lasted without any clock on either side.
//!
//! ## The fragmented leg (issue 0917)
//!
//! [`nros_freertos_an536_cyclone_listener_keeps_receiving_a_fragmented_sample`]
//! turns the crossing around: the same bringup's second image is a LISTENER,
//! and the host publishes a `std_msgs/String` of [`BULK_LEN`] bytes, which
//! Cyclone sends as eight `DATA_FRAG`s — the island trajectory's shape. Issue
//! 0917 found exactly this failing on RECEIVE: the burst lands in the emulated
//! LAN9118's 10.5 KB RX FIFO faster than a 5 ms polled drain empties it, and a
//! reliable reader that loses a fragment's tail stalls the stream. The image
//! prints each sample's sequence number and length, so the test asks the same
//! question the talker cell does — did delivery LAST — and also that every
//! sample arrived whole. Both legs share the baked domain and forwarded ports,
//! so `.config/nextest.toml` runs them one at a time.
//!
//! ## Constants that are baked
//!
//! The guest's config is in the image, so the domain and the tag are build
//! constants on both sides. The domain is the allocator's
//! (`alloc::domain_of`), the same rule every baked Cyclone cell takes;
//! `the_bringup_bakes_the_pairs_guest_half` holds the committed XML to
//! `CycloneSlirpPair::guest_xml` so the two cannot drift.
//!
//! ## Preconditions, all of which FAIL when absent
//!
//! ROS 2 + `rmw_cyclonedds_cpp`, the FreeRTOS kernel + lwIP trees,
//! `arm-none-eabi-gcc`, `qemu-system-arm` with the `mps3-an536` machine, a
//! default IPv4 route (the host half selects that one interface) and the
//! `workspace-cpp-mps3-an536-freertos-slirp` fixture (`just build freertos`).
//!
//! Run with `just freertos test-ros2-cyclone`.

use nros_tests::{
    alloc::domain_of,
    dds_isolation::{CycloneSlirpPair, default_route_interface},
    fixtures::{
        QemuProcess, RequireFixture, build_freertos_workspace_cpp_an536_slirp_bulk_entry,
        build_freertos_workspace_cpp_an536_slirp_entry, freertos, is_qemu_available,
    },
    matrix::{Lang, PlatformId, Workload},
    process::ManagedProcess,
    ros2::{DEFAULT_ROS_DISTRO, require_ros2_cyclonedds, ros2_env_setup_cyclonedds_with_profile},
    unmet,
};
use std::{process::Command, time::Duration};

/// The image's baked domain — the allocator's answer for this coordinate.
const DOMAIN: u8 = domain_of(PlatformId::FreertosMps2, Lang::Cpp, Workload::EntryPubsub);

/// The `<Discovery><Tag>` baked into the guest.
const TAG: &str = "nros-441-w6-an536";

/// The guest's address on the board's QEMU network plan.
const GUEST_IP: &str = "10.0.2.15";

/// How long the echo runs. A cold boot, lwIP bring-up and SPDP take a few
/// seconds; the rest is the lease window issue 0997 died in, several times
/// over.
const HOLD: Duration = Duration::from_secs(75);

/// Samples must still be arriving this long after the first one — four times
/// Cyclone's default 10 s lease, so a participant that stopped announcing
/// would have been expired by every peer well before.
const LATE_SAMPLE_GAP: i64 = 45;

fn pair() -> CycloneSlirpPair {
    CycloneSlirpPair::new(u32::from(DOMAIN), TAG)
}

/// The counter values a `ros2 topic echo` of `std_msgs/msg/Int32` printed.
fn received_values(echo: &str) -> Vec<i64> {
    echo.lines()
        .filter_map(|l| l.trim().strip_prefix("data:"))
        .filter_map(|v| v.trim().parse().ok())
        .collect()
}

#[test]
fn nros_freertos_an536_cyclone_publisher_stays_reachable_from_ros2() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::FreertosMps2],
        "the FreeRTOS mps3-an536 Cyclone ROS 2 interop cell",
    );
    require_ros2_cyclonedds();
    if !freertos::is_freertos_available() {
        unmet!(
            "no FreeRTOS kernel tree provisioned — run `just freertos setup`, or set FREERTOS_DIR"
        );
    }
    if !freertos::is_lwip_available() {
        unmet!("no lwIP tree provisioned — run `just freertos setup`, or set LWIP_DIR");
    }
    if !freertos::is_arm_gcc_available() {
        unmet!("arm-none-eabi-gcc not found — `nros setup --tool arm-none-eabi-gcc`");
    }
    if !is_qemu_available() {
        unmet!("qemu-system-arm not found");
    }
    let iface = default_route_interface()
        .unwrap_or_else(|| unmet!("no default IPv4 route: the host half needs one interface"));

    let entry =
        build_freertos_workspace_cpp_an536_slirp_entry().require("mps3-an536 slirp Cyclone entry");
    let pair = pair();

    let mut guest = QemuProcess::start_mps3_an536_slirp(entry, &pair.qemu_hostfwd(GUEST_IP))
        .unwrap_or_else(|e| panic!("boot freertos QEMU (mps3-an536, slirp): {e}"));

    let mut echo_cmd = Command::new("bash");
    echo_cmd.args(["-c"]).arg(format!(
        "{} && exec ros2 topic echo --no-daemon /chatter std_msgs/msg/Int32 < /dev/null",
        ros2_env_setup_cyclonedds_with_profile(
            DEFAULT_ROS_DISTRO,
            pair.domain,
            &pair.host_config_uri(&iface)
        )
    ));
    let mut echo =
        ManagedProcess::spawn_command(echo_cmd, "an536-cyclone-echo").expect("spawn ros2 echo");

    // A sentinel neither side prints, so the call runs the whole window and
    // returns everything (issue 0670: never default away a wait's error).
    const SENTINEL: &str = "\u{1}nros-hold-sentinel";
    let out = echo.collect_until(SENTINEL, HOLD);
    echo.kill();
    let console = guest.collect_until(SENTINEL, Duration::from_secs(2));
    guest.kill();

    let values = received_values(&out);
    let (first, last) = match (values.first(), values.iter().max()) {
        (Some(f), Some(l)) => (*f, *l),
        _ => panic!(
            "`ros2 topic echo` (rmw_cyclonedds_cpp, domain {DOMAIN}, tag {TAG}) received \
             nothing from the mps3-an536 FreeRTOS Cyclone talker in {}s over slirp \
             (phase-441 W6).\nhost iface: {iface}; hostfwd: {}\n\
             echo output:\n{out}\n--- guest console ---\n{console}",
            HOLD.as_secs(),
            pair.qemu_hostfwd(GUEST_IP)
        ),
    };
    assert!(
        last - first >= LATE_SAMPLE_GAP,
        "the first sample arrived (counter {first}) but delivery did not last: the \
         highest counter received was {last}, {} s of a 1 Hz talker, where at least \
         {LATE_SAMPLE_GAP} s were required — the shape of issue 0997 (one SPDP \
         announcement, then every peer expires the participant's lease).\n\
         {} samples: {values:?}\n--- guest console ---\n{console}",
        last - first,
        values.len()
    );
}

/// The fragmented sample's `data` length: with Cyclone's 1344-byte fragment
/// payload, a serialized `std_msgs/String` of this size is EIGHT fragments
/// (7 x 1344 = 9408 < 10 584 <= 10 752), the 120-point trajectory issue 0917
/// was measured with. Within `examples/workspaces/cpp/nros-codegen.toml`'s
/// 16384-byte bound.
const BULK_LEN: usize = 10_576;

/// The host publisher's rate, the trajectory's.
const BULK_HZ: u32 = 10;

/// The host half of the fragmented leg: a stock rclpy node publishing
/// `<seq>:` padded to [`BULK_LEN`] at [`BULK_HZ`], reliable, until killed.
fn bulk_publisher_script() -> String {
    format!(
        "import rclpy, time\n\
         from rclpy.node import Node\n\
         from std_msgs.msg import String\n\
         rclpy.init()\n\
         n = Node('nros_441_w6_bulk_pub')\n\
         p = n.create_publisher(String, '/bulk', 10)\n\
         i = 0\n\
         while rclpy.ok():\n\
         \x20   head = '%d:' % i\n\
         \x20   p.publish(String(data=head + 'x' * ({BULK_LEN} - len(head))))\n\
         \x20   print('published', i, flush=True)\n\
         \x20   i += 1\n\
         \x20   time.sleep({period})\n",
        period = 1.0 / f64::from(BULK_HZ)
    )
}

/// `(seq, len)` for each `Received bulk:` line the guest printed.
fn received_bulk(console: &str) -> Vec<(i64, usize)> {
    console
        .lines()
        .filter_map(|l| l.split_once("Received bulk: seq=").map(|(_, r)| r))
        .filter_map(|r| {
            let (seq, rest) = r.split_once(" len=")?;
            let len = rest.split_whitespace().next()?;
            Some((seq.trim().parse().ok()?, len.parse().ok()?))
        })
        .collect()
}

#[test]
fn nros_freertos_an536_cyclone_listener_keeps_receiving_a_fragmented_sample() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::FreertosMps2],
        "the FreeRTOS mps3-an536 Cyclone ROS 2 interop cell",
    );
    require_ros2_cyclonedds();
    if !freertos::is_freertos_available() {
        unmet!(
            "no FreeRTOS kernel tree provisioned — run `just freertos setup`, or set FREERTOS_DIR"
        );
    }
    if !freertos::is_lwip_available() {
        unmet!("no lwIP tree provisioned — run `just freertos setup`, or set LWIP_DIR");
    }
    if !freertos::is_arm_gcc_available() {
        unmet!("arm-none-eabi-gcc not found — `nros setup --tool arm-none-eabi-gcc`");
    }
    if !is_qemu_available() {
        unmet!("qemu-system-arm not found");
    }
    let iface = default_route_interface()
        .unwrap_or_else(|| unmet!("no default IPv4 route: the host half needs one interface"));

    let entry = build_freertos_workspace_cpp_an536_slirp_bulk_entry()
        .require("mps3-an536 slirp Cyclone bulk-listener entry");
    let pair = pair();

    let mut guest = QemuProcess::start_mps3_an536_slirp(entry, &pair.qemu_hostfwd(GUEST_IP))
        .unwrap_or_else(|e| panic!("boot freertos QEMU (mps3-an536, slirp): {e}"));

    let mut pub_cmd = Command::new("bash");
    pub_cmd.args(["-c"]).arg(format!(
        "{} && exec python3 -c \"$NROS_BULK_PUB\" < /dev/null",
        ros2_env_setup_cyclonedds_with_profile(
            DEFAULT_ROS_DISTRO,
            pair.domain,
            &pair.host_config_uri(&iface)
        )
    ));
    pub_cmd.env("NROS_BULK_PUB", bulk_publisher_script());
    let mut publisher = ManagedProcess::spawn_command(pub_cmd, "an536-cyclone-bulk-pub")
        .expect("spawn rclpy publisher");

    const SENTINEL: &str = "\u{1}nros-hold-sentinel";
    let console = guest.collect_until(SENTINEL, HOLD);
    guest.kill();
    let published = publisher.collect_until(SENTINEL, Duration::from_millis(500));
    publisher.kill();
    let sent = published
        .lines()
        .filter(|l| l.starts_with("published"))
        .count();

    let got = received_bulk(&console);
    let short: Vec<_> = got.iter().filter(|(_, len)| *len != BULK_LEN).collect();
    assert!(
        short.is_empty(),
        "{} of {} fragmented samples reached the callback with the wrong length \
         (expected {BULK_LEN}): {short:?}\n--- guest console ---\n{console}",
        short.len(),
        got.len()
    );
    let (first, last) = match (got.first(), got.iter().map(|(s, _)| *s).max()) {
        (Some((f, _)), Some(l)) => (*f, l),
        _ => panic!(
            "the mps3-an536 FreeRTOS Cyclone listener received NO {BULK_LEN}-byte \
             (8-fragment) sample in {}s over slirp, from a host rclpy publisher that \
             sent {sent} (domain {DOMAIN}, tag {TAG}) — issue 0917's shape \
             (phase-441 W6, fragmented leg).\nhost iface: {iface}; hostfwd: {}\n\
             --- guest console ---\n{console}\n--- publisher ---\n{published}",
            HOLD.as_secs(),
            pair.qemu_hostfwd(GUEST_IP)
        ),
    };
    let late = LATE_SAMPLE_GAP * i64::from(BULK_HZ);
    assert!(
        last - first >= late,
        "fragmented delivery did not last: the first sample was seq {first} and the \
         highest {last}, {:.1} s of a {BULK_HZ} Hz publisher, where at least \
         {LATE_SAMPLE_GAP} s were required — the stall issue 0917 describes (a \
         reliable reader that lost a fragment's tail never recovers).\n\
         {} received of {sent} sent\n--- guest console ---\n{console}",
        (last - first) as f64 / f64::from(BULK_HZ),
        got.len()
    );
    eprintln!(
        "fragmented leg: {} of {sent} samples received, seq {first}..={last} \
         ({:.1} s of delivery)",
        got.len(),
        (last - first) as f64 / f64::from(BULK_HZ)
    );
}

/// The bringup's baked guest config is the pair's guest half, so the image
/// and the test agree on domain, tag, participant index and peer port. No
/// fixture: reads the committed file.
#[test]
fn the_bringup_bakes_the_pairs_guest_half() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../examples/workspaces/cpp/src/slirp_bringup/rmw/cyclonedds.xml"
    );
    let committed = std::fs::read_to_string(path).expect("read the bringup's cyclonedds.xml");
    // Compare the document, not the comment above it.
    let body = |xml: &str| -> String {
        let start = xml.find("<CycloneDDS").expect("a <CycloneDDS> element");
        xml[start..]
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(
        body(&committed),
        body(&pair().guest_xml()),
        "examples/workspaces/cpp/src/slirp_bringup/rmw/cyclonedds.xml is not \
         CycloneSlirpPair::new({DOMAIN}, {TAG:?}).guest_xml(); regenerate it, or change \
         the constants here, so the image and this test agree"
    );
}

// Issue 0352 / phase-324 — bind this test to `interop::CELLS`.
#[test]
fn cases_bound_to_interop_cells() {
    #[allow(unused_imports)]
    use nros_tests::matrix::{Lang::*, PlatformId::*, Rmw::*, Workload::*};
    nros_tests::interop::assert_test_bound(
        "pubsub_freertos_an536_cyclone_ros2_interop_e2e",
        &[(FreertosMps2, Cpp, Cyclonedds, EntryPubsub)],
    );
}
