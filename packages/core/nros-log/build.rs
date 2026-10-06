//! Build script for nros-log.
//!
//! Resolves the logging tenant (`[knobs.log]`, RFC-0086 / RFC-0049) and writes
//! it to `$OUT_DIR/nros_log_config.rs`:
//!
//! | knob                        | builtin | what it sizes or gates                          |
//! | --------------------------- | ------- | ----------------------------------------------- |
//! | `NROS_LOG_DYNAMIC_LOGGERS`  | 16      | runtime-logger arenas (phase-479 W5)            |
//! | `NROS_LOG_MAX_LEVEL`        | `trace` | the compile-time severity CEILING (issue 1037)  |
//! | `NROS_LOG_BUFFER_SIZE`      | 256     | the per-call formatting buffer (issue 1037)     |
//! | `NROS_LOG_EARLY_RECORDS`    | 4       | records held before `init` (issue 1037)         |
//! | `NROS_LOG_ROSOUT_RECORDS`   | 16      | the `/rosout` queue depth (issue 1037)          |
//!
//! and derives one `cfg`, `nros_log_clock`, from the platform's `clock`
//! CAPABILITY (below).
//!
//! # The ladder (RFC-0049), highest rung first
//!
//! 1. the environment variable — which is also how an image's
//!    `[image.<id>] env` reaches this script on the cargo road (the generated
//!    `nros-cargo.toml`'s `[env]`) and the cmake road (`corrosion_set_env_vars`);
//! 2. `CONFIG_<knob>` in the `.config` named by `$DOTCONFIG` (the west road,
//!    issue 0460);
//! 3. the platform/board rung, `[board.knobs.log] <name>` (or `[knobs.log]` in
//!    an `nros-platform.toml`), reached through `NROS_BOARD_TOML` /
//!    `NROS_PLATFORM_NAME`;
//! 4. the builtin.
//!
//! The values are COUNTS (and one level name), so `rerun-if-env-changed` on
//! them is right; the board descriptor is a PATH and is watched by content
//! inside `BuildRungs` (issue 0491).
//!
//! # The deprecated cargo features — one release
//!
//! Each knob replaced a pick-one feature family (`dynamic-loggers-<N>`,
//! `max-level-*`, `buffer-size-<N>`, `early-records-<N>`, `rosout-records-<N>`).
//! They cannot express "the image overrides the board": cargo UNIONS features
//! with no precedence, and each old `const fn` tested its members in a fixed
//! order, so two crates picking different members silently got whichever came
//! first — and three of the families spelled "off" as a feature, which RFC-0086
//! D5 forbids (issue 1037). The rule while they last, per family:
//!
//! * a feature and NO stated knob — honoured, with a `cargo:warning` naming the
//!   knob to use instead;
//! * a feature AGREEING with a stated knob (any rung 1–3) — the knob wins and
//!   the feature is reported as redundant;
//! * a feature DISAGREEING with a stated knob — the build FAILS, naming both.
//!   Silently picking either would be the defect the knob exists to remove;
//! * two features of one family that disagree with each other — the build
//!   FAILS, for the same reason: there is no answer to "which one did the image
//!   mean".
//!
//! # The clock is a capability, not a knob (issue 1037)
//!
//! `Record::timestamp_ns` and the `nros_*_throttle!` family need the platform's
//! `nros_platform_clock_ns`. `nros_log_clock` is set when EITHER
//!
//! * the lane resolved a platform (or board) declaring `[capabilities] clock =
//!   true` — every in-tree port does — or
//! * the `platform-clock` feature is on. That one survives, because it only
//!   PULLS CODE IN (a link-time requirement), which D5 allows: `nros-c`'s
//!   `platform-*` arms set it, since a linked port is exactly when the symbol
//!   exists, and so does a road that names no platform to this script.
//!
//! A bare `cargo test -p nros-log` has neither, so its test binaries never
//! reference the symbol and link without a port. A platform that declares
//! `clock = false` while the feature is on is refused: one of the two is wrong,
//! and records stamped `0` would be the silent answer.
//!
//! # C and C++
//!
//! Every knob here reaches the C and C++ surfaces through this crate, which
//! `nros-c` / `nros-cpp` link: the ceiling through [`Logger::is_enabled`]
//! (which `nros_log_emit_at` and `nros_logger_is_enabled` call), the arenas and
//! rings because they ARE this crate's. The one thing that does not follow is
//! the C printf front-ends' own stack frame (`NROS_LOG_FMT_BUFFER_SIZE` in
//! `<nros/log.h>`, 256): it is compiled in the CALLER's translation unit from a
//! header that has no per-build value to read. A C record is therefore bounded
//! by `min(255, NROS_LOG_BUFFER_SIZE)` — the Rust side clips it where it is
//! stored or rendered.
//!
//! [`Logger::is_enabled`]: https://docs.rs/nros-log

