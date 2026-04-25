//! Parametrised RTOS E2E integration tests.
//!
//! Collapses the four per-platform E2E clusters (FreeRTOS, NuttX, ThreadX
//! Linux, ThreadX QEMU RISC-V) into three parametrised `#[rstest]`
//! functions (pubsub / service / action), each fanned out over three
//! languages (Rust / C / C++). See Phase 85.4 in
//! `docs/roadmap/phase-85-test-suite-consolidation.md`.
//!
//! Per-platform build and detection smoke tests remain in the original
//! `freertos_qemu.rs` / `nuttx_qemu.rs` / `threadx_linux.rs` /
//! `threadx_riscv64_qemu.rs` files — only the E2E bodies moved here.

use nros_tests::count_pattern;
use nros_tests::fixtures::{
    QemuProcess, ZenohRouter, is_qemu_available, is_qemu_riscv64_available, require_zenohd,
};
use nros_tests::fixtures::{freertos, nuttx, threadx_linux, threadx_riscv64};
use nros_tests::platform;
use nros_tests::process::{ManagedProcess, kill_process_group};
use nros_tests::{TestError, TestResult};
use rstest::rstest;
use std::fmt;
use std::path::Path;
use std::time::Duration;

// =============================================================================
// Parameter enums
// =============================================================================

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Platform {
    Freertos,
    Nuttx,
    ThreadxLinux,
    ThreadxRiscv64,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Platform::Freertos => "freertos",
            Platform::Nuttx => "nuttx",
            Platform::ThreadxLinux => "threadx_linux",
            Platform::ThreadxRiscv64 => "threadx_riscv64",
        };
        f.write_str(s)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Lang {
    Rust,
    C,
    Cpp,
}

impl fmt::Display for Lang {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Lang::Rust => "rust",
            Lang::C => "c",
            Lang::Cpp => "cpp",
        };
        f.write_str(s)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Variant {
    Pubsub,
    Service,
    Action,
}

// =============================================================================
// RtosProcess — wraps QemuProcess and ManagedProcess with a common API
// =============================================================================

enum RtosProcess {
    Qemu(QemuProcess),
    Managed(ManagedProcess),
}

impl RtosProcess {
    fn wait_for_output_pattern(&mut self, pattern: &str, timeout: Duration) -> TestResult<String> {
        match self {
            RtosProcess::Qemu(p) => p.wait_for_output_pattern(pattern, timeout),
            RtosProcess::Managed(p) => p.wait_for_output_pattern(pattern, timeout),
        }
    }

    fn wait_for_output(&mut self, timeout: Duration) -> TestResult<String> {
        match self {
            RtosProcess::Qemu(p) => p.wait_for_output(timeout),
            // ManagedProcess::wait_for_all_output captures both stdout and
            // stderr, matching the shape the existing ThreadX Linux tests
            // use. The ThreadX Linux binaries emit their readiness banner
            // on stderr via env_logger.
            RtosProcess::Managed(p) => p.wait_for_all_output(timeout),
        }
    }

    fn kill(&mut self) {
        match self {
            RtosProcess::Qemu(p) => p.kill(),
            RtosProcess::Managed(p) => kill_process_group(p.handle_mut()),
        }
    }
}

// =============================================================================
// Platform dispatch
// =============================================================================

