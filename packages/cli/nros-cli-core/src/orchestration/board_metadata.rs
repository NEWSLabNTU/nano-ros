//! Phase 215.C.2 — strict reader for `[package.metadata.nros.board]`.
//!
//! Each `packages/boards/nros-board-*` crate carries TWO faces of the same
//! board manifest:
//!
//! * `board.cmake` — sidecar consumed by the Zephyr cmake module
//!   `zephyr/cmake/nano_ros_use_board.cmake`.
//! * `[package.metadata.nros.board]` in `Cargo.toml` — consumed by Rust /
//!   `nros` CLI tooling (drift audit, `nros board info`).
//!
//! Both must stay in lock-step; Phase 215.F drift audit guards that
//! invariant. This module is the strict reader for the Cargo.toml face —
//! mirrors the discipline of `cargo_metadata_schema.rs` (Phase 212.B):
//! `deny_unknown_fields` so typos surface at parse time instead of being
//! silently dropped.

use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};

/// `[package.metadata.nros.board]` — strict schema.
///
/// Field semantics mirror the `NROS_BOARD_*` cmake variables defined in
/// Phase 215.A.1:
///
/// | Cargo field         | `board.cmake` variable        |
/// |---------------------|-------------------------------|
/// | `zephyr_board`      | `NROS_BOARD_ZEPHYR_ID`        |
/// | `toolchain`         | `NROS_BOARD_TOOLCHAIN`        |
/// | `gated`             | `NROS_BOARD_GATED_PKGS`       |
/// | `default_rmw`       | `NROS_BOARD_DEFAULT_RMW`      |
/// | `default_transport` | `NROS_BOARD_DEFAULT_TRANSPORT`|
/// | `runner`            | `NROS_BOARD_RUNNER`           |
/// | `prj_conf`          | `NROS_BOARD_PRJ_CONF`         |
/// | `board_conf`        | `NROS_BOARD_BOARD_CONF`       |
/// | `board_overlay`     | `NROS_BOARD_BOARD_OVERLAY`    |
///
/// `prj_conf` / `board_conf` / `board_overlay` are RELATIVE to the
/// host `Cargo.toml`'s directory (the cmake face stores the absolute
/// form after resolution via the board crate dir).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoardMetadata {
    /// Zephyr `BOARD` string (`fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp`).
    pub zephyr_board: String,
    /// SDK abi target (e.g. `aarch64-zephyr-elf`).
    pub toolchain: String,
    /// Optional semicolon-list (in cmake) of `[features.<flag>]` gates.
    /// Defaults to empty — boards w/o gated pkgs simply omit the field.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gated: Vec<String>,
    /// `cyclonedds` / `zenoh` / `xrce`.
    pub default_rmw: String,
    /// `ethernet` / `serial` / …
    pub default_transport: String,
    /// `armfvp` / `qemu` / `native` / …
    pub runner: String,
    /// Relative path to `prj.conf` (relative to `Cargo.toml`).
    pub prj_conf: String,
    /// Relative path to per-board hwv2 `<board>.conf` overlay.
    pub board_conf: String,
    /// Relative path to per-board DTS overlay.
    pub board_overlay: String,
}

/// Parse `[package.metadata.nros.board]` from a board crate's `Cargo.toml`.
///
/// Strict on absent table — callers that need a fallback should handle the
/// `Err` themselves. Strict on unknown fields (`deny_unknown_fields`).
pub fn parse_board_metadata(cargo_toml: &Path) -> Result<BoardMetadata, eyre::Report> {
    let raw = std::fs::read_to_string(cargo_toml).map_err(|e| {
        eyre::eyre!(
            "failed to read {} for `[package.metadata.nros.board]`: {e}",
            cargo_toml.display()
        )
    })?;
    let value: toml::Value = toml::from_str(&raw)
        .map_err(|e| eyre::eyre!("invalid TOML in {}: {e}", cargo_toml.display()))?;
    let table = value
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("nros"))
        .and_then(|n| n.get("board"))
        .ok_or_else(|| {
            eyre::eyre!(
                "no `[package.metadata.nros.board]` table in {}",
                cargo_toml.display()
            )
        })?;
    let cloned = table.clone();
    let board: BoardMetadata = cloned.try_into().map_err(|e| {
        eyre::eyre!(
            "invalid `[package.metadata.nros.board]` in {}: {e}",
            cargo_toml.display()
        )
    })?;
    Ok(board)
}

