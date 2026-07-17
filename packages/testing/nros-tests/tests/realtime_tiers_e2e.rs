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
        build_int32_sink, build_native_workspace_c_realtime_entry,
        build_native_workspace_cpp_rclcpp_realtime_entry,
        build_native_workspace_cpp_realtime_entry, build_native_workspace_rust_realtime_entry,
        build_nuttx_riscv_workspace_c_realtime_entry,
        build_nuttx_riscv_workspace_cpp_realtime_entry,
        build_nuttx_riscv_workspace_rust_realtime_entry, build_nuttx_workspace_c_realtime_entry,
        build_nuttx_workspace_cpp_realtime_entry, build_nuttx_workspace_rust_realtime_entry,
        build_zephyr_workspace_c_realtime_entry, build_zephyr_workspace_cpp_realtime_entry,
        build_zephyr_workspace_rust_realtime_entry, freertos, is_qemu_available, require_zenohd,
    },
    matrix::{Lang as ML, PlatformId as MP, Workload as MW},
};
use rstest::rstest;
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
}

type Resolver = fn() -> TestResult<PathBuf>;

/// One realtime-tiers matrix cell.
struct Cell {
    platform: &'static str,
    lang: &'static str,
    resolver: Resolver,
    /// Baked router port — the allocator's number for the cell's
    /// coordinate (matches the fixture's baked locator). `None` =
    /// ephemeral (native).
    port: Option<u16>,
    boot: Boot,
    proof: Proof,
    /// Provenance / nuance — folded into failure messages so a red cell
    /// still names the seam it pins.
    note: &'static str,
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
}

// =============================================================================
// Shared helpers
// =============================================================================

/// Spawn a native `int32-sink` observer on `topic` (prints `Received: <n>`
/// per message) dialing `locator`.
fn spawn_listener(topic: &'static str, locator: &str) -> ManagedProcess {
    let listener = build_int32_sink()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("int32-sink fixture not built: {e}"));
    let mut cmd = Command::new(listener);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_SUB_TOPIC", topic);
    let mut proc =
        ManagedProcess::spawn_command(cmd, topic).unwrap_or_else(|e| panic!("spawn {topic}: {e}"));
    proc.wait_for_output_pattern(
        nros_tests::output::INT32_SINK_READY_MARKER,
        Duration::from_secs(10),
    )
    .unwrap_or_else(|_| panic!("{topic} listener did not become ready"));
    proc
}