use std::{env, fmt::Write as _, path::Path};

use nros_platform_config::platform_config::{BuildRungs, LOG_LEVELS, parse_log_level};

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();

    // `None` when no lane names a platform (a bare `cargo build` or `cargo
    // test`), and then every rung is simply absent.
    let build = BuildRungs::from_build_env();
    let rungs = build.as_ref().map(|r| r.log_rungs()).unwrap_or_default();

    // Each read is spelled LITERALLY, not through a table: the knob gates and
    // the census harvest a read from its call site (`check-knob-single-reader`).
    let dynamic_loggers = family(
        &DYNAMIC_LOGGERS,
        nros_zephyr_build::knob("NROS_LOG_DYNAMIC_LOGGERS")
            .rung(rungs.dynamic_loggers)
            .stated(),
    );
    let buffer_size = family(
        &BUFFER_SIZE,
        nros_zephyr_build::knob("NROS_LOG_BUFFER_SIZE")
            .rung(rungs.buffer_size)
            .strict_env()
            .stated(),
    );
    let early_records = family(
        &EARLY_RECORDS,
        nros_zephyr_build::knob("NROS_LOG_EARLY_RECORDS")
            .rung(rungs.early_records)
            .strict_env()
            .stated(),
    );
    let rosout_records = family(
        &ROSOUT_RECORDS,
        nros_zephyr_build::knob("NROS_LOG_ROSOUT_RECORDS")
            .rung(rungs.rosout_records)
            .strict_env()
            .stated(),
    );
    // A level NAME as well as a number, so it is read as text. `stated_str`
    // emits no env watch (it also serves a PATH-valued caller); this value is a
    // word, so watching it as text is right.
    println!("cargo:rerun-if-env-changed=NROS_LOG_MAX_LEVEL");
    let max_level_stated = nros_zephyr_build::knob("NROS_LOG_MAX_LEVEL")
        .stated_str()
        .map(|raw| {
            parse_log_level(&raw).unwrap_or_else(|| {
                panic!(
                    "\n\nnros-log: NROS_LOG_MAX_LEVEL='{raw}' is not a level. Expected one of \
                     {} (or 0..={}). issue 1037.\n",
                    LOG_LEVELS.join(", "),
                    LOG_LEVELS.len() - 1
                )
            })
        })
        .or(rungs.max_level);
    let max_level = family(&MAX_LEVEL, max_level_stated);

    let clock = clock(build.as_ref());
    println!("cargo::rustc-check-cfg=cfg(nros_log_clock)");
    if clock {
        println!("cargo::rustc-cfg=nros_log_clock");
    }

    let mut contents = String::new();
    let _ = write!(
        contents,
        "// Written by `build.rs` (phase-479 W5, issue 1037). See its module docs.\n\
         /// `NROS_LOG_DYNAMIC_LOGGERS`: loggers `get_or_create_logger` may create (0 = lookup only).\n\
         pub(crate) const DYNAMIC_LOGGER_CAPACITY: usize = {dynamic_loggers};\n\
         /// `NROS_LOG_MAX_LEVEL`: the compile-time ceiling, as an index into \
         trace..fatal, off ({name}).\n\
         pub(crate) const MAX_LEVEL: u8 = {max_level};\n\
         /// `NROS_LOG_BUFFER_SIZE`: bytes of the per-call formatting buffer.\n\
         pub(crate) const BUFFER_SIZE: usize = {buffer_size};\n\
         /// `NROS_LOG_EARLY_RECORDS`: records held before `init` (0 = none).\n\
         pub(crate) const EARLY_RECORDS: usize = {early_records};\n\
         /// `NROS_LOG_ROSOUT_RECORDS`: the `/rosout` queue depth.\n\
         #[cfg_attr(not(feature = \"rosout\"), allow(dead_code))]\n\
         pub(crate) const ROSOUT_RECORDS: usize = {rosout_records};\n",
        name = LOG_LEVELS[max_level],
    );
    std::fs::write(Path::new(&out_dir).join("nros_log_config.rs"), contents).unwrap();
}

