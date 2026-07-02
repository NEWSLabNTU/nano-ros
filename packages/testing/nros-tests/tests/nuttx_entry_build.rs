//! #127 (Phase 275 W1 tail) — NuttX per-role Entry-pkg demos build-assert.
//!
//! The six `examples/qemu-arm-nuttx/rust/{role}_entry` demos (each a
//! standalone `nros::main!` Entry pkg that bakes board + zenoh RMW through
//! the `nros-board-nuttx-qemu-arm` shim) were the last uncovered Phase 275
//! W1 slice: their standalone `[[bin]]` link used to fail on unresolved
//! NuttX libc/syscall symbols (issue #127). The board-centric image link
//! (RFC-0032 "third leg": dynamic link pieces propagate from the board
//! dep's build.rs via `nros_board_common::nuttx_image_link`; static args in
//! each entry's `.cargo/config.toml`) makes each a real, bootable NuttX
//! flat-build ELF with ZERO entry build.rs. This asserts each links.
//!
//! No compilation at run time (CLAUDE.md "No compilation inside tests"):
//! the artifacts are prebuilt in the **build stage** by the `[[fixture]]`
//! rows in `examples/fixtures.toml` (`just nuttx build-examples` →
//! `fixtures-build.sh nuttx rust`, which runs `nros sync` + cargo with
//! `NUTTX_DIR` exported). This test only resolves + inspects the prebuilt
//! ELF.
//!
//! Gating: NuttX tree absent/unprovisioned → environment skip. Provisioned
//! but artifact missing → hard fail in the full tier (a real fixture gap),
//! `[SKIPPED]` in the `NROS_FIXTURES_OPTIONAL` light tier (see
//! `require_prebuilt_binary`).

use nros_tests::fixtures::nuttx;

/// (role dir suffix, `[[bin]]` name) for each Entry-pkg demo.
const ENTRIES: &[(&str, &str)] = &[
    ("talker", "nuttx_rs_talker_entry"),
    ("listener", "nuttx_rs_listener_entry"),
    ("service-server", "nuttx_rs_service_server_entry"),
    ("service-client", "nuttx_rs_service_client_entry"),
    ("action-server", "nuttx_rs_action_server_entry"),
    ("action-client", "nuttx_rs_action_client_entry"),
];

#[test]
fn nuttx_entry_demos_build() -> nros_tests::TestResult<()> {
    if !nuttx::is_nuttx_available() {
        nros_tests::skip!("NUTTX_DIR unset/invalid — run `just nuttx setup`");
    }
    if !nuttx::is_nuttx_configured() {
        nros_tests::skip!(
            "NuttX tree not configured (no include/nuttx/config.h) — run `just nuttx build`"
        );
    }

    for (role, bin) in ENTRIES {
        let path = nuttx::require_entry_binary(role, bin)?;
        let meta = std::fs::metadata(&path).map_err(|e| {
            nros_tests::TestError::BuildFailed(format!("stat {}: {e}", path.display()))
        })?;
        assert!(
            meta.len() > 0,
            "entry binary is empty: {} ({role}_entry)",
            path.display()
        );
    }
    Ok(())
}