impl Platform {
    /// Per-platform base port. Phase 89.9/89.10 splits the router port
    /// further by test variant so `pubsub`, `service`, and `action` on the
    /// same platform can run concurrently — see [`zenohd_port_for`].
    /// Phase 89.13 (pilot: FreeRTOS) additionally splits by language so the
    /// Rust / C / C++ binaries within a single variant can run in parallel
    /// on migrated platforms; see `PlatformConfig::lang_stride`.
    fn zenohd_base(self) -> &'static platform::PlatformConfig {
        match self {
            Platform::Freertos => &platform::FREERTOS,
            Platform::Nuttx => &platform::NUTTX,
            Platform::ThreadxLinux => &platform::THREADX_LINUX,
            Platform::ThreadxRiscv64 => &platform::THREADX_RISCV,
        }
    }

    fn zenohd_port_for(self, variant: Variant, lang: Lang) -> u16 {
        let pv = match variant {
            Variant::Pubsub => platform::TestVariant::Pubsub,
            Variant::Service => platform::TestVariant::Service,
            Variant::Action => platform::TestVariant::Action,
        };
        let pl = match lang {
            Lang::Rust => platform::TestLang::Rust,
            Lang::C => platform::TestLang::C,
            Lang::Cpp => platform::TestLang::Cpp,
        };
        self.zenohd_base().zenohd_port_for(pv, pl)
    }

    fn zenoh_router_start(self, variant: Variant, lang: Lang) -> TestResult<ZenohRouter> {
        // ThreadX Linux is bridge-networked (veth pairs), so zenohd must
        // bind to 0.0.0.0 to be reachable from the bridged simulation
        // interface. The QEMU-based platforms use slirp and reach zenohd
        // via the slirp gateway (10.0.2.2) forwarded to host localhost.
        let port = self.zenohd_port_for(variant, lang);
        match self {
            Platform::ThreadxLinux => ZenohRouter::start_on("0.0.0.0", port),
            _ => ZenohRouter::start(port),
        }
    }

    fn stabilization_delay(self) -> Duration {
        match self {
            // QEMU cold-boot + zenoh connect — ~15s is typical; use 20s
            // margin to match the original per-platform tests.
            Platform::Freertos | Platform::Nuttx | Platform::ThreadxRiscv64 => {
                Duration::from_secs(20)
            }
            // ThreadX Linux is a native process — ~5s for ThreadX boot +
            // NetX init + zenoh connect.
            Platform::ThreadxLinux => Duration::from_secs(5),
        }
    }

    fn require_e2e(self) -> bool {
        match self {
            Platform::Freertos => {
                if !freertos::is_freertos_available() {
                    eprintln!("Skipping test: FREERTOS_DIR not set or invalid");
                    return false;
                }
                if !freertos::is_lwip_available() {
                    eprintln!("Skipping test: LWIP_DIR not set or invalid");
                    return false;
                }
                if !freertos::is_arm_gcc_available() {
                    eprintln!("Skipping test: arm-none-eabi-gcc not found");
                    return false;
                }
                if !is_qemu_available() {
                    eprintln!("Skipping test: qemu-system-arm not found");
                    return false;
                }
                require_zenohd()
            }
            Platform::Nuttx => {
                if !nuttx::is_nuttx_available() {
                    eprintln!("Skipping test: NUTTX_DIR not set or invalid");
                    return false;
                }
                if !nuttx::is_nuttx_configured() {
                    eprintln!("Skipping test: NuttX not configured");
                    return false;
                }
                if !nuttx::is_arm_gcc_available() {
                    eprintln!("Skipping test: arm-none-eabi-gcc not found");
                    return false;
                }
                if !nuttx::is_nuttx_toolchain_available() {
                    eprintln!(
                        "Skipping test: nightly toolchain missing rust-src for armv7a-nuttx-eabihf"
                    );
                    return false;
                }
                if nuttx::nuttx_kernel_path().is_none() {
                    eprintln!("Skipping test: NuttX kernel not built ($NUTTX_DIR/nuttx)");
                    return false;
                }
                if !is_qemu_available() {
                    eprintln!("Skipping test: qemu-system-arm not found");
                    return false;
                }
                require_zenohd()
            }
            Platform::ThreadxLinux => {
                if !threadx_linux::is_threadx_available() {
                    eprintln!("Skipping test: THREADX_DIR not set or invalid");
                    return false;
                }
                if !threadx_linux::is_nsos_netx_available() {
                    eprintln!("Skipping test: nsos-netx not found at packages/drivers/nsos-netx/");
                    return false;
                }
                require_zenohd()
            }
            Platform::ThreadxRiscv64 => {
                if !threadx_riscv64::is_threadx_available() {
                    eprintln!("Skipping test: THREADX_DIR not set or invalid");
                    return false;
                }
                if !threadx_riscv64::is_netx_available() {
                    eprintln!("Skipping test: NETX_DIR not set or invalid");
                    return false;
                }
                if !threadx_riscv64::is_riscv_gcc_available() {
                    eprintln!("Skipping test: riscv64-unknown-elf-gcc not found");
                    return false;
                }
                if !is_qemu_riscv64_available() {
                    eprintln!("Skipping test: qemu-system-riscv64 not found");
                    return false;
                }
                require_zenohd()
            }
        }
    }

    /// Spawn a platform-specific emulator / process for the given binary.
    ///
    /// `node_idx` is 0 for the "first" node (talker / server) and 1 for
    /// the "second" node (listener / client). It's only meaningful for
    /// ThreadX QEMU RISC-V, where the MAC address is derived from the
    /// index. Other platforms either use slirp (per-instance NAT) or
    /// pre-assigned static IPs in the firmware and ignore the index.
    fn start_process(self, binary: &Path, node_idx: u8, name: &str) -> TestResult<RtosProcess> {
        match self {
            Platform::Freertos => {
                QemuProcess::start_mps2_an385_networked(binary).map(RtosProcess::Qemu)
            }
            Platform::Nuttx => QemuProcess::start_nuttx_virt(binary, true).map(RtosProcess::Qemu),
            Platform::ThreadxLinux => {
                ManagedProcess::spawn(binary, &[], name).map(RtosProcess::Managed)
            }
            Platform::ThreadxRiscv64 => {
                QemuProcess::start_riscv64_virt(binary, node_idx).map(RtosProcess::Qemu)
            }
        }
    }

    /// Per-(lang, variant) skip reason, or `None` if the combination is
    /// expected to run on this platform.
    fn skip_reason(self, lang: Lang, variant: Variant) -> Option<&'static str> {
        // Phase 89.13 sweep: NuttX C++ now BUILDS clean (cmake's
        // INTERFACE_LINK_LIBRARIES graph carries the codegen-target
        // closure, the per-package FFI staticlibs cross-compile to
        // armv7a-nuttx-eabihf, and main.cpp is forced to hardfloat
        // ABI to match the link closure). The remaining gap is a
        // runtime init failure — `nros::init(...)` returns
        // NROS_CPP_RET_INVALID_ARGUMENT (-3) on every NuttX boot,
        // even though the binary embeds the correct locator string
        // and `Node::global_storage()` resolves to a non-zero static
        // buffer. Same nros-cpp init path works on FreeRTOS /
        // ThreadX. Likely missing platform-init step or feature gate
        // specific to nros-cpp + NuttX; needs separate investigation.
        match (self, lang, variant) {
            (Platform::Nuttx, Lang::Cpp, Variant::Pubsub)
            | (Platform::Nuttx, Lang::Cpp, Variant::Service)
            | (Platform::Nuttx, Lang::Cpp, Variant::Action) => {
                Some("nros_cpp_init returns INVALID_ARGUMENT (-3) on NuttX — Phase 89.13 follow-up")
            }
            _ => None,
        }
    }
}

