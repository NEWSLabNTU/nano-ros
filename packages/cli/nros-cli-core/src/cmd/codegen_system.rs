//! `nros codegen system` — Phase 212.E host-time system bake.
//!
//! Reads `<bringup>/system.toml` + `<bringup>/launch/system.launch.xml` and
//! emits the baked compile-time C config + component-registration glue that
//! every embedded RTOS adapter consumes (see
//! `docs/design/rtos-integration-pattern.md`).
//!
//! Outputs land under `<out>/nros-system/`:
//!
//! * `system_config.h` — `#define`s for domain, RMW, locator, QoS.
//! * `system_main.c`   — extern decls of `nros_component_<name>_register`
//!                       symbols, an entry `main()` that calls each in turn
//!                       and spins.
//! * `Cargo.toml`      — workspace stub for Rust components (only emitted if
//!                       at least one component lives in a Rust package).
//! * `nros-plan.json`  — the resolved plan (a thin host-side record of the
//!                       inputs the bake consumed; keeps `nros explain` /
//!                       `nros check` self-contained).
//!
//! Optional `--ahead-of-vendor <kind>` mode emits hookless-vendor artifacts:
//!
//! * `--ahead-of-vendor pio`  — `library.json` snippet next to the bake dir.
//! * `--ahead-of-vendor px4`  — one `nros_<component>/` PX4-native module dir
//!                              per component: `px4_add_module()` CMakeLists,
//!                              `Kconfig` w/ `menuconfig MODULES_NROS_<NAME>`,
//!                              and a `nros_<name>.cpp` stub entry point.
//!                              See Phase 212.H.7 for the shape.

use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use clap::{Args as ClapArgs, ValueEnum};
use eyre::{Context, Result, bail};
use serde::Serialize;

use crate::orchestration::{
    cargo_metadata_schema::{SystemComponentEntry, SystemToml},
    launch_synth::{LaunchInput, resolve_launch},
    nros_config::{BringupPackageEntry, NrosConfig},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum AheadOfVendor {
    /// Emit a PlatformIO `library.json` augment next to the bake dir.
    Pio,
    /// Emit one PX4-native `nros_<component>/` module dir per component
    /// (CMakeLists.txt + Kconfig + cpp/h stub) — see Phase 212.H.7.
    Px4,
}

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Workspace root (defaults to cwd).
    #[arg(long)]
    pub workspace: Option<PathBuf>,

    /// Bringup package name (or path to its directory). Defaults to the
    /// workspace's `[workspace.metadata.nros].default_system`.
    #[arg(long)]
    pub bringup: Option<String>,

    /// Target triple (for cross-compile bake context; recorded into the
    /// plan but doesn't drive codegen logic).
    #[arg(long)]
    pub target: Option<String>,

    /// Output directory (the `nros-system/` subdir is created inside this).
    /// Defaults to `<workspace>/build/<bringup>/`.
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Hookless-vendor mode (`pio` or `px4`). Emits vendor-native artifacts
    /// in addition to the standard bake tree.
    #[arg(long = "ahead-of-vendor", value_enum)]
    pub ahead_of_vendor: Option<AheadOfVendor>,

    /// Phase 212.L.6 — multi-launch disambiguation: pass `<file>` and
    /// the resolver picks `<bringup>/launch/<file>` (cwd / absolute as
    /// fallbacks). `--launch` is the canonical Phase 212.E flag name;
    /// `--file` is kept as an alias for back-compat with the L.6 docs.
    #[arg(long = "launch", visible_alias = "file")]
    pub file: Option<String>,

    /// Phase 212.L.6 — `<node exec="…">` override for synthesised
    /// launches (when the bringup pkg has multiple `[[bin]]` /
    /// `add_executable` targets).
    #[arg(long = "exec")]
    pub exec: Option<String>,
}

pub fn run(args: Args) -> Result<()> {
    let workspace = match args.workspace {
        Some(p) => p,
        None => std::env::current_dir().context("resolve cwd")?,
    };

    let cfg = NrosConfig::from_workspace(&workspace)
        .with_context(|| format!("load workspace at {}", workspace.display()))?;

    let bringup = resolve_bringup(&cfg, args.bringup.as_deref())?;

    let out_dir = args
        .out
        .unwrap_or_else(|| workspace.join("build").join(&bringup.name));
    let bake_dir = out_dir.join("nros-system");

    let component_kinds = classify_components(&cfg, &bringup.system.components);

    // Phase 212.L.6 — resolve the launch input. For a Path A bringup
    // pkg (no Cargo.toml, no CMakeLists.txt) we surface the resolver's
    // hard error unchanged; for synthesisable pkgs the synth XML is
    // dropped after the plan is recorded (codegen-system does not feed
    // the XML to the launch parser today — the bake reads system.toml
    // directly — but resolving now keeps the policy uniform across
    // verbs and rejects nonsense input early).
    //
    // The resolved file path (real or synth temp) is recorded into
    // `nros-plan.json::launch_file` so `nros check` / `nros explain` can
    // see what was used.
    let bringup_dir = bringup
        .manifest_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| workspace.clone());

    // 212.E.2 — when `--target <name>` is given and the bringup has a
    // matching `[deploy.<target>]` block with `launch = "..."`, that
    // override takes precedence over the bringup's
    // `[system].default_launch` (per system-toml-schema-v0.1 §3.1 step 1).
    // An explicit `--launch`/`--file` flag still beats both.
    let effective_file: Option<String> = args.file.clone().or_else(|| {
        args.target
            .as_deref()
            .and_then(|t| bringup.system.deploy.get(t).and_then(|d| d.launch.clone()))
    });
    let launch_input = resolve_launch(
        &bringup_dir,
        effective_file.as_deref(),
        args.exec.as_deref(),
    )?;
    let resolved_launch = match &launch_input {
        LaunchInput::File(p) => Some(p.to_string_lossy().into_owned()),
        LaunchInput::Synth(_) => None, // not persisted; record nothing
    };

    emit_bake_tree(
        &bake_dir,
        bringup,
        &component_kinds,
        args.target.as_deref(),
        resolved_launch.as_deref(),
    )?;

    if let Some(mode) = args.ahead_of_vendor {
        emit_ahead_of_vendor(&out_dir, bringup, mode)?;
        // Phase 212.E.3 — also drop a `vendor_hint.json` skeleton inside
        // the bake tree describing the hookless-vendor intent. Downstream
        // PIO `extra_script.py` (H.6) + the PX4 board overlay generator
        // (H.7) read this to know which vendor-specific augment to apply
        // — keeps the contract uniform across kinds even though the rich
        // per-vendor artifacts (library.json / module dirs) live under
        // `<out>/`.
        write_if_changed(
            &bake_dir.join("vendor_hint.json"),
            &render_vendor_hint(bringup, mode),
        )?;
    }

    eprintln!(
        "nros codegen system: wrote bake tree at {}",
        bake_dir.display()
    );
    Ok(())
}

