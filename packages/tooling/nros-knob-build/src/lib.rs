//! A package's own configuration rows, for its build script (RFC-0140 D5,
//! phase-581 W3).
//!
//! A board, platform or RMW package declares its knobs and run-time variables
//! in `nros-knobs.toml` beside its descriptor. The compiled knob table cannot
//! hold an out-of-tree package's rows, so the package reads them itself:
//!
//! ```ignore
//! // build.rs
//! fn main() {
//!     nros_knob_build::emit(env!("CARGO_MANIFEST_DIR"));
//! }
//! // lib.rs
//! include!(concat!(env!("OUT_DIR"), "/nros_knobs.rs"));
//! ```
//!
//! [`emit`] parses the file with `nros-knob-catalog` (the one loader), resolves
//! each knob through `nros_zephyr_build::knob` (the one ladder: the build
//! environment, where `nros build` writes an image's
//! `[image.<id>.knobs.<owner>]` values, then `$DOTCONFIG` on Zephyr), and writes
//! `$OUT_DIR/nros_knobs.rs`: a typed `const` per knob, holding the stated value
//! or the row's default, a `const` per env row naming its variable, and the
//! rows as `static` tables.

use std::{
    fmt::Write as _,
    path::{Path, PathBuf},
};

use nros_knob_catalog::{PACKAGE_FILE, Row, RowKind, parse_own_file};
use nros_zephyr_build::RetiredRoad;

/// One knob as the build resolved it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The row's key (`acme.pool_slots`).
    pub key: String,
    /// Its build env spelling (`NROS_ACME_POOL_SLOTS`).
    pub env: String,
    /// The value the build stated (environment or `.config`); `None` when
    /// nothing did, and the row's default applies.
    pub stated: Option<String>,
    /// [`Self::stated`], else the row's default.
    pub value: Option<String>,
}

/// Read `<manifest_dir>/nros-knobs.toml`, resolve its knobs, and write
/// `$OUT_DIR/nros_knobs.rs`. Returns what it resolved, so the build script
/// can use a value itself.
///
/// # Panics
///
/// The file is missing or breaks RFC-0140 D2's per-file rules, a retired
/// spelling of one of its knobs is set, or `OUT_DIR` is unset.
pub fn emit(manifest_dir: impl AsRef<Path>) -> Vec<Resolved> {
    let file = manifest_dir.as_ref().join(PACKAGE_FILE);
    println!("cargo:rerun-if-changed={}", file.display());
    let package = std::env::var("CARGO_PKG_NAME").unwrap_or_default();
    let text = std::fs::read_to_string(&file)
        .unwrap_or_else(|e| panic!("nros-knob-build: reading {}: {e}", file.display()));
    let (owner, rows) =
        parse_own_file(&package, &file, &text).unwrap_or_else(|e| panic!("nros-knob-build: {e}"));
    watch(&rows);
    let env = |k: &str| std::env::var(k).ok();
    let resolved = resolve(&rows, &env, &|name| {
        nros_zephyr_build::knob(name).stated_str()
    });
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("nros-knob-build: OUT_DIR is unset"))
        .join("nros_knobs.rs");
    let body = render(&owner, &rows, &resolved);
    if std::fs::read_to_string(&out).ok().as_deref() != Some(body.as_str()) {
        std::fs::write(&out, body)
            .unwrap_or_else(|e| panic!("nros-knob-build: writing {}: {e}", out.display()));
    }
    resolved
}

/// `cargo:rerun-if-env-changed` for every build env spelling of each knob row,
/// canonical and retired: knob values, never paths.
fn watch(rows: &[Row]) {
    for r in rows.iter().filter(|r| r.kind == RowKind::Knob) {
        let canonical = r.spelling(RetiredRoad::Env).unwrap_or_default();
        println!("cargo:rerun-if-env-changed={canonical}");
        for e in r.retired.iter().filter(|e| e.road == RetiredRoad::Env) {
            println!("cargo:rerun-if-env-changed={}", e.spelling);
        }
    }
}

