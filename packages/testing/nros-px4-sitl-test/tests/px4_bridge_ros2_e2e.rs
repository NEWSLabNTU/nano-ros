//! phase-325 W3.4 — a STOCK PX4 module's uORB topic reaches a REAL ROS 2
//! subscriber through the nano-ros bridge.
//!
//! ```text
//! px4_mavlink_debug ──uORB──▶ nros_uorb_bridge ──zenoh/CDR──▶ rmw_zenohd ──▶ `ros2 topic echo`
//!   (stock PX4 module)        (translate + typed publish)                     (rmw_zenoh_cpp + px4_msgs)
//! ```
//!
//! Both ends are FOREIGN to nano-ros, and that is the measurement (phase-325
//! "Goal"): a nano-ros subscriber reading a nano-ros bridge would be satisfied
//! identically by a correct and a broken encoding, because both ends would
//! share the bug (issue 0351's shape). Here the producer is PX4's own
//! `px4_mavlink_debug` example module and the consumer is `ros2 topic echo`
//! decoding with the `px4_msgs` typesupport rosidl generated — neither has any
//! nano-ros code in it. So a transposed field, a wrong CDR layout or a wrong
//! type name shows up as either no delivery or wrong decoded VALUES, never as a
//! pass. (Mutation-tested: forcing `key[0] = 'X'` in the bridge's `translate()`
//! turns this red with `ROS 2 decoded key "Xelx"`.)
//!
//! ## No build here
//!
//! Per CLAUDE.md the compile belongs to the build stage. This test consumes
//! two prebuilt artifacts and fails LOUDLY, naming the command, when either is
//! missing:
//!
//! * the PX4 SITL image with the bridge linked —
//!   `just px4 build-bridge-example debug_key_value <distro>`;
//! * the `px4_msgs` ament overlay a ROS 2 process needs to load the type —
//!   `just px4 build-px4-msgs-ros <distro>`.
//!
//! `<distro>` is [`ros2::DEFAULT_ROS_DISTRO`], the edition the subscriber runs.
//!
//! The generated header's TYPE HASH is deliberately NOT a precondition here.
//! The first draft refused an Iron+ (RIHS01) image against a Humble subscriber
//! on the theory that the hash segment of the key expression would hide it —
//! and an image generated with `--ros-edition jazzy` delivered to the Humble
//! subscriber exactly as the `humble` one did (`rmw_zenoh_cpp` 0.1.9, measured
//! 2026-10-14). Not because the hash is ignored: the zenoh backend picks the
//! DATA key's hash segment from its own `ros-*` cargo feature, which the
//! bridge's archive never receives, so every bridge image keys Humble's
//! `TypeHashNotSupported` whatever the header says (issue 1893). A guard on the
//! header asserts something the wire does not carry; the recipe builds
//! `humble`, the one pairing whose halves agree.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

use nros_tests::{
    fixtures::{ZenohRouter, or_unmet},
    ros2::{self, Ros2Process},
};
use px4_sitl_tests::Px4Sitl;

/// The bridged topic — `kOutTopic` in `NrosUorbBridge.cpp`.
const TOPIC: &str = "/fmu/out/debug_key_value";
/// The ROS 2 type the subscriber decodes with — from the `px4_msgs` overlay,
/// NOT from anything nano-ros generated.
const MSG_TYPE: &str = "px4_msgs/msg/DebugKeyValue";
/// The key PX4's stock `px4_mavlink_debug` module publishes on
/// `debug_key_value` (`src/examples/px4_mavlink_debug/px4_mavlink_debug.cpp`:
/// `strncpy(dbg_key.key, "velx", 10)`).
const PX4_KEY: &str = "velx";
/// How many decoded samples the subscriber must print. More than one, so the
/// assertion that `value` ADVANCES has something to compare.
const SAMPLES: usize = 3;
/// The subscriber's whole lifetime — and the wait on it. `px4_mavlink_debug`
/// publishes at 2 Hz for 50 s and the bridge polls at 5 Hz, so 3 samples
/// arrive in a few seconds; the rest is ROS 2 Python startup under load.
const ECHO_WINDOW: Duration = Duration::from_secs(45);

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("canonicalize project root")
}

