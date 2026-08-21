//! FreeRTOS POSIX simulator end-to-end tests (phase-370 W3).
//!
//! The freertos family's first runtime lane with no emulator behind it. The
//! image is a host process running the FreeRTOS kernel's `ThirdParty/GCC/Posix`
//! port: tasks are pthreads, the tick is a host timer signal, and the network
//! stack is the HOST's — so the RMW is the same host CycloneDDS the native
//! cells use, and the whole cell costs a process spawn.
//!
//! # Why these cells are not in `entry_e2e.rs`
//!
//! That file owns the RTOS `EntryPubsub` subset, and every cell in it is
//! ZENOH: it starts a `ZenohRouter` on the cell's port and dials a
//! `tcp/127.0.0.1:<port>` observer. CycloneDDS needs no router and has no
//! locator, so those two steps have nothing to do here. `w1_consumer_of`
//! therefore leaves these cells unclaimed, which its own doc comment names as
//! the shape for "covered by a platform-e2e file".
//!
//! # What is asserted
//!
//! The bringup's default launch places a talker and a listener in ONE image, so
//! delivery is observable from the image's own output without a peer: the
//! listener's line proves a sample crossed a real CycloneDDS writer and reader
//! while running on FreeRTOS tasks. Both bounds are checked — the listener must
//! print, and it must print AFTER the talker, which is what distinguishes
//! delivery from a listener that logged its own startup.
//!
//! Prerequisites: `FREERTOS_DIR` (the kernel source). Deliberately NOT lwIP,
//! `arm-none-eabi-gcc`, or QEMU — that this lane needs none of the three is the
//! point of the board.
//!
//! Run with: `cargo nextest run -p nros-tests --test freertos_posix`

use nros_tests::{
    alloc::domain_of,
    fixtures::{
        ManagedProcess, build_freertos_posix_workspace_c_entry,
        build_freertos_posix_workspace_cpp_entry, freertos,
    },
    matrix::{Lang, PlatformId, Workload},
    output::{INT32_LISTENER_LOG_PREFIX, INT32_TALKER_LOG_PREFIX, WORKSPACE_C_TALKER_LOG_PREFIX},
};
use std::{path::PathBuf, process::Command, time::Duration};

/// Bounded spin so the image exits on its own rather than running until the
/// harness kills it — the same knob the ThreadX host sim takes in `entry_e2e`.
const SPIN_MS: &str = "8000";
const WINDOW: Duration = Duration::from_secs(30);

fn require_freertos() {
    if !freertos::is_freertos_available() {
        nros_tests::skip!(
            "FREERTOS_DIR not set or invalid — `just setup-freertos`, then \
             export FREERTOS_DIR=$PWD/third-party/freertos/kernel"
        );
    }
}

/// Boot the image and return everything it wrote.
///
/// Each cell gets its OWN DDS domain, from the matrix allocator that every
/// other cyclone pair uses (`domain_of(platform, lang, workload)`; CLAUDE.md:
/// "Cyclone fixture pairs bake distinct domains for parallel SPDP"). Without
/// it the C and C++ images sit on the default domain together, and since both
/// carry a talker AND a listener, each hears the other: the C listener can
/// print before its own talker does, and the ordering assertion below — the
/// part that makes this evidence of DELIVERY rather than of a startup log —
/// fails. It reproduced exactly that way, passing solo and failing in a pair.
///
/// `ROS_DOMAIN_ID` is the right knob because this image is a HOST process; the
/// hosted rung of the domain ladder reads it (`nros-c/src/support.rs`, #206).
fn run_entry(entry: &PathBuf, domain: u8, label: &str) -> String {
    let mut cmd = Command::new(entry);
    cmd.env("ROS_DOMAIN_ID", domain.to_string());
    cmd.env("NROS_ENTRY_SPIN_MS", SPIN_MS);
    let mut proc = ManagedProcess::spawn_command(cmd, label)
        .unwrap_or_else(|e| panic!("spawn the {label} image at {}: {e}", entry.display()));
    proc.wait_for_output(WINDOW)
        .unwrap_or_else(|e| panic!("{label} produced no output within {WINDOW:?}: {e}"))
}