// ---------------------------------------------------------------------------
// `board.cmake` sidecar parser + drift compare
// ---------------------------------------------------------------------------

/// One drift mismatch surfaced by [`compute_drift`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DriftEntry {
    /// Cargo-side field name (e.g. `"runner"`).
    pub field: &'static str,
    /// Value in `[package.metadata.nros.board]`.
    pub cargo_metadata: String,
    /// Value parsed out of `board.cmake`.
    pub board_cmake: String,
}

/// Map a `BoardMetadata` field → the `board.cmake` variable name it
/// mirrors (Phase 215.A.1).
const FIELD_MAP: &[(&str, &str)] = &[
    ("zephyr_board", "NROS_BOARD_ZEPHYR_ID"),
    ("toolchain", "NROS_BOARD_TOOLCHAIN"),
    ("default_rmw", "NROS_BOARD_DEFAULT_RMW"),
    ("default_transport", "NROS_BOARD_DEFAULT_TRANSPORT"),
    ("runner", "NROS_BOARD_RUNNER"),
    ("prj_conf", "NROS_BOARD_PRJ_CONF"),
    ("board_conf", "NROS_BOARD_BOARD_CONF"),
    ("board_overlay", "NROS_BOARD_BOARD_OVERLAY"),
];

/// Tokenise `board.cmake` and return a `name → value` map for every
/// `set(NROS_BOARD_<KEY> <value> …)` call. Values are returned with
/// surrounding quotes stripped. Cache annotations (`CACHE STRING "doc"`)
/// are tolerated — only the variable's value is captured. Semicolon-
/// delimited lists are preserved verbatim; downstream consumers can
/// `.split(';')` as needed.
pub fn parse_board_cmake(source: &str) -> BTreeMap<String, String> {
    let normalised = join_multiline_set_calls(source);
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for raw_line in normalised.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if !lower.starts_with("set(") {
            continue;
        }
        let after_set = &line["set(".len()..];
        let body = after_set.trim_end_matches(')').trim();

        let mut tokens = tokenise_cmake_args(body);
        let var = tokens.next().unwrap_or_default();
        if !var.starts_with("NROS_BOARD_") {
            continue;
        }
        let mut value: Option<String> = None;
        for tok in tokens.by_ref() {
            let upper = tok.to_ascii_uppercase();
            if matches!(upper.as_str(), "CACHE" | "FORCE" | "PARENT_SCOPE") {
                continue;
            }
            if value.is_some() {
                break;
            }
            value = Some(tok);
        }
        if let Some(v) = value {
            out.insert(var, v);
        }
    }
    out
}

