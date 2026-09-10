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

    // phase-446 W4 -- what the image's contract DECLARES, carried by the CMake
    // road (`NanoRosEntityFacts.cmake`) as a DEFAULT below every stated rung.
    // Spelled literally so the wire is greppable (`check-declared-fact-carriers`).
    // The Zephyr road delivers the same numbers as the knob itself
    // (`_nros_resolve_derivable_knob`), and only the NEEDS facts by these names.
    println!("cargo:rerun-if-env-changed=NROS_DECLARED_MAX_PARAMETERS");
    println!("cargo:rerun-if-env-changed=NROS_DECLARED_MAX_PARAM_NAME_LEN");
    println!("cargo:rerun-if-env-changed=NROS_DECLARED_MAX_STRING_VALUE_LEN");
    println!("cargo:rerun-if-env-changed=NROS_DECLARED_MAX_ARRAY_LEN");
    println!("cargo:rerun-if-env-changed=NROS_DECLARED_MAX_BYTE_ARRAY_LEN");
    println!("cargo:rerun-if-env-changed=NROS_DECLARED_PARAM_NEEDS_MAX_STRING_VALUE_LEN");
    println!("cargo:rerun-if-env-changed=NROS_DECLARED_PARAM_NEEDS_MAX_ARRAY_LEN");
    println!("cargo:rerun-if-env-changed=NROS_DECLARED_PARAM_NEEDS_MAX_BYTE_ARRAY_LEN");

    let max_parameters = knob(
        "NROS_MAX_PARAMETERS",
        rungs.max_parameters,
        declared("NROS_DECLARED_MAX_PARAMETERS"),
        32,
    );
    let max_param_name_len = knob(
        "NROS_MAX_PARAM_NAME_LEN",
        rungs.max_param_name_len,
        declared("NROS_DECLARED_MAX_PARAM_NAME_LEN"),
        64,
    );
    let max_string_value_len = capacity(
        "NROS_MAX_STRING_VALUE_LEN",
        "max_string_value_len",
        rungs.max_string_value_len,
        declared("NROS_DECLARED_MAX_STRING_VALUE_LEN"),
        needs("NROS_DECLARED_PARAM_NEEDS_MAX_STRING_VALUE_LEN"),
        256,
    );
    let max_array_len = capacity(
        "NROS_MAX_ARRAY_LEN",
        "max_array_len",
        rungs.max_array_len,
        declared("NROS_DECLARED_MAX_ARRAY_LEN"),
        needs("NROS_DECLARED_PARAM_NEEDS_MAX_ARRAY_LEN"),
        32,
    );
    let max_byte_array_len = capacity(
        "NROS_MAX_BYTE_ARRAY_LEN",
        "max_byte_array_len",
        rungs.max_byte_array_len,
        declared("NROS_DECLARED_MAX_BYTE_ARRAY_LEN"),
        needs("NROS_DECLARED_PARAM_NEEDS_MAX_BYTE_ARRAY_LEN"),
        256,
    );

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

/// The STATED rungs of one parameter knob: env, then Kconfig, then the descriptor
/// rung. `None` when nobody states a number.
///
/// The front-end keeps winning. Migrating a knob into the ladder must not take
/// an operator's override away, which is half of this wave's own gate. A
/// Kconfig `-1` is the tree's DERIVE sentinel and reads as no value here
/// (`dotconfig_usize` parses a `usize`).
fn stated(name: &str, rung: Option<usize>) -> Option<usize> {
    println!("cargo:rerun-if-env-changed={name}");
    if let Some(v) = env::var(name).ok().and_then(|v| v.trim().parse().ok()) {
        return Some(v);
    }
    if let Some(v) = nros_zephyr_build::dotconfig_usize(&format!("CONFIG_{name}")) {
        return Some(v);
    }
    rung
}

/// One count knob: the stated rungs, then what the contract declared, then
/// the built-in default -- env > Kconfig/board > derived > crate default.
fn knob(name: &str, rung: Option<usize>, declared: Option<usize>, default: usize) -> usize {
    stated(name, rung).or(declared).unwrap_or(default)
}

/// A number the CMake road handed down; absent means "no answer", never 0.
fn declared(key: &str) -> Option<usize> {
    env::var(key).ok().and_then(|v| v.trim().parse().ok())
}

/// `<node>:<param>:<type>` -- the declared parameter whose type needs a
/// capacity the contract cannot give (`DeclaredParam::token` in nros-cli-core).
fn needs(key: &str) -> Option<String> {
    env::var(key).ok().filter(|v| !v.trim().is_empty())
}

/// phase-446 W4 -- a per-slot CAPACITY: a string, array or byte-array length.
///
/// The contract states names and types, never sizes. So when no declared type
/// uses a capacity the declaration derives 0, and when one does, the number
/// must come from a stated rung (the board's `[knobs.params]`, Kconfig on
/// Zephyr, or the environment). This is the one place every rung meets, so it
/// is where "a declared string and no board capacity" refuses, on every lane.
fn capacity(
    name: &str,
    board_key: &str,
    rung: Option<usize>,
    declared: Option<usize>,
    needed_by: Option<String>,
    default: usize,
) -> usize {
    match (stated(name, rung), needed_by) {
        (Some(0), Some(who)) => refuse(name, board_key, &who, "is stated as 0"),
        (Some(v), _) => v,
        (None, Some(who)) => refuse(name, board_key, &who, "is stated nowhere"),
        (None, None) => declared.unwrap_or(default),
    }
}

fn refuse(name: &str, board_key: &str, who: &str, what: &str) -> ! {
    let mut parts = who.rsplitn(3, ':');
    let ty = parts.next().unwrap_or("?");
    let param = parts.next().unwrap_or(who);
    let node = parts.next().unwrap_or("?");
    panic!(
        "\n\nnros-params: parameter `{param}` on node `{node}` is declared `{ty}` in its \
         contract, so the parameter store needs {name}, and {name} {what}.\n\
         A contract states a parameter's name and type; how long a string or an array \
         may be is a fact about the BOARD it is built for. State {name} as one of:\n  \
         - the board's `[knobs.params] {board_key} = <n>`\n  \
         - CONFIG_{name}=<n> in the image's Kconfig (Zephyr)\n  \
         - the {name} environment variable\n\
         (phase-446 W4)\n"
    );
}
