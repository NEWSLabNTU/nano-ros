//! Build script for nros-params
//!
//! Reads NROS_* environment variables and generates `nros_params_config.rs`
//! with compile-time configurable constants for parameter storage limits.

use std::{env, path::Path};

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    // phase-400 W6 — the platform and board rungs sit under the env/Kconfig
    // front-end each of these already had. phase-292's ASI consumer needed
    // `NROS_MAX_PARAMETERS=256` and set it in a `build.sh`: a board fact living
    // in a shell script because there was nowhere to declare it.
    //
    // `None` when no lane names a platform (a bare `cargo build`), and then
    // every knob below is exactly the env-or-default it always was.
    let rungs = nros_board_common::platform_config::BuildRungs::from_build_env()
        .map(|r| r.param_rungs())
        .unwrap_or_default();

    let max_parameters = knob("NROS_MAX_PARAMETERS", rungs.max_parameters, 32);
    let max_param_name_len = knob("NROS_MAX_PARAM_NAME_LEN", rungs.max_param_name_len, 64);
    let max_string_value_len = knob("NROS_MAX_STRING_VALUE_LEN", rungs.max_string_value_len, 256);
    let max_array_len = knob("NROS_MAX_ARRAY_LEN", rungs.max_array_len, 32);
    let max_byte_array_len = knob("NROS_MAX_BYTE_ARRAY_LEN", rungs.max_byte_array_len, 256);

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

/// One parameter knob: env → Kconfig → the descriptor rung → built-in default.
///
/// The front-end keeps winning. Migrating a knob into the ladder must not take
/// an operator's override away, which is half of this wave's own gate.
fn knob(name: &str, rung: Option<usize>, default: usize) -> usize {
    println!("cargo:rerun-if-env-changed={name}");
    if let Some(v) = env::var(name).ok().and_then(|v| v.trim().parse().ok()) {
        return v;
    }
    if let Some(v) = nros_zephyr_build::dotconfig_usize(&format!("CONFIG_{name}")) {
        return v;
    }
    rung.unwrap_or(default)
}
