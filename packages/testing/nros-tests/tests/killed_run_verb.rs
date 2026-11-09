//! A SIGKILLed `nros run` leaves no image behind (phase-544 W3, issue 2307).
//!
//! The verb starts its runner with `PR_SET_PDEATHSIG`, and since issue 2307
//! every first-party runner ties what it starts to itself
//! (`nros_runner_lifetime`): one PDEATHSIG hop for an image it starts
//! directly, a guarded process group for a tool that starts the image further
//! down. So a SIGKILL to the verb ALONE, which no handler sees, must take the
//! runner and the image with it.
//!
//! Each case starts `nros run --plan` on a prebuilt image, waits until the
//! image is running under it, records every descendant of the verb, sends
//! SIGKILL to the verb's pid only (never its group), and scans `/proc` until
//! none of the recorded processes is alive. The record keys each pid on its
//! start time, so a reused pid is not mistaken for a survivor, and a survivor
//! is killed before the test reports it.
//!
//! The last case is the harness's own stop, `nros_tests::run::stop`, which
//! kills the groups under the verb before the verb; it must leave nothing
//! either.
//!
//! Compiles nothing: the images are fixture rows (`linux/rust/talker`, the
//! mps2-an385 `qemu-bsp-talker`). A missing fixture is an unmet precondition.

#![cfg(target_os = "linux")]

use std::{
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use nros_tests::{
    fixtures::{
        RequireFixture, ZenohRouter, build_native_talker, build_qemu_bsp_talker, zenohd_unique,
    },
    lane_scope,
    matrix::PlatformId,
    process::require_zenohd,
    run,
};
use rstest::rstest;

/// How long an image gets to appear under the verb.
const STARTUP: Duration = Duration::from_secs(60);
/// How long the image, once its verb is killed, gets to be gone.
const TEARDOWN: Duration = Duration::from_secs(10);

/// A process as `/proc` names it: its pid and start time (field 22 of
/// `/proc/<pid>/stat`, clock ticks since boot), so a reused pid is a
/// different process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Proc {
    pid: u32,
    start: u64,
}

fn stat_fields(pid: u32) -> Option<Vec<String>> {
    let s = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_, rest) = s.rsplit_once(") ")?;
    Some(rest.split(' ').map(str::to_string).collect())
}

fn proc_of(pid: u32) -> Option<Proc> {
    // `rest` starts at field 3 (state), so field 22 is index 19.
    let f = stat_fields(pid)?;
    Some(Proc {
        pid,
        start: f.get(19)?.parse().ok()?,
    })
}

/// Alive: the same process (pid and start time) and not a zombie, which has
/// exited and only waits for whoever reaps it.
fn alive(p: Proc) -> bool {
    stat_fields(p.pid).is_some_and(|f| {
        f.first().map(String::as_str) != Some("Z")
            && f.get(19).and_then(|s| s.parse().ok()) == Some(p.start)
    })
}

