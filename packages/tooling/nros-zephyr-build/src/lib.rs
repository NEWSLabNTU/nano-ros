//! phase-291 (#211) — the canonical zephyr-leaf Kconfig→`rustc-env` bake.
//!
//! Every zephyr Rust leaf (standalone example or workspace `zephyr_entry`)
//! calls [`bake_nros_config`] from its `build.rs`, collapsing the previously
//! copy-pasted ~81-line file to:
//!
//! ```ignore
//! fn main() {
//!     zephyr_build::export_kconfig_bool_options(); // Kconfig→cfg bridge (phase-92.4)
//!     nros_zephyr_build::bake_nros_config();       // #17 locator/domain + 0163 XRCE bake
//! }
//! ```
//!
//! What it bakes (known-issue #17): `nros::main!`'s Zephyr branch and
//! `nros::zephyr_component_main!` read `option_env!("NROS_LOCATOR")` /
//! `option_env!("NROS_DOMAIN_ID")` at compile time; without a baked value the
//! locator falls back to EMPTY → zenoh-pico multicast scouting, which
//! native_sim NSOS can never satisfy (no `connect()` is ever issued). The C
//! API path consumes `CONFIG_NROS_ZENOH_LOCATOR` from Kconfig directly; this
//! helper re-exports the same Kconfig values so Kconfig stays the single
//! source of truth for BOTH languages. (`DOTCONFIG` — the generated
//! `.config` path — is exported by the Zephyr build system.)
//!
//! The bake MUST run in the LEAF's own `build.rs`: `cargo:rustc-env` from a
//! dependency's build script never reaches other crates' compilation, and the
//! `option_env!` reads expand in the leaf. That is why this is a shared
//! build-DEPENDENCY, not logic inside a runtime crate.
//!
//! Zero dependencies by design: upstream `zephyr-build` resolves as a
//! west-module PATH dep only (a leaf `Cargo.lock` entry with no `source =`),
//! so depending on it here would break host `cargo check --workspace`. The
//! `export_kconfig_bool_options()` call therefore stays in the leaf.

use std::{env, fs};

/// Bake the nros Kconfig values into `rustc-env` directives:
///
/// - `CONFIG_NROS_ZENOH_LOCATOR` → `NROS_LOCATOR` (quoted string, phase-225)
/// - `CONFIG_NROS_DOMAIN_ID` → `NROS_DOMAIN_ID` (integer, issue 0161)
/// - issue 0163 — when `CONFIG_NROS_RMW_XRCE=y`, synthesize the `host:port`
///   agent locator from `CONFIG_NROS_XRCE_AGENT_{ADDR,PORT}` (defaults
///   `127.0.0.1:2018`) into the SAME `NROS_LOCATOR` env (mutually exclusive
///   with the zenoh bake — an image selects exactly one RMW). Self-gated, so
///   zenoh-only images (and workspace entries) are unaffected.
///
/// No-op (beyond `rerun-if-env-changed`) when `DOTCONFIG` is unset or the
/// Kconfigs are absent/empty — a host `cargo check` of a leaf stays quiet.
pub fn bake_nros_config() {
    println!("cargo:rerun-if-env-changed=DOTCONFIG");
    println!("cargo:rerun-if-env-changed=NROS_LOCATOR");
    println!("cargo:rerun-if-env-changed=NROS_DOMAIN_ID");
    let Some(body) = env::var("DOTCONFIG").ok().and_then(|p| {
        println!("cargo:rerun-if-changed={p}");
        fs::read_to_string(&p).ok()
    }) else {
        return;
    };
    for line in bake_directives(&body) {
        println!("{line}");
    }
}