/// Resolve each knob row: refuse a set retired build spelling (the row is the
/// error table, RFC-0140 D4), then ask `stated` (the ladder) for its value.
fn resolve(
    rows: &[Row],
    env: &dyn Fn(&str) -> Option<String>,
    stated: &dyn Fn(&str) -> Option<String>,
) -> Vec<Resolved> {
    let mut out = Vec::new();
    for r in rows.iter().filter(|r| r.kind == RowKind::Knob) {
        let canonical = r.spelling(RetiredRoad::Env).unwrap_or_default().to_string();
        for e in r.retired.iter().filter(|e| e.road == RetiredRoad::Env) {
            if env(&e.spelling).is_some_and(|v| !v.trim().is_empty()) {
                panic!(
                    "nros-knob-build: {}",
                    nros_zephyr_build::RETIRED_TEMPLATE
                        .replace("{legacy}", &e.spelling)
                        .replace("{env}", &canonical)
                );
            }
        }
        let value = stated(&canonical);
        out.push(Resolved {
            key: r.name.clone(),
            env: canonical,
            value: value.clone().or_else(|| r.meta.default.clone()),
            stated: value,
        });
    }
    out
}

/// `acme.pool_slots` under owner `acme` -> `POOL_SLOTS`.
fn const_name(owner: &str, key: &str) -> String {
    let rest = key
        .strip_prefix(owner)
        .and_then(|k| k.strip_prefix('.'))
        .unwrap_or(key);
    rest.to_ascii_uppercase().replace(['.', '-'], "_")
}

fn rs_str(s: &str) -> String {
    format!("{s:?}")
}

fn rs_opt(s: Option<&str>) -> String {
    s.map_or_else(|| "None".to_string(), |v| format!("Some({})", rs_str(v)))
}

/// The text of `nros_knobs.rs`.
fn render(owner: &str, rows: &[Row], resolved: &[Resolved]) -> String {
    let mut s = format!(
        "// @generated by nros_knob_build::emit from this package's {PACKAGE_FILE}\n\
         // (owner `{owner}`) -- do not edit.\n\n\
         /// One knob this package declares, as the build resolved it.\n\
         #[derive(Debug, Clone, Copy, PartialEq, Eq)]\n\
         pub struct NrosKnob {{\n    pub key: &'static str,\n    pub env: &'static str,\n    \
         pub kconfig: &'static str,\n    pub ty: Option<&'static str>,\n    \
         /// The stated value, else the row's default, verbatim.\n    \
         pub value: Option<&'static str>,\n    \
         /// Whether the build stated it (the environment or `.config`).\n    \
         pub stated: bool,\n}}\n\n\
         /// One run-time variable this package declares.\n\
         #[derive(Debug, Clone, Copy, PartialEq, Eq)]\n\
         pub struct NrosEnvRow {{\n    pub name: &'static str,\n    \
         pub disposition: &'static str,\n}}\n\n"
    );
    let knobs: Vec<(&Row, &Resolved)> = rows
        .iter()
        .filter(|r| r.kind == RowKind::Knob)
        .zip(resolved)
        .collect();
    s.push_str("/// Every knob of this package.\npub static NROS_KNOBS: &[NrosKnob] = &[\n");
    for (r, v) in &knobs {
        let _ = writeln!(
            s,
            "    NrosKnob {{ key: {}, env: {}, kconfig: {}, ty: {}, value: {}, stated: {} }},",
            rs_str(&r.name),
            rs_str(&v.env),
            rs_str(r.spelling(RetiredRoad::Kconfig).unwrap_or_default()),
            rs_opt(r.meta.ty.as_deref()),
            rs_opt(v.value.as_deref()),
            v.stated.is_some()
        );
    }
    s.push_str("];\n\n");
    for (r, v) in &knobs {
        let name = const_name(owner, &r.name);
        let doc = r.meta.prompt.as_deref().unwrap_or(&r.name);
        let _ = writeln!(s, "/// {doc} (`{}`).", r.name);
        let raw = v.value.as_deref();
        match (r.meta.ty.as_deref(), raw) {
            (Some("bool"), Some(x)) => {
                let b = matches!(x.trim(), "1" | "y" | "true");
                let _ = writeln!(s, "pub const {name}: bool = {b};");
            }
            (Some("int" | "hex") | None, Some(x)) if parse_int(x).is_some() => {
                let _ = writeln!(s, "pub const {name}: i64 = {};", parse_int(x).unwrap());
            }
            (Some("string"), Some(x)) => {
                let _ = writeln!(s, "pub const {name}: &str = {};", rs_str(x));
            }
            _ => {
                let _ = writeln!(s, "pub const {name}: Option<&str> = {};", rs_opt(raw));
            }
        }
    }
    s.push_str("\n/// Every run-time variable of this package.\npub static NROS_ENV_ROWS: &[NrosEnvRow] = &[\n");
    let env_rows: Vec<&Row> = rows.iter().filter(|r| r.kind == RowKind::Env).collect();
    for r in &env_rows {
        let _ = writeln!(
            s,
            "    NrosEnvRow {{ name: {}, disposition: {} }},",
            rs_str(&r.name),
            rs_str(r.meta.disposition.as_deref().unwrap_or("listed"))
        );
    }
    s.push_str("];\n");
    for r in &env_rows {
        let stem = r
            .meta
            .key
            .as_deref()
            .map_or_else(|| r.name.to_ascii_uppercase(), |k| const_name(owner, k));
        let _ = writeln!(
            s,
            "/// The variable `{}`.\npub const ENV_{stem}: &str = {};",
            r.name,
            rs_str(&r.name)
        );
    }
    s
}

