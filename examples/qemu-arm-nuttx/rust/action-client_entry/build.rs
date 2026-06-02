//! Phase 212.N.7 step-2 — Entry pkg `build.rs`. See talker_entry/build.rs.

fn main() {
    let launch =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("launch/system.launch.xml");
    println!("cargo:rerun-if-changed={}", launch.display());
    println!("cargo:rerun-if-changed=build.rs");

    match nros_build::generate_run_plan(&launch) {
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
