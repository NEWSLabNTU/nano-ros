//! Shared setup for this crate's integration tests.
//!
//! One spelling, for the same reason `src/test_support.rs` exists (issue 0455):
//! the differences between hand-written copies were the bug.

/// Scope model discovery to the fixture under test, once per test process.
///
/// `model_search_paths` consults ambient `$OUT_DIR`, which is right when the
/// caller IS the build script of the crate whose model is being resolved — the
/// zephyr module and the pio extra_script both shell `codegen system` that way.
/// It is wrong in a test: `nros-cli-core` has a build script, so a test process
/// inherits an `OUT_DIR` belonging to a DIFFERENT crate, and the build-output
/// candidate is keyed on the bringup's directory NAME. Fixtures here call their
/// bringup `demo_bringup`, as does whatever last generated into that directory,
/// so discovery matched across two unrelated workspaces and loaded a stale
/// model.
///
/// Three binaries hit it: `codegen_system_basic` (wrong provenance, and a model
/// whose components the fixture never had), the `cmd::codegen_system` lib tests,
/// and `executor_sizing_bake_gate`, where the bake read someone else's entity
/// counts and the over-capacity check it exists to make simply did not trip.
///
/// Reordering the search does not fix it: `OUT_DIR` precedes the committed
/// fallback by design, since a build artifact should outrank a checked-in copy.
/// Pointing it at an empty per-process directory removes the collision without
/// changing the order these tests exercise.
pub fn isolate_model_discovery() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let dir =
            std::env::temp_dir().join(format!("nros-cli-core-it-outdir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch OUT_DIR");
        // SAFETY: once, before any test body reads the environment; every
        // reader is this process's own model resolution.
        unsafe {
            std::env::set_var("OUT_DIR", &dir);
            std::env::remove_var("NROS_MODEL_DIR");
        }
    });
}