/// Pure core of [`bake_nros_config`]: `.config` body → the `cargo:rustc-env`
/// directive lines. Split out so tests assert emission without a cargo run.
fn bake_directives(dotconfig: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(val) = kconfig_str(dotconfig, "CONFIG_NROS_ZENOH_LOCATOR") {
        out.push(format!("cargo:rustc-env=NROS_LOCATOR={val}"));
    }
    if let Some(val) = kconfig_raw(dotconfig, "CONFIG_NROS_DOMAIN_ID") {
        out.push(format!("cargo:rustc-env=NROS_DOMAIN_ID={val}"));
    }
    // Issue 0163 — the XRCE path has no CONFIG_NROS_ZENOH_LOCATOR; its agent
    // endpoint lives in CONFIG_NROS_XRCE_AGENT_{ADDR,PORT}. Synthesize the
    // `host:port` locator the xrce session parser expects.
    if kconfig_raw(dotconfig, "CONFIG_NROS_RMW_XRCE").as_deref() == Some("y") {
        let addr = kconfig_str(dotconfig, "CONFIG_NROS_XRCE_AGENT_ADDR")
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let port = kconfig_raw(dotconfig, "CONFIG_NROS_XRCE_AGENT_PORT")
            .unwrap_or_else(|| "2018".to_string());
        out.push(format!("cargo:rustc-env=NROS_LOCATOR={addr}:{port}"));
    }
    out
}

/// `CONFIG_X="value"` → `Some("value")`; unset/empty → `None`.
fn kconfig_str(body: &str, key: &str) -> Option<String> {
    let raw = kconfig_raw(body, key)?;
    let val = raw.trim_matches('"');
    (!val.is_empty()).then(|| val.to_string())
}

/// `CONFIG_X=rhs` → `Some(rhs)` (verbatim, trimmed); unset/empty → `None`.
fn kconfig_raw(body: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    body.lines()
        .find_map(|l| l.strip_prefix(&prefix))
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zenoh_locator_and_domain_bake() {
        let cfg = "CONFIG_NROS_RMW_ZENOH=y\n\
                   CONFIG_NROS_ZENOH_LOCATOR=\"tcp/127.0.0.1:7456\"\n\
                   CONFIG_NROS_DOMAIN_ID=42\n";
        assert_eq!(
            bake_directives(cfg),
            vec![
                "cargo:rustc-env=NROS_LOCATOR=tcp/127.0.0.1:7456".to_string(),
                "cargo:rustc-env=NROS_DOMAIN_ID=42".to_string(),
            ]
        );
    }

    #[test]
    fn unset_and_empty_are_no_ops() {
        assert!(bake_directives("").is_empty());
        assert!(bake_directives("CONFIG_NROS_ZENOH_LOCATOR=\"\"\n").is_empty());
        // A different key sharing the prefix must not match.
        assert!(bake_directives("CONFIG_NROS_ZENOH_LOCATOR_EXTRA=\"x\"\n").is_empty());
    }

    #[test]
    fn xrce_synthesis_with_explicit_endpoint() {
        let cfg = "CONFIG_NROS_RMW_XRCE=y\n\
                   CONFIG_NROS_XRCE_AGENT_ADDR=\"192.0.2.7\"\n\
                   CONFIG_NROS_XRCE_AGENT_PORT=8888\n";
        assert_eq!(
            bake_directives(cfg),
            vec!["cargo:rustc-env=NROS_LOCATOR=192.0.2.7:8888".to_string()]
        );
    }

    #[test]
    fn xrce_synthesis_defaults() {
        assert_eq!(
            bake_directives("CONFIG_NROS_RMW_XRCE=y\n"),
            vec!["cargo:rustc-env=NROS_LOCATOR=127.0.0.1:2018".to_string()]
        );
    }

    #[test]
    fn xrce_absent_emits_nothing() {
        // `# CONFIG_NROS_RMW_XRCE is not set` — the Kconfig-disabled shape.
        let cfg = "# CONFIG_NROS_RMW_XRCE is not set\n\
                   CONFIG_NROS_XRCE_AGENT_ADDR=\"10.0.0.1\"\n";
        assert!(bake_directives(cfg).is_empty());
    }
}
