//! Shared setup for this crate's integration tests.
//!
//! One spelling, for the same reason `src/test_support.rs` exists (issue 0455):
//! the differences between hand-written copies were the bug.

// Each integration test binary compiles this module separately and uses only
// the helpers it needs, so anything the others use is dead code from here.
#![allow(dead_code)]

use std::path::PathBuf;

/// The nano-ros checkout this test binary was compiled from.
fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is `packages/cli/nros-cli-core`.
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf()
}

/// The in-tree `nros-launch-resolve`, as a test that SPAWNS it must name it —
/// by absolute path, never `$PATH` (issue 0285), and asserting rather than
/// skipping when it is not built.
///
/// Issue 1411 folded six hand-written copies of this into one. They agreed at
/// the time, which is the usual state of a copied idiom just before it stops
/// agreeing; what made it worth collapsing is that a SEVENTH site had drifted
/// in exactly the way CLAUDE.md warns about. `entry_typed_plan` printed
/// `[SKIPPED] ...` and returned an empty path, so on any host without the
/// resolver it reported `ok` having executed no assertion — and `cargo test`
/// swallows the `eprintln!` that would have explained it. The lane those tests
/// share (`check-cli-tests`) builds the resolver as its own CI step, so a
/// missing one is an unmet precondition, not a supported configuration.
///
/// This is deliberately NOT
/// `nros_orchestration_ir::model_location::launch_resolver_bin()`. That ladder
/// ($NROS_LAUNCH_RESOLVE, $NROS_REPO_DIR, $NROS_HOME/bin) is the right question
/// for a test that exercises code which RESOLVES the binary; these tests run it
/// themselves and mean the pinned in-tree build, as their headers say.
pub fn pinned_launch_resolver() -> PathBuf {
    let resolver =
        repo_root().join("packages/cli/nros-launch-resolve/target/release/nros-launch-resolve");
    assert!(
        resolver.is_file(),
        "nros-launch-resolve not built at {} -- run `just setup-launch-resolve`",
        resolver.display()
    );
    resolver
}

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
