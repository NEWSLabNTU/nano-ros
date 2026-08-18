//! Phase 212.O.3 — Entry pkg build.rs (board-agnostic).
//!
//! **This file is byte-identical to its sibling Entry pkg's
//! `build.rs`.** The O.3 acceptance assertion compares the two
//! `OUT_DIR/run_plan.rs` outputs for byte-identity; that only holds
//! if the inputs are the same (launch XML path resolves to the same
//! canonical content + nros-build crate version is the same +
//! build.rs source is the same).
//!
//! Consumes `../launch/system.launch.xml` — both Entry pkgs read the
//! SAME launch file through a relative path that resolves to the
//! shared fixture-level `launch/` dir.

fn main() {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // `<fixture>/src/<entry>/` — the canonical layout, so the workspace root is
    // two levels up and the SHARED `launch/` dir sits there beside `src/`.
    let fixture_root = manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("manifest grandparent")
        .to_path_buf();
    let launch = fixture_root.join("src/shared_bringup/launch/system.launch.xml");
    println!("cargo:rerun-if-changed={}", launch.display());
    println!("cargo:rerun-if-changed=build.rs");

    // Issue 0683 — no `workspace_root` override: `Options::from_env`'s own
    // `manifest.parent().parent()` is now right. These Entry pkgs used to sit
    // BESIDE `src/` and patch the derivation here, which also put them outside
    // the tree `nros sync` walks — so no SystemModel was resolved for this
    // launch file and codegen fell back to the stub below.
    let opts = nros_build::Options::from_env(&launch);

    match nros_build::generate_run_plan_with(&opts) {
        Ok(path) => eprintln!("nros-build: emitted {}", path.display()),
        Err(err) => {
            // Offline / network-blocked fallback. Stub keeps the bin
            // linkable so the test can still surface a meaningful
            // skip!. The integration test detects this stub and skips
            // the byte-identical assertion (one Entry can't prove
            // codegen identity by itself).
            eprintln!("nros-build: codegen skipped: {err:?}");
            let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
            let stub = std::path::Path::new(&out_dir).join("run_plan.rs");
            // Issue 0683 — the stub carries WHY. Without it the only trace of the
            // real error is cargo's captured build-script stderr, which nobody
            // reads, so the consuming test invented a reason ("play_launch_parser
            // absent") that had been wrong since phase-330 moved SystemModels to
            // build output. A fallback that hides its cause is how a fixture
            // asserts nothing for months.
            let reason = format!("{err:?}").replace('\n', " ");
            let body = format!(
                "// Placeholder — nros-build codegen unavailable.\n\
                 // reason: {reason}\n\
                 pub fn run_plan(\n    \
                     runtime: &mut ::nros_platform::RuntimeCtx<'_>,\n\
                 ) -> ::core::result::Result<(), ::nros_platform::RuntimeError> {{\n    \
                     let _ = runtime;\n    \
                     Ok(())\n\
                 }}\n"
            );
            std::fs::write(&stub, body).expect("write stub run_plan.rs");
        }
    }
}