/// Whether a component's host package is a Rust workspace member or
/// something else (C/C++ cmake pkg, unknown).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ComponentLang {
    Rust,
    Other,
}

/// Resolve `--bringup` (name or path) → a `BringupPackageEntry`. Falls back
/// to the workspace's `default_system` pointer when no explicit hint given.
///
/// Path-style hints accept directory basenames that don't exactly match a
/// registered bringup-pkg cargo name — common when an example dir is named
/// `action-client` but its host pkg is `nros_zephyr_action_client`. The
/// resolver tries (in order):
///
/// 1. `<basename>` as-is.
/// 2. `<basename>` with hyphens → underscores (cargo-pkg-name style).
/// 3. `nros_<platform>_<basename-snake>` when the dir lives under
///    `examples/<platform>/<lang>/<basename>` (Phase 118 collapsed shape).
/// 4. Match by directory identity: any registered bringup pkg whose
///    `manifest_path.parent()` resolves to the same absolute dir as the
///    hint. This handles arbitrary naming without convention assumptions
///    (self-bringup pkgs especially — their `manifest_path` is the pkg's
///    Cargo.toml, so the parent IS the example dir).
///
/// Phase 220.I — landed to unblock `just zephyr build-fixtures` cyclonedds
/// variants under the Phase 118 collapsed `examples/<plat>/<lang>/<ex>/`
/// shape.
fn resolve_bringup<'a>(cfg: &'a NrosConfig, hint: Option<&str>) -> Result<&'a BringupPackageEntry> {
    let name = match hint {
        Some(h) => {
            // Treat as path first: if it points at an existing dir, try a
            // set of alias candidates derived from the path before giving up.
            let as_path = PathBuf::from(h);
            if as_path.is_dir() {
                match resolve_bringup_by_path(cfg, &as_path) {
                    Some((matched, kind)) => {
                        if std::env::var_os("NROS_DEBUG_BRINGUP_RESOLVER").is_some() {
                            eprintln!(
                                "nros codegen-system: bringup path {h:?} → \
                                 matched `{matched}` via {kind}"
                            );
                        }
                        matched
                    }
                    None => {
                        let tried = bringup_alias_candidates(&as_path);
                        bail!(
                            "directory {h:?} does not match any bringup package \
                             in workspace; tried aliases {tried:?}; \
                             known bringup pkgs: {:?}",
                            cfg.bringup_packages.keys().collect::<Vec<_>>()
                        );
                    }
                }
            } else {
                h.to_string()
            }
        }
        None => cfg
            .workspace_metadata
            .default_system
            .clone()
            .ok_or_else(|| {
                eyre::eyre!(
                    "no --bringup hint and `[workspace.metadata.nros].default_system` \
                     is unset; supply `--bringup <name>`"
                )
            })?,
    };

    cfg.bringup_packages.get(&name).ok_or_else(|| {
        eyre::eyre!(
            "no bringup package `{name}` in workspace; known: {:?}",
            cfg.bringup_packages.keys().collect::<Vec<_>>()
        )
    })
}

/// Phase 220.I — derive bringup-pkg alias candidates from a directory path
/// (in lookup-priority order). Pure naming-convention helpers; the dir-
/// identity match (step 4 of `resolve_bringup`) lives in the caller because
/// it needs to compare against actual entries.
fn bringup_alias_candidates(path: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let Some(base) = path.file_name().and_then(|s| s.to_str()) else {
        return out;
    };
    // 1. Exact basename.
    out.push(base.to_string());
    // 2. Hyphens → underscores.
    let snake = base.replace('-', "_");
    if snake != base {
        out.push(snake.clone());
    }
    // 3. `nros_<plat>_<basename-snake>` when path is under
    //    `examples/<plat>/<lang>/<basename>`. We walk ancestors looking
    //    for the `examples` segment + the two segments immediately after.
    if let Some(plat) = extract_examples_platform(path) {
        let candidate = format!("nros_{plat}_{snake}");
        if !out.contains(&candidate) {
            out.push(candidate);
        }
    }
    out
}

/// Walk path ancestors; when an ancestor's last component is `examples`,
/// the NEXT child component (working back toward `path`) is the platform.
/// Returns e.g. `"zephyr"` for `examples/zephyr/rust/action-client`.
fn extract_examples_platform(path: &Path) -> Option<String> {
    let comps: Vec<&str> = path
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();
    for (i, c) in comps.iter().enumerate() {
        if *c == "examples" && i + 1 < comps.len() {
            return Some(comps[i + 1].to_string());
        }
    }
    None
}

/// Phase 220.I — resolve a directory path to a bringup-pkg name using all
/// four strategies (alias candidates + dir-identity). Returns the matched
/// pkg name + a short tag describing which strategy matched (for the
/// debug log line).
fn resolve_bringup_by_path(cfg: &NrosConfig, dir: &Path) -> Option<(String, &'static str)> {
    // Steps 1–3: alias-name lookup.
    let aliases = bringup_alias_candidates(dir);
    for alias in &aliases {
        if cfg.bringup_packages.contains_key(alias) {
            return Some((alias.clone(), "alias"));
        }
    }
    // Step 4: dir-identity match. Canonicalize when possible (handles
    // symlinks + `..` segments); fall back to a logical compare.
    let want = std::fs::canonicalize(dir).ok();
    for (name, entry) in &cfg.bringup_packages {
        let Some(pkg_dir) = entry.manifest_path.parent() else {
            continue;
        };
        if let (Some(a), Ok(b)) = (want.as_ref(), std::fs::canonicalize(pkg_dir)) {
            if a == &b {
                return Some((name.clone(), "dir-identity"));
            }
        } else if pkg_dir == dir {
            return Some((name.clone(), "dir-identity"));
        }
    }
    None
}

/// For each component, decide whether its host package is a Rust workspace
/// member (so we should include it in the emitted Cargo.toml stub).
fn classify_components(
    cfg: &NrosConfig,
    components: &[SystemComponentEntry],
) -> Vec<(String, ComponentLang)> {
    components
        .iter()
        .map(|c| {
            let kind = if cfg.component_packages.contains_key(&c.pkg) {
                ComponentLang::Rust
            } else {
                ComponentLang::Other
            };
            (c.pkg.clone(), kind)
        })
        .collect()
}

/// Emit the standard `nros-system/` bake tree.
fn emit_bake_tree(
    bake_dir: &Path,
    bringup: &BringupPackageEntry,
    component_kinds: &[(String, ComponentLang)],
    target: Option<&str>,
    resolved_launch: Option<&str>,
) -> Result<()> {
    fs::create_dir_all(bake_dir)
        .with_context(|| format!("create bake dir {}", bake_dir.display()))?;

    write_if_changed(
        &bake_dir.join("system_config.h"),
        &render_system_config_h(&bringup.system),
    )?;
    write_if_changed(
        &bake_dir.join("system_main.c"),
        &render_system_main_c(&bringup.system),
    )?;

    let rust_pkgs: BTreeSet<&str> = component_kinds
        .iter()
        .filter(|(_, k)| *k == ComponentLang::Rust)
        .map(|(p, _)| p.as_str())
        .collect();
    if !rust_pkgs.is_empty() {
        write_if_changed(
            &bake_dir.join("Cargo.toml"),
            &render_cargo_workspace_stub(&rust_pkgs),
        )?;
    } else {
        // Idempotency: a previous run with Rust components may have left a
        // stale Cargo.toml; remove it so the directory matches the current
        // input.
        let stale = bake_dir.join("Cargo.toml");
        if stale.exists() {
            let _ = fs::remove_file(stale);
        }
    }

    write_if_changed(
        &bake_dir.join("nros-plan.json"),
        &render_plan_json(bringup, component_kinds, target, resolved_launch)?,
    )?;

    Ok(())
}

