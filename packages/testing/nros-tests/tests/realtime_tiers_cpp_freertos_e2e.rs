//! phase-274 W3 (#126) / phase-144 (#144) — runtime E2E for embedded C/C++
//! **RFC-0015 Model 1** (`run_tiers`) on FreeRTOS/mps2-an385 (QEMU). The
//! `ws-realtime-cpp-mps2` workspace deploys three nodes on three priority tiers
//! over ONE shared zenoh session, each an `Executor` on its own FreeRTOS task:
//!   - `ctrl`  — high tier, 10 ms period (boot task)
//!   - `aux`   — mid tier, 50 ms period (spawned BY a spawned tier)
//!   - `telem` — low tier, 100 ms period (spawned task)
//!
//! `FreertosBoard::run_tiers` (→ `nros_board_freertos_run_tiers`) opens the
//! session on the boot task, then spawns the non-boot tiers as a chain — each
//! tier's setup spawns the next only after its own declares complete (the #144
//! chained-spawn fix). The **mid** tier (`aux`) is therefore spawned by a
//! spawned tier: it is the middle hop the old loop-spawn raced (two tiers
//! declaring concurrently left aux's publisher write filter closed). Each
//! node's `on_tick` calls `publish(...)` and prints `[<tier>] tick=N` **only
//! when the publish succeeds** — so observing `[ctrl]`, `[aux]` AND `[telem]`
//! ticks proves (a) the shared session connects to the host zenohd, (b) the
//! chained spawn serialized the declares so tier-2's publisher opened, and
//! (c) all three tiers schedule + publish at their periods. The `[aux]`
//! assertion is the #144 regression signal: it would FAIL under the pre-fix
//! loop-spawn race.
//!
//! The firmware uses a static 192.0.3.x lwIP config, so `start_mps2_an385_
//! freertos_slirp` runs a matching slirp net (host 192.0.3.1) and the entry
//! dials `tcp/192.0.3.1:17851` (baked). No TAP / bridge / root.
//!
//! Run with: `cargo nextest run -p nros-tests --test realtime_tiers_cpp_freertos_e2e`

use nros_tests::fixtures::{
    QemuProcess, ZenohRouter, build_freertos_workspace_cpp_realtime_entry, freertos,
    is_qemu_available, require_zenohd,
};
use std::time::Duration;

/// Router port baked into the realtime entry's locator (see the
/// `workspace-cpp-freertos-realtime` fixture's `NROS_ENTRY_LOCATOR`).
const REALTIME_ENTRY_PORT: u16 = 17851;

#[test]
fn realtime_tiers_cpp_freertos_all_three_tiers_publish() {
    if !require_zenohd() {
        nros_tests::skip!("zenohd not found");
    }
    if !freertos::is_freertos_available() {
        nros_tests::skip!("FREERTOS_DIR not set or invalid");
    }
    if !freertos::is_lwip_available() {
        nros_tests::skip!("LWIP_DIR not set or invalid");
    }
    if !freertos::is_arm_gcc_available() {
        nros_tests::skip!("arm-none-eabi-gcc not found");
    }
    if !is_qemu_available() {
        nros_tests::skip!("qemu-system-arm not found");
    }

    let entry = build_freertos_workspace_cpp_realtime_entry()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("realtime C++ tiers fixture not built: {e}"));

    // Host zenohd on 0.0.0.0:<port>; slirp maps the board's gateway 192.0.3.1 to
    // the host, so the guest reaches it. The nodes' `publish_raw` (and thus the
    // `[tier] tick` prints) only succeed once this session is up.
    let _router = ZenohRouter::start_on("0.0.0.0", REALTIME_ENTRY_PORT).unwrap_or_else(|e| {
        nros_tests::skip!("zenohd failed to start on {REALTIME_ENTRY_PORT}: {e}")
    });

    let mut qemu = QemuProcess::start_mps2_an385_freertos_slirp(&entry)
        .unwrap_or_else(|e| panic!("boot realtime freertos QEMU: {e}"));

    // High tier (boot task) connects + publishes first — a `[ctrl] tick` proves
    // the run_tiers boot session reached the host zenohd.
    let out_ctrl = qemu
        .wait_for_output_pattern("[ctrl] tick=", Duration::from_secs(90))
        .unwrap_or_else(|e| {
            qemu.kill();
            panic!(
                "high tier (ctrl) never published over the shared session — the \
                 run_tiers boot executor did not connect.\nerr: {e:?}"
            )
        });
    assert!(out_ctrl.contains("[ctrl] tick="));

    // Mid tier (aux) is spawned BY a spawned tier — the middle hop of the
    // boot→mid→low chain. Under the pre-#144 loop-spawn race two tiers declared
    // concurrently and aux's publisher write filter stayed closed (no ticks).
    // An `[aux] tick` proves the chained spawn serialized the declares so
    // tier-2's publisher opened. This is the #144 regression signal.
    let out_aux = qemu
        .wait_for_output_pattern("[aux] tick=", Duration::from_secs(30))
        .unwrap_or_else(|e| {
            qemu.kill();
            panic!(
                "mid tier (aux) never published — the spawned-by-a-spawned tier's \
                 publisher declare did not complete (the #144 chained-spawn race \
                 the fix closes).\nerr: {e:?}"
            )
        });
    assert!(out_aux.contains("[aux] tick="));

    // Low tier (spawned task, borrowed executor over the SAME session) must also
    // publish — proving the per-tier task + shared-session gating works.
    let out_telem = qemu
        .wait_for_output_pattern("[telem] tick=", Duration::from_secs(30))
        .unwrap_or_else(|e| {
            qemu.kill();
            panic!(
                "low tier (telem) never published — the spawned tier task's \
                 borrowed executor over the shared session did not run.\nerr: {e:?}"
            )
        });
    qemu.kill();

    assert!(
        out_telem.contains("[telem] tick="),
        "expected all three tiers to publish (ctrl high 10 ms + aux mid 50 ms + \
         telem low 100 ms)"
    );
}
