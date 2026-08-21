//! phase-295 W3.b — THE realtime-tiers matrix consumer (RFC-0051).
//!
//! Consolidates the 15 per-cell `realtime_tiers_*` files into one
//! parametrized test over the `Workload::RealtimeTiers` cells of the test
//! matrix (`nros_tests::matrix`): every cell deploys a `ws-realtime-*`
//! workspace whose `system.toml` maps two callback groups onto two priority
//! tiers (`[tiers.high]` 10 ms `/ctrl` timer, `[tiers.low]` 100 ms `/telem`
//! timer — RFC-0015 Model 1, `run_tiers`), then proves BOTH tiers are
//! scheduled at their declared cadences.
//!
//! Two observation styles, preserved from the per-cell files:
//! - **Observer cells** (native / zephyr / nuttx): two `int32-sink`
//!   subscribers on `/ctrl` + `/telem` receive cross-process through a
//!   zenoh router (issue 0096 — an entry's own nodes can't observe each
//!   other in-image). Anchor on the SLOW tier (5 telem receives ≈ 0.5 s+
//!   elapsed), then require the 10 ms tier to have outrun the 100 ms tier.
//!   The per-cell [`Proof`] keeps each lane's historical assertion:
//!   `CounterRatio3x` (#158 deterministic payload-counter proof, robust to
//!   delivery batching), `CountRatio3x` (native C/C++ sample-count ≥3×),
//!   `CountStrict` (zephyr strictly-more margin).
//! - **Serial-tick cells** (freertos/mps2-an385): no host observers — each
//!   tier node prints `[<tier>] tick=N` on the QEMU serial console ONLY
//!   when its publish succeeds. The C++ cell runs a THIRD `[aux]` mid tier
//!   (50 ms) spawned BY a spawned tier: its tick is the #144 chained-spawn
//!   regression signal (the pre-fix loop-spawn race left aux's publisher
//!   write filter closed).
//!
//! Cell nuances carried over (see each case's `note`): the native
//! `cpp_rclcpp` cell is the issue-#124 proof that IS-A-node rclcpp-shape
//! components land on their tier via the phase-272 `node_name →
//! sched_context` table; the nuttx cells pin `NuttxBoard::run_tiers`
//! (pthread per tier, phase-281/285/#199); the zephyr cells pin
//! `ZephyrBoard::run_tiers` (k_thread per tier, phase-276/281).
//!
//! Tier *priority* preemption is advisory on native — the assertions prove
//! per-tier scheduling at the declared periods, not preemption.
//!
//! Isolation (phase-295 W4): every embedded cell's `port` is the ONE
//! allocator's `RealtimeTiers` number (`nros_tests::alloc::port_of`) — the
//! SAME formula the fixture bakers use (`examples/fixtures.toml` rows, the
//! west lane) — so router and baked locator can never disagree by hand.
//! `None` = native ephemeral isolation.
//!
//! Run with: `cargo nextest run -p nros-tests --test realtime_tiers_e2e`
//! (filter one platform: `-E 'binary(realtime_tiers_e2e) and test(zephyr)'`).

use nros_tests::{
    TestResult,
    alloc::port_of,
    fixtures::{
        ManagedProcess, QemuProcess, ZenohRouter, ZephyrPlatform, ZephyrProcess,
        build_freertos_workspace_c_realtime_entry, build_freertos_workspace_cpp_realtime_entry,
        build_freertos_workspace_rust_realtime_entry, build_native_workspace_c_realtime_entry,
        build_native_workspace_cpp_rclcpp_realtime_entry,
        build_native_workspace_cpp_realtime_entry, build_native_workspace_rust_realtime_entry,
        build_nuttx_riscv_workspace_c_realtime_entry,
        build_nuttx_riscv_workspace_cpp_realtime_entry,
        build_nuttx_riscv_workspace_rust_realtime_entry, build_nuttx_workspace_c_realtime_entry,
        build_nuttx_workspace_cpp_realtime_entry, build_nuttx_workspace_rust_realtime_entry,
        build_threadx_workspace_rust_realtime_entry, build_zephyr_workspace_c_realtime_entry,
        build_zephyr_workspace_cpp_realtime_entry, build_zephyr_workspace_rust_realtime_entry,
        freertos, is_qemu_available, require_zenohd,
    },
    matrix::{
        Cell as MCell, Lang as ML, PlatformId as MP, Tier as MT, W1Consumer, Workload as MW,
        w1_consumer_of,
    },
};
use std::{path::PathBuf, process::Command, time::Duration};

// =============================================================================
// Cell table types
// =============================================================================

/// How the cell's guest boots (and which skip preconditions apply).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Boot {
    /// Host-native entry process (`nros::main!` hosted spin, ephemeral router).
    Native,
    /// Zephyr native_sim image (west-lane fixture; skips when absent).
    ZephyrNativeSim,
    /// NuttX QEMU arm-virt guest (slirp gateway 10.0.2.2 → host router).
    NuttxArm,
    /// NuttX QEMU rv-virt riscv32 guest (slirp, `-icount`).
    NuttxRiscv,
    /// FreeRTOS QEMU mps2-an385 guest (static 192.0.3.x lwIP + board-net
    /// slirp; observed via serial ticks, no host subscribers).
    FreertosMps2,
    /// ThreadX-Linux hosted simulation (phase-297 W5): a host process like
    /// [`Boot::Native`] (pthread-backed ThreadX, NSOS host sockets), but the
    /// fixture bakes the allocator locator, so the router runs on the cell's
    /// `port` instead of an ephemeral one.
    ThreadxLinux,
}

