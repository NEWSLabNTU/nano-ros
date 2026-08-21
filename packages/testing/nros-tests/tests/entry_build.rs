//! Per-role Entry-pkg demos link into real, bootable images — NuttX + ThreadX-Linux.
//!
//! phase-373 W4. This is the fold of `nuttx_entry_build.rs` (#127, phase-275 W1
//! tail) and `threadx_linux_entry_build.rs` (phase-275 W1 / #102 H2). They were
//! the same test twice: the same six roles, the same loop, the same "the ELF
//! exists and is non-empty" assertion, differing only in which availability
//! probes guard the platform and which resolver finds the artifact. That is a
//! per-cell duplicate in phase-329's sense, so it folds rather than being kept
//! and labelled.
//!
//! ## What each half was for
//!
//! **NuttX** — the six `examples/qemu-arm-nuttx/rust/{role}-entry` demos are
//! standalone `nros::main!` Entry pkgs that bake board + zenoh RMW through the
//! `nros-board-nuttx-qemu` shim. Their standalone `[[bin]]` link used to fail on
//! unresolved NuttX libc/syscall symbols (issue #127). The board-centric image
//! link (RFC-0032's "third leg": dynamic link pieces propagate from the board
//! dep's build.rs via `nros_board_common::nuttx_image_link`, static args in each
//! entry's `.cargo/config.toml`) makes each a real, bootable NuttX flat-build
//! ELF with ZERO entry build.rs.
//!
//! **ThreadX-Linux** — the six `examples/threadx-linux/rust/{role}-entry` demos
//! bake the same way through `nros-board-threadx-linux`. They shipped with no
//! fixture at all: built by nothing, tested by nothing.
//!
//! ## No compilation at run time
//!
//! CLAUDE.md "No compilation inside tests". The artifacts are prebuilt in the
//! **build stage** by the `[[fixture]]` rows in `examples/fixtures.toml`; this
//! test only resolves and inspects them. An empty file means the link produced
//! nothing, which is the failure both halves existed to catch.

use nros_tests::{
    TestError, TestResult,
    fixtures::{nuttx, threadx_linux},
};
use rstest::rstest;
use std::path::PathBuf;

/// The six roles, identical on both platforms.
const ENTRIES: &[(&str, &str)] = &[
    ("talker", "talker"),
    ("listener", "listener"),
    ("service-server", "service-server"),
    ("service-client", "service-client"),
    ("action-server", "action-server"),
    ("action-client", "action-client"),
];

#[derive(Copy, Clone, Debug)]
enum Platform {
    Nuttx,
    ThreadxLinux,
}

impl Platform {
    /// Skip with the platform's own remedy when its toolchain is absent.
    ///
    /// Each probe keeps the message the pre-fold file had: they name different
    /// setup commands, and a merged "toolchain missing" would be less useful
    /// than either.
    fn require(self) {
        match self {
            Platform::Nuttx => {
                if !nuttx::is_nuttx_available() {
                    nros_tests::skip!("NUTTX_DIR unset/invalid — run `just nuttx setup`");
                }
                if !nuttx::is_nuttx_configured() {
                    nros_tests::skip!(
                        "NuttX tree not configured (no include/nuttx/config.h) — run `just nuttx build`"
                    );
                }
            }
            Platform::ThreadxLinux => {
                if !threadx_linux::is_threadx_available() {
                    nros_tests::skip!("THREADX_DIR unset/invalid — run `just threadx_linux setup`");
                }
                if !threadx_linux::is_nsos_netx_available() {
                    nros_tests::skip!(
                        "NetX Duo (NSOS) unavailable — run `just threadx_linux setup`"
                    );
                }
            }
        }
    }

    fn entry_binary(self, role: &str, bin: &str) -> TestResult<PathBuf> {
        match self {
            Platform::Nuttx => nuttx::require_entry_binary(role, bin),
            Platform::ThreadxLinux => threadx_linux::require_entry_binary(role, bin),
        }
    }
}

#[rstest]
#[case::nuttx(Platform::Nuttx)]
#[case::threadx_linux(Platform::ThreadxLinux)]
fn entry_demos_build(#[case] platform: Platform) -> TestResult<()> {
    platform.require();

    for (role, bin) in ENTRIES {
        let path = platform.entry_binary(role, bin)?;
        let meta = std::fs::metadata(&path)
            .map_err(|e| TestError::BuildFailed(format!("stat {}: {e}", path.display())))?;
        assert!(
            meta.len() > 0,
            "{platform:?}: entry binary is empty: {} ({role}_entry)",
            path.display()
        );
    }
    Ok(())
}