/// Write `contents` to `path` only if the on-disk bytes differ (preserves
/// mtimes, satisfies the idempotency contract).
fn write_if_changed(path: &Path, contents: &str) -> Result<()> {
    if let Ok(existing) = fs::read_to_string(path) {
        if existing == contents {
            return Ok(());
        }
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("create parent {}", parent.display()))?;
    }
    let mut f = fs::File::create(path).with_context(|| format!("create {}", path.display()))?;
    f.write_all(contents.as_bytes())
        .with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Renderers
// ---------------------------------------------------------------------------

fn render_system_config_h(sys: &SystemToml) -> String {
    let mut out = String::new();
    out.push_str("/* Auto-generated by `nros codegen system` — do not edit. */\n");
    out.push_str("\n");
    out.push_str("#ifndef NROS_SYSTEM_CONFIG_H\n");
    out.push_str("#define NROS_SYSTEM_CONFIG_H\n");
    out.push_str("\n");
    out.push_str(&format!(
        "#define NROS_SYSTEM_NAME \"{}\"\n",
        c_escape(&sys.system.name)
    ));
    out.push_str(&format!(
        "#define NROS_SYSTEM_DOMAIN_ID {}u\n",
        sys.system.domain_id
    ));
    out.push_str(&format!(
        "#define NROS_SYSTEM_RMW \"{}\"\n",
        c_escape(&sys.system.rmw)
    ));
    // Token form (`NROS_SYSTEM_RMW_<UPPER>`) is the form vendor adapters key
    // off (#ifdef tests against a known set, matching the per-RMW Kconfig
    // overlays).
    out.push_str(&format!(
        "#define NROS_SYSTEM_RMW_{}\n",
        sys.system.rmw.to_ascii_uppercase().replace('-', "_")
    ));
    if let Some(loc) = &sys.system.locator {
        out.push_str(&format!(
            "#define NROS_SYSTEM_LOCATOR \"{}\"\n",
            c_escape(loc)
        ));
    }
    out.push_str(&format!(
        "#define NROS_SYSTEM_COMPONENT_COUNT {}\n",
        sys.components.len()
    ));
    out.push_str("\n");
    for (idx, c) in sys.components.iter().enumerate() {
        out.push_str(&format!(
            "#define NROS_SYSTEM_COMPONENT_{}_NAME \"{}\"\n",
            idx,
            c_escape(&c.name)
        ));
        out.push_str(&format!(
            "#define NROS_SYSTEM_COMPONENT_{}_PKG \"{}\"\n",
            idx,
            c_escape(&c.pkg)
        ));
        out.push_str(&format!(
            "#define NROS_SYSTEM_COMPONENT_{}_CLASS \"{}\"\n",
            idx,
            c_escape(&c.class)
        ));
    }
    // QoS placeholder — until the planner lowers QoS overrides into the
    // SystemToml, the bake emits a sentinel macro so adapters can detect the
    // absence rather than guess.
    out.push_str("\n");
    out.push_str("#define NROS_SYSTEM_QOS_DEFAULT 1\n");
    out.push_str("\n");
    out.push_str("#endif /* NROS_SYSTEM_CONFIG_H */\n");
    out
}

fn render_system_main_c(sys: &SystemToml) -> String {
    let mut out = String::new();
    out.push_str("/* Auto-generated by `nros codegen system` — do not edit. */\n");
    out.push_str("\n");
    out.push_str("#include \"system_config.h\"\n");
    out.push_str("\n");
    out.push_str("/* Forward declarations of per-component register hooks. */\n");
    for c in &sys.components {
        out.push_str(&format!(
            "extern int nros_component_{}_register(void);\n",
            c_ident(&c.name)
        ));
    }
    out.push_str("\n");
    out.push_str("/* Implemented by the linked nano-ros runtime. */\n");
    out.push_str("extern int  nros_system_init(void);\n");
    out.push_str("extern void nros_system_spin(void);\n");
    out.push_str("\n");
    out.push_str("int nros_system_main(void) {\n");
    out.push_str("    int rc = nros_system_init();\n");
    out.push_str("    if (rc != 0) { return rc; }\n");
    for c in &sys.components {
        out.push_str(&format!(
            "    rc = nros_component_{}_register();\n",
            c_ident(&c.name)
        ));
        out.push_str("    if (rc != 0) { return rc; }\n");
    }
    out.push_str("    nros_system_spin();\n");
    out.push_str("    return 0;\n");
    out.push_str("}\n");
    out
}

fn render_cargo_workspace_stub(rust_pkgs: &BTreeSet<&str>) -> String {
    let mut out = String::new();
    out.push_str("# Auto-generated by `nros codegen system` — do not edit.\n");
    out.push_str("[workspace]\n");
    out.push_str("resolver = \"2\"\n");
    out.push_str("members = [\n");
    for p in rust_pkgs {
        out.push_str(&format!("    \"{p}\",\n"));
    }
    out.push_str("]\n");
    out
}

#[derive(Serialize)]
struct PlanComponent<'a> {
    name: &'a str,
    pkg: &'a str,
    class: &'a str,
    lang: &'a str,
}

#[derive(Serialize)]
struct PlanDoc<'a> {
    bringup: &'a str,
    system: &'a str,
    rmw: &'a str,
    domain_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    locator: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    launch_file: Option<&'a str>,
    components: Vec<PlanComponent<'a>>,
}

fn render_plan_json(
    bringup: &BringupPackageEntry,
    component_kinds: &[(String, ComponentLang)],
    target: Option<&str>,
    resolved_launch: Option<&str>,
) -> Result<String> {
    let launch_file: Option<String> = resolved_launch
        .map(|s| s.to_string())
        .or_else(|| {
            bringup
                .system
                .deploy
                .values()
                .find_map(|d| d.launch.clone())
        })
        .or_else(|| {
            // Fall back to the conventional path.
            let candidate = bringup
                .manifest_path
                .parent()
                .map(|p| p.join("launch").join("system.launch.xml"));
            candidate.and_then(|c| c.exists().then(|| c.to_string_lossy().into_owned()))
        });

    let components: Vec<PlanComponent> = bringup
        .system
        .components
        .iter()
        .zip(component_kinds.iter())
        .map(|(c, (_, kind))| PlanComponent {
            name: &c.name,
            pkg: &c.pkg,
            class: &c.class,
            lang: match kind {
                ComponentLang::Rust => "rust",
                ComponentLang::Other => "other",
            },
        })
        .collect();

    let doc = PlanDoc {
        bringup: &bringup.name,
        system: &bringup.system.system.name,
        rmw: &bringup.system.system.rmw,
        domain_id: bringup.system.system.domain_id,
        locator: bringup.system.system.locator.as_deref(),
        target,
        launch_file: launch_file.as_deref(),
        components,
    };
    let mut s = serde_json::to_string_pretty(&doc).context("serialize plan json")?;
    s.push('\n');
    Ok(s)
}