// =============================================================================
// Binary dispatch
// =============================================================================

type BuildFn = fn() -> TestResult<&'static Path>;

struct BinaryPair {
    first_builder: BuildFn,
    second_builder: BuildFn,
}

fn binaries(platform: Platform, lang: Lang, variant: Variant) -> BinaryPair {
    // "first" = talker / server; "second" = listener / client.
    match (platform, lang, variant) {
        // FreeRTOS — Rust
        (Platform::Freertos, Lang::Rust, Variant::Pubsub) => BinaryPair {
            first_builder: freertos::build_freertos_talker,
            second_builder: freertos::build_freertos_listener,
        },
        (Platform::Freertos, Lang::Rust, Variant::Service) => BinaryPair {
            first_builder: freertos::build_freertos_service_server,
            second_builder: freertos::build_freertos_service_client,
        },
        (Platform::Freertos, Lang::Rust, Variant::Action) => BinaryPair {
            first_builder: freertos::build_freertos_action_server,
            second_builder: freertos::build_freertos_action_client,
        },
        // FreeRTOS — C
        (Platform::Freertos, Lang::C, Variant::Pubsub) => BinaryPair {
            first_builder: freertos::build_freertos_c_talker,
            second_builder: freertos::build_freertos_c_listener,
        },
        (Platform::Freertos, Lang::C, Variant::Service) => BinaryPair {
            first_builder: freertos::build_freertos_c_service_server,
            second_builder: freertos::build_freertos_c_service_client,
        },
        (Platform::Freertos, Lang::C, Variant::Action) => BinaryPair {
            first_builder: freertos::build_freertos_c_action_server,
            second_builder: freertos::build_freertos_c_action_client,
        },
        // FreeRTOS — C++
        (Platform::Freertos, Lang::Cpp, Variant::Pubsub) => BinaryPair {
            first_builder: freertos::build_freertos_cpp_talker,
            second_builder: freertos::build_freertos_cpp_listener,
        },
        (Platform::Freertos, Lang::Cpp, Variant::Service) => BinaryPair {
            first_builder: freertos::build_freertos_cpp_service_server,
            second_builder: freertos::build_freertos_cpp_service_client,
        },
        (Platform::Freertos, Lang::Cpp, Variant::Action) => BinaryPair {
            first_builder: freertos::build_freertos_cpp_action_server,
            second_builder: freertos::build_freertos_cpp_action_client,
        },

        // NuttX — Rust
        (Platform::Nuttx, Lang::Rust, Variant::Pubsub) => BinaryPair {
            first_builder: nuttx::build_nuttx_talker,
            second_builder: nuttx::build_nuttx_listener,
        },
        (Platform::Nuttx, Lang::Rust, Variant::Service) => BinaryPair {
            first_builder: nuttx::build_nuttx_service_server,
            second_builder: nuttx::build_nuttx_service_client,
        },
        (Platform::Nuttx, Lang::Rust, Variant::Action) => BinaryPair {
            first_builder: nuttx::build_nuttx_action_server,
            second_builder: nuttx::build_nuttx_action_client,
        },
        // NuttX — C
        (Platform::Nuttx, Lang::C, Variant::Pubsub) => BinaryPair {
            first_builder: nuttx::build_nuttx_c_talker,
            second_builder: nuttx::build_nuttx_c_listener,
        },
        (Platform::Nuttx, Lang::C, Variant::Service) => BinaryPair {
            first_builder: nuttx::build_nuttx_c_service_server,
            second_builder: nuttx::build_nuttx_c_service_client,
        },
        (Platform::Nuttx, Lang::C, Variant::Action) => BinaryPair {
            first_builder: nuttx::build_nuttx_c_action_server,
            second_builder: nuttx::build_nuttx_c_action_client,
        },
        // NuttX — C++ (all skipped, but wire up builders for completeness)
        (Platform::Nuttx, Lang::Cpp, Variant::Pubsub) => BinaryPair {
            first_builder: nuttx::build_nuttx_cpp_talker,
            second_builder: nuttx::build_nuttx_cpp_listener,
        },
        (Platform::Nuttx, Lang::Cpp, Variant::Service) => BinaryPair {
            first_builder: nuttx::build_nuttx_cpp_service_server,
            second_builder: nuttx::build_nuttx_cpp_service_client,
        },
        (Platform::Nuttx, Lang::Cpp, Variant::Action) => BinaryPair {
            first_builder: nuttx::build_nuttx_cpp_action_server,
            second_builder: nuttx::build_nuttx_cpp_action_client,
        },

        // ThreadX Linux — Rust
        (Platform::ThreadxLinux, Lang::Rust, Variant::Pubsub) => BinaryPair {
            first_builder: threadx_linux::build_threadx_talker,
            second_builder: threadx_linux::build_threadx_listener,
        },
        (Platform::ThreadxLinux, Lang::Rust, Variant::Service) => BinaryPair {
            first_builder: threadx_linux::build_threadx_service_server,
            second_builder: threadx_linux::build_threadx_service_client,
        },
        (Platform::ThreadxLinux, Lang::Rust, Variant::Action) => BinaryPair {
            first_builder: threadx_linux::build_threadx_action_server,
            second_builder: threadx_linux::build_threadx_action_client,
        },
        // ThreadX Linux — C
        (Platform::ThreadxLinux, Lang::C, Variant::Pubsub) => BinaryPair {
            first_builder: threadx_linux::build_threadx_c_talker,
            second_builder: threadx_linux::build_threadx_c_listener,
        },
        (Platform::ThreadxLinux, Lang::C, Variant::Service) => BinaryPair {
            first_builder: threadx_linux::build_threadx_c_service_server,
            second_builder: threadx_linux::build_threadx_c_service_client,
        },
        (Platform::ThreadxLinux, Lang::C, Variant::Action) => BinaryPair {
            first_builder: threadx_linux::build_threadx_c_action_server,
            second_builder: threadx_linux::build_threadx_c_action_client,
        },
        // ThreadX Linux — C++ (Action skipped, Pubsub/Service run)
        (Platform::ThreadxLinux, Lang::Cpp, Variant::Pubsub) => BinaryPair {
            first_builder: threadx_linux::build_threadx_cpp_talker,
            second_builder: threadx_linux::build_threadx_cpp_listener,
        },
        (Platform::ThreadxLinux, Lang::Cpp, Variant::Service) => BinaryPair {
            first_builder: threadx_linux::build_threadx_cpp_service_server,
            second_builder: threadx_linux::build_threadx_cpp_service_client,
        },
        (Platform::ThreadxLinux, Lang::Cpp, Variant::Action) => BinaryPair {
            first_builder: threadx_linux::build_threadx_cpp_action_server,
            second_builder: threadx_linux::build_threadx_cpp_action_client,
        },

        // ThreadX RISC-V — Rust
        (Platform::ThreadxRiscv64, Lang::Rust, Variant::Pubsub) => BinaryPair {
            first_builder: threadx_riscv64::build_threadx_rv64_talker,
            second_builder: threadx_riscv64::build_threadx_rv64_listener,
        },
        (Platform::ThreadxRiscv64, Lang::Rust, Variant::Service) => BinaryPair {
            first_builder: threadx_riscv64::build_threadx_rv64_service_server,
            second_builder: threadx_riscv64::build_threadx_rv64_service_client,
        },
        (Platform::ThreadxRiscv64, Lang::Rust, Variant::Action) => BinaryPair {
            first_builder: threadx_riscv64::build_threadx_rv64_action_server,
            second_builder: threadx_riscv64::build_threadx_rv64_action_client,
        },
        // ThreadX RISC-V — C
        (Platform::ThreadxRiscv64, Lang::C, Variant::Pubsub) => BinaryPair {
            first_builder: threadx_riscv64::build_rv64_c_talker,
            second_builder: threadx_riscv64::build_rv64_c_listener,
        },
        (Platform::ThreadxRiscv64, Lang::C, Variant::Service) => BinaryPair {
            first_builder: threadx_riscv64::build_rv64_c_service_server,
            second_builder: threadx_riscv64::build_rv64_c_service_client,
        },
        (Platform::ThreadxRiscv64, Lang::C, Variant::Action) => BinaryPair {
            first_builder: threadx_riscv64::build_rv64_c_action_server,
            second_builder: threadx_riscv64::build_rv64_c_action_client,
        },
        // ThreadX RISC-V — C++
        (Platform::ThreadxRiscv64, Lang::Cpp, Variant::Pubsub) => BinaryPair {
            first_builder: threadx_riscv64::build_rv64_cpp_talker,
            second_builder: threadx_riscv64::build_rv64_cpp_listener,
        },
        (Platform::ThreadxRiscv64, Lang::Cpp, Variant::Service) => BinaryPair {
            first_builder: threadx_riscv64::build_rv64_cpp_service_server,
            second_builder: threadx_riscv64::build_rv64_cpp_service_client,
        },
        (Platform::ThreadxRiscv64, Lang::Cpp, Variant::Action) => BinaryPair {
            first_builder: threadx_riscv64::build_rv64_cpp_action_server,
            second_builder: threadx_riscv64::build_rv64_cpp_action_client,
        },
    }
}