/// One knob and the deprecated feature family it replaced.
struct Family {
    /// The env front-end; the Kconfig symbol is `CONFIG_<knob>`.
    knob: &'static str,
    /// The `[board.knobs.log]` / `[knobs.log]` key.
    toml_key: &'static str,
    builtin: usize,
    /// Legal values, inclusive. Out of range is a build error naming the knob.
    range: (usize, usize),
    /// The deprecated features, by the value each one names.
    features: &'static [(usize, &'static str)],
    /// Who added the knob, for the messages.
    since: &'static str,
}

const DYNAMIC_LOGGERS: Family = Family {
    knob: "NROS_LOG_DYNAMIC_LOGGERS",
    toml_key: "dynamic_loggers",
    builtin: 16,
    range: (0, 1024),
    features: &[
        (0, "dynamic-loggers-0"),
        (8, "dynamic-loggers-8"),
        (32, "dynamic-loggers-32"),
    ],
    since: "phase-479 W5",
};

/// Below 128 a C++ runtime refusal (`RUNTIME_REFUSAL_MAX` = 160 with its
/// prefix, `log.hpp`) stops fitting; the old family's floor was 128 too.
const BUFFER_SIZE: Family = Family {
    knob: "NROS_LOG_BUFFER_SIZE",
    toml_key: "buffer_size",
    builtin: 256,
    range: (128, 4096),
    features: &[
        (128, "buffer-size-128"),
        (256, "buffer-size-256"),
        (512, "buffer-size-512"),
        (1024, "buffer-size-1024"),
    ],
    since: "issue 1037",
};

const EARLY_RECORDS: Family = Family {
    knob: "NROS_LOG_EARLY_RECORDS",
    toml_key: "early_records",
    builtin: 4,
    range: (0, 256),
    features: &[
        (0, "early-records-0"),
        (8, "early-records-8"),
        (16, "early-records-16"),
    ],
    since: "issue 1037",
};

/// A depth of 0 would be a `KEEP_LAST(0)` `/rosout` publisher, which is not a
/// QoS; an image that wants no `/rosout` leaves the `rosout` feature off.
const ROSOUT_RECORDS: Family = Family {
    knob: "NROS_LOG_ROSOUT_RECORDS",
    toml_key: "rosout_records",
    builtin: 16,
    range: (1, 1024),
    features: &[
        (8, "rosout-records-8"),
        (32, "rosout-records-32"),
        (64, "rosout-records-64"),
    ],
    since: "issue 1037",
};

/// Indices into [`LOG_LEVELS`]; `fatal` (5) had no feature.
const MAX_LEVEL: Family = Family {
    knob: "NROS_LOG_MAX_LEVEL",
    toml_key: "max_level",
    builtin: 0,
    range: (0, 6),
    features: &[
        (0, "max-level-trace"),
        (1, "max-level-debug"),
        (2, "max-level-info"),
        (3, "max-level-warn"),
        (4, "max-level-error"),
        (6, "max-level-off"),
    ],
    since: "issue 1037",
};