/// The per-cell assertion, preserved 1:1 from the pre-consolidation files.
#[derive(Copy, Clone, Debug)]
enum Proof {
    /// #158 deterministic proof: each tier publishes a MONOTONIC counter,
    /// so its highest delivered value = how many times ITS OWN timer fired
    /// (robust to zenoh delivery batching/drops). Assert `telem_max > 0`
    /// and `ctrl_max ≥ 3 × telem_max` (10 ms vs 100 ms ⇒ ~10×).
    CounterRatio3x,
    /// Sample-count proof (native C/C++/rclcpp historical form): after the
    /// 5-sample telem anchor, `ctrl_n ≥ 3 × telem_n`.
    CountRatio3x,
    /// Zephyr strictly-more margin: `ctrl_n > telem_n` — proves the high
    /// tier runs FASTER while staying robust to native_sim NSOS jitter and
    /// zenoh delivery batching (the anchor already proves the low tier).
    CountStrict,
    /// FreeRTOS serial proof: each listed tier's `[<tier>] tick=` marker
    /// must appear on the QEMU serial console (publish-gated prints).
    SerialTicks(&'static [&'static str]),
    /// issue 0636 gap 2 — serial-console proof for a RUST cell on a lane with
    /// no host observers. Each named tier must print its dispatch marker
    /// (`nros_tests::output::tier_dispatch_marker`), which the Rust realtime
    /// nodes emit on their first successful publish. `SerialTicks` cannot be
    /// reused: those nodes deliberately print no per-tick line (issue 0572 —
    /// the 10 ms tier would swamp the console).
    SerialDispatch(&'static [&'static str]),
}

type Resolver = fn() -> TestResult<PathBuf>;

/// The per-cell EXECUTION data for one realtime-tiers matrix cell. The
/// coordinate lives in `matrix::Cell`; this carries the boot/resolver/proof.
/// A coordinate may yield MORE than one `Exec` — the `(Linux, Cpp)` cell runs
/// both the component-shape and the #124 rclcpp-shape entry, a sub-variant the
/// matrix's `(platform, lang)` axes cannot distinguish (hence [`exec_for`]
/// returns a `Vec`). `label` is the display lang, so failure messages tell the
/// two `cpp` variants apart.
struct Exec {
    label: &'static str,
    resolver: Resolver,
    /// Baked router port — the allocator's number for the cell's coordinate.
    /// `None` = ephemeral (native).
    port: Option<u16>,
    boot: Boot,
    proof: Proof,
    /// Provenance / nuance — folded into failure messages so a red cell
    /// still names the seam it pins.
    note: &'static str,
}

