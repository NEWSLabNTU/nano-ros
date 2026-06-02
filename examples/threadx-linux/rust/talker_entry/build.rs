//! Phase 212.N.7 step-2 — Entry pkg `build.rs`.
//!
//! Drives the [`nros_build::generate_run_plan`] codegen library to
//! emit `$OUT_DIR/run_plan.rs`. The Entry pkg's `main.rs` `include!`s
//! the emitted file and calls `run_plan(runtime)` from inside
//! `<ThreadxLinux as BoardEntry>::run`'s setup closure.
//!
//! Step-2 ships no `launch/*.xml` — `nros_build::generate_run_plan`
//! falls through to a stub body (mirrors the §212.N.7 step-1 POC).
//! A future commit lands a real `launch/system.launch.xml` that maps
//! the sibling Component pkg's `register` fn to a `<node>` entry.

fn main() {
    let launch =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("launch/system.launch.xml");
    println!("cargo:rerun-if-changed={}", launch.display());
    println!("cargo:rerun-if-changed=build.rs");

    match nros_build::generate_run_plan(&launch) {
        Ok(path) => eprintln!("nros-build: emitted {}", path.display()),
        Err(err) => {
            // Step-2 has no launch file — write a stub `run_plan`
            // whose body is empty (`Ok(())`). The `main.rs` still
            // compiles; `Executor::spin` enters cleanly. The
            // §212.N.4 follow-up wires a real launch.xml.
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
