//! phase-295 W3.b — THE Entry-pkg boot/delivery matrix consumer (RFC-0051).
//!
//! Consolidates the 15 per-cell embedded Entry files —
//! `{c_,cpp_,mixed_}threadx_entry_e2e`, `{c_,cpp_,mixed_}freertos_entry_e2e`,
//! `{,cpp_,mixed_}zephyr_entry_e2e`, `{c_,rust_}nuttx_entry_e2e`, and the
//! zephyr feature-workspace entries
//! `{params,qos,lifecycle,safety}_zephyr_entry_e2e` — into one parametrized
//! test over the RTOS `Workload::{EntryPubsub,Params,Qos,Lifecycle,Safety}`
//! workspace cells of the test matrix (`nros_tests::matrix`).
//!
//! Every cell boots a compile-time-locator Entry image (the embedded
//! domain/locator rule — never a runtime env) and proves its workload's
//! contract per the cell's [`Proof`], preserved 1:1 from the per-cell files:
//! - **EntryPubsub** (phase-263 C2a–C2d): the image's `demo_bringup` talker
//!   delivers `/chatter` CROSS-PROCESS (issue 0096 — an entry's own listener
//!   can't observe its in-image talker) to a SEPARATE native listener
//!   through a host zenohd on the baked port. The C-family cells observe via
//!   the language-agnostic C `native_entry_robot2` listener entry
//!   (`Received:`); the rust nuttx cell (#130) keeps its native Rust
//!   String listener (`I heard:`).
//! - **Params** (phase-276 W1 / #128): the zephyr `ws-params-rust` entry
//!   LIVE-reads its launch-baked `publish_period_ms` initial (250) in the
//!   node callback and publishes it — the `int32-sink` must see
//!   `Received: 250`, proving bake → store seed (`apply_param_services`) →
//!   on-target live read.
//! - **Qos** (phase-276 W5): the on-target reliable+transient_local pair
//!   matches + delivers in-image (`Z_FEATURE_LOCAL_SUBSCRIBER`); the
//!   listener's republished count on `/qos_ok` reaches an external sink.
//! - **Lifecycle** (phase-276 W3 / #128): `apply_lifecycle` autostart drives
//!   Configure→Activate at boot, observed over the REP-2002 service surface
//!   (`ros2 lifecycle …`; requires ROS 2 + rmw_zenoh_cpp, skips when absent).
//! - **Safety** (phase-276 W4): the CRC attach → in-image deliver → validate
//!   chain republishes the CRC-valid count on `/safe_ok` to an external sink.
//!
//! Skip semantics are identical to the per-cell files: missing fixture /
//! west image / QEMU / RTOS trees / zenohd → `nros_tests::skip!`. Note the
//! historical gate asymmetry is preserved: the qos/safety cells never probed
//! zenohd up front (their router-start skip covers it) and the lifecycle
//! cell gates on ROS 2 instead.
//!
//! NOTE (phase-295 W4): the `port` column below mirrors the locator bakes in
//! `examples/fixtures.toml` (`NROS_ENTRY_LOCATOR` cmake_defs) and the west
//! lane (`scripts/build/zephyr-fixture-leaves.sh`
//! `-DCONFIG_NROS_ZENOH_LOCATOR` bakes). W4 re-bakes them through the matrix
//! allocator; until then this table is the mirror, not the source of truth.
//!
//! Run with: `cargo nextest run -p nros-tests --test entry_e2e`
//! (filter one platform: `-E 'binary(entry_e2e) and test(zephyr)'`).