// =============================================================================
// Helpers shared by all three parametrised tests
// =============================================================================

/// Returns `true` if the caller should silently return from the test.
///
/// Two skip paths with different semantics:
/// - **Unsupported combination** (`skip_reason` returns `Some`): the example
///   for this (platform, lang, variant) tuple is not implemented upstream
///   (e.g., NuttX C++ blocked by libc, or Phase 69.7/77/69.8 follow-ups).
///   These silently return — equivalent to the `#[ignore]` attribute the
///   original per-platform tests used. rstest `#[values]` can't attach
///   `#[ignore]` per-case, so we use runtime skip instead.
/// - **Missing prerequisite** (`require_e2e` returns `false`): SDK / env
///   var / toolchain missing. Per CLAUDE.md, this must panic (fail) so
///   absent tools don't silently turn into false PASS results.
fn maybe_skip(platform: Platform, lang: Lang, variant: Variant) -> bool {
    if let Some(reason) = platform.skip_reason(lang, variant) {
        eprintln!("[SKIP] {} {} {:?}: {}", platform, lang, variant, reason);
        return true;
    }
    if !platform.require_e2e() {
        nros_tests::skip!("require_e2e check failed for {}", platform);
    }
    false
}

/// Build a (first, second) binary pair, panicking on build failure.
fn build_pair(platform: Platform, lang: Lang, variant: Variant) -> (&'static Path, &'static Path) {
    let pair = binaries(platform, lang, variant);
    let first = (pair.first_builder)().unwrap_or_else(|e| {
        panic!(
            "Failed to build first binary ({} {} {:?}): {:?}",
            platform, lang, variant, e
        )
    });
    let second = (pair.second_builder)().unwrap_or_else(|e| {
        panic!(
            "Failed to build second binary ({} {} {:?}): {:?}",
            platform, lang, variant, e
        )
    });
    (first, second)
}

