//! Build script for nros-params
//!
//! Reads NROS_* environment variables and generates `nros_params_config.rs`
//! with compile-time configurable constants for parameter storage limits.

use std::env;
use std::path::Path;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    let max_parameters = env_usize("NROS_MAX_PARAMETERS", 32);
    let max_param_name_len = env_usize("NROS_MAX_PARAM_NAME_LEN", 64);
    let max_string_value_len = env_usize("NROS_MAX_STRING_VALUE_LEN", 256);
    let max_array_len = env_usize("NROS_MAX_ARRAY_LEN", 32);
    let max_byte_array_len = env_usize("NROS_MAX_BYTE_ARRAY_LEN", 256);

    let contents = format!(
        "/// Maximum number of parameters the server can store \
         (set via NROS_MAX_PARAMETERS, default 32).\n\
         pub const MAX_PARAMETERS: usize = {max_parameters};\n\
         \n\
         /// Maximum length for parameter names \
         (set via NROS_MAX_PARAM_NAME_LEN, default 64).\n\
         pub const MAX_PARAM_NAME_LEN: usize = {max_param_name_len};\n\
         \n\
         /// Maximum length for parameter string values \
         (set via NROS_MAX_STRING_VALUE_LEN, default 256).\n\
         pub const MAX_STRING_VALUE_LEN: usize = {max_string_value_len};\n\
         \n\
         /// Maximum length for array parameters \
         (set via NROS_MAX_ARRAY_LEN, default 32).\n\
         pub const MAX_ARRAY_LEN: usize = {max_array_len};\n\
         \n\
         /// Maximum length for byte array parameters \
         (set via NROS_MAX_BYTE_ARRAY_LEN, default 256).\n\
         pub const MAX_BYTE_ARRAY_LEN: usize = {max_byte_array_len};\n"
    );

    std::fs::write(Path::new(&out_dir).join("nros_params_config.rs"), contents).unwrap();
}

/// Read a usize from an environment variable, falling back to a default.
fn env_usize(name: &str, default: usize) -> usize {
    println!("cargo:rerun-if-env-changed={name}");
    env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