use nros_tests::{
    TestResult,
    fixtures::{
        ManagedProcess, QemuProcess, ZenohRouter, ZephyrPlatform, ZephyrProcess,
        build_freertos_workspace_c_entry, build_freertos_workspace_cpp_entry,
        build_freertos_workspace_mixed_entry, build_int32_sink, build_native_listener,
        build_native_workspace_c_entry_robot2, build_nuttx_workspace_c_entry,
        build_threadx_linux_workspace_c_entry, build_threadx_linux_workspace_cpp_entry,
        build_threadx_linux_workspace_mixed_entry, build_zephyr_workspace_c_entry,
        build_zephyr_workspace_cpp_entry, build_zephyr_workspace_mixed_entry,
        build_zephyr_workspace_rust_lifecycle_entry, build_zephyr_workspace_rust_params_entry,
        build_zephyr_workspace_rust_qos_entry, build_zephyr_workspace_rust_safety_entry, freertos,
        is_qemu_available, nuttx, require_zenohd,
        threadx_linux::{is_nsos_netx_available, is_threadx_available},
    },
    ros2::{DEFAULT_ROS_DISTRO, require_ros2, ros2_env_setup_with_locator},
};
use rstest::rstest;
use std::{
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

// =============================================================================
// Cell table types
// =============================================================================

/// How the cell's guest boots (and which environment gates apply).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Boot {
    /// ThreadX-on-Linux host sim: the entry is a NATIVE process (the booted
    /// ELF *is* the ThreadX kernel; nsos-netx forwards to host sockets), so
    /// it takes a bounded `NROS_ENTRY_SPIN_MS` and exits.
    ThreadxLinux,
    /// FreeRTOS QEMU mps2-an385 (static 192.0.3.x lwIP + board-matching
    /// slirp net whose host 192.0.3.1 forwards to the host machine).
    FreertosMps2,
    /// Zephyr native_sim image (west-lane fixture; NSOS host sockets reach
    /// a `tcp/127.0.0.1` locator with no bridge / root).
    ZephyrNativeSim,
    /// NuttX QEMU arm-virt guest (slirp gateway 10.0.2.2 → host router).
    NuttxArm,
}

