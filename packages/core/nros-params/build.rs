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

    // phase-454 (issue 1408, RFC-0100 D4) -- the DESCRIPTOR road for the same
    // five facts, read once. `[params]` is derived from the contract alone, so
    // BOTH producers state it: a cargo leaf and a workspace/cmake image get the
    // same numbers from the same `ParamDeclarations` derivation.
    //
    // It sits at the rung the env carrier already occupied -- below every
    // STATED rung (env, Kconfig, board) and above the crate default -- and the
    // carrier is read BELOW it, not replaced. See `contract` for why the
    // carriers stay.
    let desc = sizing_descriptor();
    let params = desc.as_ref().map(|d| &d.params);

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
        contract(
            params.map(|p| p.max_parameters()),
            declared("NROS_DECLARED_MAX_PARAMETERS"),
        ),
        32,
    );
    let max_param_name_len = knob(
        "NROS_MAX_PARAM_NAME_LEN",
        rungs.max_param_name_len,
        contract(
            params.map(|p| p.max_param_name_len()),
            declared("NROS_DECLARED_MAX_PARAM_NAME_LEN"),
        ),
        64,
    );
    // The three CAPACITIES are BOARD facts (RFC-0100 D1 *target*), owned by
    // `[board.knobs.params]`, and they are deliberately NOT in the descriptor:
    // an MCU and a PC want different string lengths for the same node, so a
    // contract cannot name the number. `NROS_DECLARED_MAX_{...}_LEN` therefore
    // has no descriptor spelling to be preferred over, and stays exactly as it
    // was. What the descriptor DOES carry is the NEED -- who declared a type
    // that uses the capacity -- which is the half the contract owns.
    let max_string_value_len = capacity(
        "NROS_MAX_STRING_VALUE_LEN",
        "max_string_value_len",
        rungs.max_string_value_len,
        declared("NROS_DECLARED_MAX_STRING_VALUE_LEN"),
        need(
            params.map(|p| p.needs_max_string_value_len()),
            "NROS_DECLARED_PARAM_NEEDS_MAX_STRING_VALUE_LEN",
        ),
        256,
    );
    let max_array_len = capacity(
        "NROS_MAX_ARRAY_LEN",
        "max_array_len",
        rungs.max_array_len,
        declared("NROS_DECLARED_MAX_ARRAY_LEN"),
        need(
            params.map(|p| p.needs_max_array_len()),
            "NROS_DECLARED_PARAM_NEEDS_MAX_ARRAY_LEN",
        ),
        32,
    );
    let max_byte_array_len = capacity(
        "NROS_MAX_BYTE_ARRAY_LEN",
        "max_byte_array_len",
        rungs.max_byte_array_len,
        declared("NROS_DECLARED_MAX_BYTE_ARRAY_LEN"),
        need(
            params.map(|p| p.needs_max_byte_array_len()),
            "NROS_DECLARED_PARAM_NEEDS_MAX_BYTE_ARRAY_LEN",
        ),
        256,
    );
    // phase-446 F2 -- a DESCRIPTION's capacity, its own knob. The contract
    // declares no descriptions (they are code-supplied text), so nothing here
    // is derived: a board states it, and 0 is a legitimate statement meaning
    // "no descriptions". It used to be MAX_STRING_VALUE_LEN, which the
    // contract derives to 0 for an image with no string parameter -- so such
    // an image had no room for any description. The default keeps the old
    // effective 256, which is also the most the describe reply can carry
    // (`rcl_interfaces` strings are 256 bytes; nros-node refuses a larger
    // value at compile time).
    let max_param_description_len = knob(
        "NROS_MAX_PARAM_DESCRIPTION_LEN",
        rungs.max_param_description_len,
        None,
        256,
    );

    // phase-417 W4.a -- the descriptor's OTHER free text,
    // `rcl_interfaces/msg/ParameterDescriptor::additional_constraints`, which
    // `ros2 param describe` prints and rclc's
    // `rclc_add_parameter_description` takes as its fourth argument.
    //
    // ITS OWN CAPACITY, DEFAULT 0, and the default is the decision. Sharing
    // `NROS_MAX_PARAM_DESCRIPTION_LEN` was the first shape and it was wrong in
    // a way only the derived service buffer showed: the describe reply's bound
    // is a worst case over CAPACITY, not over use, so every image would have
    // paid 256 bytes per parameter for a field it never sets -- 2,048 bytes on
    // the eight-parameter island in `parameter_services.rs`, taking its derived
    // reply from 2,741 to 4,789 and past the 4,096 fallback. Nothing is
    // derived here for the same reason F2 derives nothing for the description:
    // the contract states a parameter's name and type, and descriptor prose is
    // code-supplied, so a board STATES it and 0 is the legitimate statement
    // meaning "no constraint text". A too-long value is truncated and REPORTED
    // (`ParameterServer::take_truncated_descriptions`), never dropped in
    // silence, so an image that states nothing and passes constraints anyway
    // gets a log line rather than a mystery.
    let max_param_constraints_len = knob(
        "NROS_MAX_PARAM_CONSTRAINTS_LEN",
        rungs.max_param_constraints_len,
        None,
        0,
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
         pub const MAX_BYTE_ARRAY_LEN: usize = {max_byte_array_len};\n\
         \n\
         /// Maximum length for a parameter description, in bytes \
         (set via NROS_MAX_PARAM_DESCRIPTION_LEN, default 256; 0 = no \
         descriptions). phase-446 F2.\n\
         pub const MAX_PARAM_DESCRIPTION_LEN: usize = {max_param_description_len};\n\
         \n\
         /// Maximum length for a parameter's `additional_constraints`, in \
         bytes (set via NROS_MAX_PARAM_CONSTRAINTS_LEN, default 0 = no \
         constraint text). phase-417 W4.a.\n\
         pub const MAX_PARAM_CONSTRAINTS_LEN: usize = {max_param_constraints_len};\n"
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

/// The sizing descriptor this build was pointed at, or `None`.
///
/// A parse or schema failure PANICS, which is a build error naming the file --
/// the same rule `nros-node/build.rs` holds, and for the same reason: a
/// descriptor EXISTS, so falling back to a literal would size from numbers a
/// user believes they supplied.
fn sizing_descriptor() -> Option<nros_sizing_descriptor::SizingDescriptor> {
    match nros_sizing_descriptor::from_build_env() {
        Ok(d) => d,
        Err(e) => panic!("{e}"),
    }
}

/// What the CONTRACT declared, from the descriptor first and the CMake carrier
/// below it (phase-454, issue 1408).
///
/// **A `Refused` or `Absent` fact yields `None`**, so the knob keeps the rung
/// above it or its literal default. That is RFC-0100 D6 in this crate's
/// vocabulary and it is what makes a PARTIAL descriptor safe to publish: a
/// consumer cannot read a refusal as a value, because `Fact::stated` is the only
/// accessor that yields one.
///
/// **The env carrier is NOT retired here, deliberately.** It is the only road
/// for a standalone leaf with no model, for a multi-entry cmake configure that
/// names no descriptor to cargo, and for the Zephyr west lane -- and phase-454
/// W9 is titled "the retirement wave that mostly did not retire" for exactly
/// that reason. Retirement is its own wave, once both roads are MEASURED
/// delivering; `check-knob-single-reader.py`'s KEPT ledger carries the carriers
/// in this state against issue 1408. Ranked rather than unioned because both
/// numbers come from the ONE `ParamDeclarations` derivation in `nros-cli-core`
/// -- there is no second derivation for them to disagree about.
fn contract(
    fact: Option<nros_sizing_descriptor::Fact<usize>>,
    carrier: Option<usize>,
) -> Option<usize> {
    fact.and_then(|f| f.get()).or(carrier)
}

/// The declared parameter whose TYPE needs a capacity the contract cannot give.
///
/// `ty` is `None` on the descriptor road and that is not a gap being papered
/// over: `[params]`'s `CapacityNeed` carries the node and the parameter name and
/// deliberately not the type, because the type is the DERIVATION's input (which
/// types use which capacity is `ParamStoreSizing`'s rule, not a consumer's) and
/// a consumer that restated it would be the second author of that table. The
/// refusal below prints the type clause only when the road supplied one.
struct Need {
    node: String,
    name: String,
    ty: Option<String>,
}

/// The capacity need: the descriptor first, the CMake carrier below it.
///
/// The carrier's spelling is `<node>:<param>:<type>` (`DeclaredParam::token` in
/// nros-cli-core), and a malformed one is not silently dropped -- a need that
/// cannot be read is a need, so the fields that do not parse become `?` and the
/// refusal still fires and still names the knob.
fn need(
    fact: Option<nros_sizing_descriptor::Fact<nros_sizing_descriptor::CapacityNeed>>,
    key: &str,
) -> Option<Need> {
    // The `rerun-if-env-changed` for `key` is printed with its siblings at the
    // head of `main`, literally, because `check-declared-fact-carriers` greps
    // for the spelling and a name assembled here would be invisible to it.
    if let Some(f) = fact {
        // STATED, including `unused` -- which is an ANSWER ("no declared type
        // uses this capacity") and not an absence, so it must stop the fallback
        // rather than fall through to a carrier that might say otherwise.
        if let Some(c) = f.stated() {
            return match c.needed_by() {
                None => None,
                Some((node, name)) => Some(Need {
                    node: node.to_string(),
                    name: name.to_string(),
                    ty: None,
                }),
            };
        }
    }
    let raw = env::var(key).ok().filter(|v| !v.trim().is_empty())?;
    let mut parts = raw.rsplitn(3, ':');
    let ty = parts.next().map(str::to_string);
    let name = parts.next().unwrap_or(&raw).to_string();
    let node = parts.next().unwrap_or("?").to_string();
    Some(Need { node, name, ty })
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
    needed_by: Option<Need>,
    default: usize,
) -> usize {
    match (stated(name, rung), needed_by) {
        (Some(0), Some(who)) => refuse(name, board_key, &who, "is stated as 0"),
        (Some(v), _) => v,
        (None, Some(who)) => refuse(name, board_key, &who, "is stated nowhere"),
        (None, None) => declared.unwrap_or(default),
    }
}

fn refuse(name: &str, board_key: &str, who: &Need, what: &str) -> ! {
    let Need {
        node,
        name: param,
        ty,
    } = who;
    // The descriptor road states the node and the parameter and not the type
    // (see `Need`), so the type clause is printed only when a road supplied
    // one. A sentence that says "is declared `?`" reads as a bug in the build
    // script rather than as a fact about the contract.
    let declares = match ty {
        Some(t) => format!("is declared `{t}` in its contract"),
        None => "is declared with a type that needs it".to_string(),
    };
    panic!(
        "\n\nnros-params: parameter `{param}` on node `{node}` {declares}, so the parameter \
         store needs {name}, and {name} {what}.\n\
         A contract states a parameter's name and type; how long a string or an array \
         may be is a fact about the BOARD it is built for. State {name} as one of:\n  \
         - the board's `[knobs.params] {board_key} = <n>`\n  \
         - CONFIG_{name}=<n> in the image's Kconfig (Zephyr)\n  \
         - the {name} environment variable\n\
         (phase-446 W4)\n"
    );
}
