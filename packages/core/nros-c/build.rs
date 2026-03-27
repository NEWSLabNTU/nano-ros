//! Build script for nros-c
//!
//! 1. Reads nros-c-specific env vars and `DEP_NROS_NODE_*` metadata (from
//!    nros-node's `links = "nros_node"`) to generate `nros_c_config.rs`.
//! 2. Generates a C config header with the opaque executor storage size.
//! 3. Runs cbindgen to generate `include/nros/nros_generated.h`.
//!
//! The opaque storage size is an upper bound computed from the executor's
//! arena, entries, and overhead. A compile-time assertion in executor.rs
//! validates that `size_of::<Executor>()` fits within this bound — if the
//! estimate drifts, the build fails instead of silently corrupting memory.

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());

    generate_config(&out_dir, &manifest_dir);
    generate_header(&manifest_dir);

    // Re-run if source files change (for library rebuild + header regen)
    println!("cargo:rerun-if-changed=src/");
    println!("cargo:rerun-if-changed=cbindgen.toml");
}

/// Generate `nros_c_config.rs` with build-time configurable constants.
fn generate_config(out_dir: &str, manifest_dir: &Path) {
    // --- Executor layout from nros-node (via Cargo `links` metadata) ---
    // nros-node is the single source of truth for these values.
    // MESSAGE_BUFFER_SIZE must equal nrs-node's RX_BUF_SIZE because the
    // C API places entries (SubRawEntry<MESSAGE_BUFFER_SIZE>) into the
    // nros-node arena, which is sized for RX_BUF_SIZE entries.
    let max_cbs = dep_usize("DEP_NROS_NODE_MAX_CBS");
    let arena_size = dep_usize("DEP_NROS_NODE_ARENA_SIZE");
    let message_buffer_size = dep_usize("DEP_NROS_NODE_RX_BUF_SIZE");

    // --- C API knobs (nros-c only, not shared with nros-node) ---
    let let_buffer_size = env_usize("NROS_LET_BUFFER_SIZE", 512);

    // --- Opaque storage upper bound ---
    // This MUST be >= size_of::<Executor>(). We use a generous estimate;
    // the compile-time assertion in executor.rs catches any undercount.
    //
    // Layout: SessionStore + arena + entries + trigger + misc fields
    let session_upper = 512; // SessionStore enum (Owned or Borrowed)
    let entries_upper = max_cbs * 80; // Option<CallbackMeta> ~72 bytes, rounded up
    let overhead = 512; // Trigger + semantics + node_name + namespace + halt_flag + padding
    let executor_bytes = session_upper + arena_size + entries_upper + overhead;
    let executor_opaque_u64s = executor_bytes.div_ceil(8);
    let executor_storage_bytes = executor_opaque_u64s * 8;

    // --- Action storage upper bounds ---
    // These must be >= size_of::<ActionClientInternal>() / size_of::<ActionServerInternal>()
    // for every supported target architecture.
    // Validated at compile time by assertions in opaque_sizes.rs.
    //
    // ActionClientInternal now stores only arena_entry_index (i32) + executor_ptr (*mut c_void).
    // The ActionClientCore lives in the executor's arena.
    let action_client_bytes = 16usize; // i32 + pointer + padding
    let action_client_opaque_u64s = action_client_bytes.div_ceil(8);
    let action_client_storage_bytes = action_client_opaque_u64s * 8;

    let action_server_storage_bytes = 256usize; // ActionServerInternal: ~64 bytes on ARM64
    let action_server_opaque_u64s = action_server_storage_bytes.div_ceil(8);

    let contents = format!(
        "/// Maximum number of handles in an executor \
         (derived from NROS_EXECUTOR_MAX_CBS via nros-node).\n\
         pub const NROS_EXECUTOR_MAX_HANDLES: usize = {max_cbs};\n\
         \n\
         /// Buffer size for LET semantics per handle \
         (set via NROS_LET_BUFFER_SIZE, default 512).\n\
         pub const LET_BUFFER_SIZE: usize = {let_buffer_size};\n\
         \n\
         /// Maximum buffer size for subscription/service data \
         (derived from NROS_SUBSCRIPTION_BUFFER_SIZE via nros-node).\n\
         pub const MESSAGE_BUFFER_SIZE: usize = {message_buffer_size};\n\
         \n\
         /// Inline opaque storage for `Executor` inside `nros_executor_t` (in u64 units).\n\
         /// Upper bound derived from nros-node's MAX_CBS and ARENA_SIZE.\n\
         /// Validated at compile time by `size_of::<Executor>()` assertion.\n\
         pub const EXECUTOR_OPAQUE_U64S: usize = {executor_opaque_u64s};\n\
         \n\
         /// Inline opaque storage for `ActionClientInternal` inside `nros_action_client_t` (in u64 units).\n\
         /// Upper bound: 3 × service_client (384) + subscriber (128) + 3 × message_buffer + overhead.\n\
         /// Validated at compile time by assertion in opaque_sizes.rs.\n\
         pub const ACTION_CLIENT_INTERNAL_OPAQUE_U64S: usize = {action_client_opaque_u64s};\n\
         \n\
         /// Inline opaque storage for `ActionServerInternal` inside `nros_action_server_t` (in u64 units).\n\
         /// Conservative upper bound for a small struct with function pointers.\n\
         /// Validated at compile time by assertion in opaque_sizes.rs.\n\
         pub const ACTION_SERVER_INTERNAL_OPAQUE_U64S: usize = {action_server_opaque_u64s};\n"
    );

    std::fs::write(Path::new(out_dir).join("nros_c_config.rs"), contents).unwrap();

    // Generate C config header with opaque storage sizes
    let c_header = format!(
        "/* Auto-generated by nros-c build.rs — do not edit */\n\
         #ifndef NROS_CONFIG_GENERATED_H\n\
         #define NROS_CONFIG_GENERATED_H\n\
         \n\
         /** Inline opaque storage size (bytes) for nros_executor_t. */\n\
         #define NROS_EXECUTOR_STORAGE_SIZE {executor_storage_bytes}\n\
         \n\
         /** Inline opaque storage size (bytes) for nros_action_client_t._internal. */\n\
         #define NROS_ACTION_CLIENT_STORAGE_SIZE {action_client_storage_bytes}\n\
         \n\
         /** Inline opaque storage size (bytes) for nros_action_server_t._internal. */\n\
         #define NROS_ACTION_SERVER_STORAGE_SIZE {action_server_storage_bytes}\n\
         \n\
         #endif /* NROS_CONFIG_GENERATED_H */\n"
    );
    let config_header_path = manifest_dir.join("include/nros/nros_config_generated.h");
    std::fs::write(config_header_path, c_header).unwrap();
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

/// Read a usize from a `DEP_*` environment variable (Cargo `links` metadata).
///
/// Panics if the variable is missing — this means nros-node's `links` export
/// is broken, which is a build system bug that should fail loudly.
fn dep_usize(name: &str) -> usize {
    env::var(name)
        .unwrap_or_else(|_| {
            panic!("{name} not set — is nros-node's `links = \"nros_node\"` configured?")
        })
        .parse()
        .unwrap_or_else(|_| panic!("{name} is not a valid usize"))
}