/// Skip-precondition gate per boot mechanism (identical semantics to the
/// pre-consolidation files: missing fixture / west image / qemu → skip).
fn require_cell_env(boot: Boot) {
    match boot {
        Boot::Native | Boot::NuttxArm | Boot::NuttxRiscv | Boot::FreertosMps2 => {
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
        Boot::Native | Boot::ZephyrNativeSim => {}
    }
}

/// How long the slow-tier 5-sample anchor may take: QEMU guests need a
/// cold-boot + zenoh-discovery budget; native connects in seconds.
fn anchor_timeout(boot: Boot) -> Duration {
    match boot {
        Boot::Native => Duration::from_secs(20),
        Boot::ZephyrNativeSim => Duration::from_secs(60),
        // Freertos cells never take this path (serial proof).
        Boot::NuttxArm | Boot::NuttxRiscv | Boot::FreertosMps2 => Duration::from_secs(90),
    }
}

// =============================================================================
// The parametrized matrix consumer
// =============================================================================

/// One realtime-tiers cell: boot the workspace entry, observe both tiers,
/// assert the 10 ms high tier outruns the 100 ms low tier per the cell's
/// [`Proof`]. Case names carry `<platform>_<lang>` so nextest `test(...)`
/// filters can slice by platform (e.g. `test(nuttx_riscv)`).
#[rstest]
// Native (ephemeral router; posix sched contexts — RFC-0015 §4.2).
#[case::native_rust(Cell {
    platform: "native", lang: "rust", resolver: native_rust_entry,
    port: None, boot: Boot::Native, proof: Proof::CounterRatio3x,
    note: "phase-263 B2 `nros::main!` run_tiers (RFC-0032 §5); #158 counter proof",
})]
#[case::native_c(Cell {
    platform: "native", lang: "c", resolver: native_c_entry,
    port: None, boot: Boot::Native, proof: Proof::CountRatio3x,
    note: "phase-269 W4 C sched-context (nros_cpp_create_sched_context + node_create_ex)",
})]
#[case::native_cpp(Cell {
    platform: "native", lang: "cpp", resolver: native_cpp_entry,
    port: None, boot: Boot::Native, proof: Proof::CountRatio3x,
    note: "phase-269 W4 C++ configure-shape sched-context (NodeBuilder::sched())",
})]
#[case::native_cpp_rclcpp(Cell {
    platform: "native", lang: "cpp-rclcpp", resolver: native_cpp_rclcpp_entry,
    port: None, boot: Boot::Native, proof: Proof::CountRatio3x,
    note: "issue #124 / phase-272 W3: IS-A-node rclcpp-shape components bind via the \
           node_name → sched_context table at Executor::node_builder — a miss here \
           means rclcpp-shape nodes lost their tier again",
})]
// Zephyr native_sim (west lane; ZephyrBoard::run_tiers, one k_thread/tier).
#[case::zephyr_rust(Cell {
    platform: "zephyr", lang: "rust", resolver: build_zephyr_workspace_rust_realtime_entry,
    port: Some(port_of(MP::ZephyrNativeSim, ML::Rust, MW::RealtimeTiers)),
    boot: Boot::ZephyrNativeSim, proof: Proof::CountStrict,
    note: "phase-276 W2 / #128 half 2: ZephyrBoard::run_tiers (RFC-0015 Model 1)",
})]
#[case::zephyr_cpp(Cell {
    platform: "zephyr", lang: "cpp", resolver: build_zephyr_workspace_cpp_realtime_entry,
    port: Some(port_of(MP::ZephyrNativeSim, ML::Cpp, MW::RealtimeTiers)),
    boot: Boot::ZephyrNativeSim, proof: Proof::CountStrict,
    note: "phase-281 W3b: first full west link + runtime proof of the run_tiers seam",
})]
#[case::zephyr_c(Cell {
    platform: "zephyr", lang: "c", resolver: build_zephyr_workspace_c_realtime_entry,
    port: Some(port_of(MP::ZephyrNativeSim, ML::C, MW::RealtimeTiers)),
    boot: Boot::ZephyrNativeSim, proof: Proof::CountStrict,
    note: "phase-281 W3c: C nodes over the shared ZephyrBoard::run_tiers glue",
})]
// NuttX QEMU arm-virt (NuttxBoard::run_tiers, one SCHED_FIFO pthread/tier).
#[case::nuttx_arm_cpp(Cell {
    platform: "nuttx-arm", lang: "cpp", resolver: nuttx_cpp_entry,
    port: Some(port_of(MP::NuttxArm, ML::Cpp, MW::RealtimeTiers)), boot: Boot::NuttxArm, proof: Proof::CounterRatio3x,
    note: "phase-281 W3-nuttx: NuttxBoard::run_tiers (commit 37cfaf728)",
})]
#[case::nuttx_arm_c(Cell {
    platform: "nuttx-arm", lang: "c", resolver: nuttx_c_entry,
    port: Some(port_of(MP::NuttxArm, ML::C, MW::RealtimeTiers)), boot: Boot::NuttxArm, proof: Proof::CounterRatio3x,
    note: "phase-281 W3-nuttx: pure-C lane over NuttxBoard::run_tiers",
})]
#[case::nuttx_arm_rust(Cell {
    platform: "nuttx-arm", lang: "rust", resolver: nuttx_rust_entry,
    port: Some(port_of(MP::NuttxArm, ML::Rust, MW::RealtimeTiers)), boot: Boot::NuttxArm, proof: Proof::CounterRatio3x,
    note: "phase-281 W3-nuttx: QemuArmVirt::run_tiers (std::thread per tier), \
           the cell that completed the 12-cell Model-1 matrix",
})]
// NuttX QEMU rv-virt riscv32 (#199 follow-ups; -icount boot profile).
#[case::nuttx_riscv_rust(Cell {
    platform: "nuttx-riscv", lang: "rust", resolver: nuttx_riscv_rust_entry,
    port: Some(port_of(MP::NuttxRiscv, ML::Rust, MW::RealtimeTiers)), boot: Boot::NuttxRiscv, proof: Proof::CounterRatio3x,
    note: "phase-285 W6 / #165: QemuRvVirt::run_tiers",
})]
#[case::nuttx_riscv_c(Cell {
    platform: "nuttx-riscv", lang: "c", resolver: nuttx_riscv_c_entry,
    port: Some(port_of(MP::NuttxRiscv, ML::C, MW::RealtimeTiers)), boot: Boot::NuttxRiscv, proof: Proof::CounterRatio3x,
    note: "#199 follow-up: C riscv_nuttx_entry over NuttxBoard::run_tiers",
})]
#[case::nuttx_riscv_cpp(Cell {
    platform: "nuttx-riscv", lang: "cpp", resolver: nuttx_riscv_cpp_entry,
    port: Some(port_of(MP::NuttxRiscv, ML::Cpp, MW::RealtimeTiers)), boot: Boot::NuttxRiscv, proof: Proof::CounterRatio3x,
    note: "#199 follow-up: C++ riscv_nuttx_entry over NuttxBoard::run_tiers",
})]
// FreeRTOS QEMU mps2-an385 (FreertosBoard::run_tiers; serial-tick proof).
#[case::freertos_cpp(Cell {
    platform: "freertos", lang: "cpp", resolver: freertos_cpp_entry,
    port: Some(port_of(MP::FreertosMps2, ML::Cpp, MW::RealtimeTiers)),
    boot: Boot::FreertosMps2,
    // THREE tiers — [aux] (50 ms, spawned BY a spawned tier) is the #144
    // chained-spawn regression signal: under the pre-fix loop-spawn race
    // two tiers declared concurrently and aux's publisher write filter
    // stayed closed (no ticks). Order matters: boot tier first.
    proof: Proof::SerialTicks(&["ctrl", "aux", "telem"]),
    note: "phase-274 W3 (#126) + #144 chained tier spawn: ctrl(10ms)/aux(50ms)/telem(100ms)",
})]
#[case::freertos_c(Cell {
    platform: "freertos", lang: "c", resolver: freertos_c_entry,
    port: Some(port_of(MP::FreertosMps2, ML::C, MW::RealtimeTiers)),
    boot: Boot::FreertosMps2,
    proof: Proof::SerialTicks(&["ctrl", "telem"]),
    note: "phase-281 W2: C nodes over the SHARED nros_board_freertos_run_tiers glue \
           (codegen routes embedded-C via the C++ emitter + NROS_C_COMPONENT seam)",
})]
fn realtime_tiers(#[case] cell: Cell) {
    require_cell_env(cell.boot);

    let entry = (cell.resolver)().unwrap_or_else(|e| {
        nros_tests::skip!(
            "{} {} realtime workspace entry fixture not built: {e}",
            cell.platform,
            cell.lang
        )
    });

    // Router: ephemeral on native; otherwise the EXACT port the fixture's
    // locator was baked with (0.0.0.0 for slirp guests, whose gateway maps
    // to the host; 127.0.0.1 suffices for native_sim NSOS sockets).
    let router = match (cell.boot, cell.port) {
        (Boot::Native, _) => ZenohRouter::start_unique()
            .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start: {e}")),
        (Boot::ZephyrNativeSim, Some(port)) => ZenohRouter::start_on("127.0.0.1", port)
            .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {port}: {e}")),
        (_, Some(port)) => ZenohRouter::start_on("0.0.0.0", port)
            .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {port}: {e}")),
        (_, None) => unreachable!("non-native cells carry a baked port"),
    };
    // Observers always dial the host loopback (the guest side dials the
    // slirp gateway / native_sim host address baked into the fixture).
    let observer_locator = format!("tcp/127.0.0.1:{}", router.port());

    // FreeRTOS: serial-tick proof, no host observers.
    if let Proof::SerialTicks(tiers) = cell.proof {
        let mut qemu = QemuProcess::start_mps2_an385_freertos_slirp(&entry)
            .unwrap_or_else(|e| panic!("boot {} {} QEMU: {e}", cell.platform, cell.lang));
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
                        cell.platform, cell.lang, cell.note
                    )
                });
            assert!(out.contains(&marker));
            timeout = Duration::from_secs(30);
        }
        qemu.kill();
        return;
    }

    // Observer cells: subscriptions live BEFORE the guest publishes.
    let mut ctrl = spawn_listener("/ctrl", &observer_locator);
    let mut telem = spawn_listener("/telem", &observer_locator);

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
        Boot::FreertosMps2 => unreachable!("freertos cells use SerialTicks"),
    };

    // Anchor on the SLOW tier: once telem (100 ms) has delivered 5 samples,
    // enough wall time (~0.5 s+) has elapsed that the 10 ms ctrl tier must
    // have published many more — both tiers live, high runs faster.
    let prefix = nros_tests::output::INT32_LISTENER_LOG_PREFIX;
    let telem_out = telem
        .wait_for_output_count(prefix, 5, anchor_timeout(cell.boot))
        .unwrap_or_else(|_| {
            guest.kill();
            ctrl.kill();
            telem.kill();
            panic!(
                "[{} {}] low-tier /telem never reached 5 deliveries — the low tier was \
                 not scheduled ({})",
                cell.platform, cell.lang, cell.note
            )
        });

    match cell.proof {
        Proof::CounterRatio3x => {
            // #158 — stop the guest, then drain everything each observer
            // received; the deterministic proof reads the MONOTONIC payload
            // counter, not raw sample counts (delivery batching/drops under
            // scheduler/QEMU jitter distort counts, never the counter).
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
                 low tier did not run ({})",
                cell.platform,
                cell.lang,
                cell.note
            );
            assert!(
                ctrl_max >= 3 * telem_max,
                "[{} {}] high-tier /ctrl counter {ctrl_max} is not ≥3× the low-tier \
                 /telem counter {telem_max} — the 10 ms tier is not outrunning the \
                 100 ms tier ({})",
                cell.platform,
                cell.lang,
                cell.note
            );
        }
        Proof::CountRatio3x | Proof::CountStrict => {
            let ctrl_out = ctrl
                .wait_for_output_count(prefix, 1, Duration::from_secs(2))
                .unwrap_or_else(|_| {
                    guest.kill();
                    ctrl.kill();
                    telem.kill();
                    panic!(
                        "[{} {}] high-tier /ctrl produced nothing — the high tier was \
                         not scheduled ({})",
                        cell.platform, cell.lang, cell.note
                    )
                });
            guest.kill();
            ctrl.kill();
            telem.kill();

            let telem_n = nros_tests::count_pattern(&telem_out, prefix);
            let ctrl_n = nros_tests::count_pattern(&ctrl_out, prefix);
            assert!(
                telem_n >= 5,
                "[{} {}] expected ≥5 low-tier /telem deliveries, got {telem_n} ({})",
                cell.platform,
                cell.lang,
                cell.note
            );
            if matches!(cell.proof, Proof::CountRatio3x) {
                // 10 ms vs 100 ms ⇒ ~10×; a clear ≥3× margin stays robust
                // against native timer jitter and zenoh delivery batching.
                assert!(
                    ctrl_n >= telem_n * 3,
                    "[{} {}] expected the high tier (/ctrl, 10 ms) to deliver ≥3× the \
                     low tier (/telem, 100 ms): ctrl={ctrl_n} telem={telem_n} ({})",
                    cell.platform,
                    cell.lang,
                    cell.note
                );
            } else {
                assert!(
                    ctrl_n > telem_n,
                    "[{} {}] ctrl (10 ms tier) delivered {ctrl_n} ≤ telem's {telem_n} — \
                     the high tier is not outrunning the low tier ({})",
                    cell.platform,
                    cell.lang,
                    cell.note
                );
            }
        }
        Proof::SerialTicks(_) => unreachable!("handled above"),
    }
}
