//! Build script for nros-rmw-xrce
//!
//! Reads XRCE_* environment variables and generates `xrce_config.rs`
//! with compile-time configurable constants.

use std::env;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    // Slot counts
    let max_subscribers = env_usize("XRCE_MAX_SUBSCRIBERS", 8);
    let max_service_servers = env_usize("XRCE_MAX_SERVICE_SERVERS", 4);
    let max_service_clients = env_usize("XRCE_MAX_SERVICE_CLIENTS", 4);
    let buffer_size = env_usize("XRCE_BUFFER_SIZE", 1024);
    let stream_history = env_usize("XRCE_STREAM_HISTORY", 4);

    // Validate stream history >= 2
    if stream_history < 2 {
        panic!(
            "XRCE_STREAM_HISTORY={stream_history} is invalid. \
             Must be >= 2. XRCE reliable streams with history=1 fail to recycle \
             the single slot between separate run_session_until_all_status calls, \
             causing entity creation timeouts."
        );
    }

    // Timeouts and retries
    let entity_creation_timeout_ms = env_usize("XRCE_ENTITY_CREATION_TIMEOUT_MS", 1000);
    let service_reply_timeout_ms = env_usize("XRCE_SERVICE_REPLY_TIMEOUT_MS", 1000);
    let service_reply_retries = env_usize("XRCE_SERVICE_REPLY_RETRIES", 5);

    // Generate xrce_config.rs
    let contents = format!(
        "/// Maximum subscribers that can be created simultaneously \
         (set via XRCE_MAX_SUBSCRIBERS, default 8).\n\
         pub const MAX_SUBSCRIBERS: usize = {max_subscribers};\n\
         \n\
         /// Maximum service servers that can be created simultaneously \
         (set via XRCE_MAX_SERVICE_SERVERS, default 4).\n\
         pub const MAX_SERVICE_SERVERS: usize = {max_service_servers};\n\
         \n\
         /// Maximum service clients that can be created simultaneously \
         (set via XRCE_MAX_SERVICE_CLIENTS, default 4).\n\
         pub const MAX_SERVICE_CLIENTS: usize = {max_service_clients};\n\
         \n\
         /// Size of each receive buffer slot in bytes \
         (set via XRCE_BUFFER_SIZE, default 1024).\n\
         pub const BUFFER_SIZE: usize = {buffer_size};\n\
         \n\
         /// Stream history depth. Must be >= 2 \
         (set via XRCE_STREAM_HISTORY, default 4).\n\
         pub const STREAM_HISTORY: u16 = {stream_history};\n\
         pub const STREAM_HISTORY_USIZE: usize = STREAM_HISTORY as usize;\n\
         \n\
         /// Timeout for entity creation confirmation in milliseconds \
         (set via XRCE_ENTITY_CREATION_TIMEOUT_MS, default 1000).\n\
         pub const ENTITY_CREATION_TIMEOUT_MS: core::ffi::c_int = {entity_creation_timeout_ms};\n\
         \n\
         /// Timeout for service client reply in milliseconds \
         (set via XRCE_SERVICE_REPLY_TIMEOUT_MS, default 1000).\n\
         pub const SERVICE_REPLY_TIMEOUT_MS: core::ffi::c_int = {service_reply_timeout_ms};\n\
         \n\
         /// Maximum number of retries for service client replies \
         (set via XRCE_SERVICE_REPLY_RETRIES, default 5).\n\
         pub const SERVICE_REPLY_RETRIES: usize = {service_reply_retries};\n"
    );

    std::fs::write(Path::new(&out_dir).join("xrce_config.rs"), contents).unwrap();
}

/// Read a usize from an environment variable, falling back to a default.
fn env_usize(name: &str, default: usize) -> usize {
    println!("cargo:rerun-if-env-changed={name}");
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