fn parse_int(x: &str) -> Option<i64> {
    let x = x.trim();
    match x.strip_prefix("0x").or_else(|| x.strip_prefix("0X")) {
        Some(h) => i64::from_str_radix(h, 16).ok(),
        None => x.parse().ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACME: &str = "owner = \"acme\"\n\n\
        [[knob]]\nkey = \"acme.pool_slots\"\ntype = \"int\"\nprompt = \"Acme pool slots\"\n\
        kconfig_attrs = [\"default 2\"]\nretired = [{ spelling = \"env:ACME_SLOTS\" }]\n\n\
        [[knob]]\nkey = \"acme.fast\"\ntype = \"bool\"\n\n\
        [[env]]\nkey = \"acme.trace\"\ndisposition = \"listed\"\n";

    fn rows() -> Vec<Row> {
        parse_own_file("acme-rmw", Path::new("nros-knobs.toml"), ACME)
            .unwrap()
            .1
    }

    #[test]
    fn a_stated_value_and_a_default_become_typed_consts() {
        let rows = rows();
        let env = |_: &str| None;
        let stated = |k: &str| (k == "NROS_ACME_POOL_SLOTS").then(|| "4".to_string());
        let r = resolve(&rows, &env, &stated);
        assert_eq!(r[0].stated.as_deref(), Some("4"));
        assert_eq!(r[1].value, None);
        let text = render("acme", &rows, &r);
        assert!(text.contains("pub const POOL_SLOTS: i64 = 4;"), "{text}");
        assert!(
            text.contains("pub const FAST: Option<&str> = None;"),
            "{text}"
        );
        assert!(
            text.contains("pub const ENV_TRACE: &str = \"NROS_ACME_TRACE\";"),
            "{text}"
        );
        // Unstated: the row's default.
        let r = resolve(&rows, &env, &|_| None);
        assert_eq!(r[0].value.as_deref(), Some("2"));
        assert!(render("acme", &rows, &r).contains("pub const POOL_SLOTS: i64 = 2;"));
    }

    #[test]
    #[should_panic(expected = "ACME_SLOTS is a retired spelling of NROS_ACME_POOL_SLOTS")]
    fn a_set_retired_spelling_is_refused() {
        let env = |k: &str| (k == "ACME_SLOTS").then(|| "4".to_string());
        let _ = resolve(&rows(), &env, &|_| None);
    }
}
