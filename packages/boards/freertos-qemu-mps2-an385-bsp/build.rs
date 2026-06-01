//! Phase 212.H.3 / M.5.a.3 — FreeRTOS BSP `build.rs`.
//!
//! Cargo-native adapter for FreeRTOS per
//! `docs/design/rtos-integration-pattern.md` §3 (the cargo path IS the
//! adapter — no separate `integrations/freertos/` directory).
//!
//! Two emissions per build:
//!
//! 1. `$OUT_DIR/nros-system/nros_config_generated.h` — diagnostic
//!    header carrying the resolved `[system]` scalars (system name,
//!    domain id, RMW choice, zenoh locator, component count). Kept
//!    around for downstream C glue + the H.3 acceptance test which
//!    asserts its presence.
//! 2. `$OUT_DIR/nros-system/system_main.rs` — Rust shim consumed by
//!    `src/lib.rs` via `include!`. Declares the per-pkg
//!    `__nros_component_<sanitised_pkg>_register` symbols (M.5.a.1
//!    ABI), assembles them into a `&[ComponentRegisterFn]` static, and
//!    defines the `nros_system_run` entry that drives
//!    `ExecutorComponentRuntime` through the codegen-system component
//!    list. Phase 212.M.5.a.3.
//!
//! Bringup-spec sources (first match wins):
//!
//! 1. `$OUT_DIR/nros-system/nros-plan.json` if `nros codegen-system`
//!    succeeded (probed at the start of the build).
//! 2. `NROS_SYSTEM_TOML` env var (a `<bringup>/system.toml` path).
//! 3. `<NROS_BRINGUP_DIR>/system.toml` if `NROS_BRINGUP_DIR` is set.
//! 4. Crate-local defaults below (empty component list — weak-stub
//!    runtime that wakes the board crate but never registers anything).

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let system_dir = out_dir.join("nros-system");
    fs::create_dir_all(&system_dir).expect("create nros-system out dir");

    // Optional: shell `nros codegen-system` if available. The probe is
    // cheap (one --help call) and the bake itself is best-effort — we
    // continue with the toml/default baker regardless so the BSP keeps
    // building even when nros-cli isn't on PATH.
    let _codegen_ok = try_nros_codegen_system(&system_dir);

    // Prefer the planner's `nros-plan.json` (richer; future-proof) but
    // fall back to a hand-parse of `system.toml` for fixtures that
    // can't reach the planner yet.
    let spec = read_spec_from_plan_json(&system_dir)
        .or_else(read_spec_from_system_toml)
        .unwrap_or_default();

    bake_system_header(&spec, &system_dir);
    bake_system_main_rs(&spec, &system_dir);

    // Expose the emitted dir to `src/lib.rs::include!`.
    println!("cargo:rustc-env=NROS_SYSTEM_DIR={}", system_dir.display());
    println!("cargo:nros_system_dir={}", system_dir.display());

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=NROS_SYSTEM_TOML");
    println!("cargo:rerun-if-env-changed=NROS_BRINGUP_DIR");
    println!("cargo:rerun-if-env-changed=NROS_WORKSPACE_DIR");
    if let Ok(p) = env::var("NROS_SYSTEM_TOML") {
        println!("cargo:rerun-if-changed={p}");
    }
    if let Ok(d) = env::var("NROS_BRINGUP_DIR") {
        println!("cargo:rerun-if-changed={d}/system.toml");
    }
}

/// Subset of `system.toml` the BSP bakes into the per-system tree.
struct SystemSpec {
    name: String,
    domain_id: u32,
    rmw: String,
    zenoh_locator: String,
    components: Vec<String>,
}

impl Default for SystemSpec {
    fn default() -> Self {
        Self {
            name: "freertos_qemu_mps2_an385".to_string(),
            domain_id: 0,
            rmw: "zenoh".to_string(),
            zenoh_locator: "tcp/10.0.2.2:7447".to_string(),
            components: Vec::new(),
        }
    }
}

fn read_spec_from_system_toml() -> Option<SystemSpec> {
    let path = env::var_os("NROS_SYSTEM_TOML")
        .map(PathBuf::from)
        .or_else(|| env::var_os("NROS_BRINGUP_DIR").map(|d| PathBuf::from(d).join("system.toml")))
        .filter(|p| p.is_file())?;
    let raw = fs::read_to_string(&path).ok()?;
    let mut spec = SystemSpec::default();
    let mut in_system = false;
    for line in raw.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            in_system = section.trim() == "system";
            continue;
        }
        if !in_system {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim();
            let v = v.trim().trim_matches(|c: char| c == '"' || c == '\'');
            match k {
                "name" => spec.name = v.to_string(),
                "domain_id" => spec.domain_id = v.parse().unwrap_or(0),
                "rmw" => spec.rmw = v.to_string(),
                "zenoh_locator" => spec.zenoh_locator = v.to_string(),
                "components" => {
                    let inner = v.trim_matches(|c: char| c == '[' || c == ']');
                    spec.components = inner
                        .split(',')
                        .map(|s| s.trim().trim_matches('"').to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                _ => {}
            }
        }
    }
    Some(spec)
}