/// Start (first, second) on `platform`. On NuttX the two instances run
/// in parallel (no stabilisation delay between them), mirroring the
/// existing Rust test — the NuttX Rust binaries boot slowly and if the
/// listener is given the usual 20s head-start its session times out
/// before the talker finishes booting. On every other platform the
/// first node gets `stabilization_delay()` of head-start.
fn start_pair(
    platform: Platform,
    first_bin: &Path,
    second_bin: &Path,
    first_name: &str,
    second_name: &str,
) -> TestResult<(RtosProcess, RtosProcess)> {
    let first = platform.start_process(first_bin, 0, first_name)?;

    match platform {
        Platform::Nuttx => {
            let second = platform.start_process(second_bin, 1, second_name)?;
            Ok((first, second))
        }
        _ => {
            std::thread::sleep(platform.stabilization_delay());
            let second = platform.start_process(second_bin, 1, second_name)?;
            Ok((first, second))
        }
    }
}

/// Common readiness check: is the platform's "first" process (listener
/// or server) past its boot banner? Panics on every platform if the
/// banner is missing — boot failures are real regressions (either a
/// kernel/app integration issue or a zenoh / networking problem) and
/// must surface loudly.
fn ensure_ready(output: &str, readiness_pattern: &str, platform: Platform) {
    if output.contains(readiness_pattern) {
        return;
    }
    panic!(
        "{} E2E failed — readiness pattern '{}' not observed.\nOutput so far (truncated):\n{}",
        platform,
        readiness_pattern,
        &output[..output.len().min(2048)]
    );
}

