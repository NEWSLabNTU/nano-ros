//! Issue 1251 — the Cyclone profile for a pair whose nano side is behind QEMU
//! user-mode networking, measured through libslirp itself.
//!
//! `dds_isolation`'s loopback profile cannot be the host half of such a pair
//! (the guest's `127.0.0.1` is its own, and Cyclone refuses to rewrite a
//! loopback-pinned host's advertised address), so `CycloneSlirpPair` isolates
//! by `<Discovery><Tag>` instead. This test runs the real thing, minus the
//! RTOS:
//!
//! * the "guest" is our Cyclone backend's `ros2_pub` (publishes
//!   `hello-from-nros` on `/chatter`) inside a user + network namespace whose
//!   only way out is `slirp4netns` — the same libslirp QEMU's `-netdev user`
//!   uses, with the same `10.0.2.2` alias for the host's loopback and the
//!   same inbound-only-through-`hostfwd` rule;
//! * the host peer is a stock `ros2 topic echo` on `rmw_cyclonedds_cpp`.
//!
//! Three cases, one positive and two negative controls:
//!
//! | host profile | expected |
//! | --- | --- |
//! | `CycloneSlirpPair::host_xml`, same tag | samples delivered |
//! | same, DIFFERENT tag | nothing — the isolation |
//! | the issue-1009 loopback profile | nothing — the collision 1251 names |
//!
//! Not modelled: an RTOS IP stack and the embedded Cyclone build (phase-441 W2
//! lists both). The guest here runs Linux's stack and the hosted backend.
//!
//! Preconditions SKIP by name: ROS 2 with `rmw_cyclonedds_cpp`, `unshare` and
//! `slirp4netns` on PATH, unprivileged user namespaces, the Cyclone backend's
//! test tree built (`just cyclonedds build-rmw`), and a default IPv4 route.

use nros_tests::{
    dds_isolation::{CycloneSlirpPair, cyclone_config_uri, default_route_interface},
    fixtures::{RequireFixture, require_prebuilt_artifact},
    process::ManagedProcess,
    ros2::{DEFAULT_ROS_DISTRO, require_ros2, ros2_env_setup_cyclonedds_with_profile},
    skip,
};
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

const PAYLOAD: &str = "hello-from-nros";

fn on_path(tool: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {tool}")])
        .output()
        .is_ok_and(|o| o.status.success())
}

/// The Cyclone backend's own `ros2_pub` test binary, through the lane-aware
/// fixture funnel (an absent one fails in a lane that promised it, issue 0584).
fn guest_binary() -> PathBuf {
    let path = nros_tests::project_root().join(
        "packages/rmw/cyclonedds/nros-rmw-cyclonedds/build/tests/nros_rmw_cyclonedds_ros2_pub",
    );
    require_prebuilt_artifact(&path, "just cyclonedds build-rmw")
        .require("the Cyclone backend's ros2_pub (issue 1251's slirp guest)")
}

fn preconditions() -> String {
    // Host tools and kernel features only; the guest binary is a build-stage
    // artifact and goes through `guest_binary`'s fixture funnel instead.
    if !require_ros2() {
        skip!("ROS 2 is not installed");
    }
    for tool in ["unshare", "slirp4netns"] {
        if !on_path(tool) {
            skip!("`{tool}` is not on PATH (issue 1251's libslirp guest needs it)");
        }
    }
    let userns = Command::new("unshare")
        .args(["--user", "--map-root-user", "--net", "true"])
        .status()
        .is_ok_and(|s| s.success());
    if !userns {
        skip!("unprivileged user + network namespaces are not available on this host");
    }
    default_route_interface()
        .unwrap_or_else(|| skip!("no default IPv4 route: the host half needs one interface"))
}

/// The pid of `unshare --fork`'s child, which is the process inside the new
/// namespaces that `slirp4netns` attaches to.
fn namespace_child(unshare_pid: u32) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(5);
    let path = format!("/proc/{unshare_pid}/task/{unshare_pid}/children");
    while Instant::now() < deadline {
        if let Some(pid) = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| s.split_whitespace().next().and_then(|p| p.parse().ok()))
        {
            return pid;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("unshare (pid {unshare_pid}) never forked its namespace child");
}

fn add_hostfwd(api: &Path, port: u32) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut sock = loop {
        match UnixStream::connect(api) {
            Ok(s) => break s,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => panic!(
                "slirp4netns API socket {} never came up: {e}",
                api.display()
            ),
        }
    };
    let req = format!(
        r#"{{"execute":"add_hostfwd","arguments":{{"proto":"udp","host_addr":"0.0.0.0","host_port":{port},"guest_port":{port}}}}}"#
    );
    sock.write_all(req.as_bytes())
        .expect("write hostfwd request");
    let mut reply = String::new();
    let _ = sock.read_to_string(&mut reply);
    assert!(
        reply.contains("\"return\""),
        "slirp4netns refused hostfwd for udp/{port}: {reply}"
    );
}