/// Assert the image delivered: the listener printed, and it printed after the
/// talker did.
///
/// Ordering is asserted rather than just presence because a listener line on
/// its own does not separate "a sample arrived" from "the listener logged
/// something at startup". Greps go through `nros_tests::output` constants, never
/// literals — the example banners get slimmed and a literal here would make a
/// green test into a timeout nobody could read (CLAUDE.md).
/// What the environment looked like, for a failure that does not reproduce.
///
/// issue 0737 reported both cells publishing and delivering nothing, on a tree
/// where they pass here — bare, under saturating load, and through the lane
/// recipe. An unreproducible red costs a second investigation unless the first
/// one leaves its evidence behind, so a failure prints the inputs that could
/// differ between hosts rather than only the output that did.
fn env_report(domain: u8) -> String {
    let show = |k: &str| std::env::var(k).unwrap_or_else(|_| "<unset>".into());
    format!(
        "  domain asked for (ROS_DOMAIN_ID)       : {domain}\n\
           ambient ROS_DOMAIN_ID                  : {}\n\
           ambient CYCLONEDDS_URI                 : {}\n\
           ambient RMW_IMPLEMENTATION             : {}\n\
         Both nodes live in ONE participant in this image, so delivery needs no\n\
         network — a total absence of `Received:` points at the participant or\n\
         the domain, not the transport (issue 0737).",
        show("ROS_DOMAIN_ID"),
        show("CYCLONEDDS_URI"),
        show("RMW_IMPLEMENTATION"),
    )
}

fn assert_delivered(out: &str, talker_marker: &str, domain: u8, label: &str) {
    let env = env_report(domain);
    let talked = out.find(talker_marker).unwrap_or_else(|| {
        panic!("{label}: no `{talker_marker}` line — the talker never published.\n{env}\n{out}")
    });
    let heard = out.find(INT32_LISTENER_LOG_PREFIX).unwrap_or_else(|| {
        panic!("{label}: no `{INT32_LISTENER_LOG_PREFIX}` line — nothing was delivered.\n{out}")
    });
    assert!(
        heard > talked,
        "{label}: the listener's first `{INT32_LISTENER_LOG_PREFIX}` precedes the talker's \
         first `{talker_marker}`, so it is not evidence of delivery.\n{env}\n{out}"
    );
}

#[test]
fn freertos_posix_c_entry_delivers_over_cyclonedds() {
    require_freertos();
    let entry = build_freertos_posix_workspace_c_entry().unwrap_or_else(|e| {
        nros_tests::skip!(
            "freertos-posix C workspace entry not built \
             (just freertos build-fixtures): {e:?}"
        )
    });
    let domain = domain_of(PlatformId::FreertosPosix, Lang::C, Workload::EntryPubsub);
    let out = run_entry(&entry.to_path_buf(), domain, "freertos-posix-c");
    // The pure-C workspace talker spells its marker differently from the C++
    // one; both listeners agree on `Received:`.
    assert_delivered(
        &out,
        WORKSPACE_C_TALKER_LOG_PREFIX,
        domain,
        "freertos-posix-c",
    );
}

#[test]
fn freertos_posix_cpp_entry_delivers_over_cyclonedds() {
    require_freertos();
    let entry = build_freertos_posix_workspace_cpp_entry().unwrap_or_else(|e| {
        nros_tests::skip!(
            "freertos-posix C++ workspace entry not built \
             (just freertos build-fixtures): {e:?}"
        )
    });
    let domain = domain_of(PlatformId::FreertosPosix, Lang::Cpp, Workload::EntryPubsub);
    let out = run_entry(&entry.to_path_buf(), domain, "freertos-posix-cpp");
    assert_delivered(&out, INT32_TALKER_LOG_PREFIX, domain, "freertos-posix-cpp");
}