/// Try to lift component identities from `nros-plan.json` if the
/// codegen-system bake landed one. Plan-shape parsing is intentionally
/// permissive — we scan for `"package": "..."` lines under the
/// `components` section, which keeps the build.rs from pulling in a
/// JSON parser just for this lookup.
fn read_spec_from_plan_json(system_dir: &Path) -> Option<SystemSpec> {
    let path = system_dir.join("nros-plan.json");
    let raw = fs::read_to_string(&path).ok()?;
    let mut spec = SystemSpec::default();
    let mut in_components = false;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("\"components\"") {
            in_components = true;
            continue;
        }
        if in_components && (trimmed.starts_with(']') || trimmed.starts_with("\"")) {
            if trimmed.starts_with(']') {
                in_components = false;
                continue;
            }
            // `"package": "talker_pkg",` — grab the value.
            if let Some(rest) = trimmed.strip_prefix("\"package\"") {
                if let Some((_, v)) = rest.split_once(':') {
                    let v = v.trim().trim_matches(|c: char| c == ',' || c == '"');
                    if !v.is_empty() {
                        spec.components.push(v.to_string());
                    }
                }
            }
        }
    }
    if spec.components.is_empty() {
        None
    } else {
        Some(spec)
    }
}

/// Optimistic call into `nros codegen-system` (Phase 212.E). Verb is
/// the hyphenated `codegen-system` per `nros --help`.
fn try_nros_codegen_system(out: &Path) -> bool {
    let cli = env::var("NROS_CLI").unwrap_or_else(|_| "nros".to_string());
    let probe = Command::new(&cli)
        .args(["codegen-system", "--help"])
        .output();
    if !matches!(&probe, Ok(o) if o.status.success()) {
        return false;
    }
    let Some(bringup) = env::var_os("NROS_BRINGUP_DIR").map(PathBuf::from) else {
        return false;
    };
    let Some(workspace) = env::var_os("NROS_WORKSPACE_DIR").map(PathBuf::from) else {
        return false;
    };
    let out_arg = out.to_str().unwrap_or_default();
    let status = Command::new(&cli)
        .args(["codegen-system"])
        .arg("--workspace")
        .arg(&workspace)
        .arg("--bringup")
        .arg(&bringup)
        .args(["--target", "thumbv7m-none-eabi"])
        .args(["--out", out_arg])
        .status();
    matches!(status, Ok(s) if s.success())
}

fn bake_system_header(spec: &SystemSpec, out: &Path) {
    let mut s = String::new();
    s.push_str("// AUTO-GENERATED by freertos-qemu-mps2-an385-bsp build.rs\n");
    s.push_str("#ifndef NROS_CONFIG_GENERATED_H\n#define NROS_CONFIG_GENERATED_H\n");
    s.push_str(&format!("#define NROS_SYSTEM_NAME \"{}\"\n", spec.name));
    s.push_str(&format!("#define NROS_DOMAIN_ID {}u\n", spec.domain_id));
    s.push_str(&format!("#define NROS_RMW_NAME \"{}\"\n", spec.rmw));
    s.push_str(&format!(
        "#define NROS_ZENOH_LOCATOR \"{}\"\n",
        spec.zenoh_locator
    ));
    s.push_str(&format!(
        "#define NROS_COMPONENT_COUNT {}u\n",
        spec.components.len()
    ));
    s.push_str("#endif\n");
    fs::write(out.join("nros_config_generated.h"), s).expect("write generated header");
}