// ---------------------------------------------------------------------------
// Ahead-of-vendor emit
// ---------------------------------------------------------------------------

fn emit_ahead_of_vendor(
    out_dir: &Path,
    bringup: &BringupPackageEntry,
    mode: AheadOfVendor,
) -> Result<()> {
    match mode {
        AheadOfVendor::Pio => emit_pio(out_dir, bringup),
        AheadOfVendor::Px4 => emit_px4(out_dir, bringup),
    }
}

/// Phase 212.E.3 — render a `vendor_hint.json` skeleton documenting the
/// ahead-of-vendor intent. The shape is intentionally minimal v1 — H.6 +
/// H.7 will extend it as the PlatformIO `extra_script.py` and PX4 board
/// overlay generators come online. Today's downstream consumers only key
/// off `kind` + `bringup`.
///
/// TODO(E.3) — H.6 will need PIO-specific keys (transport, framework,
/// monitor speed); H.7 will need the per-component module name list +
/// the PX4 board-overlay path. Both are flat additions to this JSON;
/// the existing keys stay stable.
fn render_vendor_hint(bringup: &BringupPackageEntry, mode: AheadOfVendor) -> String {
    let kind = match mode {
        AheadOfVendor::Pio => "platformio",
        AheadOfVendor::Px4 => "px4",
    };
    let mut components: Vec<String> = bringup
        .system
        .components
        .iter()
        .map(|c| c.name.clone())
        .collect();
    components.sort();
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!("  \"kind\": \"{}\",\n", json_escape(kind)));
    out.push_str(&format!(
        "  \"bringup\": \"{}\",\n",
        json_escape(&bringup.name)
    ));
    out.push_str(&format!(
        "  \"system\": \"{}\",\n",
        json_escape(&bringup.system.system.name)
    ));
    out.push_str(&format!(
        "  \"rmw\": \"{}\",\n",
        json_escape(&bringup.system.system.rmw)
    ));
    out.push_str("  \"components\": [");
    for (i, c) in components.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("\"{}\"", json_escape(c)));
    }
    out.push_str("],\n");
    let todo_msg = match mode {
        AheadOfVendor::Pio => {
            "TODO(E.3): augment PlatformIO library.json with transport + framework"
        }
        AheadOfVendor::Px4 => {
            "TODO(E.3): emit PX4 board overlay flipping CONFIG_MODULES_NROS_<NAME>=y"
        }
    };
    out.push_str(&format!("  \"todo\": \"{}\"\n", json_escape(todo_msg)));
    out.push_str("}\n");
    out
}

fn emit_pio(out_dir: &Path, bringup: &BringupPackageEntry) -> Result<()> {
    fs::create_dir_all(out_dir).with_context(|| format!("create {}", out_dir.display()))?;
    // Minimal `library.json` snippet pointing at the staticlib build tree.
    // Full PIO integration (extra_script.py, transport selection) is
    // deferred to Phase 212.H.6; this emits the manifest skeleton only.
    let body = format!(
        "{{\n  \"name\": \"{}\",\n  \"version\": \"0.0.0\",\n  \
         \"description\": \"nano-ros bake for {} (auto-generated)\",\n  \
         \"build\": {{\n    \"srcDir\": \"nros-system\"\n  }}\n}}\n",
        json_escape(&bringup.name),
        json_escape(&bringup.system.system.name),
    );
    write_if_changed(&out_dir.join("library.json"), &body)?;
    Ok(())
}

fn emit_px4(out_dir: &Path, bringup: &BringupPackageEntry) -> Result<()> {
    // PX4 expects one module dir per `px4_add_module` call (see Phase 212.H.7
    // + `third-party/px4/PX4-Autopilot/src/modules/time_persistor/` for the
    // reference shape). For each component we emit:
    //
    //   <out>/nros_<name>/CMakeLists.txt   -- px4_add_module(...) invocation
    //   <out>/nros_<name>/Kconfig          -- menuconfig MODULES_NROS_<NAME>
    //   <out>/nros_<name>/nros_<name>.cpp  -- stub entry point
    //   <out>/nros_<name>/nros_<name>.h    -- stub header
    //
    // Modules emit disabled-by-default (Kconfig `default n`). Operators
    // opt-in via a board overlay (`CONFIG_MODULES_NROS_<NAME>=y` in the
    // `.px4board` file) — same gate as every other PX4 module.
    fs::create_dir_all(out_dir).with_context(|| format!("create {}", out_dir.display()))?;

    for c in &bringup.system.components {
        let name = c_ident(&c.name);
        let mod_name = format!("nros_{name}");
        let mod_dir = out_dir.join(&mod_name);
        fs::create_dir_all(&mod_dir).with_context(|| format!("create {}", mod_dir.display()))?;

        write_if_changed(
            &mod_dir.join("CMakeLists.txt"),
            &render_px4_cmakelists(&mod_name, &name, &c.class, &c.pkg),
        )?;
        write_if_changed(
            &mod_dir.join("Kconfig"),
            &render_px4_kconfig(&mod_name, &name, &bringup.name),
        )?;
        write_if_changed(
            &mod_dir.join(format!("{mod_name}.cpp")),
            &render_px4_module_cpp(&mod_name, &name, &c.class, &c.pkg),
        )?;
        write_if_changed(
            &mod_dir.join(format!("{mod_name}.h")),
            &render_px4_module_h(&mod_name, &name),
        )?;
    }

    // Mirror the other emit paths: drop a flat plan json next to the module
    // dirs so downstream tooling (PX4 board overlay generators, the H.7
    // gate) can read the resolved plan w/o re-parsing system.toml.
    let kinds = classify_components(
        // re-classify w/o a full NrosConfig — we only need the rust-ness for
        // the plan, and the px4 emit only fires after emit_bake_tree() which
        // already ran the real classify. For the side-car plan json we mark
        // everything as `other` since PX4 components are C++-only.
        &NrosConfig::default(),
        &bringup.system.components,
    );
    write_if_changed(
        &out_dir.join("nros-plan.json"),
        &render_plan_json(bringup, &kinds, Some("px4"), None)?,
    )?;

    Ok(())
}

