//! Phase 212.N.7 step-5 — firmware build.rs.
//!
//! Drives the [`nros_build::generate_run_plan`] codegen library to emit
//! `$OUT_DIR/run_plan.rs`. `main.rs` `include!`s the emitted file and
//! invokes `run_plan(runtime)` from inside `<Mps2An385 as
//! BoardEntry>::run`'s setup closure.

fn main() {
    let launch =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("launch/system.launch.xml");
    println!("cargo:rerun-if-changed={}", launch.display());
    println!("cargo:rerun-if-changed=build.rs");

    match nros_build::generate_run_plan(&launch) {
        Ok(path) => eprintln!("nros-build: emitted {}", path.display()),
        Err(err) => {
            // Fixture fallback: if codegen can't load (no network
            // access in offline CI for the `nros-build` git dep, or
            // missing launch file), fall through to an empty
            // placeholder so the bin still compiles. Production Entry
            // pkgs would `panic!` here.
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