/// Emit `system_main.rs` — declares each component's M.5.a.1 mangled
/// register fn `extern "Rust"`, packs them into a static slice, and
/// defines the `nros_system_run` entry that builds the executor +
/// drives the M.5.a.2 `ExecutorComponentRuntime`.
fn bake_system_main_rs(spec: &SystemSpec, out: &Path) {
    let mut s = String::new();
    s.push_str("// AUTO-GENERATED by freertos-qemu-mps2-an385-bsp build.rs (Phase 212.M.5.a.3)\n");
    s.push_str("// Included by `src/lib.rs` via `include!(concat!(env!(\"NROS_SYSTEM_DIR\"), \"/system_main.rs\"))`.\n\n");
    s.push_str(&format!(
        "/// Resolved bringup name: `{}`. Domain id `{}`. RMW `{}`.\n",
        spec.name, spec.domain_id, spec.rmw
    ));
    s.push_str(&format!(
        "pub const NROS_SYSTEM_NAME: &str = \"{}\";\n",
        spec.name
    ));
    s.push_str(&format!(
        "pub const NROS_DOMAIN_ID: u32 = {};\n",
        spec.domain_id
    ));
    s.push_str(&format!(
        "pub const NROS_ZENOH_LOCATOR: &str = \"{}\";\n\n",
        spec.zenoh_locator
    ));

    // Per-component extern decls. Empty components → emit an empty
    // static; the runtime still spins so the example bring-up shape
    // (board::run -> Executor::open -> spin loop) is preserved.
    //
    // `safe fn` inside the `unsafe extern "Rust"` block matches the
    // `nros::component!()` macro emit (a plain `pub extern "Rust"
    // fn`, safely callable, just `#[unsafe(no_mangle)]`-exported).
    // The `safe` keyword is required for the coercion into the safe
    // `ComponentRegisterFn = fn(...)` fn-pointer type to succeed.
    // Phase 212.M.5.a.4 — emit `_init` / `_dispatch` / `_tick` decls
    // alongside `_register` so the BSP can pair them into matching fn
    // tables. `_dispatch` + `_tick` are emitted `unsafe` by the macro
    // (their `*mut ()` arg has Box-leak provenance), so the extern
    // decls keep the `unsafe` qualifier — only `_register` and `_init`
    // qualify for the `safe` coercion into a non-unsafe fn pointer.
    s.push_str("unsafe extern \"Rust\" {\n");
    for c in &spec.components {
        let p = sanitize(c);
        s.push_str(&format!(
            "    safe fn __nros_component_{p}_register(\n        ctx: &mut ::nros::ComponentContext<'_>,\n    ) -> ::nros::ComponentResult<()>;\n"
        ));
        s.push_str(&format!(
            "    safe fn __nros_component_{p}_init() -> *mut ();\n"
        ));
        s.push_str(&format!(
            "    fn __nros_component_{p}_dispatch(\n        state: *mut (),\n        callback: ::nros::CallbackId<'_>,\n        ctx: &mut ::nros::CallbackCtx<'_>,\n    );\n"
        ));
        s.push_str(&format!(
            "    fn __nros_component_{p}_tick(\n        state: *mut (),\n        ctx: &mut ::nros::TickCtx<'_>,\n    );\n"
        ));
    }
    s.push_str("}\n\n");

    s.push_str("/// M.5.a.1 mangled register fns packed in plan order.\n");
    s.push_str("pub static NROS_REGISTER_FNS: &[::nros::ComponentRegisterFn] = &[\n");
    for c in &spec.components {
        // SAFETY: the extern decl above resolves at link time to the
        // `#[unsafe(no_mangle)] extern "Rust" fn` that `nros::component!()`
        // emits, whose signature is exactly `ComponentRegisterFn`.
        s.push_str(&format!("    __nros_component_{}_register,\n", sanitize(c)));
    }
    s.push_str("];\n\n");

    s.push_str("/// M.5.a.4 parallel init fns — index-paired with NROS_REGISTER_FNS.\n");
    s.push_str("pub static NROS_INIT_FNS: &[::nros::ComponentInitFn] = &[\n");
    for c in &spec.components {
        s.push_str(&format!("    __nros_component_{}_init,\n", sanitize(c)));
    }
    s.push_str("];\n\n");

    s.push_str("/// M.5.a.4 parallel dispatch fns — index-paired with NROS_REGISTER_FNS.\n");
    s.push_str("pub static NROS_DISPATCH_FNS: &[::nros::ComponentDispatchFn] = &[\n");
    for c in &spec.components {
        s.push_str(&format!("    __nros_component_{}_dispatch,\n", sanitize(c)));
    }
    s.push_str("];\n\n");

    s.push_str("/// M.5.a.4 parallel tick fns — index-paired with NROS_REGISTER_FNS.\n");
    s.push_str("pub static NROS_TICK_FNS: &[::nros::ComponentTickFn] = &[\n");
    for c in &spec.components {
        s.push_str(&format!("    __nros_component_{}_tick,\n", sanitize(c)));
    }
    s.push_str("];\n\n");

    s.push_str("pub const NROS_COMPONENT_NAMES: &[&str] = &[\n");
    for c in &spec.components {
        s.push_str(&format!("    \"{}\",\n", c));
    }
    s.push_str("];\n");

    fs::write(out.join("system_main.rs"), s).expect("write system_main.rs");
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}