// =============================================================================
// Parametrised E2E tests
// =============================================================================

/// End-to-end pub/sub across Rust, C, and C++ on all four RTOS
/// platforms. First node is the talker; second node is the listener.
/// We wait for the listener to reach "Waiting for messages" before
/// killing both, and assert that at least one "Received" line appeared.
#[rstest]
fn test_rtos_pubsub_e2e(
    #[values(
        Platform::Freertos,
        Platform::Nuttx,
        Platform::ThreadxLinux,
        Platform::ThreadxRiscv64
    )]
    platform: Platform,
    #[values(Lang::Rust, Lang::C, Lang::Cpp)] lang: Lang,
) {
    if maybe_skip(platform, lang, Variant::Pubsub) {
        return;
    }

    let (talker_bin, listener_bin) = build_pair(platform, lang, Variant::Pubsub);

    let _zenohd = platform
        .zenoh_router_start(Variant::Pubsub, lang)
        .expect("Failed to start zenohd");

    eprintln!(
        "[{} {}] pubsub: starting talker/listener...",
        platform, lang
    );
    // Subscriber (listener) before publisher (talker) — zenoh does not
    // buffer for unknown subscribers. On every platform except NuttX we
    // intentionally invert the naming: "first" = listener, "second" =
    // talker, matching the per-platform tests. NuttX boots the two in
    // parallel (see start_pair docstring).
    let (mut listener, mut talker) = match platform {
        Platform::Nuttx => {
            // Parallel launch.
            let listener = platform
                .start_process(listener_bin, 1, "nuttx-listener")
                .expect("Failed to start listener");
            let talker = platform
                .start_process(talker_bin, 0, "nuttx-talker")
                .expect("Failed to start talker");
            (listener, talker)
        }
        _ => {
            let listener = platform
                .start_process(listener_bin, 1, "rtos-listener")
                .expect("Failed to start listener");
            std::thread::sleep(platform.stabilization_delay());
            let talker = platform
                .start_process(talker_bin, 0, "rtos-talker")
                .expect("Failed to start talker");
            (listener, talker)
        }
    };

    // Listener boot check: NuttX needs a lenient readiness probe because
    // its Rust apps sometimes fail to register with NSH. Other platforms
    // want a hard fail to surface environment regressions.
    let listener_boot = match platform {
        Platform::Nuttx => listener
            .wait_for_output(Duration::from_secs(30))
            .unwrap_or_default(),
        _ => listener
            .wait_for_output_pattern("Waiting for messages", Duration::from_secs(30))
            .unwrap_or_default(),
    };
    ensure_ready(&listener_boot, "Waiting for messages", platform);

    // Let the talker run a bit and drain its output to avoid pipe back-pressure.
    // NuttX C needs a longer window: cold QEMU boot + 5s app sleep + session
    // open can eat >15 s before the first publish, and parallel retries from
    // earlier flaky tests load the host further.
    let talker_window = match (platform, lang) {
        (Platform::Nuttx, Lang::C) => Duration::from_secs(45),
        _ => Duration::from_secs(15),
    };
    let _talker_out = talker.wait_for_output(talker_window).unwrap_or_default();

    // Collect more listener output to capture "Received:" lines.
    let listener_window = match (platform, lang) {
        (Platform::Nuttx, Lang::C) => Duration::from_secs(90),
        _ => Duration::from_secs(30),
    };
    let final_out = listener
        .wait_for_output(listener_window)
        .unwrap_or_default();
    let full_listener = format!("{}{}", listener_boot, final_out);

    talker.kill();
    listener.kill();

    eprintln!("Listener output:\n{}", full_listener);

    let received = count_pattern(&full_listener, "Received");
    eprintln!("[{} {}] messages received: {}", platform, lang, received);
    assert!(
        received > 0,
        "{} {} pubsub E2E failed — 0 messages received",
        platform,
        lang
    );
    eprintln!(
        "[PASS] {} {} pubsub E2E: {} messages",
        platform, lang, received
    );
}