/// Run one pair: the guest behind slirp publishing, the host echoing under
/// `host_uri`. Returns how many `PAYLOAD` samples the host received before
/// the first arrived or the window closed (so `> 0` is the verdict, not a rate).
fn run_pair(guest: &Path, pair: &CycloneSlirpPair, host_uri: &str, label: &str) -> usize {
    let work = tempfile::tempdir().expect("tempdir");
    let go = work.path().join("go");
    let api = work.path().join("slirp.sock");

    let mut guest_cmd = Command::new("unshare");
    guest_cmd
        .args(["--user", "--map-root-user", "--net", "--fork", "sh", "-c"])
        .arg(format!(
            "while [ ! -e '{go}' ]; do sleep 0.1; done; ip link set lo up; exec '{bin}'",
            go = go.display(),
            bin = guest.display()
        ))
        .env("CYCLONEDDS_URI", pair.guest_config_uri())
        .env("ROS_DOMAIN_ID", pair.domain.to_string());
    let mut guest_proc = ManagedProcess::spawn_command(guest_cmd, format!("{label}-guest"))
        .expect("spawn the namespaced guest");
    let ns_pid = namespace_child(guest_proc.handle_mut().id());

    let mut slirp_cmd = Command::new("slirp4netns");
    slirp_cmd
        .args(["--configure", "--mtu=1500", "--api-socket"])
        .arg(&api)
        .arg(ns_pid.to_string())
        .arg("tap0");
    let mut slirp = ManagedProcess::spawn_command(slirp_cmd, format!("{label}-slirp"))
        .expect("spawn slirp4netns");
    let (meta, data) = pair.guest_ports();
    add_hostfwd(&api, meta);
    add_hostfwd(&api, data);

    // The host peer. `CYCLONEDDS_URI` is THIS profile, not the loopback one
    // `env_exports_for_rmw` would export: choosing it is the subject here.
    let mut echo_cmd = Command::new("bash");
    echo_cmd.args(["-c"]).arg(format!(
        "{} && exec ros2 topic echo --no-daemon /chatter std_msgs/msg/String < /dev/null",
        ros2_env_setup_cyclonedds_with_profile(DEFAULT_ROS_DISTRO, pair.domain, host_uri)
    ));
    let mut echo =
        ManagedProcess::spawn_command(echo_cmd, format!("{label}-echo")).expect("spawn ros2 echo");
    std::thread::sleep(Duration::from_secs(3));
    std::fs::write(&go, b"").expect("release the guest");

    // The guest waits 2 s for discovery, then publishes 50 samples over ~5 s.
    // Returns at the FIRST sample; a negative case spends the whole window.
    // `collect_until` returns only what the process printed, never a
    // diagnostic naming the pattern (issue 0670).
    let out = echo.collect_until(PAYLOAD, Duration::from_secs(12));
    echo.kill();
    slirp.kill();
    guest_proc.kill();
    out.matches(PAYLOAD).count()
}

#[test]
fn a_tag_isolated_host_profile_reaches_a_slirp_guest_and_nothing_else() {
    let iface = preconditions();
    let guest = guest_binary();
    let domain = u32::from(nros_tests::unique_ros_domain_id());
    let tag = format!("nros-1251-{}", std::process::id());
    let pair = CycloneSlirpPair::new(domain, tag.clone());

    let delivered = run_pair(&guest, &pair, &pair.host_config_uri(&iface), "matched");
    assert!(
        delivered > 0,
        "the host half of CycloneSlirpPair received nothing from a guest behind slirp \
         (domain {domain}, iface {iface}): the profile does not cross the NAT"
    );

    // Negative control 1 — the ISOLATION: a host participant whose tag differs
    // is a foreign participant, and must see nothing.
    let foreign = CycloneSlirpPair::new(domain, format!("{tag}-foreign"));
    let leaked = run_pair(&guest, &pair, &foreign.host_config_uri(&iface), "foreign");
    assert_eq!(
        leaked, 0,
        "a host participant with a DIFFERENT tag received the guest's samples: the tag \
         does not isolate"
    );

    // Negative control 2 — the COLLISION issue 1251 names: the loopback-pinned
    // host half of issue 1009 cannot be reached from the guest.
    let pinned = run_pair(&guest, &pair, &cyclone_config_uri(), "loopback-pinned");
    assert_eq!(
        pinned, 0,
        "the loopback-pinned host profile received samples from a slirp guest, so issue \
         1251's premise no longer holds and the refusal in `cyclone_peer_config_uri` \
         should be revisited"
    );
}
