//! Build script for xrce-smoltcp
//!
//! Reads XRCE_* environment variables and generates `xrce_smoltcp_config.rs`
//! with compile-time configurable constants.

use std::env;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    let udp_meta_count = env_usize("XRCE_UDP_META_COUNT", 4);

    let contents = format!(
        "/// Number of packet metadata slots per direction \
         (set via XRCE_UDP_META_COUNT, default 4).\n\
         pub(crate) const UDP_META_COUNT: usize = {udp_meta_count};\n"
    );

    std::fs::write(Path::new(&out_dir).join("xrce_smoltcp_config.rs"), contents).unwrap();
}

/// Read a usize from an environment variable, falling back to a default.
fn env_usize(name: &str, default: usize) -> usize {
    println!("cargo:rerun-if-env-changed={name}");
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
