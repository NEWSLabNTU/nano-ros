//! Issue 1896 — the uORB bridge on the XRCE backend, in ONE PX4 image with
//! PX4's own `uxrce_dds_client`, both delivering to a real ROS 2 subscriber.
//!
//! ```text
//! px4_mavlink_debug ─uORB─▶ nros_uorb_bridge ─XRCE─┐
//!                                                  ├─▶ MicroXRCEAgent ─DDS─▶ `ros2 topic echo` ×2
//! PX4 timesync      ─uORB─▶ uxrce_dds_client ─XRCE─┘                        (rmw_fastrtps_cpp + px4_msgs)
//! ```
//!
//! The sibling of `px4_bridge_ros2_e2e` (the zenoh image). The XRCE image is
//! the interesting one because it holds TWO Micro-XRCE-DDS-Clients: PX4's stock
//! `uxrce_dds_client` (RFC-0039 Track B) links its own, and `libnros_cpp.a`
//! carries nano-ros's. Until issue 1896 both defined the same strong `uxr_*` /
//! `ucdr_*` symbols, the link silently picked one, and PX4's client SEGVed in
//! `uxr_ping_agent_attempts` before the startup script returned. So the
//! acceptance is NOT a link: it is a boot, the bridge's samples decoded by
//! ROS 2, AND PX4's own client's samples decoded by ROS 2 — the last because a
//! fix that silenced PX4's client (or bound it to our copy) would pass the
//! first two.
//!
//! No build here (CLAUDE.md): it consumes
//! `NROS_PX4_BRIDGE_RMW=xrce just px4 build-bridge-example debug_key_value humble`
//! and `just px4 build-px4-msgs-ros humble`, and fails naming them.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

use nros_tests::{
    fixtures::{XrceAgent, require_xrce_agent},
    ros2,
};
use px4_sitl_tests::Px4Sitl;

/// The bridged topic — `kOutTopic` in `NrosUorbBridge.cpp`.
const BRIDGE_TOPIC: &str = "/fmu/out/debug_key_value";
const BRIDGE_TYPE: &str = "px4_msgs/msg/DebugKeyValue";
/// A topic PX4's OWN `uxrce_dds_client` publishes unconditionally
/// (`dds_topics.yaml`), whatever the estimator's state.
const PX4_TOPIC: &str = "/fmu/out/timesync_status";
const PX4_TYPE: &str = "px4_msgs/msg/TimesyncStatus";
/// The key PX4's stock `px4_mavlink_debug` module publishes.
const PX4_KEY: &str = "velx";
/// The UDP port SITL's `rcS` points `uxrce_dds_client` at (`UXRCE_DDS_PRT`'s
/// default). The bridge dials the SAME agent, so one agent proves both clients
/// share it.
const AGENT_PORT: u16 = 8888;
const SAMPLES: usize = 3;
const ECHO_WINDOW: Duration = Duration::from_secs(60);
/// Both images publish on domain 0 (`NrosCppSessionSpec`'s domain in
/// `NrosUorbBridge.cpp`, `UXRCE_DDS_DOM_ID`'s default). The bus is pinned to
/// loopback on both sides by `dds_isolation` (issue 1009).
const DOMAIN: u8 = 0;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("canonicalize project root")
}