/// Map a RealtimeTiers coordinate to its execution row(s). Non-native cells
/// carry the allocator's baked port; `(Linux, Cpp)` returns two rows, the
/// component and rclcpp shapes. An unmapped coordinate is a HARD panic
/// (phase-329 W1: a new RealtimeTiers cell must wire its boot here).
fn exec_for(platform: MP, lang: ML) -> Vec<Exec> {
    let port = if matches!(platform, MP::Linux) {
        None
    } else {
        Some(port_of(platform, lang, MW::RealtimeTiers))
    };
    match (platform, lang) {
        (MP::Linux, ML::Rust) => vec![Exec {
            label: "rust",
            resolver: native_rust_entry,
            port,
            boot: Boot::Native,
            proof: Proof::CounterRatio3x,
            note: "phase-263 B2 `nros::main!` run_tiers (RFC-0032 §5); #158 counter proof",
        }],
        (MP::Linux, ML::C) => vec![Exec {
            label: "c",
            resolver: native_c_entry,
            port,
            boot: Boot::Native,
            proof: Proof::CountRatio3x,
            note: "phase-269 W4 C sched-context (nros_cpp_create_sched_context + node_create_ex)",
        }],
        (MP::Linux, ML::Cpp) => vec![
            Exec {
                label: "cpp",
                resolver: native_cpp_entry,
                port,
                boot: Boot::Native,
                proof: Proof::CountRatio3x,
                note: "phase-269 W4 C++ configure-shape sched-context (NodeBuilder::sched())",
            },
            Exec {
                label: "cpp-rclcpp",
                resolver: native_cpp_rclcpp_entry,
                port,
                boot: Boot::Native,
                proof: Proof::CountRatio3x,
                note: "issue #124 / phase-272 W3: IS-A-node rclcpp-shape components bind via the \
                       node_name → sched_context table at Executor::node_builder — a miss here \
                       means rclcpp-shape nodes lost their tier again",
            },
        ],
        (MP::ZephyrNativeSim, ML::Rust) => vec![Exec {
            label: "rust",
            resolver: build_zephyr_workspace_rust_realtime_entry,
            port,
            boot: Boot::ZephyrNativeSim,
            proof: Proof::CountStrict,
            note: "phase-276 W2 / #128 half 2: ZephyrBoard::run_tiers (RFC-0015 Model 1)",
        }],
        (MP::ZephyrNativeSim, ML::Cpp) => vec![Exec {
            label: "cpp",
            resolver: build_zephyr_workspace_cpp_realtime_entry,
            port,
            boot: Boot::ZephyrNativeSim,
            proof: Proof::CountStrict,
            note: "phase-281 W3b: first full west link + runtime proof of the run_tiers seam",
        }],
        (MP::ZephyrNativeSim, ML::C) => vec![Exec {
            label: "c",
            resolver: build_zephyr_workspace_c_realtime_entry,
            port,
            boot: Boot::ZephyrNativeSim,
            proof: Proof::CountStrict,
            note: "phase-281 W3c: C nodes over the shared ZephyrBoard::run_tiers glue",
        }],
        (MP::NuttxArm, ML::Cpp) => vec![Exec {
            label: "cpp",
            resolver: nuttx_cpp_entry,
            port,
            boot: Boot::NuttxArm,
            proof: Proof::CounterRatio3x,
            note: "phase-281 W3-nuttx: NuttxBoard::run_tiers (commit 37cfaf728)",
        }],
        (MP::NuttxArm, ML::C) => vec![Exec {
            label: "c",
            resolver: nuttx_c_entry,
            port,
            boot: Boot::NuttxArm,
            proof: Proof::CounterRatio3x,
            note: "phase-281 W3-nuttx: pure-C lane over NuttxBoard::run_tiers",
        }],
        (MP::NuttxArm, ML::Rust) => vec![Exec {
            label: "rust",
            resolver: nuttx_rust_entry,
            port,
            boot: Boot::NuttxArm,
            proof: Proof::CounterRatio3x,
            note: "phase-281 W3-nuttx: QemuArmVirt::run_tiers (std::thread per tier), \
                   the cell that completed the 12-cell Model-1 matrix",
        }],
        (MP::NuttxRiscv, ML::Rust) => vec![Exec {
            label: "rust",
            resolver: nuttx_riscv_rust_entry,
            port,
            boot: Boot::NuttxRiscv,
            proof: Proof::CounterRatio3x,
            note: "phase-285 W6 / #165: QemuRvVirt::run_tiers",
        }],
        (MP::NuttxRiscv, ML::C) => vec![Exec {
            label: "c",
            resolver: nuttx_riscv_c_entry,
            port,
            boot: Boot::NuttxRiscv,
            proof: Proof::CounterRatio3x,
            note: "#199 follow-up: C riscv_nuttx_entry over NuttxBoard::run_tiers",
        }],
        (MP::NuttxRiscv, ML::Cpp) => vec![Exec {
            label: "cpp",
            resolver: nuttx_riscv_cpp_entry,
            port,
            boot: Boot::NuttxRiscv,
            proof: Proof::CounterRatio3x,
            note: "#199 follow-up: C++ riscv_nuttx_entry over NuttxBoard::run_tiers",
        }],
        (MP::FreertosMps2, ML::Cpp) => vec![Exec {
            label: "cpp",
            resolver: freertos_cpp_entry,
            port,
            boot: Boot::FreertosMps2,
            proof: Proof::SerialTicks(&["ctrl", "aux", "telem"]),
            note: "phase-274 W3 (#126) + #144 chained tier spawn: ctrl(10ms)/aux(50ms)/telem(100ms)",
        }],
        (MP::FreertosMps2, ML::C) => vec![Exec {
            label: "c",
            resolver: freertos_c_entry,
            port,
            boot: Boot::FreertosMps2,
            proof: Proof::SerialTicks(&["ctrl", "telem"]),
            note: "phase-281 W2: C nodes over the SHARED nros_board_freertos_run_tiers glue \
                   (codegen routes embedded-C via the C++ emitter + NROS_C_COMPONENT seam)",
        }],
        // issue 0636 gap 2 — the Rust arm of the FreeRTOS multi-tier path, which
        // was exported from `nros-board-freertos`, reachable from the macro, and
        // called by NOTHING: every FreeRTOS realtime fixture was C or C++, and
        // the only Rust FreeRTOS entry is single-tier `run_entry`. That is the
        // arm #0636's fix had to be reasoned onto rather than measured.
        //
        // `SerialDispatch`, not `SerialTicks` like the C/C++ siblings: the Rust
        // nodes print no per-tick line by decision (issue 0572). Both tiers
        // dispatching IS the property this issue is about — a tier that never
        // runs is the defect.
        (MP::FreertosMps2, ML::Rust) => vec![Exec {
            label: "rust",
            resolver: freertos_rust_entry,
            port,
            boot: Boot::FreertosMps2,
            proof: Proof::SerialDispatch(&["low", "high"]),
            note: "issue 0636 gap 2: Mps2An385Freertos::run_tiers — boot tier is `low` \
                   (least urgent, bigger-is-more-urgent kernel), `high` chain-spawned",
        }],
        (MP::ThreadxLinux, ML::Rust) => vec![Exec {
            label: "rust",
            resolver: threadx_linux_rust_entry,
            port,
            boot: Boot::ThreadxLinux,
            proof: Proof::CounterRatio3x,
            note: "phase-297 W4/W5: ThreadxLinux::run_tiers over nros_threadx_create_task \
                   (RFC-0053 byte-pool stacks); also the W2 shim's two-threads-run proof. \
                   NOTE resolve_tiers sorts descending by raw number, so on ThreadX the \
                   BOOT tier is `low` (telem) and `high` (ctrl) is chain-spawned",
        }],
        (p, l) => panic!(
            "realtime_tiers_e2e: no execution mapping for matrix cell {p:?}/{l:?} — add an \
             `exec_for` arm (phase-329 W1)"
        ),
    }
}

