//! Phase 212.N.7 step-2 — Entry pkg `build.rs` (FreeRTOS MPS2-AN385).
//!
//! Drives the [`nros_build::generate_run_plan`] codegen library to
//! emit `$OUT_DIR/run_plan.rs`. `main.rs` `include!`s the emitted file
//! and calls `run_plan(runtime)` from inside `<Mps2An385 as
//! BoardEntry>::run`'s setup closure.
//!
//! This step-2 sweep ships an empty `launch/system.launch.xml` so the
//! emitted `run_plan` body is `Ok(())`. The Node pkg's
//! `register()` wrapper is presently a TODO stub (see the sibling
//! Node pkg's `lib.rs` for the gap); wiring the live registration
//! path is step-3+ work.

fn main() {
    let launch =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("launch/system.launch.xml");
    println!("cargo:rerun-if-changed={}", launch.display());
    println!("cargo:rerun-if-changed=build.rs");

    match nros_build::generate_run_plan(&launch) {
        Ok(path) => eprintln!("nros-build: emitted {}", path.display()),
        Err(err) => {
            // Step-2 sweep: don't fail the build on a missing/empty
            // launch file — fall through to an empty placeholder so
            // the Entry pkg's `main.rs` still compiles. Production
            // Entry pkgs would `panic!` here.
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
