//! Phase 212.O.5 fixture — demo_entry build.rs.
//!
//! Drives [`nros_build::generate_run_plan`] against the nav2-style
//! `launch/system.launch.xml`. The launch.xml exercises every directive
//! in the Phase 212.N.11 v1 tag set; this build script is the codegen
//! seam that must accept all of them and emit
//! `$OUT_DIR/run_plan.rs`.
//!
//! Same offline-CI fallback shape as the H.3 firmware build.rs: if the
//! git-based `nros-build` dep is unavailable or the planner trips on
//! the launch file, fall through to a placeholder stub so the bin still
//! compiles. The integration test inspects the emitted body and skips
//! when only the placeholder is present (matches H.3 gating).

fn main() {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let launch = manifest.join("launch/system.launch.xml");
    println!("cargo:rerun-if-changed={}", launch.display());
    println!("cargo:rerun-if-changed=build.rs");

    // Issue 0683 — no `workspace_root` override. This Entry pkg lives at the
    // canonical `<workspace>/src/<entry>/Cargo.toml`, so `Options::from_env`'s
    // own `manifest.parent().parent()` is right. It used to sit BESIDE `src/`
    // and patch the derivation here; that also put it outside the tree
    // `nros sync` walks, so no SystemModel was ever resolved for this launch
    // file and the codegen below fell back to a stub for months.
    let opts = nros_build::Options::from_env(&launch);

    match nros_build::generate_run_plan_with(&opts) {
        Ok(path) => eprintln!("nros-build: emitted {}", path.display()),
        Err(err) => {
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