/// Collapse multi-line `set(NAME\n  "value"\n)` calls onto a single
/// line so the per-line parser sees the full call. Tracks paren depth
/// only outside double-quoted strings; comments (`#…\n`) are stripped
/// before counting since CMake treats `#` as a line comment.
fn join_multiline_set_calls(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut depth: usize = 0;
    let mut in_string = false;
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if !in_string && c == '#' {
            // Strip to end of line.
            while let Some(&n) = chars.peek() {
                if n == '\n' {
                    break;
                }
                chars.next();
            }
            continue;
        }
        if c == '"' {
            in_string = !in_string;
            out.push(c);
            continue;
        }
        if !in_string {
            if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth = depth.saturating_sub(1);
            }
            if c == '\n' && depth > 0 {
                // Inside an open `set(...)` call — collapse the newline
                // (and any leading whitespace on the next line) to a
                // single space so the per-line parser sees one statement.
                out.push(' ');
                while let Some(&n) = chars.peek() {
                    if n == ' ' || n == '\t' {
                        chars.next();
                    } else {
                        break;
                    }
                }
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// Minimal CMake `set()` argument tokeniser. Handles bare words and
/// double-quoted strings; CMake bracket syntax `[[…]]` is NOT supported
/// (the board.cmake schema doesn't use it).
fn tokenise_cmake_args(s: &str) -> impl Iterator<Item = String> + '_ {
    let mut iter = s.chars().peekable();
    std::iter::from_fn(move || {
        while let Some(&c) = iter.peek() {
            if c.is_whitespace() {
                iter.next();
            } else {
                break;
            }
        }
        let first = iter.next()?;
        let mut tok = String::new();
        if first == '"' {
            while let Some(c) = iter.next() {
                if c == '\\' {
                    if let Some(esc) = iter.next() {
                        tok.push(esc);
                    }
                } else if c == '"' {
                    return Some(tok);
                } else {
                    tok.push(c);
                }
            }
            Some(tok)
        } else {
            tok.push(first);
            while let Some(&c) = iter.peek() {
                if c.is_whitespace() {
                    break;
                }
                tok.push(c);
                iter.next();
            }
            Some(tok)
        }
    })
}

/// Compare the typed Cargo.toml view to the parsed board.cmake map.
///
/// Path-shaped fields (`prj_conf` / `board_conf` / `board_overlay`)
/// compare by basename — the Cargo.toml face stores them relative to
/// `Cargo.toml`, while the cmake face stores absolute paths post-
/// `${CMAKE_CURRENT_LIST_DIR}` resolution. Basename comparison keeps
/// the audit meaningful without canonicalising both surfaces.
///
/// A board.cmake variable that is not authored is treated as
/// "no opinion" — not drift.
pub fn compute_drift(cargo: &BoardMetadata, cmake: &BTreeMap<String, String>) -> Vec<DriftEntry> {
    let mut out = Vec::new();
    for &(field, cmake_var) in FIELD_MAP {
        let cargo_val = cargo_field(cargo, field);
        let Some(cmake_val) = cmake.get(cmake_var) else {
            continue;
        };
        let (lhs, rhs) = match field {
            "prj_conf" | "board_conf" | "board_overlay" => (
                basename(&cargo_val).to_string(),
                basename(cmake_val).to_string(),
            ),
            _ => (cargo_val.clone(), cmake_val.clone()),
        };
        if lhs != rhs {
            out.push(DriftEntry {
                field,
                cargo_metadata: cargo_val,
                board_cmake: cmake_val.clone(),
            });
        }
    }
    if let Some(cmake_gated) = cmake.get("NROS_BOARD_GATED_PKGS") {
        let mut cargo_gated = cargo.gated.clone();
        cargo_gated.sort();
        let mut cmake_gated_v: Vec<String> = cmake_gated
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        cmake_gated_v.sort();
        if cargo_gated != cmake_gated_v {
            out.push(DriftEntry {
                field: "gated",
                cargo_metadata: cargo.gated.join(";"),
                board_cmake: cmake_gated.clone(),
            });
        }
    }
    out
}

fn cargo_field(m: &BoardMetadata, field: &str) -> String {
    match field {
        "zephyr_board" => m.zephyr_board.clone(),
        "toolchain" => m.toolchain.clone(),
        "default_rmw" => m.default_rmw.clone(),
        "default_transport" => m.default_transport.clone(),
        "runner" => m.runner.clone(),
        "prj_conf" => m.prj_conf.clone(),
        "board_conf" => m.board_conf.clone(),
        "board_overlay" => m.board_overlay.clone(),
        other => panic!("cargo_field: unknown field {other}"),
    }
}

fn basename(p: &str) -> &str {
    p.rsplit(|c| c == '/' || c == '\\').next().unwrap_or(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const FVP_GOLDEN: &str = r#"
[package]
name = "nros-board-fvp-aemv8r-smp"
version = "0.1.0"
edition = "2024"

[package.metadata.nros.board]
zephyr_board = "fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp"
toolchain    = "aarch64-zephyr-elf"
gated        = ["arm-fvp"]
default_rmw  = "cyclonedds"
default_transport = "ethernet"
runner       = "armfvp"
prj_conf      = "prj.conf"
board_conf    = "boards/fvp_baser_aemv8r_fvp_aemv8r_aarch64_smp.conf"
board_overlay = "boards/fvp_baser_aemv8r_fvp_aemv8r_aarch64_smp.overlay"
"#;

    fn write_tmp(name: &str, body: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nros-board-metadata-test-{}-{}",
            std::process::id(),
            name
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("Cargo.toml");
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        p
    }

    #[test]
    fn parses_basic_metadata_table() {
        let p = write_tmp("basic", FVP_GOLDEN);
        let m = parse_board_metadata(&p).expect("golden fixture parses");
        assert_eq!(m.zephyr_board, "fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp");
        assert_eq!(m.toolchain, "aarch64-zephyr-elf");
        assert_eq!(m.gated, vec!["arm-fvp"]);
        assert_eq!(m.default_rmw, "cyclonedds");
        assert_eq!(m.default_transport, "ethernet");
        assert_eq!(m.runner, "armfvp");
        assert_eq!(m.prj_conf, "prj.conf");
        assert_eq!(
            m.board_conf,
            "boards/fvp_baser_aemv8r_fvp_aemv8r_aarch64_smp.conf"
        );
        assert_eq!(
            m.board_overlay,
            "boards/fvp_baser_aemv8r_fvp_aemv8r_aarch64_smp.overlay"
        );

        // Round-trip
        let reser = toml::to_string(&m).expect("serialize");
        let m2: BoardMetadata = toml::from_str(&reser).expect("reparse");
        assert_eq!(m, m2);
    }

    #[test]
    fn omitted_gated_defaults_to_empty() {
        // Bare board w/o the `gated` knob still parses.
        let raw = r#"
[package]
name = "nros-board-bare"
version = "0.1.0"

[package.metadata.nros.board]
zephyr_board = "qemu_cortex_m3"
toolchain    = "arm-zephyr-eabi"
default_rmw  = "zenoh"
default_transport = "ethernet"
runner       = "qemu"
prj_conf      = "prj.conf"
board_conf    = "boards/qemu_cortex_m3.conf"
board_overlay = "boards/qemu_cortex_m3.overlay"
"#;
        let p = write_tmp("bare", raw);
        let m = parse_board_metadata(&p).expect("bare board parses");
        assert!(m.gated.is_empty());
    }

    #[test]
    fn rejects_unknown_field() {
        // Typo on `default_rmw` → `default_rwm`. `deny_unknown_fields`
        // surfaces the typo at parse time.
        let raw = r#"
[package]
name = "nros-board-typo"
version = "0.1.0"

[package.metadata.nros.board]
zephyr_board = "fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp"
toolchain    = "aarch64-zephyr-elf"
default_rwm  = "cyclonedds"
default_transport = "ethernet"
runner       = "armfvp"
prj_conf      = "prj.conf"
board_conf    = "boards/x.conf"
board_overlay = "boards/x.overlay"
"#;
        let p = write_tmp("typo", raw);
        let err = parse_board_metadata(&p).expect_err("unknown field must be rejected");
        let msg = format!("{err}");
        assert!(
            msg.contains("default_rwm") || msg.contains("unknown field"),
            "diagnostic should name the typo: {msg}"
        );
    }

    #[test]
    fn rejects_missing_required_field() {
        // `runner` dropped → parse error (required field).
        let raw = r#"
[package]
name = "nros-board-incomplete"
version = "0.1.0"

[package.metadata.nros.board]
zephyr_board = "fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp"
toolchain    = "aarch64-zephyr-elf"
default_rmw  = "cyclonedds"
default_transport = "ethernet"
prj_conf      = "prj.conf"
board_conf    = "boards/x.conf"
board_overlay = "boards/x.overlay"
"#;
        let p = write_tmp("missing", raw);
        let err = parse_board_metadata(&p).expect_err("missing required field must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("runner") || msg.contains("missing field"),
            "diagnostic should mention `runner`: {msg}"
        );
    }

    #[test]
    fn rejects_absent_table() {
        let raw = r#"
[package]
name = "nros-board-empty"
version = "0.1.0"
"#;
        let p = write_tmp("absent", raw);
        let err = parse_board_metadata(&p).expect_err("absent table must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("package.metadata.nros.board"),
            "diagnostic should mention the table path: {msg}"
        );
    }

    // -------------------------------------------------------------------
    // board.cmake parser + drift tests
    // -------------------------------------------------------------------

    #[test]
    fn parses_board_cmake_basic() {
        let src = r#"
# Phase 215.A.2 — FVP board manifest
set(NROS_BOARD_ZEPHYR_ID "fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp")
set(NROS_BOARD_TOOLCHAIN "aarch64-zephyr-elf")
set(NROS_BOARD_GATED_PKGS "arm-fvp")
set(NROS_BOARD_DEFAULT_RMW "cyclonedds")
set(NROS_BOARD_DEFAULT_TRANSPORT "ethernet")
set(NROS_BOARD_RUNNER "armfvp")
set(NROS_BOARD_PRJ_CONF "${CMAKE_CURRENT_LIST_DIR}/prj.conf")
set(NROS_BOARD_BOARD_CONF "${CMAKE_CURRENT_LIST_DIR}/boards/x.conf")
set(NROS_BOARD_BOARD_OVERLAY "${CMAKE_CURRENT_LIST_DIR}/boards/x.overlay")
"#;
        let m = parse_board_cmake(src);
        assert_eq!(
            m.get("NROS_BOARD_ZEPHYR_ID").map(String::as_str),
            Some("fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp")
        );
        assert_eq!(
            m.get("NROS_BOARD_RUNNER").map(String::as_str),
            Some("armfvp")
        );
        assert!(m.contains_key("NROS_BOARD_GATED_PKGS"));
    }

    #[test]
    fn parses_board_cmake_cache_variant() {
        let src = r#"set(NROS_BOARD_RUNNER "armfvp" CACHE STRING "runner")"#;
        let m = parse_board_cmake(src);
        assert_eq!(
            m.get("NROS_BOARD_RUNNER").map(String::as_str),
            Some("armfvp")
        );
    }

    #[test]
    fn parses_board_cmake_multiline_set() {
        // The real FVP board.cmake at Phase 215.A.2 uses the multi-line
        // `set(NAME\n    "value")` form. The parser must collapse
        // multi-line calls onto a single statement before tokenising.
        // Regression test for Phase 215.A/C verification.
        let src = r#"
# multiline form (as in nros-board-fvp-aemv8r-smp/board.cmake)
set(NROS_BOARD_ZEPHYR_ID
    "fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp")
set(NROS_BOARD_TOOLCHAIN
    "aarch64-zephyr-elf")
set(NROS_BOARD_PRJ_CONF
    "${CMAKE_CURRENT_LIST_DIR}/prj.conf")
"#;
        let m = parse_board_cmake(src);
        assert_eq!(
            m.get("NROS_BOARD_ZEPHYR_ID").map(String::as_str),
            Some("fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp"),
            "multi-line ZEPHYR_ID must round-trip",
        );
        assert_eq!(
            m.get("NROS_BOARD_TOOLCHAIN").map(String::as_str),
            Some("aarch64-zephyr-elf"),
        );
        assert!(
            m.get("NROS_BOARD_PRJ_CONF")
                .map(|s| s.contains("prj.conf"))
                .unwrap_or(false),
            "multi-line PRJ_CONF must round-trip",
        );
    }

    #[test]
    fn drift_compute_agreement() {
        let cargo = BoardMetadata {
            zephyr_board: "fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp".into(),
            toolchain: "aarch64-zephyr-elf".into(),
            gated: vec!["arm-fvp".into()],
            default_rmw: "cyclonedds".into(),
            default_transport: "ethernet".into(),
            runner: "armfvp".into(),
            prj_conf: "prj.conf".into(),
            board_conf: "boards/x.conf".into(),
            board_overlay: "boards/x.overlay".into(),
        };
        let mut cmake: BTreeMap<String, String> = BTreeMap::new();
        cmake.insert(
            "NROS_BOARD_ZEPHYR_ID".into(),
            "fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp".into(),
        );
        cmake.insert("NROS_BOARD_TOOLCHAIN".into(), "aarch64-zephyr-elf".into());
        cmake.insert("NROS_BOARD_GATED_PKGS".into(), "arm-fvp".into());
        cmake.insert("NROS_BOARD_DEFAULT_RMW".into(), "cyclonedds".into());
        cmake.insert("NROS_BOARD_DEFAULT_TRANSPORT".into(), "ethernet".into());
        cmake.insert("NROS_BOARD_RUNNER".into(), "armfvp".into());
        cmake.insert("NROS_BOARD_PRJ_CONF".into(), "/abs/path/to/prj.conf".into());
        cmake.insert("NROS_BOARD_BOARD_CONF".into(), "/abs/path/to/x.conf".into());
        cmake.insert(
            "NROS_BOARD_BOARD_OVERLAY".into(),
            "/abs/path/to/x.overlay".into(),
        );
        let drift = compute_drift(&cargo, &cmake);
        assert!(drift.is_empty(), "no drift expected: {drift:?}");
    }

    #[test]
    fn drift_compute_runner_mismatch() {
        let cargo = BoardMetadata {
            zephyr_board: "x".into(),
            toolchain: "y".into(),
            gated: vec![],
            default_rmw: "zenoh".into(),
            default_transport: "ethernet".into(),
            runner: "qemu".into(),
            prj_conf: "prj.conf".into(),
            board_conf: "x.conf".into(),
            board_overlay: "x.overlay".into(),
        };
        let mut cmake: BTreeMap<String, String> = BTreeMap::new();
        cmake.insert("NROS_BOARD_RUNNER".into(), "armfvp".into());
        let drift = compute_drift(&cargo, &cmake);
        assert_eq!(drift.len(), 1);
        assert_eq!(drift[0].field, "runner");
        assert_eq!(drift[0].cargo_metadata, "qemu");
        assert_eq!(drift[0].board_cmake, "armfvp");
    }
}
