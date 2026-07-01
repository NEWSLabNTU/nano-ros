//! Phase 275 W1 (#102 H2) — ThreadX-Linux per-role Entry-pkg demos build-assert.
//!
//! The six `examples/threadx-linux/rust/{role}_entry` demos (each a standalone
//! `nros::main!` Entry pkg that bakes board + zenoh RMW through the
//! `nros-board-threadx-linux` shim) shipped with no fixture — built by nothing,
//! tested by nothing. This asserts each is a real, linkable host binary.
//!
//! No compilation at run time (CLAUDE.md "No compilation inside tests"): the
//! artifacts are prebuilt in the **build stage** by the `[[fixture]]` rows in
//! `examples/fixtures.toml` (`just threadx_linux build-examples` →
//! `fixtures-build.sh threadx-linux rust`, which runs `nros sync` + cargo).
//! This test only resolves + inspects the prebuilt ELF.
//!
//! Gating: ThreadX/NetX not provisioned → environment skip. Provisioned but
//! artifact missing → hard fail in the full tier (a real fixture gap),
//! `[SKIPPED]` in the `NROS_FIXTURES_OPTIONAL` light tier (see
//! `require_prebuilt_binary`).

use nros_tests::fixtures::threadx_linux;

/// (role dir suffix, `[[bin]]` name) for each Entry-pkg demo.
const ENTRIES: &[(&str, &str)] = &[
    ("talker", "threadx_linux_rs_talker_entry"),
    ("listener", "threadx_linux_rs_listener_entry"),
    ("service-server", "threadx_linux_rs_service_server_entry"),
    ("service-client", "threadx_linux_rs_service_client_entry"),
    ("action-server", "threadx_linux_rs_action_server_entry"),
    ("action-client", "threadx_linux_rs_action_client_entry"),
];

#[test]
fn threadx_linux_entry_demos_build() -> nros_tests::TestResult<()> {
    if !threadx_linux::is_threadx_available() {
        nros_tests::skip!("THREADX_DIR unset/invalid — run `just threadx_linux setup`");
    }
    if !threadx_linux::is_nsos_netx_available() {
        nros_tests::skip!("NetX Duo (NSOS) unavailable — run `just threadx_linux setup`");
    }

    for (role, bin) in ENTRIES {
        let path = threadx_linux::require_entry_binary(role, bin)?;
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