fn plat_str(p: MP) -> &'static str {
    match p {
        MP::Linux => "native",
        MP::ZephyrNativeSim => "zephyr",
        MP::NuttxArm => "nuttx-arm",
        MP::NuttxRiscv => "nuttx-riscv",
        MP::FreertosMps2 => "freertos",
        MP::ThreadxLinux => "threadx-linux",
        _ => "?",
    }
}

// Resolver adapters: normalize the `&'static Path` builders onto the
// `PathBuf`-returning zephyr shape so one fn-pointer column fits all.
fn native_rust_entry() -> TestResult<PathBuf> {
    build_native_workspace_rust_realtime_entry().map(|p| p.to_path_buf())
}
fn native_c_entry() -> TestResult<PathBuf> {
    build_native_workspace_c_realtime_entry().map(|p| p.to_path_buf())
}
fn native_cpp_entry() -> TestResult<PathBuf> {
    build_native_workspace_cpp_realtime_entry().map(|p| p.to_path_buf())
}
fn native_cpp_rclcpp_entry() -> TestResult<PathBuf> {
    build_native_workspace_cpp_rclcpp_realtime_entry().map(|p| p.to_path_buf())
}
fn nuttx_rust_entry() -> TestResult<PathBuf> {
    build_nuttx_workspace_rust_realtime_entry().map(|p| p.to_path_buf())
}
fn nuttx_c_entry() -> TestResult<PathBuf> {
    build_nuttx_workspace_c_realtime_entry().map(|p| p.to_path_buf())
}
fn nuttx_cpp_entry() -> TestResult<PathBuf> {
    build_nuttx_workspace_cpp_realtime_entry().map(|p| p.to_path_buf())
}
fn nuttx_riscv_rust_entry() -> TestResult<PathBuf> {
    build_nuttx_riscv_workspace_rust_realtime_entry().map(|p| p.to_path_buf())
}
fn nuttx_riscv_c_entry() -> TestResult<PathBuf> {
    build_nuttx_riscv_workspace_c_realtime_entry().map(|p| p.to_path_buf())
}
fn nuttx_riscv_cpp_entry() -> TestResult<PathBuf> {
    build_nuttx_riscv_workspace_cpp_realtime_entry().map(|p| p.to_path_buf())
}
fn freertos_c_entry() -> TestResult<PathBuf> {
    build_freertos_workspace_c_realtime_entry().map(|p| p.to_path_buf())
}
fn threadx_linux_rust_entry() -> TestResult<PathBuf> {
    build_threadx_workspace_rust_realtime_entry().map(|p| p.to_path_buf())
}
fn freertos_rust_entry() -> TestResult<PathBuf> {
    build_freertos_workspace_rust_realtime_entry().map(|p| p.to_path_buf())
}
fn freertos_cpp_entry() -> TestResult<PathBuf> {
    build_freertos_workspace_cpp_realtime_entry().map(|p| p.to_path_buf())
}

// =============================================================================
// Guest process — one kill() over the three process kinds
// =============================================================================

enum Guest {
    Managed(ManagedProcess),
    Zephyr(ZephyrProcess),
    Qemu(QemuProcess),
}

impl Guest {
    fn kill(&mut self) {
        match self {
            Guest::Managed(p) => p.kill(),
            Guest::Zephyr(p) => p.kill(),
            Guest::Qemu(p) => p.kill(),
        }
    }

    /// Whatever the guest has printed so far — issue 0565.
    ///
    /// The tier failures report "the low tier was not scheduled", and the guest
    /// is the only thing that knows WHICH of the two that is: it prints
    /// `nros: FAILED to spawn tier <name> after N attempts — tier will not run`
    /// (and a per-attempt line carrying the OS error) when the spawn is what
    /// failed, and nothing at all when it never reached the entry banner.
    /// Killing it unread threw that away, so the two were indistinguishable
    /// from the verdict. Same rule as issue 0445: a verdict states what it
    /// examined.
    ///
    /// Best-effort by construction — a guest that is wedged returns what it has
    /// buffered, and an empty string is itself the diagnosis.
    fn drain(&mut self, timeout: Duration) -> String {
        match self {
            Guest::Managed(p) => p.wait_for_all_output(timeout).unwrap_or_default(),
            Guest::Zephyr(p) => p.wait_for_output(timeout).unwrap_or_default(),
            Guest::Qemu(p) => p.wait_for_output(timeout).unwrap_or_default(),
        }
    }
}