/// The per-cell workload contract, preserved 1:1 from the
/// pre-consolidation files.
#[derive(Copy, Clone, Debug)]
enum Proof {
    /// EntryPubsub via the C `native_entry_robot2` listener entry: ≥3
    /// `Received:` lines. `observer_spin_ms` / `window_secs` keep each
    /// lane's historical budgets (threadx 15 s/20 s; QEMU + native_sim
    /// 60 s/90 s cold-boot windows).
    ChatterToCListener {
        observer_spin_ms: u32,
        window_secs: u64,
    },
    /// EntryPubsub via the native Rust String listener (#130 rust nuttx
    /// cell): ≥3 `I heard:` lines.
    ChatterToRustListener { window_secs: u64 },
    /// Params: the `int32-sink` on `/chatter` must log the exact
    /// `Received: <value>` live-read line ≥3 times.
    SinkValueLine { value: i64 },
    /// Qos/Safety: the `int32-sink` on `topic` must log ≥3 republished
    /// counts.
    SinkCount { topic: &'static str },
    /// Lifecycle: `ros2 lifecycle get` on the discovered managed node must
    /// report `active` with no manual transition.
    LifecycleActive,
}

type Resolver = fn() -> TestResult<PathBuf>;

/// One Entry-pkg matrix cell.
struct Cell {
    platform: &'static str,
    lang: &'static str,
    workload: &'static str,
    resolver: Resolver,
    /// Baked router port (mirrors the fixture's locator bake until the
    /// phase-295 W4 allocator re-bake) — verified against
    /// `examples/fixtures.toml` / `zephyr-fixture-leaves.sh`.
    port: u16,
    boot: Boot,
    proof: Proof,
    /// Provenance / nuance — folded into failure messages so a red cell
    /// still names the seam it pins.
    note: &'static str,
}

// Resolver adapters: normalize the `&'static Path` builders onto the
// `PathBuf`-returning zephyr shape so one fn-pointer column fits all.
fn threadx_c_entry() -> TestResult<PathBuf> {
    build_threadx_linux_workspace_c_entry().map(|p| p.to_path_buf())
}
fn threadx_cpp_entry() -> TestResult<PathBuf> {
    build_threadx_linux_workspace_cpp_entry().map(|p| p.to_path_buf())
}
fn threadx_mixed_entry() -> TestResult<PathBuf> {
    build_threadx_linux_workspace_mixed_entry().map(|p| p.to_path_buf())
}
fn freertos_c_entry() -> TestResult<PathBuf> {
    build_freertos_workspace_c_entry().map(|p| p.to_path_buf())
}
fn freertos_cpp_entry() -> TestResult<PathBuf> {
    build_freertos_workspace_cpp_entry().map(|p| p.to_path_buf())
}
fn freertos_mixed_entry() -> TestResult<PathBuf> {
    build_freertos_workspace_mixed_entry().map(|p| p.to_path_buf())
}
fn nuttx_c_entry() -> TestResult<PathBuf> {
    build_nuttx_workspace_c_entry().map(|p| p.to_path_buf())
}
fn nuttx_rust_entry() -> TestResult<PathBuf> {
    // Prebuilt standalone `nros::main!` Entry-pkg demo image (#130) —
    // consume-only, exactly like the old rust_nuttx_entry_e2e.rs.
    nuttx::require_entry_binary("talker", "nuttx_rs_talker_entry")
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

/// Skip-precondition gates, identical to the pre-consolidation files.
/// Historical asymmetry preserved: the qos/safety cells never probed zenohd
/// up front (the router-start skip covers a missing zenohd) and the
/// lifecycle cell gates on ROS 2 instead.
fn require_cell_env(cell: &Cell) {
    match cell.proof {
        Proof::LifecycleActive => {
            if !require_ros2() {
                nros_tests::skip!(
                    "ROS 2 / rmw_zenoh_cpp not available — run: just rmw_zenoh setup"
                );
            }
        }
        Proof::SinkCount { .. } => {}
        Proof::ChatterToCListener { .. }
        | Proof::ChatterToRustListener { .. }
        | Proof::SinkValueLine { .. } => {
            if !require_zenohd() {
                nros_tests::skip!("zenohd not found");
            }
        }
    }
    match cell.boot {
        Boot::ThreadxLinux => {
            if !is_threadx_available() {
                nros_tests::skip!("THREADX_DIR not set or invalid");
            }
            if !is_nsos_netx_available() {
                nros_tests::skip!("nsos-netx not found at packages/drivers/nsos-netx/");
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
        Boot::NuttxArm => {
            if !is_qemu_available() {
                nros_tests::skip!("qemu-system-arm not found");
            }
        }
        Boot::ZephyrNativeSim => {}
    }
}

/// Spawn the C `native_entry_robot2` listener entry (a language-agnostic
/// `/chatter` subscriber on the wire) and block until its subscription is
/// live.
fn spawn_c_listener_observer(locator: &str, spin_ms: u32) -> ManagedProcess {
    let observer = build_native_workspace_c_entry_robot2()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native C listener entry fixture not built: {e}"));
    let mut cmd = Command::new(observer);
    cmd.env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_ENTRY_SPIN_MS", spin_ms.to_string());
    let mut obs = ManagedProcess::spawn_command(cmd, "native-observer")
        .unwrap_or_else(|e| panic!("spawn observer: {e}"));
    obs.wait_for_output_pattern(
        nros_tests::output::WS_C_LISTENER_READY_MARKER,
        Duration::from_secs(10),
    )
    .unwrap_or_else(|_| {
        obs.kill();
        panic!("native observer listener never became ready")
    });
    obs
}

/// Spawn the native Rust String listener (`examples/native/rust/listener`)
/// and block until it is ready. `RUST_LOG=info` is required for its
/// `info!("I heard: …")` line to surface.
fn spawn_rust_listener_observer(locator: &str) -> ManagedProcess {
    let observer = build_native_listener()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("native Rust listener fixture not built: {e}"));
    let mut cmd = Command::new(observer);
    cmd.env("RUST_LOG", "info").env("NROS_LOCATOR", locator);
    let mut obs = ManagedProcess::spawn_command(cmd, "native-observer")
        .unwrap_or_else(|e| panic!("spawn observer: {e}"));
    obs.wait_for_output_pattern("Waiting for", Duration::from_secs(10))
        .unwrap_or_else(|_| {
            obs.kill();
            panic!("native observer listener never became ready")
        });
    obs
}

/// Spawn the `int32-sink` fixture on `topic` (prints `Received: <n>` per
/// message) and block until its subscription is live.
fn spawn_int32_sink(topic: &'static str, locator: &str) -> ManagedProcess {
    let sink = build_int32_sink()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|e| nros_tests::skip!("int32-sink fixture not built: {e}"));
    let mut cmd = Command::new(sink);
    cmd.env("RUST_LOG", "info")
        .env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        .env("NROS_SUB_TOPIC", topic);
    let mut obs = ManagedProcess::spawn_command(cmd, "int32-sink")
        .unwrap_or_else(|e| panic!("spawn int32-sink: {e}"));
    obs.wait_for_output_pattern(
        nros_tests::output::INT32_SINK_READY_MARKER,
        Duration::from_secs(10),
    )
    .unwrap_or_else(|_| {
        obs.kill();
        panic!("int32-sink never became ready")
    });
    obs
}

/// Boot the cell's guest image.
fn boot_guest(cell: &Cell, entry: &PathBuf) -> Guest {
    match cell.boot {
        Boot::ThreadxLinux => {
            // The embedded locator is COMPILE-TIME baked, so NROS_LOCATOR is
            // ignored; only the bounded spin is threaded so the process exits.
            let mut cmd = Command::new(entry);
            cmd.env("NROS_ENTRY_SPIN_MS", "12000");
            Guest::Managed(
                ManagedProcess::spawn_command(cmd, "threadx-entry")
                    .unwrap_or_else(|e| panic!("spawn threadx entry: {e}")),
            )
        }
        Boot::FreertosMps2 => Guest::Qemu(
            QemuProcess::start_mps2_an385_freertos_slirp(entry)
                .unwrap_or_else(|e| panic!("boot freertos QEMU: {e}")),
        ),
        Boot::ZephyrNativeSim => Guest::Zephyr(
            ZephyrProcess::start(entry, ZephyrPlatform::NativeSim)
                .unwrap_or_else(|e| panic!("boot zephyr native_sim: {e}")),
        ),
        Boot::NuttxArm => Guest::Qemu(
            QemuProcess::start_nuttx_virt(entry, true)
                .unwrap_or_else(|e| panic!("boot NuttX QEMU: {e}")),
        ),
    }
}

/// Run `ros2 <subcommand>` against `locator`; return combined stdout+stderr.
fn run_ros2(locator: &str, subcommand: &str) -> String {
    let (env, _config_guard) = ros2_env_setup_with_locator(DEFAULT_ROS_DISTRO, locator);
    let script = format!("{env} && timeout 10 ros2 {subcommand} 2>&1");
    let out = Command::new("bash")
        .args(["-c", &script])
        .output()
        .expect("failed to spawn bash for ros2 invocation");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Poll `ros2 <subcommand>` until its output contains `marker`
/// (case-insensitive) or timeout.
fn poll_ros2_until(locator: &str, subcommand: &str, marker: &str, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    let marker_lc = marker.to_lowercase();
    let mut last = String::new();
    while Instant::now() < deadline {
        last = run_ros2(locator, subcommand);
        if last.to_lowercase().contains(&marker_lc) {
            return last;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    last
}

/// First `/`-prefixed node name in `ros2 lifecycle nodes` output, if any.
fn first_lifecycle_node(nodes_out: &str) -> Option<String> {
    nodes_out
        .lines()
        .map(|l| l.trim())
        .find(|l| l.starts_with('/'))
        .map(|l| l.to_string())
}

// =============================================================================
// The parametrized matrix consumer
// =============================================================================

/// One Entry-pkg cell: boot the compile-time-locator entry image against a
/// router on its baked port and prove the workload contract per the cell's
/// [`Proof`]. Case names carry `<platform>_<lang>[_<workload>]` so nextest
/// `test(...)` filters can slice (e.g. `test(nuttx_arm)`,
/// `test(zephyr_rust_qos)` — the `.config/nextest.toml` groups key on these).
#[rstest]
// ThreadX-on-Linux (phase-263 C2a — the FIRST embedded LAUNCH entry, issue
// 0097): startup.c::main → tx_kernel_enter → app thread → app_main →
// ThreadxBoard::run_components → nros::init(baked locator) → spin.
#[case::threadx_linux_c(Cell {
    platform: "threadx-linux", lang: "c", workload: "entry_pubsub",
    resolver: threadx_c_entry, port: 17553, boot: Boot::ThreadxLinux,
    proof: Proof::ChatterToCListener { observer_spin_ms: 15000, window_secs: 20 },
    note: "phase-263 C2a: codegen emits nros_app_main (NOT int main — would double-main \
           the ThreadX startup.c); nsos-netx forwards nx_bsd_connect to a host connect()",
})]
#[case::threadx_linux_cpp(Cell {
    platform: "threadx-linux", lang: "cpp", workload: "entry_pubsub",
    resolver: threadx_cpp_entry, port: 17803, boot: Boot::ThreadxLinux,
    proof: Proof::ChatterToCListener { observer_spin_ms: 15000, window_secs: 20 },
    note: "phase-263 C2c: C2a's per-board wiring (locator bake, header-mirror ordering) \
           reused verbatim through the C++ emitter",
})]
#[case::threadx_linux_mixed(Cell {
    platform: "threadx-linux", lang: "mixed", workload: "entry_pubsub",
    resolver: threadx_mixed_entry, port: 17821, boot: Boot::ThreadxLinux,
    proof: Proof::ChatterToCListener { observer_spin_ms: 15000, window_secs: 20 },
    note: "phase-263 C2c: C talker + C++ listener + Rust heartbeat in ONE image; the \
           nros_ws_runtime umbrella targets the host triple (ThreadX sim = pthreads)",
})]
// FreeRTOS QEMU mps2-an385 (phase-263 C2b/C2c — the first QEMU-cross
// embedded entries; static 192.0.3.x lwIP, board-matching slirp net).
#[case::freertos_c(Cell {
    platform: "freertos", lang: "c", workload: "entry_pubsub",
    resolver: freertos_c_entry, port: 17601, boot: Boot::FreertosMps2,
    proof: Proof::ChatterToCListener { observer_spin_ms: 60000, window_secs: 90 },
    note: "phase-263 C2b: nros_app_main + FreertosBoard::run_components; startup.c _start \
           spawns the app task, brings up netif + zenoh/poll tasks, dispatches app_main",
})]
#[case::freertos_cpp(Cell {
    platform: "freertos", lang: "cpp", workload: "entry_pubsub",
    resolver: freertos_cpp_entry, port: 17811, boot: Boot::FreertosMps2,
    proof: Proof::ChatterToCListener { observer_spin_ms: 60000, window_secs: 90 },
    note: "phase-263 C2c: C2b's per-board wiring reused verbatim through the C++ emitter",
})]
#[case::freertos_mixed(Cell {
    platform: "freertos", lang: "mixed", workload: "entry_pubsub",
    resolver: freertos_mixed_entry, port: 17841, boot: Boot::FreertosMps2,
    proof: Proof::ChatterToCListener { observer_spin_ms: 60000, window_secs: 90 },
    note: "phase-263 C2c: GENUINELY-no_std cross target (thumbv7m) — the umbrella selects \
           the board's alloc;panic-halt tier and Corrosion cross-compiles it",
})]
// Zephyr native_sim (phase-263 C2c/C2d — west lane; nano_ros_entry's zephyr
// branch puts the generated entry TU into `app`, the C/C++ analog of
// zephyr-lang-rust's rust_cargo_application; CONFIG_NROS_ZENOH_LOCATOR bake).
#[case::zephyr_c(Cell {
    platform: "zephyr", lang: "c", workload: "entry_pubsub",
    resolver: build_zephyr_workspace_c_entry, port: 17831, boot: Boot::ZephyrNativeSim,
    proof: Proof::ChatterToCListener { observer_spin_ms: 60000, window_secs: 90 },
    note: "phase-263 C2d: generated int main driving ZephyrBoard::run_components, \
           whole-archived into `app` (strong main)",
})]
#[case::zephyr_cpp(Cell {
    platform: "zephyr", lang: "cpp", workload: "entry_pubsub",
    resolver: build_zephyr_workspace_cpp_entry, port: 17833, boot: Boot::ZephyrNativeSim,
    proof: Proof::ChatterToCListener { observer_spin_ms: 60000, window_secs: 90 },
    note: "phase-263 C2c: TYPED std_msgs::msg::Int32 nodes — idempotent interface \
           generator, ::setvbuf (std::setvbuf absent on picolibc), if(TARGET)-guarded \
           interface link",
})]
#[case::zephyr_mixed(Cell {
    platform: "zephyr", lang: "mixed", workload: "entry_pubsub",
    resolver: build_zephyr_workspace_mixed_entry, port: 17843, boot: Boot::ZephyrNativeSim,
    proof: Proof::ChatterToCListener { observer_spin_ms: 60000, window_secs: 90 },
    note: "phase-263 C2c-zephyr: NROS_WS_RUST_NODE_DIRS before find_package(Zephyr) → the \
           module synthesises the nros_ws_runtime umbrella (single-runtime invariant: one \
           nros-rmw-cffi REGISTRY) in place of plain nros-cpp",
})]
// NuttX QEMU arm-virt (kernel-linked entries; slirp gateway 10.0.2.2).
#[case::nuttx_arm_c(Cell {
    platform: "nuttx-arm", lang: "c", workload: "entry_pubsub",
    resolver: nuttx_c_entry, port: 17861, boot: Boot::NuttxArm,
    proof: Proof::ChatterToCListener { observer_spin_ms: 60000, window_secs: 90 },
    note: "phase-263 C2b (last C2 gap): NROS_ENTRY_LOCATOR must be set BEFORE the board's \
           nros_platform_link_app (ferried into the cc-rs entry-TU compile at CONFIGURE \
           time) — the old 'console issue' was this missing bake",
})]
#[case::nuttx_arm_rust(Cell {
    platform: "nuttx-arm", lang: "rust", workload: "entry_pubsub",
    resolver: nuttx_rust_entry, port: 7452, boot: Boot::NuttxArm,
    proof: Proof::ChatterToRustListener { window_secs: 90 },
    note: "#130 / phase-280 W3 (commit 703e840dd): the Rust entry path's entry_net_init \
           pushes the guest IP into eth0 via SIOCSIFADDR before Executor::open — without \
           it the image dies in Transport(ConnectionFailed)",
})]
// Zephyr feature-workspace entries (phase-276 #102 H1 — the rust ws-* cells).
#[case::zephyr_rust_params(Cell {
    platform: "zephyr", lang: "rust", workload: "params",
    resolver: build_zephyr_workspace_rust_params_entry, port: 17845,
    boot: Boot::ZephyrNativeSim,
    proof: Proof::SinkValueLine { value: 250 },
    note: "phase-276 W1 / #128: the Framework::Zephyr emit arm gained apply_param_services \
           (launch-baked initials) — before it, [param_services] was silently ignored. \
           #147/#278: the observer must be the TYPED int32-sink (the old String listener \
           only matched while its fixture was a stale pre-W4 Int32 build)",
})]
#[case::zephyr_rust_qos(Cell {
    platform: "zephyr", lang: "rust", workload: "qos",
    resolver: build_zephyr_workspace_rust_qos_entry, port: 17849,
    boot: Boot::ZephyrNativeSim,
    proof: Proof::SinkCount { topic: "/qos_ok" },
    note: "phase-276 W5 (RFC-0041): per-entity reliable+transient_local declared IN NODE \
           CODE on both on-target endpoints; in-image delivery rides \
           Z_FEATURE_LOCAL_SUBSCRIBER. Port 17849 shared with qos_zephyr_ros2_interop_e2e \
           (issue #141 — the zephyr-qos-port nextest group serializes them)",
})]
#[case::zephyr_rust_lifecycle(Cell {
    platform: "zephyr", lang: "rust", workload: "lifecycle",
    resolver: build_zephyr_workspace_rust_lifecycle_entry, port: 17847,
    boot: Boot::ZephyrNativeSim,
    proof: Proof::LifecycleActive,
    note: "phase-276 W3 / #128: apply_lifecycle installs the five REP-2002 services and \
           drives the boot autostart (Configure→Activate) on-target — no manual set",
})]
#[case::zephyr_rust_safety(Cell {
    platform: "zephyr", lang: "rust", workload: "safety",
    resolver: build_zephyr_workspace_rust_safety_entry, port: 17851,
    boot: Boot::ZephyrNativeSim,
    proof: Proof::SinkCount { topic: "/safe_ok" },
    note: "phase-276 W4 (RFC-0028): [system].features = [\"safety\"] lowers to the \
           safety-e2e backend feature — CRC+seq attached per publish, validated on \
           receive, CallbackCtx::integrity() read on-target",
})]
fn entry_matrix(#[case] cell: Cell) {
    require_cell_env(&cell);

    let entry = (cell.resolver)().unwrap_or_else(|e| {
        nros_tests::skip!(
            "{} {} {} entry fixture not built: {e}",
            cell.platform,
            cell.lang,
            cell.workload
        )
    });

    // Router on the EXACT port the fixture's locator was baked with
    // (0.0.0.0 for slirp guests, whose gateway maps to the host; 127.0.0.1
    // suffices for the threadx host sim + native_sim NSOS sockets). The
    // observer always dials the host loopback.
    let bind_host = match cell.boot {
        Boot::ThreadxLinux | Boot::ZephyrNativeSim => "127.0.0.1",
        Boot::FreertosMps2 | Boot::NuttxArm => "0.0.0.0",
    };
    // Bound to `_router` (NOT `let _ = router` — that pattern drops the
    // guard, and Drop kills zenohd) so the router lives for the whole cell.
    let _router = ZenohRouter::start_on(bind_host, cell.port)
        .unwrap_or_else(|e| nros_tests::skip!("zenohd failed to start on {}: {e}", cell.port));
    let observer_locator = format!("tcp/127.0.0.1:{}", cell.port);

    match cell.proof {
        Proof::ChatterToCListener {
            observer_spin_ms,
            window_secs,
        } => {
            // Observer first, so its subscription is live before the
            // guest's talker publishes.
            let mut obs = spawn_c_listener_observer(&observer_locator, observer_spin_ms);
            let mut guest = boot_guest(&cell, &entry);

            let prefix = nros_tests::output::INT32_LISTENER_LOG_PREFIX;
            let out = obs
                .wait_for_output_count(prefix, 3, Duration::from_secs(window_secs))
                .unwrap_or_else(|_| {
                    guest.kill();
                    obs.kill();
                    panic!(
                        "[{} {}] native observer never received the entry's /chatter — the \
                         embedded LAUNCH-entry runtime delivery did not work ({})",
                        cell.platform, cell.lang, cell.note
                    )
                });
            guest.kill();
            obs.kill();

            let n = nros_tests::count_pattern(&out, prefix);
            assert!(
                n >= 3,
                "[{} {}] expected ≥3 cross-process deliveries, got {n}",
                cell.platform,
                cell.lang
            );
        }

        Proof::ChatterToRustListener { window_secs } => {
            let mut obs = spawn_rust_listener_observer(&observer_locator);
            let mut guest = boot_guest(&cell, &entry);

            let prefix = nros_tests::output::LISTENER_LOG_PREFIX;
            let out = obs
                .wait_for_output_count(prefix, 3, Duration::from_secs(window_secs))
                .unwrap_or_else(|_| {
                    guest.kill();
                    obs.kill();
                    panic!(
                        "[{} {}] native observer never received the entry image's /chatter \
                         ({})",
                        cell.platform, cell.lang, cell.note
                    )
                });
            guest.kill();
            obs.kill();

            let n = nros_tests::count_pattern(&out, prefix);
            assert!(
                n >= 3,
                "[{} {}] expected ≥3 cross-process deliveries, got {n}",
                cell.platform,
                cell.lang
            );
        }

        Proof::SinkValueLine { value } => {
            let mut obs = spawn_int32_sink("/chatter", &observer_locator);
            let mut guest = boot_guest(&cell, &entry);

            // The published value IS the live param read: ≥3 exact
            // `Received: <value>` lines prove the launch <param> was
            // compile-baked, seeded into the on-target store, and live-read
            // by the node's callback.
            let line = nros_tests::output::int32_listener_line(value);
            let out = obs
                .wait_for_output_count(&line, 3, Duration::from_secs(90))
                .unwrap_or_else(|_| {
                    guest.kill();
                    obs.kill();
                    panic!(
                        "[{} {} {}] subscriber never saw the live-read baked param value \
                         ({value}) — {}",
                        cell.platform, cell.lang, cell.workload, cell.note
                    )
                });
            guest.kill();
            obs.kill();

            let n = nros_tests::count_pattern(&out, &line);
            assert!(
                n >= 3,
                "[{} {} {}] expected ≥3 live-read publishes of {value}, got {n}",
                cell.platform,
                cell.lang,
                cell.workload
            );
        }

        Proof::SinkCount { topic } => {
            let mut obs = spawn_int32_sink(topic, &observer_locator);
            let mut guest = boot_guest(&cell, &entry);

            // `topic` carries the on-target listener's running receive
            // count — samples there mean the in-image pair matched,
            // delivered, and the republish reached the wire.
            let _ = obs
                .wait_for_output_count(
                    nros_tests::output::INT32_LISTENER_LOG_PREFIX,
                    3,
                    Duration::from_secs(90),
                )
                .unwrap_or_else(|_| {
                    guest.kill();
                    obs.kill();
                    panic!(
                        "[{} {} {}] observer never saw 3 `{topic}` republishes from the \
                         entry — {}",
                        cell.platform, cell.lang, cell.workload, cell.note
                    )
                });
            guest.kill();
            obs.kill();
        }

        Proof::LifecycleActive => {
            let mut guest = boot_guest(&cell, &entry);

            // Discover the managed node. `--no-daemon`: the env snippet
            // stops the daemon (it holds its own zenoh session), so a
            // daemon-using invocation would respawn one per poll and drain
            // the budget on CLI churn instead of discovery. `--spin-time 2`
            // (not the native cells' 0.1): the native_sim responder answers
            // noticeably slower through NSOS.
            let nodes_out = poll_ros2_until(
                &observer_locator,
                "lifecycle nodes --no-daemon --spin-time 2",
                "/",
                Duration::from_secs(40),
            );
            let lifecycle_node = first_lifecycle_node(&nodes_out).unwrap_or_else(|| {
                guest.kill();
                panic!(
                    "[{} {} {}] `ros2 lifecycle nodes` listed no managed node — the entry's \
                     REP-2002 services are not on the wire ({}):\n{nodes_out}",
                    cell.platform, cell.lang, cell.workload, cell.note
                )
            });

            // Autostart should already have driven it to active — no
            // manual `ros2 lifecycle set` issued.
            let state = poll_ros2_until(
                &observer_locator,
                &format!("lifecycle get --no-daemon --spin-time 2 {lifecycle_node}"),
                "active",
                Duration::from_secs(40),
            );
            guest.kill();

            assert!(
                state.to_lowercase().contains("active"),
                "[{} {} {}] expected the autostart-managed node {lifecycle_node} to be \
                 `active` at boot ({}), got:\n{state}",
                cell.platform,
                cell.lang,
                cell.workload,
                cell.note
            );
        }
    }
}
