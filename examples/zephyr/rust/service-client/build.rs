// Bridge Zephyr Kconfig values into Rust `cfg` flags. Pattern from
// modules/lang/rust/samples/philosophers — required for embedded
// ARM Cortex-A targets (silent-boot bug if absent — Phase 92.4).
use std::{env, fs};

fn main() {
    zephyr_build::export_kconfig_bool_options();

    // Phase 225.P (known-issue #17) — bake the zenoh locator + domain id
    // into the Rust path. `nros::zephyr_component_main!` reads
    // `option_env!("NROS_LOCATOR")` / `option_env!("NROS_DOMAIN_ID")` at
    // compile time; without a baked value it falls back to an EMPTY
    // locator, which makes zenoh-pico do multicast scouting — and
    // native_sim NSOS never reaches the host `zenohd` that way (no
    // `connect()` syscall ever issues). The C API path already consumes
    // `CONFIG_NROS_ZENOH_LOCATOR` from Kconfig; mirror that here so the
    // Kconfig value is the single source of truth for BOTH languages.
    bake_kconfig_str("CONFIG_NROS_ZENOH_LOCATOR", "NROS_LOCATOR");
    bake_kconfig_int("CONFIG_NROS_DOMAIN_ID", "NROS_DOMAIN_ID");

    // Issue 0163 — the XRCE path has no CONFIG_NROS_ZENOH_LOCATOR; its agent
    // endpoint lives in CONFIG_NROS_XRCE_AGENT_{ADDR,PORT}. Synthesize the
    // `host:port` locator the xrce session parser expects and bake it into
    // the SAME `NROS_LOCATOR` env the macro reads (mutually exclusive with
    // the zenoh bake above — an image selects exactly one RMW).
    if kconfig_line("CONFIG_NROS_RMW_XRCE").is_some_and(|l| l.ends_with("=y")) {
        let addr = kconfig_line("CONFIG_NROS_XRCE_AGENT_ADDR")
            .and_then(|l| l.split_once('=').map(|(_, v)| v.trim().trim_matches('"').to_string()))
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let port = kconfig_line("CONFIG_NROS_XRCE_AGENT_PORT")
            .and_then(|l| l.split_once('=').map(|(_, v)| v.trim().to_string()))
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "2018".to_string());
        println!("cargo:rustc-env=NROS_LOCATOR={addr}:{port}");
    }
}

/// Read a quoted string Kconfig (`CONFIG_X="value"`) from the generated
/// `.config` (path in `$DOTCONFIG`) and re-export it as a `rustc-env` so
/// `option_env!(rust_env)` sees it at compile time. No-op if the
/// Kconfig is unset or empty.
fn bake_kconfig_str(kconfig: &str, rust_env: &str) {
    println!("cargo:rerun-if-env-changed=DOTCONFIG");
    println!("cargo:rerun-if-env-changed={rust_env}");
    let Some(line) = kconfig_line(kconfig) else {
        return;
    };
    // `CONFIG_X="tcp/127.0.0.1:7456"` → strip key, `=`, and the quotes.
    let Some(rhs) = line.split_once('=').map(|(_, v)| v.trim()) else {
        return;
    };
    let val = rhs.trim_matches('"');
    if !val.is_empty() {
        println!("cargo:rustc-env={rust_env}={val}");
    }
}

/// Read an integer Kconfig (`CONFIG_X=N`) and re-export it as a
/// `rustc-env` string. No-op if unset.
fn bake_kconfig_int(kconfig: &str, rust_env: &str) {
    println!("cargo:rerun-if-env-changed={rust_env}");
    let Some(line) = kconfig_line(kconfig) else {
        return;
    };
    let Some(rhs) = line.split_once('=').map(|(_, v)| v.trim()) else {
        return;
    };
    if !rhs.is_empty() {
        println!("cargo:rustc-env={rust_env}={rhs}");
    }
}

fn kconfig_line(kconfig: &str) -> Option<String> {
    let dotconfig = env::var("DOTCONFIG").ok()?;
    let body = fs::read_to_string(&dotconfig).ok()?;
    let prefix = format!("{kconfig}=");
    body.lines()
        .find(|l| l.starts_with(&prefix))
        .map(|l| l.to_string())
}
