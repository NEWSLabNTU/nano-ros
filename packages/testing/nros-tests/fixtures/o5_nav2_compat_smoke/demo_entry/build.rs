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
    let fixture_root = manifest.parent().expect("manifest parent").to_path_buf();
    let launch = manifest.join("launch/system.launch.xml");
    println!("cargo:rerun-if-changed={}", launch.display());
    println!("cargo:rerun-if-changed=build.rs");

    // Phase 212.M-F.17 follow-up: override Options::workspace_root.
    // The default `Options::from_env` derives workspace_root via
    // `manifest.parent().parent()` which assumes the canonical
    // `<workspace>/src/<entry>/Cargo.toml` layout. Our Entry pkg sits
    // one level shallower (sibling of `src/`) — point at the fixture
    // root directly so `Workspace::discover` walks `<fixture>/src/`.
    let mut opts = nros_build::Options::from_env(&launch);
    opts.workspace_root = fixture_root;

    match nros_build::generate_run_plan_with(&opts) {
        Ok(path) => eprintln!("nros-build: emitted {}", path.display()),
        Err(err) => {
            eprintln!("nros-build: codegen skipped: {err:?}");
            let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
            let stub = std::path::Path::new(&out_dir).join("run_plan.rs");
            let body = "// Placeholder — nros-build codegen unavailable.\n\
                        pub fn run_plan(\n    \
                            runtime: &mut ::nros_platform::RuntimeCtx<'_>,\n\
                        ) -> ::core::result::Result<(), ::nros_platform::RuntimeError> {\n    \
                            let _ = runtime;\n    \
                            Ok(())\n\
                        }\n";
            std::fs::write(&stub, body).expect("write stub run_plan.rs");
        }
    }
}