fn comm(pid: u32) -> String {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Spawn the verb `run` built, its output discarded (the test reads `/proc`,
/// not the console).
fn spawn_verb(run: &mut run::HostRun) -> Child {
    let mut cmd = run.take_command();
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // A test binary that dies mid-case still stops its verb (and, through it,
    // everything below).
    nros_tests::process::set_orphan_group_suicide(&mut cmd);
    cmd.spawn().expect("spawning `nros run`")
}

/// Wait until a descendant of `verb` satisfies `is_image`, then return every
/// descendant (runner, image, any guard) as it stands.
fn running_tree(verb: &mut Child, what: &str, is_image: impl Fn(u32) -> bool) -> Vec<Proc> {
    let deadline = Instant::now() + STARTUP;
    loop {
        if let Ok(Some(status)) = verb.try_wait() {
            panic!("`nros run` exited ({status}) before {what} was running under it");
        }
        let tree = run::descendants(verb.id());
        if tree.iter().any(|&p| is_image(p)) {
            // Let the image settle into whatever it does next (a QEMU guest
            // booting, a talker connecting) before the verb is killed.
            std::thread::sleep(Duration::from_secs(1));
            return run::descendants(verb.id())
                .into_iter()
                .filter_map(proc_of)
                .collect();
        }
        assert!(
            Instant::now() < deadline,
            "{what} did not appear under `nros run` within {STARTUP:?}; descendants: {:?}",
            tree.iter().map(|&p| (p, comm(p))).collect::<Vec<_>>()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Wait up to [`TEARDOWN`] for every recorded process to be gone. A survivor
/// is SIGKILLed before the test fails, so a red run leaves nothing either.
fn assert_no_survivor(tree: &[Proc], how: &str) {
    let deadline = Instant::now() + TEARDOWN;
    let mut left: Vec<Proc> = tree.to_vec();
    while !left.is_empty() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
        left.retain(|&p| alive(p));
    }
    if left.is_empty() {
        return;
    }
    let named: Vec<(u32, String)> = left.iter().map(|p| (p.pid, comm(p.pid))).collect();
    for p in &left {
        // SAFETY: SIGKILL to a process this test started (recorded as a
        // descendant of its own verb, pid and start time both checked).
        unsafe { libc::kill(p.pid as libc::pid_t, libc::SIGKILL) };
    }
    panic!("{how} left processes running {TEARDOWN:?} later: {named:?}");
}

/// SIGKILL the verb's pid alone, not its group: what `kill -9 <pid>` from a
/// terminal, an OOM kill or a CI timeout does.
fn sigkill_verb_only(verb: &mut Child) {
    verb.kill().expect("SIGKILL to `nros run`");
    let _ = verb.wait();
}

fn native_talker(locator: &str) -> Command {
    let talker = build_native_talker().require("native-rs-talker");
    let mut cmd = Command::new(talker);
    cmd.env("NROS_LOCATOR", locator);
    cmd
}

fn is_exe(pid: u32, exe: &Path) -> bool {
    let want = exe.canonicalize().ok();
    want.is_some() && std::fs::read_link(format!("/proc/{pid}/exe")).ok() == want
}

fn require_linux_lane() {
    if !lane_scope::admits(PlatformId::Linux) {
        nros_tests::lane_skip!(
            "{}",
            lane_scope::skip_note(PlatformId::Linux, "killed-run-verb")
        );
    }
}

fn require_qemu_baremetal_lane() {
    if !lane_scope::admits(PlatformId::QemuBaremetal) {
        nros_tests::lane_skip!(
            "{}",
            lane_scope::skip_note(PlatformId::QemuBaremetal, "killed-run-verb")
        );
    }
}

/// A host image, started directly (one PDEATHSIG hop) and under a `prefix`
/// (the runner's guarded chain: `stdbuf` here, `gdb --args` for a user).
#[rstest]
#[case::direct(&[])]
#[case::prefixed(run::LINE_BUFFERED)]
fn sigkilled_verb_leaves_no_native_image(
    #[case] plan_args: &[(&str, &str)],
    #[from(zenohd_unique)] router: ZenohRouter,
) {
    require_linux_lane();
    require_zenohd();
    let image = native_talker(&router.locator());
    let exe = build_native_talker()
        .require("native-rs-talker")
        .to_path_buf();
    let mut plan = run::host_image(&image, plan_args).expect("writing the plan");
    let mut verb = spawn_verb(&mut plan);
    let tree = running_tree(&mut verb, "the talker", |p| is_exe(p, &exe));
    sigkill_verb_only(&mut verb);
    assert_no_survivor(&tree, "a SIGKILLed `nros run` on a host image");
}

/// A QEMU guest (mps2-an385). The boot names no semihosting, so the guest
/// cannot end QEMU on its own: QEMU is still running when the verb is killed,
/// whatever the firmware does without a router.
#[test]
fn sigkilled_verb_leaves_no_qemu_guest() {
    require_qemu_baremetal_lane();
    let elf = build_qemu_bsp_talker().require("qemu-bsp-talker");
    let boot = run::QemuBoot::table("arm", "mps2-an385").arg("cpu", "cortex-m3");
    let mut plan = run::qemu_image(elf, &boot).expect("writing the plan");
    let mut verb = spawn_verb(&mut plan);
    let tree = running_tree(&mut verb, "QEMU", |p| comm(p).starts_with("qemu-system"));
    sigkill_verb_only(&mut verb);
    assert_no_survivor(&tree, "a SIGKILLed `nros run` on a QEMU guest");
}

/// The harness's own stop (`run::stop`), which phase-528 W5 wrote to work
/// around issue 2307, still leaves nothing.
#[test]
fn harness_stop_leaves_no_qemu_guest() {
    require_qemu_baremetal_lane();
    let elf = build_qemu_bsp_talker().require("qemu-bsp-talker");
    let boot = run::QemuBoot::table("arm", "mps2-an385").arg("cpu", "cortex-m3");
    let mut plan = run::qemu_image(elf, &boot).expect("writing the plan");
    let mut verb = spawn_verb(&mut plan);
    let tree = running_tree(&mut verb, "QEMU", |p| comm(p).starts_with("qemu-system"));
    run::stop(&mut verb);
    assert_no_survivor(&tree, "`nros_tests::run::stop`");
}