// =============================================================================
// Shared helpers
// =============================================================================

/// The guest's console, drained BEFORE it is killed, formatted for a panic
/// message.
///
/// phase-351 — issue 0565 added this reasoning to the ONE arm where the
/// symptom was noticed (the low-tier anchor) and left the others killing the
/// guest first, which by construction destroys the evidence. Issue 0572 is what
/// that costs: `/ctrl` counter 0 with a healthy `/telem`, and no guest console
/// to say whether the high tier failed to spawn, failed to open a session, or
/// published into a closed writer. Every verdict path calls this now, so the
/// gap cannot reopen in a third arm.
///
/// Best-effort by construction — a wedged guest returns what it buffered, and
/// an empty string is itself the diagnosis.
fn guest_console(guest: &mut Guest) -> String {
    console_excerpt(&guest.drain(Duration::from_secs(3)))
}

/// Excerpt a guest console: the first [`HEAD`] lines, then the last [`TAIL`].
///
/// Issue 0570 — the first cut printed only the LAST 25 lines, which is right for
/// a guest that simply stopped producing and WRONG for one that crashed: a NuttX
/// assertion dump is hundreds of lines (`stack_dump:` hex pages, then
/// `dump_tasks:`), so a 25-line tail lands in the middle of the hex and the line
/// that names the cause — the assertion, its file:line, the exception — has
/// scrolled off the top. That cost a whole diagnosis: 0570 was filed as a stack
/// overflow read off the task table in the tail, and the 8x stack raise that
/// followed left the crash untouched, because the actual reason was never in the
/// window. (It was `pthread_attr_destroy` smashing a saved `ra`.)
///
/// So show BOTH ends. The head carries why it died; the tail carries the state
/// it died in.
fn console_excerpt(console: &str) -> String {
    const HEAD: usize = 30;
    const TAIL: usize = 20;
    const SEP: &str = "\n           ";

    let lines: Vec<&str> = console.lines().collect();
    if lines.is_empty() {
        return "<the guest printed NOTHING — it did not reach the entry banner>".to_string();
    }
    if lines.len() <= HEAD + TAIL {
        return lines.join(SEP);
    }
    let elided = lines.len() - HEAD - TAIL;
    let mut out: Vec<String> = lines[..HEAD].iter().map(|l| l.to_string()).collect();
    out.push(format!("… {elided} line(s) elided …"));
    out.extend(lines[lines.len() - TAIL..].iter().map(|l| l.to_string()));
    out.join(SEP)
}

/// Skip-precondition gate per boot mechanism (identical semantics to the
/// pre-consolidation files: missing fixture / west image / qemu → skip).
fn require_cell_env(boot: Boot) {
    match boot {
        Boot::Native
        | Boot::NuttxArm
        | Boot::NuttxRiscv
        | Boot::FreertosMps2
        | Boot::ThreadxLinux => {
            if !require_zenohd() {
                nros_tests::skip!("zenohd not found");
            }
        }
        // The zephyr cells historically gate on the router START (below)
        // rather than a zenohd probe — keep that shape.
        Boot::ZephyrNativeSim => {}
    }
    match boot {
        Boot::NuttxArm => {
            if !is_qemu_available() {
                nros_tests::skip!("qemu-system-arm not found");
            }
        }
        Boot::NuttxRiscv => {
            if !nros_tests::esp32::is_qemu_riscv32_available() {
                nros_tests::skip!("qemu-system-riscv32 not found");
            }
        }
        Boot::FreertosMps2 => {
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
        }
        Boot::Native | Boot::ZephyrNativeSim | Boot::ThreadxLinux => {}
    }
}

/// How long the slow-tier 5-sample anchor may take: QEMU guests need a
/// cold-boot + zenoh-discovery budget; native connects in seconds.
fn anchor_timeout(boot: Boot) -> Duration {
    match boot {
        // ThreadX-Linux is a host process (no QEMU) but boots the ThreadX
        // kernel + chain-spawns the second tier — give it the middle budget.
        Boot::Native | Boot::ThreadxLinux => Duration::from_secs(20),
        Boot::ZephyrNativeSim => Duration::from_secs(60),
        // Freertos cells never take this path (serial proof).
        Boot::NuttxArm | Boot::NuttxRiscv | Boot::FreertosMps2 => Duration::from_secs(90),
    }
}

// =============================================================================
// The parametrized matrix consumer
// =============================================================================