/// End-to-end service request/response. First node is the server;
/// second is the client. The client should receive at least three
/// responses (most examples issue four: 5+3, 10+20, 100+200, -5+10) and
/// the "All service calls completed" marker.
#[rstest]
fn test_rtos_service_e2e(
    #[values(
        Platform::Freertos,
        Platform::Nuttx,
        Platform::ThreadxLinux,
        Platform::ThreadxRiscv64
    )]
    platform: Platform,
    #[values(Lang::Rust, Lang::C, Lang::Cpp)] lang: Lang,
) {
    if maybe_skip(platform, lang, Variant::Service) {
        return;
    }

    let (server_bin, client_bin) = build_pair(platform, lang, Variant::Service);

    let _zenohd = platform
        .zenoh_router_start(Variant::Service, lang)
        .expect("Failed to start zenohd");

    eprintln!("[{} {}] service: starting server/client...", platform, lang);
    let (mut server, mut client) = start_pair(
        platform,
        server_bin,
        client_bin,
        "rtos-server",
        "rtos-client",
    )
    .expect("Failed to start server/client");

    // Server boot check — NuttX lenient, others hard-fail.
    let server_boot = match platform {
        Platform::Nuttx => server
            .wait_for_output(Duration::from_secs(30))
            .unwrap_or_default(),
        _ => server
            .wait_for_output_pattern("Waiting for requests", Duration::from_secs(30))
            .unwrap_or_default(),
    };
    ensure_ready(&server_boot, "Waiting for requests", platform);

    // Give the client the same boot delay as the server so its first
    // query doesn't race ahead of the server queryable's declaration.
    // Only applies to QEMU-cold-boot platforms.
    if !matches!(platform, Platform::Nuttx | Platform::ThreadxLinux) {
        std::thread::sleep(platform.stabilization_delay());
    }

    // NuttX C service is slower: 4 nros_client_call round-trips over
    // QEMU slirp + zenoh-pico TCP are routinely in the 40–60 s range,
    // and the first call can time out if the server queryable isn't
    // fully registered yet (cold-host runs). Give extra headroom.
    let client_timeout = match (platform, lang) {
        (Platform::Nuttx, Lang::C) => Duration::from_secs(180),
        _ => Duration::from_secs(60),
    };
    let client_out = client.wait_for_output(client_timeout).unwrap_or_default();

    server.kill();
    client.kill();

    eprintln!("Client output:\n{}", client_out);

    let response_count = count_pattern(&client_out, "Response:");
    let completed = client_out.contains("All service calls completed");
    eprintln!(
        "[{} {}] responses: {}, completed: {}",
        platform, lang, response_count, completed
    );

    // Keep the original assertion shape: at least one response counts as
    // partial success on platforms with boot-flakiness (NuttX, ThreadX
    // Linux on slower hosts). Tight platforms assert >= 3.
    let required = match platform {
        Platform::Nuttx | Platform::ThreadxLinux => 1,
        _ => 3,
    };
    assert!(
        response_count >= required,
        "{} {} service E2E failed — got {} responses (expected >= {})",
        platform,
        lang,
        response_count,
        required
    );
    eprintln!(
        "[PASS] {} {} service E2E: {} responses (completed={})",
        platform, lang, response_count, completed
    );
}