/// Put the PX4 daemon's process group in the orphan ledger (issues 0659/0923),
/// so a run that is SIGKILLed — a nextest timeout, a cancelled lane — leaves
/// nothing behind past the next `nros-peer-sweep`.
///
/// MEASURED, the shape that leaked for 13 hours on this host: when the daemon
/// dies, its startup script (`/bin/sh etc/init.d-posix/rcS 0`) and the
/// `px4-param` client it is blocked on SURVIVE it, waiting on a server socket
/// nobody will answer. `Px4Sitl`'s `Drop` reaps them — it SIGTERMs the whole
/// group, and SIGTERM was measured to kill both — but `Drop` does not run when
/// the test process is SIGKILLed, and the harness's `PR_SET_PDEATHSIG` reaches
/// the daemon only. `Px4Sitl` exposes no pid, so the daemon is found as the
/// one `px4` child of this process; `set_new_process_group` made it its
/// group's leader, so its pid IS the pgid the ledger needs.
#[cfg(target_os = "linux")]
fn record_px4_group() -> Option<i32> {
    let me = std::process::id().to_string();
    for e in fs::read_dir("/proc").ok()?.flatten() {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<i32>() else {
            continue;
        };
        let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
            continue;
        };
        let (Some(open), Some(close)) = (stat.find('('), stat.rfind(')')) else {
            continue;
        };
        let comm = &stat[open + 1..close];
        let ppid = stat[close + 1..].split_whitespace().nth(1);
        if comm == "px4" && ppid == Some(me.as_str()) {
            nros_tests::process::group_ledger::record(pid, "px4 sitl daemon (issue 1896)");
            return Some(pid);
        }
    }
    None
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// The prebuilt SITL tree, asserted on the BINARY's contents (issue 1046: the
/// module directory survives a rebuild of another root).
fn prebuilt_xrce_bridge_sitl_dir(distro: &str) -> PathBuf {
    let px4_dir = std::env::var("PX4_AUTOPILOT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| project_root().join("third-party/px4/PX4-Autopilot"));
    let build_dir = px4_dir.join("build/px4_sitl_default");
    let bin = build_dir.join("bin/px4");
    let rebuild =
        format!("NROS_PX4_BRIDGE_RMW=xrce just px4 build-bridge-example debug_key_value {distro}");
    assert!(
        bin.is_file(),
        "PX4 SITL binary missing at {}\nBuild it first:\n    {rebuild}",
        bin.display()
    );
    let bytes = fs::read(&bin).unwrap_or_else(|e| panic!("read {}: {e}", bin.display()));
    for (needle, what) in [
        (&b"nros_uorb_bridge"[..], "the uORB->RMW bridge module"),
        (&b"nros_rmw_xrce_register"[..], "the nano-ros XRCE backend"),
        (&b"uxrce_dds_client"[..], "PX4's stock uxrce_dds_client"),
        (
            &b"px4_mavlink_debug"[..],
            "PX4's px4_mavlink_debug producer",
        ),
    ] {
        assert!(
            contains_bytes(&bytes, needle),
            "{} does not contain {what} (`{}`). Rebuild with:\n    {rebuild}",
            bin.display(),
            String::from_utf8_lossy(needle)
        );
    }
    // Issue 1896's invariant, on the artifact: nano-ros's copy of the client is
    // `nros_`-prefixed. A symbol table is what the link resolved against, so
    // this is the cheapest place a regression of the prefix pass shows.
    assert!(
        contains_bytes(&bytes, b"nros_uxr_create_session"),
        "{} has no `nros_uxr_create_session` — nano-ros's Micro-XRCE-DDS-Client copy is \
         not symbol-prefixed, so it collides with PX4's own (issue 1896). Rebuild with:\n    \
         {rebuild}",
        bin.display()
    );
    build_dir
}

fn px4_msgs_overlay(distro: &str) -> PathBuf {
    let prefix = project_root().join(format!("build/px4-msgs-ros/{distro}/install"));
    let setup = prefix.join("share/px4_msgs/local_setup.bash");
    assert!(
        setup.is_file(),
        "no `px4_msgs` ament overlay for ROS 2 {distro} at {}.\nBuild it with:\n    \
         just px4 build-px4-msgs-ros {distro}",
        setup.display()
    );
    setup
}

/// `ros2 topic echo` over `rmw_fastrtps_cpp` with the `px4_msgs` overlay,
/// unbuffered, in its own self-reaping process group.
fn dds_echo(
    distro: &str,
    overlay: &Path,
    topic: &str,
    ty: &str,
) -> nros_tests::process::ManagedProcess {
    let env_setup = ros2::ros2_env_setup_dds_with_domain(distro, DOMAIN);
    let deadline = nros_tests::process::deadline(ECHO_WINDOW.as_secs());
    let cmd = format!(
        "{env_setup} && source {} && PYTHONUNBUFFERED=1 {deadline} ros2 topic echo {topic} {ty} \
         --qos-reliability best_effort",
        overlay.display()
    );
    let mut command = Command::new("bash");
    command
        .args(["-c", &nros_tests::process::group_suicide_wrapper(&cmd)[..]])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    nros_tests::process::set_orphan_group_suicide(&mut command);
    let mut echo = nros_tests::process::ManagedProcess::spawn_command(
        command,
        format!("ros2 topic echo {topic}"),
    )
    .expect("spawn ros2 topic echo");
    // The same orphan ledger every `Ros2Process` peer is in (issue 0659).
    #[cfg(target_os = "linux")]
    nros_tests::process::group_ledger::record(
        echo.handle_mut().id() as i32,
        &format!("ros2 topic echo {topic}"),
    );
    echo
}

/// Every `key:` decoded from `ros2 topic echo`'s YAML for a `char[10]` field,
/// up to the first NUL.
fn decoded_keys(echo: &str) -> Vec<String> {
    let mut out = Vec::new();
    for doc in echo.split("\n---") {
        let mut bytes = Vec::new();
        let mut in_key = false;
        for line in doc.lines() {
            let t = line.trim();
            if in_key {
                if let Some(b) = t
                    .strip_prefix("- ")
                    .and_then(|b| b.trim().parse::<u8>().ok())
                {
                    bytes.push(b);
                    continue;
                }
                in_key = false;
            }
            if let Some(v) = t.strip_prefix("key:") {
                let v = v.trim();
                if v.is_empty() {
                    in_key = true;
                } else {
                    bytes = v
                        .split(|c: char| !c.is_ascii_digit())
                        .filter_map(|n| n.parse::<u8>().ok())
                        .collect();
                }
            }
        }
        if !bytes.is_empty() {
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            out.push(String::from_utf8_lossy(&bytes[..end]).into_owned());
        }
    }
    out
}

#[test]
fn xrce_bridge_and_px4_uxrce_dds_client_share_one_image() {
    let distro = ros2::DEFAULT_ROS_DISTRO;
    ros2::require_ros2_dds();
    require_xrce_agent();
    let overlay = px4_msgs_overlay(distro);
    let build_dir = prebuilt_xrce_bridge_sitl_dir(distro);

    // PX4 SITL instance 0's server socket is HOST-global (`/tmp/px4-sock-0`).
    // A daemon another process left there makes this boot print `PX4 server
    // already running for instance 0` and exit, which `boot_in` reports as an
    // anonymous ~~`BootTimeout`~~ — measured while a second session ran a SITL test
    // on this host. Name it instead.
    #[cfg(unix)]
    assert!(
        std::os::unix::net::UnixStream::connect("/tmp/px4-sock-0").is_err(),
        "a PX4 SITL daemon (instance 0) is already serving /tmp/px4-sock-0 on this host — \
         another test or session holds it, and this boot would exit immediately"
    );

    // PX4's client dials a FIXED port, so the agent must own it. A port already
    // bound would make `XrceAgent::start`'s readiness probe pass against
    // somebody else's socket — refuse instead.
    assert!(
        std::net::UdpSocket::bind(("127.0.0.1", AGENT_PORT)).is_ok(),
        "udp/127.0.0.1:{AGENT_PORT} is already bound — another XRCE agent (or a leaked PX4 \
         SITL) holds the port PX4's uxrce_dds_client dials"
    );
    let agent = XrceAgent::start(AGENT_PORT).expect("start MicroXRCEAgent on the port PX4 dials");

    // The bridge reads its outward locator from the DAEMON's environment.
    // SAFETY: single-threaded here; nextest runs this test in its own process.
    unsafe { std::env::set_var("NROS_LOCATOR", format!("udp/{}", agent.addr())) };

    let sitl = Px4Sitl::boot_in(&build_dir).unwrap_or_else(|e| {
        panic!(
            "PX4 SITL with the XRCE bridge did not boot: {e:?}. Issue 1896's failure is this: \
             PX4's uxrce_dds_client resolving `uxr_*` into nano-ros's copy and SEGVing"
        )
    });
    #[cfg(target_os = "linux")]
    let px4_group = Some(record_px4_group().expect(
        "found no `px4` child of this test process to record in the orphan ledger — \
         `Px4Sitl::boot_in` no longer spawns the daemon directly, so this guard needs updating",
    ));

    // Pin PX4's OWN DDS participant to loopback, the half issue 1009's rule
    // ("pin both sides or neither") does not otherwise reach. The agent's
    // default Fast-DDS profile is the loopback one `XrceAgent::start` applies,
    // and it binds every participant the agent creates from a BINARY or
    // REFERENCE request — the bridge's. PX4's client creates its participant
    // from an XML STRING, which carries its own transports and overrides that
    // default, so with `UXRCE_DDS_PTCFG=0` its writers advertised no loopback
    // locator and the loopback-pinned ROS 2 subscriber discovered none of
    // them: measured, `ros2 topic list` showed the bridge's one `/fmu/out`
    // topic and none of PX4's 26, and the same run with neither side pinned
    // showed all 27. `1` is PX4's own localhost-only participant (a
    // `127.0.0.1` interface whitelist, the same shape as ours). The client
    // reads it at `start`, so it is restarted here — before any subscriber.
    for cmd in [
        "param set UXRCE_DDS_PTCFG 1",
        "uxrce_dds_client stop",
        &format!("uxrce_dds_client start -t udp -h 127.0.0.1 -p {AGENT_PORT}"),
    ] {
        sitl.shell(cmd).unwrap_or_else(|e| {
            panic!(
                "`{cmd}` failed: {e:?}\n=== SITL log ===\n{}",
                sitl.log_snapshot()
            )
        });
    }

    let start_out = sitl.shell("nros_uorb_bridge start").unwrap_or_else(|e| {
        panic!(
            "`nros_uorb_bridge start` failed: {e:?}\n=== SITL log ===\n{}",
            sitl.log_snapshot()
        )
    });
    let bridging = format!("bridging {BRIDGE_TOPIC} (uorb) -> {BRIDGE_TOPIC} (xrce)");
    assert!(
        start_out.contains(&bridging),
        "`nros_uorb_bridge start` did not report `{bridging}`.\n=== start output ===\n\
         {start_out}\n=== SITL log ===\n{}",
        sitl.log_snapshot()
    );

    let mut bridge_echo = dds_echo(distro, &overlay, BRIDGE_TOPIC, BRIDGE_TYPE);

    let mut producer = ProducerGuard(
        Command::new(sitl.bin_dir().join("px4-px4_mavlink_debug"))
            .current_dir(&build_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn PX4's stock px4_mavlink_debug client"),
    );

    let bridge_out = bridge_echo
        .wait_for_output_count("value:", SAMPLES, ECHO_WINDOW)
        .unwrap_or_else(|e| {
            panic!(
                "ROS 2 ({BRIDGE_TYPE} over rmw_fastrtps_cpp via the XRCE agent) received fewer \
                 than {SAMPLES} BRIDGED samples: {e}\n=== SITL log ===\n{}",
                sitl.log_snapshot()
            )
        });
    let keys = decoded_keys(&bridge_out);
    assert!(
        keys.len() >= SAMPLES && keys.iter().all(|k| k == PX4_KEY),
        "decoded keys {keys:?}, expected {SAMPLES}+ of {PX4_KEY:?}:\n{bridge_out}"
    );

    drop(bridge_echo);

    // PX4's OWN client, in the same image, still delivers: the collision broke
    // exactly this half. Its own subscriber, started now, so each wait has a
    // whole window — PX4 publishes this topic continuously.
    let mut px4_echo = dds_echo(distro, &overlay, PX4_TOPIC, PX4_TYPE);
    let px4_out = px4_echo
        .wait_for_output_count("timestamp:", SAMPLES, ECHO_WINDOW)
        .unwrap_or_else(|e| {
            panic!(
                "ROS 2 received fewer than {SAMPLES} {PX4_TOPIC} samples from PX4's OWN \
                 uxrce_dds_client in the bridge image: {e}\n=== SITL log ===\n{}",
                sitl.log_snapshot()
            )
        });

    eprintln!(
        "one PX4 image, two XRCE clients: ROS 2 {distro} decoded {} bridged {BRIDGE_TYPE} \
         (keys {:?}) and {PX4_TOPIC} from PX4's uxrce_dds_client",
        keys.len(),
        keys
    );
    eprintln!(
        "first bridged document:\n{}\nfirst PX4 document:\n{}",
        bridge_out.split("\n---").next().unwrap_or("").trim(),
        px4_out.split("\n---").next().unwrap_or("").trim()
    );

    drop(px4_echo);
    producer.stop();
    drop(sitl);
    #[cfg(target_os = "linux")]
    if let Some(pgid) = px4_group {
        nros_tests::process::group_ledger::forget(pgid);
    }
    drop(agent);
}

/// Kills the producer's shell client on every exit path.
struct ProducerGuard(Child);

impl ProducerGuard {
    fn stop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl Drop for ProducerGuard {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod parser {
    use super::*;

    #[test]
    fn block_and_flow_keys_decode_to_the_first_nul() {
        let echo = "key:\n- 118\n- 101\n- 108\n- 120\n- 0\nvalue: 1.0\n---\nkey: [118, 101, 108, 120, 0]\n";
        assert_eq!(decoded_keys(echo), vec!["velx", "velx"]);
    }
}