fn render_px4_cmakelists(mod_name: &str, name: &str, class: &str, pkg: &str) -> String {
    // Mirrors `src/modules/time_persistor/CMakeLists.txt`. `MODULE` must
    // match PX4's `modules__<dir>` convention; `MAIN` is the entry symbol
    // PX4 wires up via `px4_add_module`. The DEPENDS px4_work_queue is the
    // minimum any module needs to coexist on the nuttx/sitl work queue.
    format!(
        "############################################################################\n\
         # Auto-generated by `nros codegen-system --ahead-of-vendor px4`.\n\
         #\n\
         # Component: {name}\n\
         # Class:     {class}\n\
         # Source:    {pkg}\n\
         ############################################################################\n\
         \n\
         px4_add_module(\n\
         \tMODULE modules__{mod_name}\n\
         \tMAIN {mod_name}\n\
         \tCOMPILE_FLAGS\n\
         \tSRCS\n\
         \t\t{mod_name}.cpp\n\
         \t\t{mod_name}.h\n\
         \tDEPENDS\n\
         \t\tpx4_work_queue\n\
         \t)\n"
    )
}

fn render_px4_kconfig(mod_name: &str, name: &str, bringup: &str) -> String {
    // PX4 module Kconfigs follow `menuconfig MODULES_<UPPER_NAME>` (see
    // `src/modules/time_persistor/Kconfig`). `default n` keeps the module
    // off in stock SITL configs until an operator opts in via a board
    // overlay (`CONFIG_MODULES_NROS_<NAME>=y`).
    let upper = mod_name.to_ascii_uppercase();
    format!(
        "menuconfig MODULES_{upper}\n\
         \tbool \"{name} (nano-ros component)\"\n\
         \tdefault n\n\
         \t---help---\n\
         \t\tnano-ros component `{name}`, generated from bringup `{bringup}`.\n\
         \t\tEnable to link this nano-ros component into the PX4 firmware.\n"
    )
}

fn render_px4_module_cpp(mod_name: &str, name: &str, class: &str, pkg: &str) -> String {
    // Minimal PX4 entry point. `<mod_name>_main(argc, argv)` matches what
    // `px4_add_module(MAIN ...)` expects; PX4_INFO is the px4-native log
    // sink. Wiring this to the nano-ros runtime is a follow-up — the H.7
    // acceptance only requires that PX4 can discover + parse the module.
    format!(
        "/*\n\
         * Auto-generated by `nros codegen-system --ahead-of-vendor px4`.\n\
         *\n\
         * Component: {name}\n\
         * Class:     {class}\n\
         * Source:    {pkg}\n\
         */\n\
         \n\
         #include \"{mod_name}.h\"\n\
         #include <px4_platform_common/log.h>\n\
         #include <px4_platform_common/module.h>\n\
         \n\
         extern \"C\" __EXPORT int {mod_name}_main(int argc, char *argv[]);\n\
         \n\
         int {mod_name}_main(int argc, char *argv[])\n\
         {{\n\
         \t(void)argc;\n\
         \t(void)argv;\n\
         \tPX4_INFO(\"nros component {name} started\");\n\
         \treturn 0;\n\
         }}\n"
    )
}

