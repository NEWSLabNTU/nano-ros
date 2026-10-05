//! Build script for nros-log.
//!
//! phase-479 W5 (RFC-0102 D5) — resolves `NROS_LOG_DYNAMIC_LOGGERS`, the number
//! of loggers [`get_or_create_logger`] may CREATE at run time, and writes it to
//! `$OUT_DIR/nros_log_config.rs` for `src/pool.rs` to size its two arenas from.
//!
//! # The ladder (RFC-0049), highest rung first
//!
//! 1. the `NROS_LOG_DYNAMIC_LOGGERS` environment variable — which is also how
//!    an image's `[image.<id>] env` reaches this script on the cargo road (the
//!    generated `nros-cargo.toml`'s `[env]`) and the cmake road
//!    (`corrosion_set_env_vars`);
//! 2. `CONFIG_NROS_LOG_DYNAMIC_LOGGERS` in the `.config` named by `$DOTCONFIG`
//!    (the west road, issue 0460);
//! 3. the platform/board rung, `[board.knobs.log] dynamic_loggers` (or
//!    `[knobs.log]` in an `nros-platform.toml`), reached through
//!    `NROS_BOARD_TOML` / `NROS_PLATFORM_NAME`;
//! 4. the builtin, 16.
//!
//! The value is a COUNT, so `rerun-if-env-changed` on it is right; the board
//! descriptor is a PATH and is watched by content inside `BuildRungs` (issue
//! 0491).
//!
//! # The `dynamic-loggers-<N>` features — deprecated, one release
//!
//! They were the mechanism before this knob, and they cannot express "the image
//! overrides the board": cargo UNIONS features with no precedence, and the old
//! `dynamic_logger_capacity` tested `-0`, then `-8`, then `-32`, so two crates
//! picking different sizes silently got the SMALLEST. The rule while they last:
//!
//! * a feature and NO stated knob — honoured, with a `cargo:warning` naming the
//!   knob to use instead;
//! * a feature AGREEING with a stated knob (any rung 1–3) — the knob wins and
//!   the feature is reported as redundant;
//! * a feature DISAGREEING with a stated knob — the build FAILS, naming both.
//!   Silently picking either would be the defect this knob exists to remove;
//! * two features that disagree with each other — the build FAILS, for the same
//!   reason: there is no answer to "which one did the image mean".
//!
//! `0` is a legitimate value on every rung: no runtime loggers, lookup only.
//!
//! [`get_or_create_logger`]: https://docs.rs/nros-log

use std::{env, path::Path};

/// The knob's env front-end. Its Kconfig symbol is DERIVED
/// (`CONFIG_NROS_LOG_DYNAMIC_LOGGERS`), so `KCONFIG_PAIRS` carries no row.
const KNOB: &str = "NROS_LOG_DYNAMIC_LOGGERS";

/// The builtin: an MCU's number. Linux host boards state 32.
const DEFAULT: usize = 16;

/// The deprecated features, by the size each one names.
const FEATURES: &[(usize, &str)] = &[
    (0, "dynamic-loggers-0"),
    (8, "dynamic-loggers-8"),
    (32, "dynamic-loggers-32"),
];

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    // `None` when no lane names a platform (a bare `cargo build` or `cargo
    // test`), and then the rung is simply absent.
    let rung = nros_platform_config::platform_config::BuildRungs::from_build_env()
        .and_then(|r| r.log_rungs().dynamic_loggers);
    // Spelled literally, not through `KNOB`: the knob gates and the census
    // harvest a read from its call site (`check-knob-single-reader`).
    let stated = nros_zephyr_build::knob("NROS_LOG_DYNAMIC_LOGGERS")
        .rung(rung)
        .stated();

    let selected: Vec<(usize, &str)> = FEATURES
        .iter()
        .copied()
        .filter(|(_, f)| {
            let var = format!("CARGO_FEATURE_{}", f.to_uppercase().replace('-', "_"));
            env::var_os(var).is_some()
        })
        .collect();

    let capacity = match resolve(stated, &selected) {
        Ok((n, warnings)) => {
            for w in warnings {
                println!("cargo:warning={w}");
            }
            n
        }
        Err(e) => panic!("\n\nnros-log: {e}\n"),
    };

    let contents = format!(
        "/// How many loggers `get_or_create_logger` may create at run time \
         (`NROS_LOG_DYNAMIC_LOGGERS`, default {DEFAULT}; 0 = lookup only).\n\
         /// Written by `build.rs` (phase-479 W5, RFC-0102 D5).\n\
         pub(crate) const DYNAMIC_LOGGER_CAPACITY: usize = {capacity};\n"
    );
    std::fs::write(Path::new(&out_dir).join("nros_log_config.rs"), contents).unwrap();
}

/// The resolution, pure: the stated knob (rungs 1–3) and the selected
/// deprecated features → the capacity and the warnings to print, or why the
/// build must stop.
fn resolve(
    stated: Option<usize>,
    selected: &[(usize, &str)],
) -> Result<(usize, Vec<String>), String> {
    let names = || {
        selected
            .iter()
            .map(|(_, f)| format!("`{f}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    if let Some(&(first, _)) = selected.first() {
        if selected.iter().any(|(n, _)| *n != first) {
            return Err(format!(
                "the deprecated features {} are ALL selected, and they name different \
                 sizes. Cargo unions features with no precedence, so there is no answer to \
                 which one the image meant. Drop them and state the size once as \
                 {KNOB}=<n> (`[image.<id>] env`, the board's `[board.knobs.log] \
                 dynamic_loggers`, or CONFIG_{KNOB} on Zephyr). phase-479 W5.",
                names()
            ));
        }
    }
    match (stated, selected.first()) {
        (Some(v), Some((n, f))) if v != *n => Err(format!(
            "the deprecated feature `{f}` asks for {n} runtime loggers, and {KNOB} is \
             stated as {v} (environment, `[image.<id>] env`, Kconfig, or the board's \
             `[board.knobs.log] dynamic_loggers`). Refusing to pick one silently -- drop \
             the feature; the knob is the one mechanism with a precedence. phase-479 W5."
        )),
        (Some(v), Some(_)) => Ok((
            v,
            vec![format!(
                "{} is deprecated and redundant here: {KNOB} is stated as {v}. Drop the \
                 feature (phase-479 W5; removed after one release).",
                names()
            )],
        )),
        (Some(v), None) => Ok((v, Vec::new())),
        (None, Some((n, _))) => Ok((
            *n,
            vec![format!(
                "{} is deprecated: state {KNOB}={n} instead (`[image.<id>] env`, the \
                 board's `[board.knobs.log] dynamic_loggers`, or CONFIG_{KNOB} on Zephyr). \
                 Honoured for one release (phase-479 W5).",
                names()
            )],
        )),
        (None, None) => Ok((DEFAULT, Vec::new())),
    }
}
