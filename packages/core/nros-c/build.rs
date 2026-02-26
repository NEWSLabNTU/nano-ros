//! Build script for nros-c
//!
//! 1. Reads NROS_* environment variables and generates `nros_c_config.rs`
//!    with compile-time configurable constants for the executor.
//! 2. Runs cbindgen to generate `include/nros/nros_generated.h` from
//!    Rust `#[repr(C)]` types.  This file is used for compile-time
//!    drift detection (via `-DNROS_DRIFT_CHECK`) against the
//!    hand-written per-module headers.

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    generate_config(&out_dir);
    generate_header(&manifest_dir);

    // Re-run if source files change (for library rebuild + header regen)
    println!("cargo:rerun-if-changed=src/");
    println!("cargo:rerun-if-changed=cbindgen.toml");
}

/// Generate `nros_c_config.rs` with build-time configurable constants.
fn generate_config(out_dir: &str) {
    let executor_max_handles = env_usize("NROS_EXECUTOR_MAX_HANDLES", 16);
    let max_subscriptions = env_usize("NROS_MAX_SUBSCRIPTIONS", 8);
    let max_timers = env_usize("NROS_MAX_TIMERS", 8);
    let max_services = env_usize("NROS_MAX_SERVICES", 4);
    let let_buffer_size = env_usize("NROS_LET_BUFFER_SIZE", 512);
    let message_buffer_size = env_usize("NROS_MESSAGE_BUFFER_SIZE", 4096);

    let contents = format!(
        "/// Maximum number of handles in an executor \
         (set via NROS_EXECUTOR_MAX_HANDLES, default 16).\n\
         pub const NROS_EXECUTOR_MAX_HANDLES: usize = {executor_max_handles};\n\
         \n\
         /// Maximum number of subscriptions in an executor \
         (set via NROS_MAX_SUBSCRIPTIONS, default 8).\n\
         pub const NROS_MAX_SUBSCRIPTIONS: usize = {max_subscriptions};\n\
         \n\
         /// Maximum number of timers in an executor \
         (set via NROS_MAX_TIMERS, default 8).\n\
         pub const NROS_MAX_TIMERS: usize = {max_timers};\n\
         \n\
         /// Maximum number of services in an executor \
         (set via NROS_MAX_SERVICES, default 4).\n\
         pub const NROS_MAX_SERVICES: usize = {max_services};\n\
         \n\
         /// Buffer size for LET semantics per handle \
         (set via NROS_LET_BUFFER_SIZE, default 512).\n\
         pub const LET_BUFFER_SIZE: usize = {let_buffer_size};\n\
         \n\
         /// Maximum buffer size for subscription/service data \
         (set via NROS_MESSAGE_BUFFER_SIZE, default 4096).\n\
         pub const MESSAGE_BUFFER_SIZE: usize = {message_buffer_size};\n"
    );

    std::fs::write(Path::new(out_dir).join("nros_c_config.rs"), contents).unwrap();
}

/// Generate `include/nros/nros_generated.h` using cbindgen.
///
/// cbindgen reads Rust source files and generates C header declarations
/// for all `#[repr(C)]` structs, enums, type aliases, constants, and
/// `extern "C"` functions. The generated header is the single source of
/// truth for C/Rust type layout compatibility.
fn generate_header(manifest_dir: &Path) {
    let config_path = manifest_dir.join("cbindgen.toml");
    let output_path = manifest_dir.join("include/nros/nros_generated.h");

    let config = match cbindgen::Config::from_file(&config_path) {
        Ok(c) => c,
        Err(e) => {
            println!("cargo:warning=Failed to load cbindgen config: {e}");
            return;
        }
    };

    let result = cbindgen::Builder::new()
        .with_crate(manifest_dir)
        .with_config(config)
        .generate();

    match result {
        Ok(bindings) => {
            bindings.write_to_file(&output_path);
        }
        Err(e) => {
            // cbindgen may fail if dependencies aren't available (e.g.,
            // during no-default-features builds). This is expected —
            // the generated header is only needed for builds with an
            // RMW backend enabled.
            println!("cargo:warning=cbindgen header generation skipped: {e}");
        }
    }
}

/// Read a usize from an environment variable, falling back to a default.
fn env_usize(name: &str, default: usize) -> usize {
    println!("cargo:rerun-if-env-changed={name}");
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