fn render_px4_module_h(mod_name: &str, name: &str) -> String {
    let guard = mod_name.to_ascii_uppercase();
    format!(
        "/*\n\
         * Auto-generated by `nros codegen-system --ahead-of-vendor px4`.\n\
         *\n\
         * Component: {name}\n\
         */\n\
         #ifndef {guard}_H\n\
         #define {guard}_H\n\
         \n\
         #ifdef __cplusplus\n\
         extern \"C\" {{\n\
         #endif\n\
         \n\
         int {mod_name}_main(int argc, char *argv[]);\n\
         \n\
         #ifdef __cplusplus\n\
         }}\n\
         #endif\n\
         \n\
         #endif /* {guard}_H */\n"
    )
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

/// Escape a string for use inside a C double-quoted string literal.
fn c_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

/// Lower a component name to a valid C identifier (replace non-alnum with `_`).
fn c_ident(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    // Avoid leading digit.
    if out.chars().next().map_or(false, |c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch_dir(test: &str) -> PathBuf {
        let base = std::env::var_os("CARGO_TARGET_TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("nros-cli-core-tests"));
        let dir = base.join(format!("codegen_system_{test}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    /// Write a "zephyr_native_sim" style fixture: 2 Rust components + bringup.
    fn write_rust_two_component_workspace(dir: &Path) {
        fs::write(
            dir.join("Cargo.toml"),
            r#"
[workspace]
resolver = "2"
members = ["talker_pkg", "listener_pkg", "demo_bringup"]

[workspace.metadata.nros]
default_system = "demo_bringup"
"#,
        )
        .unwrap();

        for pkg in ["talker_pkg", "listener_pkg"] {
            fs::create_dir_all(dir.join(pkg).join("src")).unwrap();
            fs::write(
                dir.join(pkg).join("Cargo.toml"),
                format!(
                    r#"
[package]
name = "{pkg}"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[package.metadata.nros.component]
default_namespace = "/demo"
"#
                ),
            )
            .unwrap();
            fs::write(dir.join(pkg).join("src/lib.rs"), "").unwrap();
        }

        fs::create_dir_all(dir.join("demo_bringup/launch")).unwrap();
        fs::create_dir_all(dir.join("demo_bringup/src")).unwrap();
        fs::write(
            dir.join("demo_bringup/Cargo.toml"),
            r#"
[package]
name = "demo_bringup"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
"#,
        )
        .unwrap();
        fs::write(dir.join("demo_bringup/src/lib.rs"), "").unwrap();
        fs::write(
            dir.join("demo_bringup/system.toml"),
            r#"
[system]
name = "demo"
rmw = "zenoh"
domain_id = 7
locator = "tcp/127.0.0.1:7447"

[[component]]
pkg = "talker_pkg"
class = "talker_pkg::TalkerNode"
name = "talker"

[[component]]
pkg = "listener_pkg"
class = "listener_pkg::ListenerNode"
name = "listener"

[deploy.zephyr_native_sim]
kind = "qemu"
target = "x86_64-unknown-linux-gnu"
board = "native_sim"
launch = "launch/system.launch.xml"
"#,
        )
        .unwrap();
        fs::write(
            dir.join("demo_bringup/launch/system.launch.xml"),
            "<launch></launch>\n",
        )
        .unwrap();
    }

    /// Workspace whose components live in non-Rust (C/C++) packages — i.e.
    /// the bringup names `pkg = "..."` entries that aren't registered in the
    /// cargo workspace's `component_packages`.
    fn write_pure_cpp_workspace(dir: &Path) {
        fs::write(
            dir.join("Cargo.toml"),
            r#"
[workspace]
resolver = "2"
members = ["demo_bringup"]

[workspace.metadata.nros]
default_system = "demo_bringup"
"#,
        )
        .unwrap();
        fs::create_dir_all(dir.join("demo_bringup/src")).unwrap();
        fs::write(
            dir.join("demo_bringup/Cargo.toml"),
            r#"
[package]
name = "demo_bringup"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"
"#,
        )
        .unwrap();
        fs::write(dir.join("demo_bringup/src/lib.rs"), "").unwrap();
        fs::write(
            dir.join("demo_bringup/system.toml"),
            r#"
[system]
name = "demo"
rmw = "cyclonedds"
domain_id = 0

[[component]]
pkg = "cpp_talker_pkg"
class = "cpp_talker_pkg::Talker"
name = "talker"
"#,
        )
        .unwrap();
    }

    /// 212.E.T1 — fixture bringup w/ 2 Rust components produces the expected
    /// baked tree under `<out>/nros-system/`.
    #[test]
    fn codegen_system_emits_baked_headers_for_zephyr_native_sim() {
        let dir = scratch_dir("emits_baked_headers_for_zephyr_native_sim");
        write_rust_two_component_workspace(&dir);

        let out = dir.join("build/demo_bringup");
        run(Args {
            workspace: Some(dir.clone()),
            bringup: None,
            target: Some("x86_64-unknown-linux-gnu".into()),
            out: Some(out.clone()),
            ahead_of_vendor: None,
            file: None,
            exec: None,
        })
        .expect("codegen runs");

        let bake = out.join("nros-system");
        let header = fs::read_to_string(bake.join("system_config.h")).unwrap();
        assert!(
            header.contains("#define NROS_SYSTEM_DOMAIN_ID 7u"),
            "header: {header}"
        );
        assert!(
            header.contains("#define NROS_SYSTEM_RMW \"zenoh\""),
            "header: {header}"
        );
        assert!(
            header.contains("#define NROS_SYSTEM_RMW_ZENOH"),
            "header: {header}"
        );
        assert!(header.contains("#define NROS_SYSTEM_LOCATOR \"tcp/127.0.0.1:7447\""));
        assert!(header.contains("#define NROS_SYSTEM_COMPONENT_COUNT 2"));
        assert!(header.contains("#define NROS_SYSTEM_COMPONENT_0_NAME \"talker\""));
        assert!(header.contains("#define NROS_SYSTEM_COMPONENT_1_NAME \"listener\""));

        let main_c = fs::read_to_string(bake.join("system_main.c")).unwrap();
        assert!(main_c.contains("extern int nros_component_talker_register(void);"));
        assert!(main_c.contains("extern int nros_component_listener_register(void);"));
        assert!(main_c.contains("nros_component_talker_register();"));
        assert!(main_c.contains("nros_component_listener_register();"));
        assert!(main_c.contains("nros_system_spin();"));

        let cargo_stub = fs::read_to_string(bake.join("Cargo.toml")).unwrap();
        assert!(cargo_stub.contains("\"talker_pkg\""));
        assert!(cargo_stub.contains("\"listener_pkg\""));

        let plan = fs::read_to_string(bake.join("nros-plan.json")).unwrap();
        assert!(plan.contains("\"bringup\": \"demo_bringup\""));
        assert!(plan.contains("\"system\": \"demo\""));
        assert!(plan.contains("\"target\": \"x86_64-unknown-linux-gnu\""));
        assert!(plan.contains("\"lang\": \"rust\""));
        // Launch file path recorded from the deploy block.
        assert!(plan.contains("launch/system.launch.xml"));
    }

    /// 212.E.T2 — re-running with identical inputs produces byte-identical
    /// outputs across all emitted files.
    #[test]
    fn codegen_system_idempotent_on_unchanged_input() {
        let dir = scratch_dir("idempotent_on_unchanged_input");
        write_rust_two_component_workspace(&dir);

        let out = dir.join("build/demo_bringup");
        let args = || Args {
            workspace: Some(dir.clone()),
            bringup: None,
            target: Some("x86_64-unknown-linux-gnu".into()),
            out: Some(out.clone()),
            ahead_of_vendor: None,
            file: None,
            exec: None,
        };
        run(args()).expect("first run");

        let bake = out.join("nros-system");
        let snap: Vec<(String, Vec<u8>)> = [
            "system_config.h",
            "system_main.c",
            "Cargo.toml",
            "nros-plan.json",
        ]
        .iter()
        .map(|f| (f.to_string(), fs::read(bake.join(f)).expect("read")))
        .collect();

        run(args()).expect("second run");

        for (name, before) in snap {
            let after = fs::read(bake.join(&name)).expect("read");
            assert_eq!(before, after, "file `{name}` changed across runs");
        }
    }

    /// 212.E.T3 — bringup whose components live entirely outside the cargo
    /// workspace (i.e. C/C++ pkgs) → no Cargo.toml stub emitted.
    #[test]
    fn codegen_system_emits_only_for_rust_components_when_no_rust() {
        let dir = scratch_dir("emits_only_for_rust_when_no_rust");
        write_pure_cpp_workspace(&dir);

        let out = dir.join("build/demo_bringup");
        run(Args {
            workspace: Some(dir.clone()),
            bringup: None,
            target: None,
            out: Some(out.clone()),
            ahead_of_vendor: None,
            file: None,
            exec: None,
        })
        .expect("codegen runs");

        let bake = out.join("nros-system");
        assert!(bake.join("system_config.h").exists());
        assert!(bake.join("system_main.c").exists());
        assert!(
            !bake.join("Cargo.toml").exists(),
            "no Rust components → no Cargo stub"
        );
        assert!(bake.join("nros-plan.json").exists());

        let plan = fs::read_to_string(bake.join("nros-plan.json")).unwrap();
        assert!(
            plan.contains("\"lang\": \"other\""),
            "non-Rust comp tagged: {plan}"
        );
    }

    /// 212.H.7 — `--ahead-of-vendor px4` emits PX4-native `nros_<name>/`
    /// module dirs (CMakeLists.txt + Kconfig + cpp + h) per component, plus
    /// a flat `nros-plan.json` next to them.
    #[test]
    fn codegen_system_ahead_of_vendor_emits_px4_module_dirs() {
        let dir = scratch_dir("ahead_of_vendor_px4_module_dirs");
        write_rust_two_component_workspace(&dir);

        let out = dir.join("build/demo_bringup");
        run(Args {
            workspace: Some(dir.clone()),
            bringup: None,
            target: Some("px4".into()),
            out: Some(out.clone()),
            ahead_of_vendor: Some(AheadOfVendor::Px4),
            file: None,
            exec: None,
        })
        .expect("codegen runs");

        for name in ["talker", "listener"] {
            let mod_dir = out.join(format!("nros_{name}"));
            assert!(mod_dir.is_dir(), "missing {}", mod_dir.display());

            let cmake = fs::read_to_string(mod_dir.join("CMakeLists.txt")).unwrap();
            assert!(
                cmake.contains("px4_add_module("),
                "no px4_add_module: {cmake}"
            );
            assert!(
                cmake.contains(&format!("MODULE modules__nros_{name}")),
                "missing MODULE marker: {cmake}"
            );
            assert!(
                cmake.contains(&format!("MAIN nros_{name}")),
                "missing MAIN marker: {cmake}"
            );
            assert!(
                cmake.contains(name),
                "missing component name reference: {cmake}"
            );

            let kconfig = fs::read_to_string(mod_dir.join("Kconfig")).unwrap();
            assert!(
                kconfig.contains(&format!(
                    "menuconfig MODULES_NROS_{}",
                    name.to_ascii_uppercase()
                )),
                "missing menuconfig: {kconfig}"
            );
            assert!(
                kconfig.contains("default n"),
                "expected default-off: {kconfig}"
            );

            let cpp = fs::read_to_string(mod_dir.join(format!("nros_{name}.cpp"))).unwrap();
            assert!(
                cpp.contains(&format!("int nros_{name}_main(int argc, char *argv[])")),
                "missing main entry: {cpp}"
            );
            assert!(cpp.contains("PX4_INFO("), "missing PX4_INFO: {cpp}");
            assert!(
                cpp.contains("px4_platform_common/module.h"),
                "missing module.h include: {cpp}"
            );

            let h = fs::read_to_string(mod_dir.join(format!("nros_{name}.h"))).unwrap();
            assert!(
                h.contains(&format!("int nros_{name}_main(int argc, char *argv[]);")),
                "missing main decl: {h}"
            );
        }

        let plan = fs::read_to_string(out.join("nros-plan.json")).unwrap();
        assert!(plan.contains("\"target\": \"px4\""), "plan: {plan}");
        assert!(plan.contains("\"name\": \"talker\""), "plan: {plan}");
        assert!(plan.contains("\"name\": \"listener\""), "plan: {plan}");

        // Standard bake still produced.
        assert!(out.join("nros-system/system_config.h").exists());
    }

    /// Phase 212.L.7 — a self-bringup component pkg (Cargo.toml +
    /// `[package.metadata.nros.component]` + `[package.metadata.nros.deploy.*]`,
    /// no sibling bringup pkg) becomes its own degenerate 1-component
    /// bringup. `nros codegen-system` (run via `run`) bakes
    /// `system_main.c` + `system_config.h` with the component + the
    /// deploy block's domain_id / rmw / locator.
    #[test]
    fn codegen_system_bakes_self_bringup_component_pkg() {
        let dir = scratch_dir("bakes_self_bringup_component_pkg");
        // Workspace w/ ONE self-bringup component pkg.
        fs::write(
            dir.join("Cargo.toml"),
            r#"
[workspace]
resolver = "2"
members = ["alpha_pkg"]
"#,
        )
        .unwrap();
        fs::create_dir_all(dir.join("alpha_pkg/src")).unwrap();
        fs::write(
            dir.join("alpha_pkg/Cargo.toml"),
            r#"
[package]
name = "alpha_pkg"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[package.metadata.nros.component]
class = "alpha_pkg::Node"
name = "alpha"

[package.metadata.nros.deploy.native]
board = "native_sim/native/64"
rmw = "zenoh"
domain_id = 7
locator = "tcp/127.0.0.1:7447"
"#,
        )
        .unwrap();
        fs::write(dir.join("alpha_pkg/src/lib.rs"), "").unwrap();

        let out = dir.join("build/alpha_pkg");
        run(Args {
            workspace: Some(dir.clone()),
            bringup: Some("alpha_pkg".into()),
            target: Some("native".into()),
            out: Some(out.clone()),
            ahead_of_vendor: None,
            file: None,
            exec: None,
        })
        .expect("codegen runs for self-bringup");

        let bake = out.join("nros-system");
        let header = fs::read_to_string(bake.join("system_config.h")).unwrap();
        assert!(
            header.contains("#define NROS_SYSTEM_DOMAIN_ID 7u"),
            "header: {header}"
        );
        assert!(header.contains("#define NROS_SYSTEM_RMW \"zenoh\""));
        assert!(header.contains("#define NROS_SYSTEM_LOCATOR \"tcp/127.0.0.1:7447\""));
        assert!(header.contains("#define NROS_SYSTEM_COMPONENT_COUNT 1"));
        assert!(header.contains("#define NROS_SYSTEM_COMPONENT_0_NAME \"alpha\""));

        let main_c = fs::read_to_string(bake.join("system_main.c")).unwrap();
        assert!(main_c.contains("nros_component_alpha_register"));

        // Self-bringup pkg is a Rust pkg → Cargo stub emitted listing the
        // host pkg.
        let stub = fs::read_to_string(bake.join("Cargo.toml")).unwrap();
        assert!(stub.contains("\"alpha_pkg\""), "stub: {stub}");

        // Plan json reflects the self-bringup pkg.
        let plan = fs::read_to_string(bake.join("nros-plan.json")).unwrap();
        assert!(plan.contains("\"bringup\": \"alpha_pkg\""), "plan: {plan}");
        assert!(plan.contains("\"system\": \"alpha_pkg\""), "plan: {plan}");
    }

    /// Phase 212.L.7 — `[workspace.metadata.nros].default_system` may point
    /// at a self-bringup component pkg; `nros codegen-system` (no
    /// `--bringup` hint) resolves through the workspace pointer.
    /// Verifies system codegen resolves a workspace default system to the self bringup package.
    #[test]
    fn codegen_system_resolve_self_bringup_default() {
        let dir = scratch_dir("workspace_default_self_bringup");
        fs::write(
            dir.join("Cargo.toml"),
            r#"
[workspace]
resolver = "2"
members = ["alpha_pkg"]

[workspace.metadata.nros]
default_system = "alpha_pkg"
"#,
        )
        .unwrap();
        fs::create_dir_all(dir.join("alpha_pkg/src")).unwrap();
        fs::write(
            dir.join("alpha_pkg/Cargo.toml"),
            r#"
[package]
name = "alpha_pkg"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[package.metadata.nros.component]
class = "alpha_pkg::Node"
name = "alpha"

[package.metadata.nros.deploy.native]
rmw = "cyclonedds"
domain_id = 3
"#,
        )
        .unwrap();
        fs::write(dir.join("alpha_pkg/src/lib.rs"), "").unwrap();

        let out = dir.join("build/alpha_pkg");
        run(Args {
            workspace: Some(dir.clone()),
            bringup: None, // resolves via [workspace.metadata.nros].default_system
            target: None,
            out: Some(out.clone()),
            ahead_of_vendor: None,
            file: None,
            exec: None,
        })
        .expect("codegen runs via workspace pointer");

        let bake = out.join("nros-system");
        let header = fs::read_to_string(bake.join("system_config.h")).unwrap();
        assert!(header.contains("#define NROS_SYSTEM_DOMAIN_ID 3u"));
        assert!(header.contains("#define NROS_SYSTEM_RMW \"cyclonedds\""));
    }

    /// 212.E.T4 — `--ahead-of-vendor pio` mode emits `library.json` alongside
    /// the standard bake tree.
    #[test]
    fn codegen_system_ahead_of_vendor_emits_pio_library_json() {
        let dir = scratch_dir("ahead_of_vendor_pio_library_json");
        write_rust_two_component_workspace(&dir);

        let out = dir.join("build/demo_bringup");
        run(Args {
            workspace: Some(dir.clone()),
            bringup: None,
            target: None,
            out: Some(out.clone()),
            ahead_of_vendor: Some(AheadOfVendor::Pio),
            file: None,
            exec: None,
        })
        .expect("codegen runs");

        let lib = out.join("library.json");
        assert!(lib.exists(), "library.json at {}", lib.display());
        let body = fs::read_to_string(&lib).unwrap();
        assert!(body.contains("\"name\": \"demo_bringup\""), "body: {body}");
        assert!(body.contains("\"srcDir\": \"nros-system\""), "body: {body}");
        // Standard bake still produced.
        assert!(out.join("nros-system/system_config.h").exists());
    }

    // ---- Phase 220.I — bringup-pkg dir → name alias resolver ---------------

    #[test]
    fn bringup_alias_candidates_hyphen_to_underscore() {
        let p = Path::new("/tmp/examples/zephyr/rust/action-client");
        let aliases = bringup_alias_candidates(p);
        assert!(aliases.contains(&"action-client".to_string()));
        assert!(aliases.contains(&"action_client".to_string()));
        assert!(aliases.contains(&"nros_zephyr_action_client".to_string()));
    }

    #[test]
    fn bringup_alias_candidates_no_examples_segment() {
        let p = Path::new("/tmp/scratch/action-client");
        let aliases = bringup_alias_candidates(p);
        assert!(aliases.contains(&"action-client".to_string()));
        assert!(aliases.contains(&"action_client".to_string()));
        // No `examples/<plat>/…` segment → no platform prefix candidate.
        assert!(!aliases.iter().any(|a| a.starts_with("nros_")));
    }

    /// Phase 220.I — invoking `nros codegen-system` with a path whose
    /// basename (`action-client`) does NOT match the bringup-pkg cargo
    /// name (`nros_zephyr_action_client`) still resolves, via either the
    /// `nros_<plat>_<snake>` alias OR dir-identity fallback. Reproduces the
    /// `just zephyr build-fixtures` cyclonedds variant failure.
    #[test]
    fn codegen_system_resolves_zephyr_alias() {
        let dir = scratch_dir("alias_collapsed_zephyr_shape");
        // Lay out a fake `examples/zephyr/rust/action-client/` self-bringup
        // pkg under a workspace root.
        let plat_dir = dir.join("examples/zephyr/rust");
        let ex_dir = plat_dir.join("action-client");
        fs::create_dir_all(ex_dir.join("src")).unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            r#"
[workspace]
resolver = "2"
members = ["examples/zephyr/rust/action-client"]
"#,
        )
        .unwrap();
        fs::write(
            ex_dir.join("Cargo.toml"),
            r#"
[package]
name = "nros_zephyr_action_client"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[package.metadata.nros.component]
class = "nros_zephyr_action_client::Node"
name = "action_client"

[package.metadata.nros.deploy.zephyr]
board = "native_sim/native/64"
rmw = "cyclonedds"
domain_id = 5
"#,
        )
        .unwrap();
        fs::write(ex_dir.join("src/lib.rs"), "").unwrap();

        let out = dir.join("build/action-client");
        run(Args {
            workspace: Some(dir.clone()),
            // Reproduce the shim's call: --bringup <abs-path-to-dir>.
            bringup: Some(ex_dir.to_string_lossy().into_owned()),
            target: Some("zephyr-cyclonedds".into()),
            out: Some(out.clone()),
            ahead_of_vendor: None,
            file: None,
            exec: None,
        })
        .expect("codegen-system resolves dir basename → cargo pkg name via alias");

        let header = fs::read_to_string(out.join("nros-system/system_config.h")).unwrap();
        assert!(
            header.contains("#define NROS_SYSTEM_DOMAIN_ID 5u"),
            "{header}"
        );
        assert!(
            header.contains("#define NROS_SYSTEM_RMW \"cyclonedds\""),
            "{header}"
        );
    }

    /// Phase 220.I — dir-identity fallback: when an example uses an
    /// arbitrary cargo name (e.g. native examples `native-rs-talker`) that
    /// matches none of the alias candidates, the resolver still picks the
    /// right bringup pkg by comparing `manifest_path.parent()` to the
    /// requested dir.
    #[test]
    fn codegen_system_resolves_dir_identity_for_arbitrary_pkg_name() {
        let dir = scratch_dir("alias_dir_identity_arbitrary_name");
        let ex_dir = dir.join("examples/native/rust/talker");
        fs::create_dir_all(ex_dir.join("src")).unwrap();
        fs::write(
            dir.join("Cargo.toml"),
            r#"
[workspace]
resolver = "2"
members = ["examples/native/rust/talker"]
"#,
        )
        .unwrap();
        fs::write(
            ex_dir.join("Cargo.toml"),
            r#"
[package]
name = "native-rs-talker"
version = "0.1.0"
edition = "2021"

[lib]
path = "src/lib.rs"

[package.metadata.nros.component]
class = "native_rs_talker::Node"
name = "talker"

[package.metadata.nros.deploy.native]
rmw = "zenoh"
domain_id = 11
"#,
        )
        .unwrap();
        fs::write(ex_dir.join("src/lib.rs"), "").unwrap();

        let out = dir.join("build/talker");
        run(Args {
            workspace: Some(dir.clone()),
            bringup: Some(ex_dir.to_string_lossy().into_owned()),
            target: Some("native".into()),
            out: Some(out.clone()),
            ahead_of_vendor: None,
            file: None,
            exec: None,
        })
        .expect("dir-identity fallback resolves arbitrary cargo name");
        let header = fs::read_to_string(out.join("nros-system/system_config.h")).unwrap();
        assert!(
            header.contains("#define NROS_SYSTEM_DOMAIN_ID 11u"),
            "{header}"
        );
    }

    /// Phase 220.I — when the path doesn't match any alias AND no bringup
    /// pkg lives in that dir, the error message lists the alias candidates
    /// tried (not just the canonical name) so users know how to rename.
    #[test]
    fn codegen_system_path_mismatch_error_lists_aliases_tried() {
        let dir = scratch_dir("alias_error_lists_candidates");
        // Workspace with one bringup pkg that DOES NOT match the dir we'll
        // probe — so the resolver exhausts all aliases + dir-identity.
        write_rust_two_component_workspace(&dir);
        let bogus = dir.join("examples/zephyr/rust/something-else");
        fs::create_dir_all(&bogus).unwrap();

        let err = run(Args {
            workspace: Some(dir.clone()),
            bringup: Some(bogus.to_string_lossy().into_owned()),
            target: None,
            out: Some(dir.join("build/out")),
            ahead_of_vendor: None,
            file: None,
            exec: None,
        })
        .expect_err("expected resolver to fail");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("tried aliases"),
            "error should list aliases tried, got: {msg}"
        );
        assert!(msg.contains("something-else"), "msg: {msg}");
        assert!(msg.contains("something_else"), "msg: {msg}");
        assert!(
            msg.contains("nros_zephyr_something_else"),
            "msg should include platform-prefixed alias: {msg}"
        );
    }
}
