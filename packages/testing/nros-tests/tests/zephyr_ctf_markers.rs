//! A traced Zephyr sched-task image's CTF stream holds the executor's callback
//! markers (issue 2766).
//!
//! **Bucket (phase-329 W4): matrix consumer.** The configured cell
//! `(ZephyrNativeSim, C, Zenoh, RealtimeSchedTasks, Workspace) + [Ctf]`.
//!
//! nano-ros's Zephyr trace sink (`nros_zephyr_trace_marker`, installed by the
//! sched task runner) forwards every callback-trace marker to
//! `sys_trace_app_marker` through a WEAK reference. Stock Zephyr has no such
//! event: nano-ros's Zephyr patch set adds it (`scripts/zephyr/ctf-app-marker-patch.sh`).
//! Without the patch the reference is null and a CTF image records no marker,
//! with no warning. This cell is the end-to-end witness: the `zephyr_ctf` image
//! of the realtime-c bringup (CTF tracing on the posix backend, callback
//! tracing on) runs its two sched tasks for a while, and the stream it wrote is
//! decoded against the TSDL metadata of the Zephyr tree that BUILT it
//! (`nros_tests::ctf`) -- every byte accounted for, not grepped.
//!
//! What must hold:
//! - the metadata of that tree declares `app_marker` (the patch is applied);
//! - the stream decodes whole;
//! - it carries `app_marker` events for callback registration and for
//!   callback start and end, and every end follows a start for its handle.
//!
//! ## Prerequisites
//!
//! - `just zephyr build-fixtures` (the `build-ws-c-realtime-ctf-zenoh` leaf), in
//!   a workspace that `nros setup` patched
//! - ROS's `rmw_zenohd`
//!
//! Run with: `cargo nextest run -p nros-tests --test zephyr_ctf_markers`

use nros_tests::{
    ctf,
    fixtures::{RequireFixture, ZenohRouter, build_zephyr_workspace_c_realtime_ctf_entry},
    matrix::{Config, PlatformId},
    output::ZEPHYR_SCHED_TASK_ENTERED_MARKER,
    zephyr::{ZephyrPlatform, ZephyrProcess},
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

/// The callback-trace marker ids (`nros-node`'s `executor::callback_trace`):
/// `MARKER_REGISTER`, `MARKER_START`, `MARKER_END`.
const MARKER_REGISTER: u64 = 16;
const MARKER_START: u64 = 18;
const MARKER_END: u64 = 19;

/// The event nano-ros's patch set adds, by its metadata name.
const APP_MARKER: &str = "app_marker";

/// How long the image runs once a sched task has entered. The ctrl task runs
/// every 10 ms, so this is ~200 callbacks.
const RUN_FOR: Duration = Duration::from_secs(2);

/// Fewest callback start events the run must show: a tenth of what the ctrl
/// task alone dispatches in [`RUN_FOR`].
const MIN_STARTS: usize = 20;

/// The Zephyr tree a west build dir was configured against
/// (`ZEPHYR_BASE:PATH=` in its `CMakeCache.txt`).
fn zephyr_base_of(exe: &Path) -> PathBuf {
    let build = exe
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| panic!("{}: not inside a west build dir", exe.display()));
    let cache = build.join("CMakeCache.txt");
    let text =
        std::fs::read_to_string(&cache).unwrap_or_else(|e| panic!("read {}: {e}", cache.display()));
    text.lines()
        .find_map(|l| l.strip_prefix("ZEPHYR_BASE:PATH="))
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("{} names no ZEPHYR_BASE", cache.display()))
}

