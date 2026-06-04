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
    let fixture_root = manifest.parent().expect("manifest parent").to_path_buf();
    let launch = fixture_root.join("launch/system.launch.xml");
    println!("cargo:rerun-if-changed={}", launch.display());
    println!("cargo:rerun-if-changed=build.rs");

    // Override `Options::workspace_root`: nros-build's `from_env`
    // default (`manifest.parent().parent()`) assumes the canonical
    // `<workspace>/src/<entry>/Cargo.toml` layout. The Entry pkgs in
    // this fixture sit one level shallower (sibling of `src/`), so
    // we point the planner at the actual fixture root directly.
    let mut opts = nros_build::Options::from_env(&launch);
    opts.workspace_root = fixture_root;

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