/// Does `haystack` contain `needle` anywhere? A byte scan, not `strings`: no
/// subprocess and no PATH dependency, and milliseconds on the ~60 MB binary.
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Resolve the prebuilt SITL tree, asserting on the BINARY's contents that the
/// bridge is linked into it.
///
/// Not on the module directory: `external_modules/modules/<name>/` and the
/// `bin/px4-<name>` shim SURVIVE across builds while only the last
/// `EXTERNAL_MODULES_LOCATION`'s modules are linked into `bin/px4` (issue 1046,
/// and the trap phase-325 W3 records for exactly this test).
fn prebuilt_bridge_sitl_dir(distro: &str) -> PathBuf {
    let px4_dir = std::env::var("PX4_AUTOPILOT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| project_root().join("third-party/px4/PX4-Autopilot"));
    let build_dir = px4_dir.join("build/px4_sitl_default");
    let bin = build_dir.join("bin/px4");
    let rebuild = format!("just px4 build-bridge-example debug_key_value {distro}");

    assert!(
        bin.is_file(),
        "PX4 SITL binary missing at {}\nBuild it first:\n    {rebuild}",
        bin.display()
    );
    let bytes = fs::read(&bin).unwrap_or_else(|e| panic!("read {}: {e}", bin.display()));

    // The module's COMMAND name — what `nros_uorb_bridge start` resolves, so
    // its absence is the actual failure.
    assert!(
        contains_bytes(&bytes, b"nros_uorb_bridge"),
        "SITL tree at {} was built WITHOUT the uORB->RMW bridge: {} does not \
         contain `nros_uorb_bridge`. PX4 takes one EXTERNAL_MODULES_LOCATION per \
         build and they share this build dir, so another root built last; its \
         module directory and shim survive that and prove nothing (issue 1046).\n\
         Rebuild with:\n    {rebuild}",
        build_dir.display(),
        bin.display()
    );

    // The OUTWARD backend is a build choice (W3.3), and this test's peer speaks
    // rmw_zenoh. Checked here, before boot, because the other choice does not
    // fail as "wrong backend": an `NROS_PX4_BRIDGE_RMW=xrce` image SEGVs inside
    // PX4's own `uxrce_dds_client` before the startup script returns (issue
    // 1896), which would read as `BootTimeout`.
    assert!(
        contains_bytes(&bytes, b"nros_rmw_zenoh_register"),
        "{} registers no zenoh backend — it was built with another \
         NROS_PX4_BRIDGE_RMW. This test's ROS 2 peer is rmw_zenoh_cpp.\n\
         Rebuild with:\n    NROS_PX4_BRIDGE_RMW=zenoh {rebuild}",
        bin.display()
    );

    // The STOCK producer this test relies on. It is a PX4 example module; a
    // board config that drops it would leave the bridge nothing to forward,
    // which would read as a bridge fault.
    assert!(
        contains_bytes(&bytes, b"px4_mavlink_debug"),
        "{} does not contain PX4's stock `px4_mavlink_debug` module, the uORB \
         producer this test bridges. The px4_sitl_default board config must \
         enable CONFIG_EXAMPLES_PX4_MAVLINK_DEBUG.",
        bin.display()
    );

    build_dir
}

/// The `px4_msgs` ament overlay a ROS 2 process sources to load the type.
fn px4_msgs_overlay(distro: &str) -> PathBuf {
    let prefix = project_root().join(format!("build/px4-msgs-ros/{distro}/install"));
    let setup = prefix.join("share/px4_msgs/local_setup.bash");
    let msg = prefix.join("share/px4_msgs/msg/DebugKeyValue.msg");
    assert!(
        setup.is_file() && msg.is_file(),
        "no `px4_msgs` ament overlay for ROS 2 {distro} at {} (need {} and {}).\n\
         A ROS 2 subscriber cannot load `{MSG_TYPE}` without it — the distro does \
         not ship px4_msgs. Build it with:\n    just px4 build-px4-msgs-ros {distro}",
        prefix.display(),
        setup.display(),
        msg.display()
    );
    setup
}

/// One decoded `px4_msgs/msg/DebugKeyValue` as `ros2 topic echo` printed it.
#[derive(Debug)]
struct Sample {
    timestamp: u64,
    key: String,
    value: f64,
}

/// Parse `ros2 topic echo`'s YAML stream (`---`-separated documents).
///
/// `char[10] key` is `uint8[10]` in ROS IDL, so `key` prints as a block list
/// of byte values; it is decoded up to the first NUL here. Hand-parsed because
/// the shape is three scalar/list fields and a YAML dependency for it would be
/// more surface than the test.
fn parse_samples(echo: &str) -> Vec<Sample> {
    let mut out = Vec::new();
    for doc in echo.split("\n---") {
        let mut timestamp = None;
        let mut value = None;
        let mut key_bytes: Vec<u8> = Vec::new();
        let mut in_key = false;
        for line in doc.lines() {
            let t = line.trim();
            if in_key {
                if let Some(b) = t.strip_prefix("- ") {
                    if let Ok(b) = b.trim().parse::<u8>() {
                        key_bytes.push(b);
                        continue;
                    }
                }
                in_key = false;
            }
            if let Some(v) = t.strip_prefix("timestamp:") {
                timestamp = v.trim().parse::<u64>().ok();
            } else if let Some(v) = t.strip_prefix("value:") {
                value = v.trim().parse::<f64>().ok();
            } else if let Some(v) = t.strip_prefix("key:") {
                let v = v.trim();
                if v.is_empty() {
                    in_key = true;
                } else {
                    // Inline flow form, e.g. `key: [118, 101, ...]` or
                    // `array('B', [118, ...])` — take every integer in it.
                    key_bytes = v
                        .split(|c: char| !c.is_ascii_digit())
                        .filter_map(|n| n.parse::<u8>().ok())
                        .collect();
                }
            }
        }
        if let (Some(timestamp), Some(value)) = (timestamp, value) {
            let end = key_bytes
                .iter()
                .position(|&b| b == 0)
                .unwrap_or(key_bytes.len());
            let key = String::from_utf8_lossy(&key_bytes[..end]).into_owned();
            out.push(Sample {
                timestamp,
                key,
                value,
            });
        }
    }
    out
}

#[test]
fn ros2_subscriber_receives_a_stock_px4_topic_through_the_bridge() {
    let distro = ros2::DEFAULT_ROS_DISTRO;
    // Lane scope first, then the probes — `require_ros2` fails (never passes)
    // on a host that claims ROS 2 and lacks it or rmw_zenoh_cpp.
    ros2::require_ros2();
    let overlay = px4_msgs_overlay(distro);
    let build_dir = prebuilt_bridge_sitl_dir(distro);

    // A private router on a leased port: the bridge and the subscriber are
    // both clients of it, multicast scouting is off on the ROS side, so no
    // other zenoh traffic on this host can stand in for the bridge's.
    let router = or_unmet(ZenohRouter::start_unique());
    let locator = router.locator();

    // The bridge reads its outward locator from `$NROS_LOCATOR` in the PX4
    // DAEMON's environment (`NrosUorbBridge::init`), and `Px4Sitl::boot_in`
    // gives the daemon this process's environment — it has no per-spawn env
    // seam. Set before the daemon spawns, with no other thread of this test
    // running: nextest runs each test in its own process, and this binary has
    // one test.
    // SAFETY: single-threaded at this point (see above); nothing reads the
    // environment concurrently.
    unsafe { std::env::set_var("NROS_LOCATOR", &locator) };

    let sitl = Px4Sitl::boot_in(&build_dir).expect("Px4Sitl::boot_in");

    // `start` runs `NrosUorbBridge::init()` in the SHELL CLIENT's command
    // context, so its `PX4_INFO` lines come back as this call's stdout, not
    // through the daemon log (the same property W2.4 measured for `listener`;
    // measured again here — the first draft waited on the log for the
    // `bridging` line and timed out while the bridge was up). The work-queue
    // `Run()` lines DO reach the daemon log.
    let start_out = sitl.shell("nros_uorb_bridge start").unwrap_or_else(|e| {
        panic!(
            "`nros_uorb_bridge start` failed: {e:?}\n=== SITL log ===\n{}",
            sitl.log_snapshot()
        )
    });
    // The outward backend is a BUILD choice (W3.3); this test drives the ROS
    // side over rmw_zenoh, so an image built with NROS_PX4_BRIDGE_RMW=xrce is
    // the wrong fixture, not a bridge fault — say so.
    let bridging = format!("bridging {TOPIC} (uorb) -> {TOPIC} (zenoh)");
    assert!(
        start_out.contains(&bridging),
        "`nros_uorb_bridge start` did not report `{bridging}`. If it bridged to \
         a different backend, the image was built with another \
         NROS_PX4_BRIDGE_RMW; rebuild with the default (zenoh).\n\
         === start output ===\n{start_out}\n=== SITL log ===\n{}",
        sitl.log_snapshot()
    );

    let mut echo = Ros2Process::topic_echo_in_overlay(
        TOPIC,
        MSG_TYPE,
        &locator,
        distro,
        ECHO_WINDOW,
        Some(Path::new(&overlay)),
    )
    .expect("spawn ros2 topic echo");

    // The STOCK producer, started LAST so its finite run (100 samples at 2 Hz)
    // overlaps a subscriber that is already up. Nothing publishes
    // `debug_key_value` by default.
    //
    // Spawned, not `sitl.shell(...)`: `px4_mavlink_debug`'s main is the publish
    // loop itself and returns only after its 100th sample, so the shell client
    // blocks ~50 s and `shell()` (which waits for exit) would hold this test
    // until the producer had already finished. The client is the same
    // `bin/px4-<module>` shim `shell()` runs.
    let mut producer = ProducerGuard(
        Command::new(sitl.bin_dir().join("px4-px4_mavlink_debug"))
            .current_dir(&build_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn PX4's stock px4_mavlink_debug client"),
    );

    let echo_out = echo.wait_for_output_count("value:", SAMPLES, ECHO_WINDOW);
    let echo_out = match echo_out {
        Ok(out) => out,
        Err(e) => panic!(
            "a real ROS 2 subscriber ({MSG_TYPE} over rmw_zenoh_cpp, ROS 2 \
             {distro}) did not receive {SAMPLES} bridged samples: {e}\n\
             === router ===\n{}\n=== SITL log ===\n{}",
            router.launch_line(),
            sitl.log_snapshot()
        ),
    };

    let samples = parse_samples(&echo_out);
    assert!(
        samples.len() >= SAMPLES,
        "parsed {} DebugKeyValue sample(s) out of the subscriber's output, \
         expected {SAMPLES}:\n{echo_out}",
        samples.len()
    );

    // The KEY is the decisive field: PX4's module wrote "velx" into a
    // `char[10]` on uORB, the bridge copied it into the CDR struct, and ROS 2
    // decoded it with rosidl's typesupport. A transposed or mis-sized field
    // comes back as other bytes.
    for s in &samples {
        assert_eq!(
            s.key, PX4_KEY,
            "ROS 2 decoded key {:?} (expected {PX4_KEY:?} from PX4's \
             px4_mavlink_debug): {s:?}\n{echo_out}",
            s.key
        );
        assert!(
            s.timestamp > 0,
            "ROS 2 decoded timestamp 0 — PX4 stamps every sample with \
             hrt_absolute_time(): {s:?}\n{echo_out}"
        );
    }
    // `px4_mavlink_debug` increments `value` by 1 every loop, so successive
    // samples must differ and move forward. Equal values would mean a frozen
    // or re-sent buffer; a non-numeric or wild value, a layout fault.
    let values: Vec<f64> = samples.iter().map(|s| s.value).collect();
    assert!(
        values.windows(2).all(|w| w[1] > w[0]),
        "decoded `value`s do not advance ({values:?}); px4_mavlink_debug \
         increments it each publish:\n{echo_out}"
    );

    eprintln!(
        "ROS 2 {distro} subscriber decoded {} bridged samples, first {:?}, last {:?}",
        samples.len(),
        samples.first(),
        samples.last()
    );
    // The first document verbatim, so a receipt shows what `ros2 topic echo`
    // printed rather than only what this file parsed out of it.
    eprintln!(
        "first echoed document:\n{}",
        echo_out.split("\n---").next().unwrap_or("").trim()
    );

    drop(echo);
    producer.stop();
    drop(sitl);
    drop(router);
}

/// Kills the producer's shell client on every exit path, so a failed
/// assertion does not leave it attached to a daemon `Drop` is tearing down.
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
    fn block_list_key_is_decoded_to_the_first_nul() {
        let echo = "timestamp: 42\nkey:\n- 118\n- 101\n- 108\n- 120\n- 0\n- 0\nvalue: 3.0\n---\n";
        let s = parse_samples(echo);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].key, "velx");
        assert_eq!(s[0].timestamp, 42);
        assert_eq!(s[0].value, 3.0);
    }
}