/// THE realtime-tiers matrix consumer (phase-329 W1). Iterates every cell
/// `w1_consumer_of` assigns to `RealtimeTiers`, expands each to its execution
/// row(s) via [`exec_for`] (the `(Linux, Cpp)` cell yields two — component +
/// #124 rclcpp), and runs each in one process, catching per-row skips/failures
/// so one missing fixture never aborts the rest.
#[test]
fn realtime_tiers() {
    let cells: Vec<&MCell> = nros_tests::matrix::CELLS
        .iter()
        .filter(|c| {
            w1_consumer_of(c) == Some(W1Consumer::RealtimeTiers) && matches!(c.tier, MT::Runtime)
        })
        .collect();
    assert!(
        !cells.is_empty(),
        "matrix regression: no RealtimeTiers runtime cells for this consumer"
    );

    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut ran = 0usize;
    let mut skipped: Vec<String> = Vec::new();
    let mut out_of_lane: Vec<String> = Vec::new();
    let mut failed: Vec<String> = Vec::new();
    for c in &cells {
        // issue 0571 — narrow by LANE here, because no name filter can reach
        // inside one test. `lane-filter.sh native` excludes platform tokens in
        // binary and test NAMES; this binary has neither, so without this a
        // tier-1 host boots every QEMU image it happens to have lying around.
        if !nros_tests::lane_scope::admits(c.platform) {
            for exec in exec_for(c.platform, c.lang) {
                out_of_lane.push(nros_tests::lane_scope::skip_note(c.platform, exec.label));
            }
            continue;
        }
        for exec in exec_for(c.platform, c.lang) {
            ran += 1;
            let label = format!("{}/{}", plat_str(c.platform), exec.label);
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_one(c, &exec)));
            if let Err(p) = res {
                let msg = p
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "<non-string panic>".to_string());
                if nros_tests::skip_marker::is_skip(&msg) {
                    skipped.push(format!("{label}: {msg}"));
                } else {
                    failed.push(format!("{label}: {msg}"));
                }
            }
        }
    }
    std::panic::set_hook(prev_hook);

    // issue 0571 — say what was NOT run, always. A row that skipped because its
    // fixture is absent used to vanish into a green verdict unless EVERY row
    // skipped, so "1 of 16 ran" and "16 of 16 passed" printed the same thing.
    // That is what let issue 0572 sit unseen behind a 12-second PASS.
    println!(
        "realtime_tiers: {ran} row(s) ran, {} skipped, {} out of lane",
        skipped.len(),
        out_of_lane.len()
    );
    for note in out_of_lane.iter().chain(skipped.iter()) {
        println!("  - {note}");
    }

    assert!(
        failed.is_empty(),
        "realtime_tiers: {} of {} row(s) FAILED:\n  {}",
        failed.len(),
        ran,
        failed.join("\n  ")
    );
    if ran == 0 || skipped.len() == ran {
        nros_tests::skip!(
            "no realtime-tiers row RAN ({} skipped, {} out of lane):\n  {}",
            skipped.len(),
            out_of_lane.len(),
            skipped
                .iter()
                .chain(out_of_lane.iter())
                .cloned()
                .collect::<Vec<_>>()
                .join("\n  ")
        );
    }
}