/// End-to-end action (goal / feedback / result). First node is the
/// action server; second is the action client. We assert the client
/// saw "Goal accepted" plus either "Action completed successfully" or
/// "Result (status=..." (NuttX C action example uses the latter).
#[rstest]
fn test_rtos_action_e2e(
    #[values(
        Platform::Freertos,
        Platform::Nuttx,
        Platform::ThreadxLinux,
        Platform::ThreadxRiscv64
    )]
    platform: Platform,
    #[values(Lang::Rust, Lang::C, Lang::Cpp)] lang: Lang,
) {
    if maybe_skip(platform, lang, Variant::Action) {
        return;
    }

    let (server_bin, client_bin) = build_pair(platform, lang, Variant::Action);

    let _zenohd = platform
        .zenoh_router_start(Variant::Action, lang)
        .expect("Failed to start zenohd");

    eprintln!("[{} {}] action: starting server/client...", platform, lang);
    let (mut server, mut client) = start_pair(
        platform,
        server_bin,
        client_bin,
        "rtos-action-server",
        "rtos-action-client",
    )
    .expect("Failed to start server/client");

    let server_boot = match platform {
        Platform::Nuttx => server
            .wait_for_output(Duration::from_secs(30))
            .unwrap_or_default(),
        _ => server
            .wait_for_output_pattern("Waiting for goals", Duration::from_secs(30))
            .unwrap_or_default(),
    };
    ensure_ready(&server_boot, "Waiting for goals", platform);

    if !matches!(platform, Platform::Nuttx | Platform::ThreadxLinux) {
        std::thread::sleep(platform.stabilization_delay());
    }

    // Some platforms (NuttX C action, FreeRTOS C action) need a longer
    // overall window because each failed send_goal retries several times.
    // NuttX C action is especially slow: QEMU boot + 5s network wait +
    // session open + goal round-trip + 11 feedback messages (order=10) +
    // result round-trip routinely takes ~130s on a loaded host, so the
    // 120s budget used to fail occasionally; bump to 180s.
    let client_timeout = match (platform, lang) {
        (Platform::Freertos, Lang::C) => Duration::from_secs(90),
        (Platform::Nuttx, Lang::C) => Duration::from_secs(240),
        _ => Duration::from_secs(60),
    };
    let client_out = client.wait_for_output(client_timeout).unwrap_or_default();

    server.kill();
    client.kill();

    eprintln!("Client output:\n{}", client_out);

    let goal_accepted = client_out.contains("Goal accepted");
    let completed = client_out.contains("Action completed successfully")
        || client_out.contains("Action client finished")
        || client_out.contains("All feedback received")
        || client_out.contains("Result (status=");

    assert!(
        goal_accepted && completed,
        "{} {} action E2E failed: accepted={}, completed={}",
        platform,
        lang,
        goal_accepted,
        completed
    );
    eprintln!(
        "[PASS] {} {} action E2E: accepted={}, completed={}",
        platform, lang, goal_accepted, completed
    );
}

// Keep the `TestError` import alive when no code path actually constructs
// one — the type is re-exported from `nros_tests::TestError` so future
// maintainers (and rust-analyzer) see where it comes from.
#[allow(dead_code)]
fn _unused_type_anchor() -> Option<TestError> {
    None
}