/// Resolve one family: print its warnings, or stop the build.
fn family(f: &Family, stated: Option<usize>) -> usize {
    let selected: Vec<(usize, &str)> = f
        .features
        .iter()
        .copied()
        .filter(|(_, feat)| {
            let var = format!("CARGO_FEATURE_{}", feat.to_uppercase().replace('-', "_"));
            env::var_os(var).is_some()
        })
        .collect();
    let value = match resolve(f, stated, &selected) {
        Ok((n, warnings)) => {
            for w in warnings {
                println!("cargo:warning={w}");
            }
            n
        }
        Err(e) => panic!("\n\nnros-log: {e}\n"),
    };
    let (lo, hi) = f.range;
    if !(lo..=hi).contains(&value) {
        panic!(
            "\n\nnros-log: {} = {value} is outside {lo}..={hi}. {}\n",
            f.knob, f.since
        );
    }
    value
}

/// The resolution, pure: the stated knob (rungs 1–3) and the selected
/// deprecated features → the value and the warnings to print, or why the build
/// must stop.
fn resolve(
    f: &Family,
    stated: Option<usize>,
    selected: &[(usize, &str)],
) -> Result<(usize, Vec<String>), String> {
    let Family {
        knob,
        toml_key,
        since,
        ..
    } = f;
    let names = || {
        selected
            .iter()
            .map(|(_, feat)| format!("`{feat}`"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    // `max_level` is carried as an index; say it by name.
    let show = |v: usize| {
        if *knob == MAX_LEVEL.knob {
            LOG_LEVELS
                .get(v)
                .map_or_else(|| v.to_string(), |l| (*l).to_string())
        } else {
            v.to_string()
        }
    };
    let where_ = format!(
        "`[image.<id>] env`, the board's `[board.knobs.log] {toml_key}`, or CONFIG_{knob} \
         on Zephyr"
    );
    if let Some(&(first, _)) = selected.first()
        && selected.iter().any(|(n, _)| *n != first)
    {
        return Err(format!(
            "the deprecated features {} are ALL selected, and they name different values. \
             Cargo unions features with no precedence, so there is no answer to which one the \
             image meant. Drop them and state the value once as {knob}=<v> ({where_}). {since}.",
            names()
        ));
    }
    match (stated, selected.first()) {
        (Some(v), Some((n, feat))) if v != *n => Err(format!(
            "the deprecated feature `{feat}` asks for {knob}={}, and {knob} is stated as {} \
             (environment, `[image.<id>] env`, Kconfig, or the board's `[board.knobs.log] \
             {toml_key}`). Refusing to pick one silently -- drop the feature; the knob is the \
             one mechanism with a precedence. {since}.",
            show(*n),
            show(v)
        )),
        (Some(v), Some(_)) => Ok((
            v,
            vec![format!(
                "{} is deprecated and redundant here: {knob} is stated as {}. Drop the \
                 feature ({since}; removed after one release).",
                names(),
                show(v)
            )],
        )),
        (Some(v), None) => Ok((v, Vec::new())),
        (None, Some((n, _))) => Ok((
            *n,
            vec![format!(
                "{} is deprecated: state {knob}={} instead ({where_}). Honoured for one \
                 release ({since}).",
                names(),
                show(*n)
            )],
        )),
        (None, None) => Ok((f.builtin, Vec::new())),
    }
}

/// Whether `nros_platform_clock_ns` is linked: the platform's `clock` FACT,
/// or the `platform-clock` feature that pulls a port in. See the module docs.
fn clock(build: Option<&BuildRungs>) -> bool {
    let feature = env::var_os("CARGO_FEATURE_PLATFORM_CLOCK").is_some();
    let fact = build.and_then(|r| r.capability("clock"));
    match (feature, fact) {
        (true, Some(false)) => panic!(
            "\n\nnros-log: the `platform-clock` feature is on, and platform `{}` declares \
             `[capabilities] clock = false`. One of them is wrong: the feature promises \
             `nros_platform_clock_ns` at link time and the descriptor says the port has none. \
             issue 1037.\n",
            build.map(|r| r.platform.as_str()).unwrap_or("?")
        ),
        (true, _) | (false, Some(true)) => true,
        (false, _) => false,
    }
}