#[test]
fn zephyr_ctf_stream_holds_the_callback_markers() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::ZephyrNativeSim],
        "the Zephyr CTF marker cell",
    );
    let exe = build_zephyr_workspace_c_realtime_ctf_entry()
        .require("zephyr realtime-c CTF image (build-ws-c-realtime-ctf-zenoh)");

    // The tree that built it must declare the event; if it does not, the image
    // could record no marker however correct the sink is.
    let meta_path = zephyr_base_of(&exe).join("subsys/tracing/ctf/tsdl/metadata");
    let meta_text = std::fs::read_to_string(&meta_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", meta_path.display()));
    let meta =
        ctf::parse_metadata(&meta_text).unwrap_or_else(|e| panic!("{}: {e}", meta_path.display()));
    assert!(
        meta.events.values().any(|e| e.name == APP_MARKER),
        "{} declares no `{APP_MARKER}` event: the Zephyr workspace that built the image \
         lacks nano-ros's patch set (re-run `nros setup zephyr`)",
        meta_path.display()
    );

    let router = nros_tests::fixtures::or_unmet(ZenohRouter::start_unique());
    let dir = tempfile::tempdir().expect("tempdir");
    let mut zephyr = ZephyrProcess::start_with_locator_in_dir(
        &exe,
        ZephyrPlatform::NativeSim,
        &router.locator(),
        dir.path(),
    )
    .expect("failed to start the native_sim image");
    let boot = zephyr.wait_for_pattern(ZEPHYR_SCHED_TASK_ENTERED_MARKER, Duration::from_secs(30));
    assert!(
        boot.contains(ZEPHYR_SCHED_TASK_ENTERED_MARKER),
        "no sched task started (`{ZEPHYR_SCHED_TASK_ENTERED_MARKER}`). Output:\n{boot}"
    );
    std::thread::sleep(RUN_FOR);
    zephyr.kill();

    let stream_path = dir.path().join("channel0_0");
    let stream = std::fs::read(&stream_path).unwrap_or_else(|e| {
        panic!(
            "the image wrote no CTF stream at {} ({e}): CONFIG_TRACING_BACKEND_POSIX is off, \
             or it writes elsewhere",
            stream_path.display()
        )
    });
    // The posix backend flushes each event, but a kill can land inside one
    // write: keep the whole events and drop a torn tail.
    let events = decode_whole(&meta, &stream);

    let markers: Vec<(u64, u64)> = events
        .iter()
        .filter(|e| e.name == APP_MARKER)
        .map(|e| {
            (
                e.field("marker_id").expect("app_marker has marker_id"),
                e.field("arg").expect("app_marker has arg"),
            )
        })
        .collect();
    let count = |id| markers.iter().filter(|(m, _)| *m == id).count();
    let (registers, starts, ends) = (
        count(MARKER_REGISTER),
        count(MARKER_START),
        count(MARKER_END),
    );
    let summary = format!(
        "{} events, {} app_marker: {registers} register, {starts} start, {ends} end",
        events.len(),
        markers.len()
    );
    assert!(
        !markers.is_empty(),
        "the stream holds no `{APP_MARKER}` event ({summary}): the sink's weak reference to \
         sys_trace_app_marker was null, or callback tracing is off"
    );
    assert!(
        registers > 0,
        "no callback was registered in the stream ({summary})"
    );
    assert!(
        starts >= MIN_STARTS,
        "fewer than {MIN_STARTS} callback starts in {RUN_FOR:?} ({summary})"
    );
    // Pairing, per handle: an end only after an open start, at most one more
    // start than end (the kill can land inside a callback).
    let mut open = std::collections::BTreeMap::<u64, i64>::new();
    for (m, handle) in &markers {
        let depth = open.entry(*handle).or_default();
        match *m {
            MARKER_START => *depth += 1,
            MARKER_END => {
                *depth -= 1;
                assert!(
                    *depth >= 0,
                    "an end for handle {handle} with no start ({summary})"
                );
            }
            _ => {}
        }
    }
    assert!(
        open.values().all(|d| (0..=1).contains(d)),
        "unbalanced start/end per handle {open:?} ({summary})"
    );
}

/// Decode every whole event; a stream torn by the kill loses only its last.
fn decode_whole(meta: &ctf::Metadata, stream: &[u8]) -> Vec<ctf::Event> {
    match ctf::decode(meta, stream) {
        Ok(e) => e,
        Err(whole_err) => {
            // Find the longest prefix that decodes, and require that only a
            // tail shorter than the largest event was dropped.
            let max_event = meta.timestamp_size
                + meta.id_size
                + meta
                    .events
                    .values()
                    .map(|d| d.payload_size())
                    .max()
                    .unwrap_or(0);
            (stream.len().saturating_sub(max_event)..stream.len())
                .rev()
                .find_map(|end| ctf::decode(meta, &stream[..end]).ok())
                .unwrap_or_else(|| {
                    panic!(
                        "the CTF stream ({} bytes) does not decode: {whole_err}",
                        stream.len()
                    )
                })
        }
    }
}

/// RFC-0051 section 7c -- the cases this file runs are exactly the Runtime cells
/// `matrix::consumer_of` assigns to it. Fixture-free.
#[test]
fn cases_bound_to_matrix_cells() {
    use nros_tests::matrix::{
        Consumer, Kind as K, Lang as L, PlatformId as P, Rmw as R, Workload as W,
        assert_consumer_config_cases,
    };
    assert_consumer_config_cases(
        Consumer::ZephyrCtfMarkers,
        &[(
            (
                P::ZephyrNativeSim,
                L::C,
                R::Zenoh,
                W::RealtimeSchedTasks,
                K::Workspace,
            ),
            &[Config::Ctf],
        )],
    );
}
