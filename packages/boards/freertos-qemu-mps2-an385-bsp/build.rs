//! Phase 212.H.3 — FreeRTOS BSP `build.rs`.
//!
//! Cargo-native adapter for FreeRTOS per
//! `docs/design/rtos-integration-pattern.md` §3 (the cargo path IS the
//! adapter — no separate `integrations/freertos/` directory). Two
//! responsibilities:
//!
//!   1. Run `nros codegen system` (when the subcommand from Phase
//!      212.E is available) to emit a per-system tree under
//!      `$OUT_DIR/nros-system/`. While 212.E is still unticked the
//!      build script bakes a minimal `nros_config_generated.h` +
//!      `system_main.c` from the inline defaults (or the
//!      `NROS_SYSTEM_*` env vars) so the adapter SHAPE lands without
//!      gating on the codegen verb.
//!   2. Compile the baked `system_main.c` via `cc::Build` into a
//!      `libnros_system.a` linked into the final firmware.
//!
//! Hard cap: this file MUST stay ≤200 LoC (Phase 212.H.8 budget
//! enforced by `tokei` in CI). Keep new logic factored into the
//! existing private helpers rather than expanding `main()`.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let system_dir = out_dir.join("nros-system");
    fs::create_dir_all(&system_dir).expect("create nros-system out dir");

    // --- Resolve [system] fields ---
    // Three sources, first-match wins:
    //   1. `NROS_SYSTEM_TOML` env var (path to a bringup `system.toml`).
    //   2. The optional `system.toml` next to the consuming crate's
    //      manifest (forwarded via `NROS_BRINGUP_DIR`).
    //   3. The crate-local defaults below.
    let system = resolve_system_spec();

    // --- Try `nros codegen system` (Phase 212.E) ---
    let codegen_ok = try_nros_codegen_system(&system, &system_dir);
    if !codegen_ok {
        // Fallback: bake the two files directly so the adapter shape
        // is exercised even before 212.E lands.
        bake_system_header(&system, &system_dir);
        bake_system_main(&system, &system_dir);
    }

    // --- Compile system_main.c ---
    // Re-use the underlying board crate's FREERTOS_CFLAGS contract so
    // the system-main TU matches the kernel + lwIP TUs' ABI.
    let mut sys = cc::Build::new();
    configure_target(&mut sys);
    sys.include(&system_dir);
    sys.file(system_dir.join("system_main.c"));
    sys.compile("nros_system");

    println!("cargo:rustc-link-search={}", system_dir.display());
    println!("cargo:nros_system_dir={}", system_dir.display());

    // --- Rerun triggers ---
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=NROS_SYSTEM_TOML");
    println!("cargo:rerun-if-env-changed=NROS_BRINGUP_DIR");
    println!("cargo:rerun-if-env-changed=FREERTOS_CFLAGS");
    if let Ok(p) = env::var("NROS_SYSTEM_TOML") {
        println!("cargo:rerun-if-changed={p}");
    }
    let _ = manifest_dir; // silence unused on minimal builds
}

/// Subset of `system.toml` the BSP actually bakes into the
/// per-system header. Mirrors `[system]` fields from the Phase 212
/// schema (`docs/design/rtos-integration-pattern.md` §4).
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

fn resolve_system_spec() -> SystemSpec {
    let toml_path = env::var_os("NROS_SYSTEM_TOML")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("NROS_BRINGUP_DIR")
                .map(|d| PathBuf::from(d).join("system.toml"))
        })
        .filter(|p| p.is_file());
    let Some(path) = toml_path else {
        return SystemSpec::default();
    };
    let raw = fs::read_to_string(&path).unwrap_or_default();
    let mut spec = SystemSpec::default();
    // Minimal hand-parser so we don't pull `toml` into the build-dep
    // closure. Recognises `[system]` scalars: `name`, `domain_id`,
    // `rmw`, `zenoh_locator`, and a one-line `components = [...]`.
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
    spec
}

/// Optimistic call into `nros codegen system` (Phase 212.E). Returns
/// `true` iff the subcommand exists AND succeeds.
fn try_nros_codegen_system(spec: &SystemSpec, out: &Path) -> bool {
    let cli = env::var("NROS_CLI").unwrap_or_else(|_| "nros".to_string());
    let probe = Command::new(&cli).args(["codegen", "system", "--help"]).output();
    match probe {
        Ok(o) if o.status.success() => {
            let out_arg = out.to_str().unwrap_or_default();
            let status = Command::new(&cli)
                .args(["codegen", "system"])
                .args(["--target", "thumbv7m-none-eabi"])
                .args(["--out", out_arg])
                .args(["--rmw", &spec.rmw])
                .status();
            matches!(status, Ok(s) if s.success())
        }
        _ => false,
    }
}

fn bake_system_header(spec: &SystemSpec, out: &Path) {
    let mut s = String::new();
    s.push_str("// AUTO-GENERATED by freertos-qemu-mps2-an385-bsp build.rs (212.H.3 fallback)\n");
    s.push_str("#ifndef NROS_CONFIG_GENERATED_H\n#define NROS_CONFIG_GENERATED_H\n");
    s.push_str(&format!("#define NROS_SYSTEM_NAME \"{}\"\n", spec.name));
    s.push_str(&format!("#define NROS_DOMAIN_ID {}u\n", spec.domain_id));
    s.push_str(&format!("#define NROS_RMW_NAME \"{}\"\n", spec.rmw));
    s.push_str(&format!("#define NROS_ZENOH_LOCATOR \"{}\"\n", spec.zenoh_locator));
    s.push_str(&format!("#define NROS_COMPONENT_COUNT {}u\n", spec.components.len()));
    s.push_str("#endif\n");
    fs::write(out.join("nros_config_generated.h"), s).expect("write generated header");
}

fn bake_system_main(spec: &SystemSpec, out: &Path) {
    let mut s = String::new();
    s.push_str("// AUTO-GENERATED by freertos-qemu-mps2-an385-bsp build.rs (212.H.3 fallback)\n");
    s.push_str("#include \"nros_config_generated.h\"\n");
    // Forward-declare each component's register entry as `weak`. When
    // the component crate ships a real impl the linker picks it; the
    // missing-symbol fallback is a no-op stub so the adapter shape
    // links even when the bringup pkg lists components whose crates
    // don't yet provide a register entry. Phase 212.F.2 lint will
    // surface drift between `[system].components` and the actual
    // crates' surfaces.
    for c in &spec.components {
        s.push_str(&format!(
            "__attribute__((weak)) void nros_component_{}_register(void) {{}}\n",
            sanitize(c)
        ));
    }
    s.push_str("void nros_system_main(void) {\n");
    for c in &spec.components {
        s.push_str(&format!("    nros_component_{}_register();\n", sanitize(c)));
    }
    s.push_str("}\n");
    fs::write(out.join("system_main.c"), s).expect("write system_main.c");
}

fn sanitize(name: &str) -> String {
    name.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

fn configure_target(build: &mut cc::Build) {
    build.opt_level(2).warnings(false).flag_if_supported("-ffunction-sections");
    let cflags = env::var("FREERTOS_CFLAGS").unwrap_or_else(|_| "-mcpu=cortex-m3 -mthumb".into());
    for f in cflags.split_whitespace() {
        build.flag_if_supported(f);
    }
}