/// Boot the workspace entry, observe both tiers, assert the 10 ms high tier
/// outruns the 100 ms low tier per the row's [`Proof`]. Panics with
/// `[SKIPPED] …` on an unmet precondition; the caller classifies.
fn run_one(pcell: &MCell, cell: &Exec) {
    let platform = plat_str(pcell.platform);
    let lang = cell.label;
    require_cell_env(cell.boot);

    let entry = (cell.resolver)().unwrap_or_else(|e| {
        nros_tests::skip!(
            "{} {} realtime workspace entry fixture not built: {e}",
            platform,
            lang
        )
    });

    // Router: ephemeral on native; otherwise the EXACT port the fixture's
    // locator was baked with (0.0.0.0 for slirp guests, whose gateway maps
    // to the host; 127.0.0.1 suffices for native_sim NSOS sockets).
    let router = match (cell.boot, cell.port) {
        (Boot::Native, _) => ZenohRouter::start_unique()
            .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start: {e}")),
        // Loopback suffices for native_sim NSOS sockets and the ThreadX-Linux
        // host process (both dial 127.0.0.1 directly — no slirp gateway).
        (Boot::ZephyrNativeSim | Boot::ThreadxLinux, Some(port)) => {
            ZenohRouter::start_on("127.0.0.1", port)
                .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {port}: {e}"))
        }
        (_, Some(port)) => ZenohRouter::start_on("0.0.0.0", port)
            .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {port}: {e}")),
        (_, None) => unreachable!("non-native cells carry a baked port"),
    };
    // Observers always dial the host loopback (the guest side dials the
    // slirp gateway / native_sim host address baked into the fixture).
    let observer_locator = format!("tcp/127.0.0.1:{}", router.port());

    // FreeRTOS: serial-tick proof, no host observers.
    if let Proof::SerialDispatch(tiers) = cell.proof {
        let mut qemu = QemuProcess::start_mps2_an385_freertos_slirp(&entry)
            .unwrap_or_else(|e| panic!("boot {} {} QEMU: {e}", platform, lang));
        // ORDER-INDEPENDENT, unlike `SerialTicks` above, and that is not
        // fussiness: `wait_for_output_pattern` CONSUMES the stream, so a
        // sequential wait silently misses a marker that already went past. The
        // boot tier here is `low` (100 ms) while `high` is 10 ms, so once both
        // are set up `high` publishes FIRST — waiting for `low` first ate
        // `high`'s line and the cell reported a tier that had in fact
        // dispatched. Accumulate instead, and only wait for what is not yet
        // seen; the assertion is "every tier dispatched", which says nothing
        // about the order they got there in.
        let mut seen = String::new();
        let mut timeout = Duration::from_secs(90);
        for tier in tiers {
            let marker = nros_tests::output::tier_dispatch_marker(tier);
            if seen.contains(&marker) {
                continue;
            }
            let out = qemu
                .wait_for_output_pattern(&marker, timeout)
                .unwrap_or_else(|e| {
                    qemu.kill();
                    panic!(
                        "[{} {}] tier `{tier}` never dispatched (`{marker}` absent) — \
                         {}.\nerr: {e:?}\n--- guest console so far ---\n{seen}",
                        platform, lang, cell.note
                    )
                });
            seen.push_str(&out);
            assert!(seen.contains(&marker));
            // The first marker carries the cold-boot budget (session open +
            // zenoh handshake); the rest only need their own period.
            timeout = Duration::from_secs(30);
        }
        qemu.kill();
        return;
    }
    if let Proof::SerialTicks(tiers) = cell.proof {
        let mut qemu = QemuProcess::start_mps2_an385_freertos_slirp(&entry)
            .unwrap_or_else(|e| panic!("boot {} {} QEMU: {e}", platform, lang));
        // The boot tier connects + publishes first (its tick proves the
        // run_tiers boot session reached the host zenohd), so it gets the
        // cold-boot budget; each subsequent tier only needs its own period.
        let mut timeout = Duration::from_secs(90);
        for tier in tiers {
            let marker = nros_tests::output::tier_tick_marker(tier);
            let out = qemu
                .wait_for_output_pattern(&marker, timeout)
                .unwrap_or_else(|e| {
                    qemu.kill();
                    panic!(
                        "[{} {}] tier `{tier}` never published (`{marker}` absent) — \
                         {}.\nerr: {e:?}",
                        platform, lang, cell.note
                    )
                });
            assert!(out.contains(&marker));
            timeout = Duration::from_secs(30);
        }
        qemu.kill();
        return;
    }

    // Observer cells: subscriptions live BEFORE the guest publishes.
    let mut ctrl = nros_tests::fixtures::spawn_int32_sink(Some("/ctrl"), &observer_locator);
    let mut telem = nros_tests::fixtures::spawn_int32_sink(Some("/telem"), &observer_locator);

    let mut guest = match cell.boot {
        Boot::Native => {
            let mut cmd = Command::new(&entry);
            cmd.env("RUST_LOG", "info")
                .env("NROS_LOCATOR", router.locator())
                .env("NROS_SESSION_MODE", "client")
                .env("NROS_ENTRY_SPIN_MS", "12000")
                .env("NROS_ENTRY_SPIN_STEP_MS", "5");
            Guest::Managed(
                ManagedProcess::spawn_command(cmd, "realtime-entry")
                    .unwrap_or_else(|e| panic!("spawn native realtime entry: {e}")),
            )
        }
        Boot::ZephyrNativeSim => Guest::Zephyr(
            ZephyrProcess::start(&entry, ZephyrPlatform::NativeSim)
                .unwrap_or_else(|e| panic!("boot zephyr native_sim: {e}")),
        ),
        Boot::NuttxArm => Guest::Qemu(
            QemuProcess::start_nuttx_virt(&entry, true)
                .unwrap_or_else(|e| panic!("boot NuttX arm-virt QEMU: {e}")),
        ),
        Boot::NuttxRiscv => Guest::Qemu(
            QemuProcess::start_nuttx_riscv(&entry, true)
                .unwrap_or_else(|e| panic!("boot NuttX rv-virt QEMU: {e}")),
        ),
        // Host process like Native, but on the baked allocator locator (the
        // ThreadX kernel never returns from tx_kernel_enter — killed below).
        Boot::ThreadxLinux => {
            let mut cmd = Command::new(&entry);
            cmd.env("RUST_LOG", "info");
            Guest::Managed(
                ManagedProcess::spawn_command(cmd, "realtime-threadx-entry")
                    .unwrap_or_else(|e| panic!("spawn threadx-linux realtime entry: {e}")),
            )
        }
        Boot::FreertosMps2 => unreachable!("freertos cells use SerialTicks"),
    };

    // Anchor on the SLOW tier: once telem (100 ms) has delivered 5 samples,
    // enough wall time (~0.5 s+) has elapsed that the 10 ms ctrl tier must
    // have published many more — both tiers live, high runs faster.
    let prefix = nros_tests::output::INT32_LISTENER_LOG_PREFIX;
    let telem_out = telem
        .wait_for_output_count(prefix, 5, anchor_timeout(cell.boot))
        .unwrap_or_else(|_| {
            // issue 0565 — this verdict used to throw away the ONE artifact that
            // says WHY. The guest prints its own diagnosis on this path
            // (`nros: FAILED to spawn tier <name> after N attempts — tier will
            // not run`, or the per-attempt `spawn tier … failed (… os error …)`),
            // and killing the guest unread meant "the low tier was not
            // scheduled" could not be told apart from "the tier never spawned".
            // Same rule as issue 0445: a verdict states what it examined.
            // Drain BEFORE killing: a live guest still has its console open.
            let tail = guest_console(&mut guest);
            guest.kill();
            ctrl.kill();
            telem.kill();
            panic!(
                "[{} {}] low-tier /telem never reached 5 deliveries — the low tier was \
                 not scheduled ({})\n         guest console:\n           {}",
                platform, lang, cell.note, tail
            )
        });

    match cell.proof {
        Proof::CounterRatio3x => {
            // #158 — stop the guest, then drain everything each observer
            // received; the deterministic proof reads the MONOTONIC payload
            // counter, not raw sample counts (delivery batching/drops under
            // scheduler/QEMU jitter distort counts, never the counter).
            // phase-351 — BEFORE the kill (issue 0565's rule, applied to this
            // arm too): #572 failed here with no guest evidence at all.
            let tail = guest_console(&mut guest);
            guest.kill();
            let ctrl_all = ctrl
                .wait_for_all_output(Duration::from_secs(3))
                .unwrap_or_default();
            let telem_all = format!(
                "{telem_out}{}",
                telem
                    .wait_for_all_output(Duration::from_secs(3))
                    .unwrap_or_default()
            );
            ctrl.kill();
            telem.kill();

            let telem_max = nros_tests::max_int_after(&telem_all, prefix).unwrap_or(0);
            let ctrl_max = nros_tests::max_int_after(&ctrl_all, prefix).unwrap_or(0);
            // The anchor already proved 5 low-tier samples; this guards
            // against a parse failure making the ratio vacuous (0-indexed
            // counter ⇒ 5 samples = max value 4 — assert advancement).
            assert!(
                telem_max > 0,
                "[{} {}] low-tier /telem counter never advanced (max {telem_max}) — the \
                 low tier did not run ({})\n         guest console:\n           {}",
                platform,
                lang,
                cell.note,
                tail
            );
            // Issue 0447 — dump the raw observer text, not just the counters.
            // `ctrl_max` is an `unwrap_or(0)`, so 0 means "nothing parsed",
            // which is equally consistent with the tier never publishing and
            // with the observer printing something `max_int_after` can't read.
            // The counters alone cannot tell those apart; one run of this can.
            assert!(
                ctrl_max >= 3 * telem_max,
                "[{} {}] high-tier /ctrl counter {ctrl_max} is not ≥3× the low-tier \
                 /telem counter {telem_max} — the 10 ms tier is not outrunning the \
                 100 ms tier ({})\n\
                 --- /ctrl observer output (empty ⇒ nothing was received at all) ---\n{}\n\
                 --- /telem observer output ---\n{}\n\
                 --- guest console ---\n           {}",
                platform,
                lang,
                cell.note,
                ctrl_all,
                telem_all,
                tail
            );
        }
        Proof::CountRatio3x | Proof::CountStrict => {
            let ctrl_out = ctrl
                .wait_for_output_count(prefix, 1, Duration::from_secs(2))
                .unwrap_or_else(|_| {
                    let tail = guest_console(&mut guest);
                    guest.kill();
                    ctrl.kill();
                    telem.kill();
                    panic!(
                        "[{} {}] high-tier /ctrl produced nothing — the high tier was \
                         not scheduled ({})\n         guest console:\n           {}",
                        platform, lang, cell.note, tail
                    )
                });
            let tail = guest_console(&mut guest);
            guest.kill();
            ctrl.kill();
            telem.kill();

            let telem_n = nros_tests::count_pattern(&telem_out, prefix);
            let ctrl_n = nros_tests::count_pattern(&ctrl_out, prefix);
            assert!(
                telem_n >= 5,
                "[{} {}] expected ≥5 low-tier /telem deliveries, got {telem_n} ({})\n\
                 --- guest console ---\n           {}",
                platform,
                lang,
                cell.note,
                tail
            );
            if matches!(cell.proof, Proof::CountRatio3x) {
                // 10 ms vs 100 ms ⇒ ~10×; a clear ≥3× margin stays robust
                // against native timer jitter and zenoh delivery batching.
                assert!(
                    ctrl_n >= telem_n * 3,
                    "[{} {}] expected the high tier (/ctrl, 10 ms) to deliver ≥3× the \
                     low tier (/telem, 100 ms): ctrl={ctrl_n} telem={telem_n} ({})\n\
                     --- guest console ---\n           {}",
                    platform,
                    lang,
                    cell.note,
                    tail
                );
            } else {
                assert!(
                    ctrl_n > telem_n,
                    "[{} {}] ctrl (10 ms tier) delivered {ctrl_n} ≤ telem's {telem_n} — \
                     the high tier is not outrunning the low tier ({})\n\
                     --- guest console ---\n           {}",
                    platform,
                    lang,
                    cell.note,
                    tail
                );
            }
        }
        Proof::SerialTicks(_) | Proof::SerialDispatch(_) => unreachable!("handled above"),
    }
}
