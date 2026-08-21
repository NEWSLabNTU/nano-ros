//! Build script for nros-params
//!
//! Reads NROS_* environment variables and generates `nros_params_config.rs`
//! with compile-time configurable constants for parameter storage limits.

use std::{env, path::Path};

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
/// Resolve a sizing knob the way every other nros build script does.
///
/// Issue 0460 — a Zephyr RUST image inherits NONE of cmake's `set(ENV{...})`
/// knob exports: that call only touches the configure-time process, the C lane
/// re-bakes them into its own command, and zephyr-lang-rust's
/// `rust_cargo_application` builds a fresh one that inherits nothing. So a plain
/// `env::var` here reads the crate DEFAULT on Zephyr no matter what Kconfig
/// says, and the two lanes then disagree about a compile-time constant — an
/// 0135-class ABI split, silently.
///
/// `knob_usize` reads `$DOTCONFIG` for `CONFIG_<name>` and falls back to the
/// environment, which is what `nros-node`'s identical helper does. Gated by
/// `check-kconfig-knob-forwarding`, which went red when #0749 taught
/// `zephyr/cmake/nros_cargo_build.cmake` to forward `NROS_MAX_PARAMETERS` while
/// this crate — its only reader — still used the env-only form.
fn env_usize(name: &str, default: usize) -> usize {
    nros_zephyr_build::knob_usize(name, &format!("CONFIG_{name}"), default)
}
