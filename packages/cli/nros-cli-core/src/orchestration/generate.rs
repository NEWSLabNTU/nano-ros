//! Generated orchestration package writer.
//!
//! This module deliberately treats `nros-plan.json` as an opaque input path.
//! Agent A owns the final plan schema; generated package `build.rs` is the
//! host-side adapter that will be tightened once that schema lands.

use eyre::{Context, Result, bail};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use super::{
    NrosPlan,
    board_descriptor::{
        BoardCatalog, BoardDescriptor, EntryKind, LinkKind, NetStack, PlatformKind, Toolchain,
    },
    plan::{
        LifecycleAutostart, PlanBuildOptions, PlanCargoOverrides, PlanEntity, PlanInstance,
        PlanSchedContext, TransportKind,
    },
    schema::{DeadlinePolicy, ParameterValue, SchedClass},
};

const CARGO_TEMPLATE: &str = include_str!("../../templates/orchestration/Cargo.toml.jinja");
const BUILD_TEMPLATE: &str = include_str!("../../templates/orchestration/build.rs.jinja");
const ZEPHYR_CMAKE_TEMPLATE: &str =
    include_str!("../../templates/orchestration/zephyr/CMakeLists.txt.jinja");
const ZEPHYR_PRJ_CONF_TEMPLATE: &str =
    include_str!("../../templates/orchestration/zephyr/prj.conf.jinja");

#[derive(Debug, Clone)]
pub struct GenerateOptions {
    pub package_name: String,
    pub output_dir: PathBuf,
    pub plan_path: PathBuf,
    pub nros_path: PathBuf,
    pub nros_orchestration_path: PathBuf,
    pub component_workspace: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct GeneratedPackage {
    pub root: PathBuf,
    pub manifest_path: PathBuf,
    pub plan_path: PathBuf,
}

pub fn generate_package(options: &GenerateOptions) -> Result<GeneratedPackage> {
    let src_dir = options.output_dir.join("src");
    fs::create_dir_all(&src_dir).wrap_err_with(|| {
        format!(
            "failed to create generated package src dir {}",
            src_dir.display()
        )
    })?;

    let mut plan = load_plan(&options.plan_path)?;
    // Phase 195.C — record the workspace root so `profile()` can load board
    // descriptors from `<workspace>/packages/boards/*/nros-board.toml`. Not
    // part of the plan wire format (a `#[serde(skip)]` field).
    plan.build.workspace_root = workspace_from_nros_path(&options.nros_path);
    // Phase 172 — fail fast with a clear message if a `[[bridge]]` forwards an
    // undeclared topic or names an unopened session, before emitting code.
    validate_bridges(&plan)?;
    let cargo_toml = render_cargo_toml(options, &plan);
    let build_rs = render_build_rs(options, &plan);
    let cargo_config = render_cargo_config(options, &plan);
    let rust_toolchain = render_rust_toolchain(&plan);

    write_if_changed(&options.output_dir.join("Cargo.toml"), &cargo_toml)?;
    write_if_changed(&options.output_dir.join("build.rs"), &build_rs)?;
    // Zephyr ships a Rust staticlib (`name = "rustapp"`,
    // `crate-type = ["staticlib"]`) consumed by zephyr-lang-rust's
    // `rust_cargo_application()` CMake function — the cargo source
    // file is `src/lib.rs`, not `src/main.rs`. Every other platform
    // uses a binary crate with `src/main.rs`.
    if matches!(
        profile(&plan.build).map(|p| p.entry_kind),
        Some(EntryKind::ZephyrStaticlib)
    ) {
        // Phase 172 entry-lib fold: Zephyr now uses the same entry-lib base
        // (build_executor + register_all) as every other platform, with a
        // `rust_main` extern "C" appended for zephyr-lang-rust to jump into.
        write_if_changed(&src_dir.join("lib.rs"), &render_zephyr_entry_lib_rs(&plan))?;
        let cmake = render_zephyr_cmake(options);
        let prj_conf = render_zephyr_prj_conf(&plan);
        write_if_changed(&options.output_dir.join("CMakeLists.txt"), &cmake)?;
        write_if_changed(&options.output_dir.join("prj.conf"), &prj_conf)?;
    } else if emits_entry_lib(&plan) {
        // Phase 172 entry lib: the wiring + Rust API live in `src/lib.rs`;
        // `src/main.rs` is a thin `self` shim (hosted `fn main`, or a no_std
        // board shim driven by the board rlib's `run()`).
        write_if_changed(&src_dir.join("lib.rs"), &render_entry_lib_rs(&plan))?;
        // BoardRun-with-board_entry → no_std board shim driven by the rlib's
        // `run()`. Everything else (HostedMain, BoardRun without a
        // board_entry like NuttX/orin-spe) → cfg-gated hosted shim.
        let prof = profile(&plan.build);
        let board_shim = matches!(
            prof.as_ref().map(|p| p.entry_kind),
            Some(EntryKind::BoardRun)
        ) && prof.as_ref().and_then(|p| p.entry.as_ref()).is_some();
        let shim = if board_shim {
            render_board_shim_main(options, &plan)
        } else {
            render_hosted_shim_main(options, &plan)
        };
        write_if_changed(&src_dir.join("main.rs"), &shim)?;
        // The C ABI + its cbindgen header + the vendor-includable CMake
        // fragment ship only with the std-hosted (alloc) entry lib; a board
        // `self` shim calls the Rust API directly and needs none of it.
        if uses_std(&plan.build) {
            let include_dir = options.output_dir.join("include");
            fs::create_dir_all(&include_dir).wrap_err_with(|| {
                format!(
                    "failed to create generated package include dir {}",
                    include_dir.display()
                )
            })?;
            write_if_changed(
                &include_dir.join(format!("{}.h", system_ident(&plan))),
                &render_entry_header(&plan),
            )?;
            write_if_changed(
                &options.output_dir.join("CMakeLists.txt"),
                &render_entry_cmake(options, &plan),
            )?;
        }
    } else {
        // Phase 172 flip: every supported platform routes through the entry
        // lib (Zephyr above; all others via `emits_entry_lib`). A plan that
        // reaches here has an unsupported board/target the planner should have
        // rejected.
        bail!(
            "unsupported board/target for code generation: {} / {}",
            plan.build.board,
            plan.build.target
        );
    }
    if let Some(cargo_config) = cargo_config {
        let cargo_dir = options.output_dir.join(".cargo");
        fs::create_dir_all(&cargo_dir).wrap_err_with(|| {
            format!(
                "failed to create generated package cargo config dir {}",
                cargo_dir.display()
            )
        })?;
        write_if_changed(&cargo_dir.join("config.toml"), &cargo_config)?;
    }
    if let Some(toolchain) = rust_toolchain {
        write_if_changed(&options.output_dir.join("rust-toolchain.toml"), &toolchain)?;
    }
    // Phase 173.7 — NuttX is RtosOwned: the NuttX kernel owns the net
    // stack, so transport IP lands in the NuttX defconfig, not the board
    // `Config`. Emit an additive `nuttx-net.defconfig` fragment from
    // `[[transport]]` for the user to merge into the board defconfig
    // (NuttX is built out-of-tree). No transports ⇒ no file.
    if let Some(fragment) = nuttx_net_fragment(&plan) {
        write_if_changed(&options.output_dir.join("nuttx-net.defconfig"), &fragment)?;
    }

    Ok(GeneratedPackage {
        root: options.output_dir.clone(),
        manifest_path: options.output_dir.join("Cargo.toml"),
        plan_path: options.plan_path.clone(),
    })
}

fn render_cargo_toml(options: &GenerateOptions, plan: &NrosPlan) -> String {
    CARGO_TEMPLATE
        .replace("{{ package_name }}", &options.package_name)
        .replace(
            "{{ lib_section }}",
            &render_lib_section(plan, &options.package_name),
        )
        .replace(
            "{{ default_features }}",
            &toml_string_array(&generated_default_features(
                &plan.build,
                plan.lifecycle.is_some(),
                plan.param_persistence.is_some(),
                plan.param_services.is_some(),
                plan.safety.is_some(),
                !plan.bridges.is_empty(),
            )),
        )
        .replace("{{ nros_path }}", &path_for_template(&options.nros_path))
        .replace(
            "{{ nros_orchestration_path }}",
            &path_for_template(&options.nros_orchestration_path),
        )
        .replace(
            "{{ component_dependencies }}",
            &format!(
                "{}{}{}",
                render_platform_dependencies(options, plan),
                render_backend_dependencies(options, plan),
                render_component_dependencies(options, plan)
            ),
        )
        .replace("{{ build_dependencies }}", &render_build_dependencies(plan))
        .replace(
            "{{ profile_section }}",
            &render_profile_section(&plan.build),
        )
}

/// Phase 204.15 — render the generated package's `[profile.release]` from the
/// `optimize` intent. Writing cargo *profile* fields (not RUSTFLAGS) is the safe
/// fan-out: it never clobbers an embedded example's `.cargo/config`
/// `[target] rustflags` (the `-Tlink.x` linker script — RUSTFLAGS env would
/// replace, not merge). `None`/unknown ⇒ empty (cargo's default release).
fn render_profile_section(build: &PlanBuildOptions) -> String {
    // Baseline from the `optimize` intent — an ordered (key, TOML-literal) list
    // so the rendered profile is deterministic.
    let mut fields: Vec<(&'static str, String)> = match build.optimize.as_deref() {
        Some("size") => vec![
            ("opt-level", "\"z\"".into()),
            ("lto", "\"fat\"".into()),
            ("codegen-units", "1".into()),
            ("strip", "true".into()),
            ("panic", "\"abort\"".into()),
        ],
        Some("speed") => vec![
            ("opt-level", "3".into()),
            ("lto", "\"fat\"".into()),
            ("codegen-units", "1".into()),
        ],
        Some("balanced") => vec![("opt-level", "\"s\"".into())],
        Some("debug") => vec![("opt-level", "1".into()), ("debug", "true".into())],
        // Unknown intent is inert; with no `[build.cargo]` overrides either, the
        // generated package keeps cargo's default release profile.
        _ => Vec::new(),
    };

    // Phase 204.15 (increment 2) — merge `[build.cargo]` over the baseline:
    // replace a baseline field in place (keep its position), else append.
    if let Some(cargo) = &build.cargo {
        for (key, value) in cargo_override_fields(cargo) {
            match fields.iter_mut().find(|(k, _)| *k == key) {
                Some(slot) => slot.1 = value,
                None => fields.push((key, value)),
            }
        }
    }

    if fields.is_empty() {
        return String::new();
    }
    let body = fields
        .iter()
        .map(|(k, v)| format!("{k} = {v}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("\n[profile.release]\n{body}\n")
}

/// Phase 204.15 (increment 2) — lower a `[build.cargo]` override table to an
/// ordered list of `(cargo-profile-key, TOML-literal)`. Each present field that
/// renders to a literal contributes; unrenderable JSON shapes are skipped (never
/// panic — an out-of-shape value just leaves the baseline untouched).
fn cargo_override_fields(cargo: &PlanCargoOverrides) -> Vec<(&'static str, String)> {
    [
        ("opt-level", &cargo.opt_level),
        ("lto", &cargo.lto),
        ("debug", &cargo.debug),
        ("strip", &cargo.strip),
        ("codegen-units", &cargo.codegen_units),
        ("panic", &cargo.panic),
    ]
    .into_iter()
    .filter_map(|(key, v)| {
        v.as_ref()
            .and_then(json_to_toml_literal)
            .map(|lit| (key, lit))
    })
    .collect()
}

/// Render a JSON scalar as a TOML value literal: string→quoted, bool/number→bare.
/// Non-scalars (array/object/null) yield `None` so the field is dropped.
fn json_to_toml_literal(v: &serde_json::Value) -> Option<String> {
    match v {
        serde_json::Value::String(s) => Some(format!("\"{s}\"")),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Phase 126.M5.zephyr — zephyr-lang-rust's
/// `rust_cargo_application()` looks for a staticlib named
/// `rustapp` (its CMakeLists.txt hard-codes the link line against
/// `libstaticlib.a → librustapp.a`). Every other platform stays a
/// regular binary crate.
fn render_lib_section(plan: &NrosPlan, package_name: &str) -> String {
    if matches!(
        profile(&plan.build).map(|p| p.entry_kind),
        Some(EntryKind::ZephyrStaticlib)
    ) {
        return "\n[lib]\nname = \"rustapp\"\ncrate-type = [\"staticlib\"]\n".to_string();
    }
    // Phase 172 entry lib: a `lib` for the self shim bin to call. The hosted
    // (alloc) lib also emits a `staticlib` for vendor linking; a board `self`
    // lib stays `lib`-only — a no_std `staticlib` is a final artifact that
    // would need its own panic handler (which lives in the bin shim).
    if emits_entry_lib(plan) {
        let crate_types = if uses_std(&plan.build) {
            "[\"lib\", \"staticlib\"]"
        } else {
            "[\"lib\"]"
        };
        return format!(
            "\n[lib]\nname = \"{}\"\ncrate-type = {crate_types}\n",
            crate_ident(package_name)
        );
    }
    String::new()
}

/// Phase 126.M5.zephyr — zephyr-lang-rust requires `zephyr-build`
/// in `[build-dependencies]` so Kconfig constants reach the Rust
/// staticlib at compile time. Other platforms have an empty
/// build-deps section today.
fn render_build_dependencies(plan: &NrosPlan) -> String {
    match profile(&plan.build).map(|p| p.entry_kind) {
        Some(EntryKind::ZephyrStaticlib) => "zephyr-build = \"0.1.0\"\n".to_string(),
        _ => String::new(),
    }
}

/// Phase 172 WP-B — the generated package's Rust crate identifier (its `[lib]`
/// name): the package name with every non-alphanumeric char folded to `_`.
fn crate_ident(package_name: &str) -> String {
    package_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// The `c.set_*` board-`Config` setter calls derived from `[[transport]]`
/// (Phase 173.5/.K.4) — IP/MAC/gateway/baud/wifi. Empty ⇒ no transport
/// override (the board uses its `Config::default()`).
fn transport_config_setter_calls(build: &PlanBuildOptions) -> Vec<String> {
    let mut calls = Vec::new();
    for t in &build.transports {
        if let Some(ip) = t.ip.as_deref()
            && !ip.eq_ignore_ascii_case("dhcp")
            && let Some((o, prefix)) = parse_ipv4_cidr(ip)
        {
            calls.push(format!(
                "    c.set_ipv4([{}, {}, {}, {}], {prefix});",
                o[0], o[1], o[2], o[3]
            ));
        }
        if let Some(mac) = t.mac.as_deref().and_then(parse_mac) {
            calls.push(format!(
                "    c.set_mac([0x{:02x}, 0x{:02x}, 0x{:02x}, 0x{:02x}, 0x{:02x}, 0x{:02x}]);",
                mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
            ));
        }
        if let Some((o, _)) = t.gateway.as_deref().and_then(parse_ipv4_cidr) {
            calls.push(format!(
                "    c.set_gateway([{}, {}, {}, {}]);",
                o[0], o[1], o[2], o[3]
            ));
        }
        if let Some(baud) = t.baudrate {
            calls.push(format!("    c.set_baudrate({baud});"));
        }
        if let Some(ssid) = t.ssid.as_deref() {
            calls.push(format!("    c.set_ssid({ssid:?});"));
        }
        if let Some(password) = t.password.as_deref() {
            calls.push(format!("    c.set_password({password:?});"));
        }
        // Phase 172.K.7 — multi-homing NIC list. Boards with a single fixed NIC
        // (every embedded target today) take the default no-op; the setter is
        // the seam a multi-homed hosted board / Cyclone `<Interfaces>` build
        // reads. Emitted only when the list is non-empty.
        if !t.interfaces.is_empty() {
            let items = t
                .interfaces
                .iter()
                .map(|i| format!("{i:?}"))
                .collect::<Vec<_>>()
                .join(", ");
            calls.push(format!("    c.set_interfaces(&[{items}]);"));
        }
    }
    calls
}

/// Parse `a.b.c.d[/prefix]` (default /24) into octets + prefix.
fn parse_ipv4_cidr(s: &str) -> Option<([u8; 4], u8)> {
    let (addr, prefix) = s.split_once('/').unwrap_or((s, "24"));
    let prefix: u8 = prefix.parse().ok()?;
    let mut octets = [0u8; 4];
    let mut n = 0;
    for part in addr.split('.') {
        if n == 4 {
            return None;
        }
        octets[n] = part.parse().ok()?;
        n += 1;
    }
    (n == 4).then_some((octets, prefix))
}

/// Parse a `:`/`-`-separated 6-octet MAC.
fn parse_mac(s: &str) -> Option<[u8; 6]> {
    let mut octets = [0u8; 6];
    let mut n = 0;
    for part in s.split([':', '-']) {
        if n == 6 {
            return None;
        }
        octets[n] = u8::from_str_radix(part, 16).ok()?;
        n += 1;
    }
    (n == 6).then_some(octets)
}

/// Whether the board owns the net stack + the plan carries transport config to
/// bake into its `Config` (drives the shim's `apply_transport_config` call).
fn emits_transport_config_override(plan: &NrosPlan) -> bool {
    let Some(p) = profile(&plan.build) else {
        return false;
    };
    p.entry.is_some()
        && p.net_stack == NetStack::NanorosOwned
        && !transport_config_setter_calls(&plan.build).is_empty()
}

/// Phase 172 WP-B — the system identifier in the entry-lib C ABI symbol prefix
/// + header name: `plan.system` lowercased, non-alphanumeric → `_`.
fn system_ident(plan: &NrosPlan) -> String {
    plan.system
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// Phase 172 WP-B — emit the two-form entry lib (compiled form) instead of a
/// bare `main`: a `src/lib.rs` exposing the Rust-native API + the C ABI
/// (`crate-type = ["lib", "staticlib"]`) plus a thin `src/main.rs` self shim.
///
/// Scoped this slice to **std-hosted, non-bridge `self`** targets
/// (native/posix/linux) — the form the `orchestration_e2e` fixture builds +
/// boots. Board, Zephyr, no_std-hosted, and bridge entries keep their current
/// emitter until later WP-B slices generalize the lib + add the source form.
fn emits_entry_lib(plan: &NrosPlan) -> bool {
    match profile(&plan.build) {
        // Hosted `self`: the std entry lib + a hosted shim.
        Some(p) if p.entry_kind == EntryKind::HostedMain => uses_std(&plan.build),
        // Board `self`: every BoardRun routes through the entry lib. Targets
        // with a `board_entry` get a no_std board shim driven by the rlib's
        // `run()`; targets without one (NuttX, orin-spe — they boot via libc
        // `main`) get the cfg-gated hosted shim (std/no_std picked at compile
        // time by the `std` feature, matching the legacy `HOSTED_MAIN`).
        Some(p) if p.entry_kind == EntryKind::BoardRun => true,
        // Native/posix remain valid hosted targets even in hermetic fixture
        // workspaces that do not carry an explicit board descriptor.
        None if matches!(plan.build.board.as_str(), "native" | "posix") => uses_std(&plan.build),
        _ => false,
    }
}

/// Phase 172 WP-B — `src/lib.rs` for the compiled-form entry lib: hosts the
/// generated wiring tables, re-exports the Rust-native API, and defines the
/// `nros_<sys>_*` C ABI over an opaque heap-owned `Executor` handle.
fn render_entry_lib_rs(plan: &NrosPlan) -> String {
    let sys = system_ident(plan);
    // Board targets compile the entry lib `#![no_std]`. The C ABI (a
    // heap-owned executor handle) needs an allocator + std-style boxing, so
    // it rides the std-hosted path; a board `self` shim calls the Rust API
    // (`register_all`) directly and doesn't need it.
    let no_std = !uses_std(&plan.build);
    let c_abi = uses_std(&plan.build);

    let mut out = String::new();
    if no_std {
        out.push_str("#![no_std]\n\n");
    }
    out.push_str("//! Generated nano-ros entry library (Phase 172 WP-B, compiled form).\n");
    out.push_str("//!\n//! Hosts the system wiring tables + the Rust-native entry API\n");
    out.push_str("//! (`build_executor` / `register_all`).\n\n");
    if c_abi {
        // The heap-owned C-ABI handle boxes through `alloc` (not `std`).
        out.push_str("extern crate alloc;\n\n");
    }
    out.push_str("mod nros_generated {\n");
    out.push_str(
        "    core::include!(core::concat!(core::env!(\"OUT_DIR\"), \"/nros_generated.rs\"));\n",
    );
    out.push_str("}\n\n");
    // Re-export the wiring the board `self` shim needs (`TRANSPORT_LOCATOR`
    // for the baked locator; `apply_transport_config` when the board owns the
    // net stack) alongside the core API.
    let mut reexports = String::from("SYSTEM, TRANSPORT_LOCATOR, build_executor, register_all");
    if emits_transport_config_override(plan) {
        reexports.push_str(", apply_transport_config");
    }
    // Multi-session (bridge or multi-domain): the open_multi path uses
    // `build_executor_bridge` (no ExecutorConfig; sessions come from baked
    // SESSION_SPECS).
    if is_multi_session(plan) {
        reexports.push_str(", build_executor_bridge");
    }
    // W.5.6 — the manual spin+tick loop the shim/C-ABI spin route through.
    if has_shared_instance(plan) {
        reexports.push_str(", run_tick_loop");
    }
    // W.5.11 — the no_std action tick loop the board/self shim spins through.
    if has_no_std_action(plan) {
        reexports.push_str(", run_tick_loop_nostd");
    }
    out.push_str(&format!("pub use nros_generated::{{{reexports}}};\n\n"));

    if !c_abi {
        // Board self: Rust API only; the shim calls `build_executor` +
        // `register_all`.
        return out;
    }

    out.push_str("// --- Entry-lib C ABI (Phase 172 WP-B) ---\n\n");
    // Phase 172 WP-B — config lowering: the optional runtime `Config` override.
    // Precedence is param > env > baked — a non-NULL `NrosConfig` overrides the
    // env/baked defaults `build_executor` would otherwise use.
    out.push_str(
        "/// Optional runtime config override passed to `build_executor`\n\
         /// (precedence: param > env > baked). Unset fields fall through.\n\
         #[repr(C)]\n\
         pub struct NrosConfig {\n\
         \x20   /// ROS 2 domain ID; negative ⇒ unset (env/baked).\n\
         \x20   pub domain_id: i32,\n\
         \x20   /// Middleware locator (NUL-terminated); NULL ⇒ unset.\n\
         \x20   pub locator: *const core::ffi::c_char,\n\
         }\n\n",
    );
    out.push_str(&format!(
        "/// Build the system executor. `cfg` overrides env/baked config\n\
         /// (precedence param > env > baked); NULL ⇒ env/baked. Heap-owned — free\n\
         /// with `nros_{sys}_destroy`; returns NULL on error.\n\
         #[unsafe(no_mangle)]\n\
         pub extern \"C\" fn nros_{sys}_build_executor(cfg: *const NrosConfig) -> *mut nros::Executor {{\n\
         \x20   let mut config: nros::ExecutorConfig<'_> =\n\
         \x20       nros::ExecutorConfig::from_env().node_name(nros_generated::SYSTEM.default_node_name());\n\
         \x20   // Apply the param override (highest precedence). The locator borrow\n\
         \x20   // lives only across the immediate `build_executor` open below.\n\
         \x20   let locator_override: Option<&str> = match unsafe {{ cfg.as_ref() }} {{\n\
         \x20       Some(cfg) => {{\n\
         \x20           if cfg.domain_id >= 0 {{ config = config.domain_id(cfg.domain_id as u32); }}\n\
         \x20           if cfg.locator.is_null() {{ None }} else {{ unsafe {{ core::ffi::CStr::from_ptr(cfg.locator) }}.to_str().ok() }}\n\
         \x20       }}\n\
         \x20       None => None,\n\
         \x20   }};\n\
         \x20   if let Some(locator) = locator_override {{ config.locator = locator; }}\n\
         \x20   match nros_generated::build_executor(&config) {{\n\
         \x20       Ok(executor) => alloc::boxed::Box::into_raw(alloc::boxed::Box::new(executor)),\n\
         \x20       Err(_) => core::ptr::null_mut(),\n\
         \x20   }}\n\
         }}\n\n"
    ));
    out.push_str(&format!(
        "/// Register sched contexts + every node + lifecycle + param persistence.\n\
         #[unsafe(no_mangle)]\n\
         pub extern \"C\" fn nros_{sys}_register_all(executor: *mut nros::Executor) -> i32 {{\n\
         \x20   match unsafe {{ executor.as_mut() }} {{\n\
         \x20       Some(executor) => match nros_generated::register_all(executor) {{ Ok(()) => 0, Err(_) => -1 }},\n\
         \x20       None => -1,\n\
         \x20   }}\n\
         }}\n\n"
    ));
    // W.5.6 — when the plan has a rust executable instance, spin routes through
    // the manual spin-once + tick loop (registration must have run first, via
    // `nros_{sys}_register_all`) so components' `tick` bodies drive action
    // feedback/result; otherwise plain blocking spin.
    let spin_body = if has_shared_instance(plan) {
        "match nros_generated::run_tick_loop(executor) { Ok(()) => 0, Err(_) => -1 }"
    } else {
        "match executor.spin_blocking(nros::SpinOptions::default()) { Ok(()) => 0, Err(_) => -1 }"
    };
    out.push_str(&format!(
        "/// Spin the executor (blocking) until shutdown.\n\
         #[unsafe(no_mangle)]\n\
         pub extern \"C\" fn nros_{sys}_spin(executor: *mut nros::Executor) -> i32 {{\n\
         \x20   match unsafe {{ executor.as_mut() }} {{\n\
         \x20       Some(executor) => {spin_body},\n\
         \x20       None => -1,\n\
         \x20   }}\n\
         }}\n\n"
    ));
    out.push_str(&format!(
        "/// Free an executor returned by `nros_{sys}_build_executor`.\n\
         #[unsafe(no_mangle)]\n\
         pub extern \"C\" fn nros_{sys}_destroy(executor: *mut nros::Executor) {{\n\
         \x20   if !executor.is_null() {{\n\
         \x20       drop(unsafe {{ alloc::boxed::Box::from_raw(executor) }});\n\
         \x20   }}\n\
         }}\n"
    ));
    out
}

/// Phase 172 WP-B — the thin `self` startup shim `src/main.rs`: opens +
/// registers the system through the entry lib, then spins. All wiring lives in
/// `lib.rs`; the shim only exists so a `self` deploy produces a runnable binary.
fn render_hosted_shim_main(options: &GenerateOptions, plan: &NrosPlan) -> String {
    let krate = crate_ident(&options.package_name);
    // W.5.6 — std builds with a rust executable instance spin via the manual
    // spin-once + tick loop (`run_tick_loop`) instead of plain `spin_blocking`.
    let shared = has_shared_instance(plan);
    // W.5.11 — no_std action plans spin via the no_std action tick loop.
    let nostd_action = has_no_std_action(plan);
    let tick_use = match (shared, nostd_action) {
        (true, _) => ", run_tick_loop",
        (false, true) => ", run_tick_loop_nostd",
        (false, false) => "",
    };
    let std_spin = if shared {
        "return run_tick_loop(&mut executor);"
    } else {
        "return executor.spin_blocking(SpinOptions::default());"
    };
    let nostd_spin = if nostd_action {
        "run_tick_loop_nostd(&mut executor)"
    } else {
        "executor.spin_default()"
    };
    // Multi-session (bridge or multi-domain): the open_multi path opens
    // sessions from baked SESSION_SPECS — no `ExecutorConfig`. Same cfg-gated
    // spin as the single-session shim.
    if is_multi_session(plan) {
        return format!(
            "//! Generated bridge `self` startup shim (Phase 172 entry-lib). The entry\n\
             //! lib's `build_executor_bridge` opens every SESSION_SPEC; we just\n\
             //! register + spin.\n\n\
             use nros::prelude::*;\n\
             use {krate}::{{build_executor_bridge, register_all{tick_use}}};\n\n\
             fn main() -> core::result::Result<(), nros::NodeError> {{\n\
             \x20   let mut executor = build_executor_bridge()?;\n\
             \x20   register_all(&mut executor)?;\n\
             \x20   #[cfg(feature = \"std\")]\n\
             \x20   {std_spin}\n\
             \x20   #[cfg(not(feature = \"std\"))]\n\
             \x20   {nostd_spin}\n\
             }}\n"
        );
    }
    // The std / no_std splits are cfg-gated like the legacy `HOSTED_MAIN`, so
    // a single shim covers std-hosted (native/posix), BoardRun-with-no-board-
    // entry (nuttx, orin-spe — they boot via libc `main` but compile no_std at
    // the runtime layer), and any future cfg-flexible target.
    format!(
        "//! Generated `self` startup shim (Phase 172 entry-lib). All wiring lives in\n\
         //! the entry lib (`lib.rs`); this only opens + registers + spins.\n\n\
         use nros::prelude::*;\n\
         use {krate}::{{SYSTEM, build_executor, register_all{tick_use}}};\n\n\
         fn main() -> core::result::Result<(), nros::NodeError> {{\n\
         \x20   #[cfg(feature = \"std\")]\n\
         \x20   let config = ExecutorConfig::from_env().node_name(SYSTEM.default_node_name());\n\
         \x20   #[cfg(not(feature = \"std\"))]\n\
         \x20   let config = ExecutorConfig::default_const().node_name(SYSTEM.default_node_name());\n\
         \x20   let mut executor = build_executor(&config)?;\n\
         \x20   register_all(&mut executor)?;\n\
         \x20   #[cfg(feature = \"std\")]\n\
         \x20   {std_spin}\n\
         \x20   #[cfg(not(feature = \"std\"))]\n\
         \x20   {nostd_spin}\n\
         }}\n"
    )
}

/// Generated Zephyr entry library: the same no_std entry-lib base (Rust API,
/// no C ABI) plus a `#[unsafe(no_mangle)] extern "C" fn rust_main()` that
/// zephyr-lang-rust's `rust_cargo_application()` jumps to from the Zephyr
/// kernel `main`. Replaces the legacy stub `LIB_TEMPLATE` that built the
/// executor by hand — Zephyr now reuses the universal `build_executor` +
/// `register_all` like every other platform.
fn render_zephyr_entry_lib_rs(plan: &NrosPlan) -> String {
    let mut out = render_entry_lib_rs(plan);
    out.push_str("\n// --- Zephyr entry (Phase 172) ---\n");
    out.push_str("use nros::prelude::*;\n\n");
    out.push_str(
        "#[unsafe(no_mangle)]\n\
         pub extern \"C\" fn rust_main() {\n\
         \x20   // Referencing the `zephyr` crate links zephyr-lang-rust's\n\
         \x20   // #[global_allocator] + #[panic_handler] into the staticlib, and\n\
         \x20   // brings up logging; then wait for the net stack before the\n\
         \x20   // transport connects (mirrors examples/zephyr/rust/talker).\n\
         \x20   unsafe {\n\
         \x20       zephyr::set_logger().ok();\n\
         \x20   }\n\
         \x20   let _ = nros_platform::zephyr::wait_network(2000);\n\
         \x20   // Connect to the baked locator (where the agent/peer is) when the\n\
         \x20   // deploy declared one; else the platform default.\n\
         \x20   let config = match TRANSPORT_LOCATOR {\n\
         \x20       Some(locator) => ExecutorConfig::new(locator),\n\
         \x20       None => ExecutorConfig::default_const(),\n\
         \x20   }\n\
         \x20   .node_name(SYSTEM.default_node_name());\n\
         \x20   let mut executor = match build_executor(&config) {\n\
         \x20       Ok(executor) => executor,\n\
         \x20       Err(_) => return,\n\
         \x20   };\n\
         \x20   if register_all(&mut executor).is_err() {\n\
         \x20       return;\n\
         \x20   }\n\
         \x20   let _ = executor.spin_default();\n\
         }\n",
    );
    out
}

/// Generated board `self` startup shim: a `#![no_std]` `main` driven by the
/// board rlib's `run()` (hardware + transport bring-up), whose closure builds,
/// registers, and spins the system via the entry lib's Rust API. Mirrors the
/// hosted shim on the board entry pattern, replacing the legacy inlined
/// `run_system`. The board self shim needs no C ABI.
fn render_board_shim_main(options: &GenerateOptions, plan: &NrosPlan) -> String {
    let krate = crate_ident(&options.package_name);
    let entry = profile(&plan.build)
        .and_then(|p| p.entry)
        .expect("render_board_shim_main: board_entry present (gated by emits_entry_lib)");
    let apply_config = emits_transport_config_override(plan);
    let bridge = is_multi_session(plan);
    // W.5.11 — a no_std action plan spins via the action tick loop instead of
    // plain `spin_default`, so the board's `tick` bodies drive feedback/result.
    let nostd_action = has_no_std_action(plan);
    let tick_use = if nostd_action {
        ", run_tick_loop_nostd"
    } else {
        ""
    };
    let board_spin = if nostd_action {
        "run_tick_loop_nostd(&mut executor)"
    } else {
        "executor.spin_default()"
    };
    let cfg_expr = if apply_config {
        format!(
            "{{\n            let mut cfg = {b}::Config::default();\n\
             \x20           {krate}::apply_transport_config(&mut cfg);\n\
             \x20           cfg\n        }}",
            b = entry.crate_name,
        )
    } else {
        format!("{b}::Config::default()", b = entry.crate_name)
    };

    let mut out = String::new();
    out.push_str("#![no_std]\n#![no_main]\n\n");
    out.push_str(
        "//! Generated board `self` startup shim (Phase 172 entry-lib). The board\n\
         //! rlib's `run()` boots hardware, then the closure builds + registers +\n\
         //! spins via the entry lib (`lib.rs`).\n\n",
    );
    out.push_str("use nros::prelude::*;\n");
    let imports = if bridge {
        format!("use {krate}::{{build_executor_bridge, register_all{tick_use}}};\n")
    } else {
        format!(
            "use {krate}::{{SYSTEM, TRANSPORT_LOCATOR, build_executor, register_all{tick_use}}};\n"
        )
    };
    out.push_str(&imports);
    if !entry.crate_root_extra.is_empty() {
        out.push_str(&entry.crate_root_extra);
        out.push('\n');
    }
    out.push('\n');
    if !entry.comment.is_empty() {
        out.push_str(&entry.comment);
        out.push('\n');
    }
    out.push_str(&entry.signature);
    if bridge {
        // Bridge mode: hardware bring-up via `<board>::run`, then the entry
        // lib's `build_executor_bridge` opens every SESSION_SPEC; the per-run
        // ExecutorConfig is unused.
        out.push_str(&format!(
            " {{\n    {b}::run(\n        {cfg_expr},\n        |_board_config| -> core::result::Result<(), nros::NodeError> {{\n\
             \x20           let mut executor = build_executor_bridge()?;\n\
             \x20           register_all(&mut executor)?;\n\
             \x20           {board_spin}\n\
             \x20       }},\n    )\n}}\n",
            b = entry.crate_name,
        ));
    } else {
        out.push_str(&format!(
            " {{\n    {b}::run(\n        {cfg_expr},\n        |board_config| -> core::result::Result<(), nros::NodeError> {{\n\
             \x20           let config = ExecutorConfig::new(TRANSPORT_LOCATOR.unwrap_or(board_config.zenoh_locator))\n\
             \x20               .domain_id(board_config.domain_id)\n\
             \x20               .node_name(SYSTEM.default_node_name()){extra};\n\
             \x20           let mut executor = build_executor(&config)?;\n\
             \x20           register_all(&mut executor)?;\n\
             \x20           {board_spin}\n\
             \x20       }},\n    )\n}}\n",
            b = entry.crate_name,
            extra = entry.closure_extra,
        ));
    }
    out
}

/// Phase 172 WP-B — the cbindgen-shaped C header for the compiled-form entry
/// lib. Emitted directly (the ABI is fixed + known at generation time, so no
/// build-time cbindgen scan is needed); names the opaque handle `NrosExecutor`
/// to match `nros-c`.
fn render_entry_header(plan: &NrosPlan) -> String {
    let sys = system_ident(plan);
    let guard = format!("NROS_ENTRY_{}_H", sys.to_uppercase());
    format!(
        "/* Generated nano-ros entry-lib C ABI (Phase 172 WP-B). Do not edit. */\n\
         #ifndef {guard}\n#define {guard}\n\n#include <stdint.h>\n\n\
         #ifdef __cplusplus\nextern \"C\" {{\n#endif\n\n\
         /* Opaque executor handle (as in nros-c). */\n\
         typedef struct NrosExecutor NrosExecutor;\n\n\
         /* Optional runtime config override (precedence: param > env > baked). */\n\
         typedef struct NrosConfig {{\n\
         \x20   int32_t domain_id;     /* < 0 => unset (env/baked) */\n\
         \x20   const char *locator;   /* NULL => unset */\n\
         }} NrosConfig;\n\n\
         NrosExecutor *nros_{sys}_build_executor(const NrosConfig *cfg);\n\
         int32_t nros_{sys}_register_all(NrosExecutor *executor);\n\
         int32_t nros_{sys}_spin(NrosExecutor *executor);\n\
         void nros_{sys}_destroy(NrosExecutor *executor);\n\n\
         #ifdef __cplusplus\n}}\n#endif\n\n#endif /* {guard} */\n"
    )
}

/// Phase 172 WP-B — the entry lib's **source-form** CMake fragment. A
/// vendor-owns-toolchain deploy (`emit = "source"`) `add_subdirectory()`s the
/// generated package; Corrosion (loaded by the vendor project) compiles the
/// crate's `staticlib` in the vendor's toolchain, and the fragment exposes it
/// as `<sys>_entry` with the C ABI header on the include path. Emitted
/// alongside the compiled artifacts so one generated package serves both forms;
/// Entry generation picks per `[deploy].emit`.
fn render_entry_cmake(options: &GenerateOptions, plan: &NrosPlan) -> String {
    let sys = system_ident(plan);
    let krate = crate_ident(&options.package_name);
    format!(
        "# Generated nano-ros entry lib — source form (Phase 172 WP-B). Do not edit.\n\
         #\n\
         # A vendor CMake project that has Corrosion loaded consumes this with:\n\
         #   add_subdirectory(<this_dir> {sys}_entry)\n\
         #   target_link_libraries(<app> PRIVATE {sys}_entry)\n\
         # Corrosion compiles the generated crate's staticlib in the vendor\n\
         # toolchain; the `nros_{sys}_*` C ABI header is on the include path.\n\
         cmake_minimum_required(VERSION 3.22)\n\n\
         if(NOT COMMAND corrosion_import_crate)\n\
         \x20   message(FATAL_ERROR\n\
         \x20       \"nano-ros entry lib (source form) needs Corrosion — load it before add_subdirectory()\")\n\
         endif()\n\n\
         corrosion_import_crate(\n\
         \x20   MANIFEST_PATH \"${{CMAKE_CURRENT_LIST_DIR}}/Cargo.toml\"\n\
         \x20   CRATES {krate}\n\
         \x20   CRATE_TYPES staticlib)\n\n\
         add_library({sys}_entry INTERFACE)\n\
         target_link_libraries({sys}_entry INTERFACE {krate})\n\
         target_include_directories({sys}_entry INTERFACE \"${{CMAKE_CURRENT_LIST_DIR}}/include\")\n"
    )
}

fn render_zephyr_cmake(options: &GenerateOptions) -> String {
    ZEPHYR_CMAKE_TEMPLATE.replace("{{ package_name }}", &options.package_name)
}

fn render_zephyr_prj_conf(plan: &NrosPlan) -> String {
    // Phase 172 W.4 — the per-RMW config is baked in (not left to a manual
    // `-DCONF_FILE` overlay) so a vendor-module `west build {entry_src}` is
    // self-contained. Phase 173.7 — the net config derived from nros.toml
    // `[[transport]]` is an additive fragment on top.
    format!(
        "{}{}{}",
        ZEPHYR_PRJ_CONF_TEMPLATE,
        zephyr_rmw_fragment(&plan.build),
        zephyr_net_fragment(&plan.build),
    )
}

/// Phase 172 W.4 — the per-RMW Zephyr Kconfig the generated app needs to build
/// and link the chosen transport. Mirrors the per-RMW overlays under
/// `examples/zephyr/rust/talker/` (the source of truth for the knobs);
/// `CONFIG_POSIX_API=y` in particular reconciles Zephyr's POSIX and net headers
/// for the zenoh-pico and Cyclone C builds. Later assignments override the base
/// prj.conf (Kconfig fragment semantics). Board-specific tuning (e.g. native_sim NSOS offload)
/// belongs in a `[deploy].config` hook overlay, not here.
fn zephyr_rmw_fragment(build: &PlanBuildOptions) -> String {
    let body = match build.rmw.as_str() {
        "zenoh" | "rmw-zenoh" => {
            "CONFIG_NROS_RMW_ZENOH=y\n\
             CONFIG_NET_TCP=y\n\
             CONFIG_POSIX_API=y\n\
             CONFIG_POSIX_THREAD_THREADS_MAX=16\n\
             CONFIG_MAIN_STACK_SIZE=16384\n\
             CONFIG_HEAP_MEM_POOL_SIZE=65536\n\
             CONFIG_SYSTEM_WORKQUEUE_STACK_SIZE=4096\n\
             CONFIG_NET_PKT_RX_COUNT=32\n\
             CONFIG_NET_PKT_TX_COUNT=32\n\
             CONFIG_NET_BUF_RX_COUNT=64\n\
             CONFIG_NET_BUF_TX_COUNT=64\n\
             CONFIG_NET_CONNECTION_MANAGER=y\n"
        }
        "xrce" | "rmw-xrce" => {
            "CONFIG_NROS_RMW_XRCE=y\n\
             CONFIG_NROS_XRCE_AGENT_ADDR=\"127.0.0.1\"\n\
             CONFIG_NROS_XRCE_AGENT_PORT=2018\n\
             CONFIG_NET_TCP=y\n\
             CONFIG_MAIN_STACK_SIZE=16384\n\
             CONFIG_HEAP_MEM_POOL_SIZE=65536\n\
             CONFIG_SYSTEM_WORKQUEUE_STACK_SIZE=4096\n\
             CONFIG_NET_PKT_RX_COUNT=32\n\
             CONFIG_NET_PKT_TX_COUNT=32\n\
             CONFIG_NET_BUF_RX_COUNT=64\n\
             CONFIG_NET_BUF_TX_COUNT=64\n\
             CONFIG_NET_CONNECTION_MANAGER=y\n"
        }
        "cyclonedds" | "rmw-cyclonedds" => {
            "CONFIG_NROS_RMW_CYCLONEDDS=y\n\
             CONFIG_CPP=y\n\
             CONFIG_NROS_CYCLONE_DOMAIN_ID=0\n\
             CONFIG_NET_IPV4_IGMP=y\n\
             CONFIG_POSIX_API=y\n\
             CONFIG_MAX_PTHREAD_MUTEX_COUNT=256\n\
             CONFIG_MAX_PTHREAD_COND_COUNT=256\n\
             CONFIG_POSIX_THREAD_THREADS_MAX=16\n\
             CONFIG_MAIN_STACK_SIZE=524288\n\
             CONFIG_HEAP_MEM_POOL_SIZE=4194304\n\
             CONFIG_SYSTEM_WORKQUEUE_STACK_SIZE=8192\n\
             CONFIG_COMMON_LIBC_MALLOC_ARENA_SIZE=16777216\n\
             CONFIG_NET_TCP=y\n"
        }
        _ => return String::new(),
    };
    format!(
        "\n# Phase 172 W.4 — per-RMW config (rmw = {}); mirrors \
         examples/zephyr/rust/talker/prj-<rmw>.conf.\n{body}",
        build.rmw
    )
}

/// Phase 173.7 — Zephyr `CONFIG_NET_CONFIG_*` lines from the ethernet
/// transport's `ip` (`"dhcp"` or `"<addr>/<prefix>"`). Empty when no
/// ethernet transport / no `ip` is declared.
fn zephyr_net_fragment(build: &PlanBuildOptions) -> String {
    let Some(eth) = build
        .transports
        .iter()
        .find(|t| t.kind == TransportKind::Ethernet)
    else {
        return String::new();
    };
    let Some(ip) = eth.ip.as_deref() else {
        return String::new();
    };
    let mut out = String::from(
        "\n# Phase 173.7 — net config from nros.toml [[transport]] (additive).\n\
         CONFIG_NET_CONFIG_SETTINGS=y\n",
    );
    if ip.eq_ignore_ascii_case("dhcp") {
        out.push_str("CONFIG_NET_DHCPV4=y\n");
    } else {
        let (addr, prefix) = ip.split_once('/').unwrap_or((ip, "24"));
        out.push_str(&format!("CONFIG_NET_CONFIG_MY_IPV4_ADDR=\"{addr}\"\n"));
        if let Some(mask) = prefix_to_netmask(prefix) {
            out.push_str(&format!("CONFIG_NET_CONFIG_MY_IPV4_NETMASK=\"{mask}\"\n"));
        }
    }
    out
}

/// IPv4 prefix length → dotted netmask (`24` → `255.255.255.0`).
fn prefix_to_netmask(prefix: &str) -> Option<String> {
    let bits: u32 = prefix.parse().ok()?;
    if bits > 32 {
        return None;
    }
    let mask: u32 = if bits == 0 {
        0
    } else {
        u32::MAX << (32 - bits)
    };
    Some(format!(
        "{}.{}.{}.{}",
        (mask >> 24) & 0xff,
        (mask >> 16) & 0xff,
        (mask >> 8) & 0xff,
        mask & 0xff
    ))
}

/// Dotted IPv4 → NuttX defconfig hex literal (`"10.0.2.50"` →
/// `"0x0a000232"`). `None` on malformed input.
fn ipv4_to_hex(addr: &str) -> Option<String> {
    let mut octets = [0u8; 4];
    let mut n = 0;
    for part in addr.split('.') {
        if n == 4 {
            return None;
        }
        octets[n] = part.parse().ok()?;
        n += 1;
    }
    if n != 4 {
        return None;
    }
    Some(format!(
        "0x{:02x}{:02x}{:02x}{:02x}",
        octets[0], octets[1], octets[2], octets[3]
    ))
}

/// Phase 173.7 — NuttX `CONFIG_NETINIT_*` defconfig fragment from the
/// ethernet transport's `ip`. `None` (no file emitted) unless the plan
/// targets NuttX *and* declares an ethernet transport with an `ip` —
/// keeping the no-transport NuttX build byte-identical (no extra file).
fn nuttx_net_fragment(plan: &NrosPlan) -> Option<String> {
    if profile(&plan.build).map(|p| p.platform) != Some(PlatformKind::Nuttx) {
        return None;
    }
    let eth = plan
        .build
        .transports
        .iter()
        .find(|t| t.kind == TransportKind::Ethernet)?;
    let ip = eth.ip.as_deref()?;
    let mut out = String::from(
        "# Phase 173.7 — NuttX net config from nros.toml [[transport]].\n\
         # Additive fragment — merge into the board defconfig (NuttX is\n\
         # built out-of-tree). nano-ros emits only the net knobs; kernel\n\
         # config stays the board's.\n\
         CONFIG_NET=y\n\
         CONFIG_NET_IPv4=y\n",
    );
    if ip.eq_ignore_ascii_case("dhcp") {
        out.push_str("CONFIG_NETINIT_DHCPC=y\n");
    } else {
        let (addr, prefix) = ip.split_once('/').unwrap_or((ip, "24"));
        if let Some(hex) = ipv4_to_hex(addr) {
            out.push_str(&format!("CONFIG_NETINIT_IPADDR={hex}\n"));
        }
        if let Some(mask) = prefix_to_netmask(prefix).as_deref().and_then(ipv4_to_hex) {
            out.push_str(&format!("CONFIG_NETINIT_NETMASK={mask}\n"));
        }
    }
    Some(out)
}

/// Phase 126.M5.nuttx — pin nightly + `rust-src` for targets that use
/// `-Z build-std`. NuttX `armv7a-nuttx-eabihf` rebuilds `std` from
/// source against the patched libc fork; the nightly date MUST match
/// the version pinned by the NuttX board crate's vendored libc. Other
/// platforms use stable rustc with prebuilt targets.
fn render_rust_toolchain(plan: &NrosPlan) -> Option<String> {
    // Phase 173.2 / 173.6 — toolchain pin driven by `profile()`. ESP32-C3
    // and NuttX need a nightly + `rust-src` pin (for `-Z build-std`);
    // ESP32-S3 (Xtensa) needs the espup `esp` channel; every other
    // platform uses stable.
    match (
        profile(&plan.build)?.toolchain,
        profile(&plan.build)?.platform,
    ) {
        (Toolchain::Esp, _) => Some(
            r#"# Auto-generated by nano-ros orchestration for the ESP32-S3 (Xtensa) target.
# Phase 173.6 — xtensa-esp32s3-none-elf is not a rustup target; it ships
# in the espup `esp` channel, which also bundles `rust-src` for the
# `-Z build-std` (no_std + alloc) build. Install with `espup install`.
[toolchain]
channel = "esp"
components = ["rust-src", "rustfmt"]
"#
            .to_string(),
        ),
        (Toolchain::Nightly, PlatformKind::Esp32) => Some(
            r#"# Auto-generated by nano-ros orchestration for the ESP32-C3 target.
# Phase 126.M5.esp32 — riscv32imc-unknown-none-elf needs `-Z build-std`
# (no_std + alloc), which needs nightly + `rust-src`. Pin matches
# `tools/rust-toolchain.toml`.
[toolchain]
channel = "nightly-2026-04-11"
components = ["rust-src", "rustfmt"]
"#
            .to_string(),
        ),
        (Toolchain::Nightly, PlatformKind::Nuttx) => Some(
            r#"# Auto-generated by nano-ros orchestration for the NuttX target.
# Phase 126.M5.nuttx — armv7a-nuttx-eabihf needs `-Z build-std`, which
# needs nightly + `rust-src`. The pinned date matches the patched libc
# version pinned by the NuttX board crate.
[toolchain]
channel = "nightly-2026-04-11"
components = ["rust-src", "rustfmt"]
"#
            .to_string(),
        ),
        _ => None,
    }
}

fn render_build_rs(options: &GenerateOptions, plan: &NrosPlan) -> String {
    let generated_tables = render_generated_tables(plan);
    BUILD_TEMPLATE
        .replace("{{ plan_path }}", &path_for_template(&options.plan_path))
        .replace(
            "{{ native_link_directives }}",
            &render_native_link_directives(options, plan),
        )
        .replace(
            "{{ platform_link_directives }}",
            &render_platform_link_directives(plan),
        )
        .replace(
            "{{ generated_tables_literal }}",
            &format!("{generated_tables:?}"),
        )
}

/// Phase 126.M5.nuttx — emit build.rs link directives for target
/// platforms that need to link external kernel/userspace libs into
/// the final ELF. NuttX (Cortex-A7) needs the staging libs at
/// `$NUTTX_DIR/staging/lib{c,sched,drivers,...}.a` plus
/// arch-specific glue + the dramboot linker script. Mirrors what
/// `examples/qemu-arm-nuttx/rust/zenoh/talker/build.rs` emits per
/// crate.
fn render_platform_link_directives(plan: &NrosPlan) -> String {
    match profile(&plan.build).map(|p| p.link_kind) {
        Some(LinkKind::NuttxStaging) => NUTTX_LINK_DIRECTIVES.to_string(),
        _ => String::new(),
    }
}

const NUTTX_LINK_DIRECTIVES: &str = r#"    println!("cargo:rerun-if-env-changed=NUTTX_DIR");
    if let Ok(nuttx_dir) = env::var("NUTTX_DIR") {
        let nuttx_dir = PathBuf::from(nuttx_dir);
        let staging = nuttx_dir.join("staging");
        if staging.join("libc.a").exists() {
            // Preprocess the dramboot linker script (it #includes <nuttx/config.h>).
            let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set"));
            let processed_ld = out_dir.join("dramboot.ld");
            let linker_script = nuttx_dir.join("boards/arm/qemu/qemu-armv7a/scripts/dramboot.ld");
            let status = Command::new("arm-none-eabi-gcc")
                .args([
                    "-E", "-P", "-x", "c",
                    &format!("-isystem{}", nuttx_dir.join("include").display()),
                    "-D__NuttX__", "-D__KERNEL__",
                    &format!("-I{}", nuttx_dir.join("arch/arm/src/chip").display()),
                    &format!("-I{}", nuttx_dir.join("arch/arm/src/common").display()),
                    &format!("-I{}", nuttx_dir.join("arch/arm/src/armv7-a").display()),
                    &format!("-I{}", nuttx_dir.join("sched").display()),
                ])
                .arg(&linker_script)
                .arg("-o")
                .arg(&processed_ld)
                .status()
                .expect("failed to preprocess linker script");
            assert!(status.success(), "linker script preprocessing failed");

            let board_src = nuttx_dir.join("arch/arm/src/board");
            let vectortab = nuttx_dir.join("arch/arm/src/arm_vectortab.o");
            let gcc_out = Command::new("arm-none-eabi-gcc")
                .args([
                    "-mcpu=cortex-a7",
                    "-mfloat-abi=hard",
                    "-mfpu=neon-vfpv4",
                    "-print-libgcc-file-name",
                ])
                .output()
                .expect("failed to find libgcc");
            let libgcc = String::from_utf8(gcc_out.stdout).unwrap().trim().to_string();

            println!("cargo:rustc-link-arg=-T{}", processed_ld.display());
            println!("cargo:rustc-link-arg=--entry=__start");
            println!("cargo:rustc-link-arg=-nostartfiles");
            println!("cargo:rustc-link-arg=-nodefaultlibs");
            println!("cargo:rustc-link-arg={}", vectortab.display());
            println!("cargo:rustc-link-arg=-L{}", staging.display());
            println!("cargo:rustc-link-arg=-L{}", board_src.display());
            println!("cargo:rustc-link-arg=-Wl,--start-group");
            for lib in [
                "sched", "drivers", "boards", "c", "mm", "arch", "xx", "apps", "net",
                "crypto", "fs", "binfmt", "openamp", "board",
            ] {
                println!("cargo:rustc-link-arg=-l{lib}");
            }
            println!("cargo:rustc-link-arg={libgcc}");
            println!("cargo:rustc-link-arg=-Wl,--end-group");
            println!("cargo:rerun-if-changed={}", linker_script.display());
        }
    }
"#;

#[derive(Debug, Clone)]
struct NativeComponentLink {
    component_id: String,
    library_path: PathBuf,
}

fn render_native_link_directives(options: &GenerateOptions, plan: &NrosPlan) -> String {
    native_component_links(options, plan)
        .into_iter()
        .map(|link| {
            let search_dir = link
                .library_path
                .parent()
                .map(path_for_template)
                .unwrap_or_default();
            let lib_name = static_library_name(&link.library_path)
                .unwrap_or_else(|| link.component_id.replace([':', '-'], "_"));
            format!(
                "    println!(\"cargo:rerun-if-changed={}\");\n    println!(\"cargo:rustc-link-search=native={search_dir}\");\n    println!(\"cargo:rustc-link-lib=static={lib_name}\");\n",
                path_for_template(&link.library_path),
            )
        })
        .collect()
}

fn render_cargo_config(options: &GenerateOptions, plan: &NrosPlan) -> Option<String> {
    // The board descriptor carries the verbatim `.cargo/config.toml` body (with
    // `${workspace}` placeholders for any layout path); the CLI bakes in no
    // per-platform config. Boards needing no config omit the field.
    let workspace = workspace_from_nros_path(&options.nros_path)?;
    let body = profile(&plan.build)?.cargo_config_rendered(&workspace);

    // Phase 204.7 — a serial/CAN-only build carries no IP link, so bake
    // `NROS_LINK_IP=0` into the generated `[env]` (the board's `LinkFeatures`
    // gate drops the zenoh-pico / XRCE TCP+UDP link C). Generator-side, not in
    // the board descriptor: the same board (e.g. mps2-an385) builds either
    // ethernet *or* serial, so only the per-build transport choice can decide.
    if plan.build.drops_ip_link() {
        Some(inject_env_var(
            body.unwrap_or_default(),
            "NROS_LINK_IP",
            "0",
        ))
    } else {
        body
    }
}

/// Phase 204.7 — set `key = "value"` in a `.cargo/config.toml` `[env]` table,
/// merging into an existing `[env]` (idempotent — skips if already present) or
/// appending a new top-level `[env]` table. Keeps the rest of the board's
/// verbatim config untouched.
fn inject_env_var(body: String, key: &str, value: &str) -> String {
    let prefix_space = format!("{key} ");
    let prefix_eq = format!("{key}=");
    if body.lines().any(|l| {
        let t = l.trim_start();
        t.starts_with(&prefix_space) || t.starts_with(&prefix_eq)
    }) {
        return body; // already set (board config or a prior pass)
    }
    let assignment = format!("{key} = \"{value}\"");
    if let Some(pos) = body.lines().position(|l| l.trim() == "[env]") {
        let mut lines: Vec<String> = body.lines().map(str::to_string).collect();
        lines.insert(pos + 1, assignment);
        lines.join("\n") + "\n"
    } else {
        let mut out = body;
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("[env]\n{assignment}\n"));
        out
    }
}

fn path_for_template(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn render_component_dependencies(options: &GenerateOptions, plan: &NrosPlan) -> String {
    let Some(workspace) = &options.component_workspace else {
        return String::new();
    };
    let mut deps = BTreeMap::new();
    for component in plan
        .components
        .iter()
        .filter(|component| matches!(component.language.as_str(), "rust" | "Rust"))
    {
        let crate_name = rust_crate_name(component.id.as_str()).unwrap_or(&component.package);
        let package_root = workspace.join("src").join(&component.package);
        if package_root.join("Cargo.toml").is_file() {
            deps.insert(crate_name.to_string(), package_root);
        }
    }
    deps.into_iter()
        .map(|(crate_name, path)| {
            format!(
                "{crate_name} = {{ path = \"{}\", default-features = false }}\n",
                path_for_template(&path)
            )
        })
        .collect()
}

fn native_component_links(options: &GenerateOptions, plan: &NrosPlan) -> Vec<NativeComponentLink> {
    plan.components
        .iter()
        .filter(|component| !matches!(component.language.as_str(), "rust" | "Rust"))
        .filter_map(|component| {
            let config_path = component.component_config.as_deref().and_then(|path| {
                resolve_workspace_path(options.component_workspace.as_deref(), path)
            });
            let library_path = config_path
                .as_deref()
                .and_then(|path| component_static_library(path).ok().flatten())?;
            Some(NativeComponentLink {
                component_id: component.id.clone(),
                library_path,
            })
        })
        .collect()
}

fn resolve_workspace_path(workspace: Option<&Path>, raw: &str) -> Option<PathBuf> {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        return Some(path);
    }
    workspace.map(|workspace| workspace.join(path))
}

fn component_static_library(config_path: &Path) -> Result<Option<PathBuf>> {
    // Handles both the folded `nros.toml` `[component]` form and the legacy
    // standalone manifest (W.1); `None` when the file carries no component.
    let Some(config) = super::workspace::load_component_config(config_path)? else {
        return Ok(None);
    };
    Ok(config.linkage.static_library.map(|raw| {
        let path = PathBuf::from(raw);
        if path.is_absolute() {
            path
        } else {
            config_path
                .parent()
                .map(|parent| parent.join(&path))
                .unwrap_or(path)
        }
    }))
}

fn static_library_name(path: &Path) -> Option<String> {
    let stem = path.file_name()?.to_str()?;
    let stem = stem.strip_suffix(".a").unwrap_or(stem);
    Some(stem.strip_prefix("lib").unwrap_or(stem).to_string())
}

/// Phase 173.5 — board transport Cargo features from `[[transport]]`,
/// deduped (`ethernet` / `serial` / `can`). Empty when no transports
/// declared.
fn transport_cargo_features(build: &PlanBuildOptions) -> Vec<String> {
    let mut feats: Vec<String> = Vec::new();
    for t in &build.transports {
        let f = t.kind.cargo_feature().to_string();
        if !feats.contains(&f) {
            feats.push(f);
        }
    }
    feats
}

/// Phase 173.5 — format a board crate path dep, merging the board's
/// intrinsic `base_features` (e.g. the stm32 chip) with the declared
/// transport features.
///
/// When `[[transport]]` is declared the board's **default** features are
/// disabled so the transport selection is authoritative (the
/// `ethernet`→`serial` swap, or a bridge's multi-transport set). With no
/// declared transports the dep is emitted exactly as pre-173.5 (board
/// defaults left on) — keeping existing generated manifests
/// byte-identical.
///
/// Phase 248 C5b (RFC-0031) — `rmw_features` (e.g. `["rmw-zenoh"]`) are the
/// **board-crate RMW lowering target**: the declared `system.toml`
/// `[system].rmw` / `[deploy.<id>].rmw` value lowers to the board's `rmw-X`
/// feature (the board self-links + registers the backend), NOT to an
/// `nros/rmw-X` feature. They are appended to the board's feature list in
/// every shape. When `rmw_features` is empty the output is byte-identical to
/// the pre-C5b form (so an `rmw = "none"`/unset plan is unchanged).
fn board_dep(
    name: &str,
    path: &str,
    base_features: &[&str],
    rmw_features: &[String],
    build: &PlanBuildOptions,
) -> String {
    let transports = transport_cargo_features(build);
    let mut feats: Vec<String> = base_features.iter().map(|s| s.to_string()).collect();
    if transports.is_empty() {
        for r in rmw_features {
            if !feats.contains(r) {
                feats.push(r.clone());
            }
        }
        if feats.is_empty() {
            format!("{name} = {{ path = \"{path}\" }}\n")
        } else {
            format!(
                "{name} = {{ path = \"{path}\", features = {} }}\n",
                toml_string_array(&feats)
            )
        }
    } else {
        for t in transports {
            if !feats.contains(&t) {
                feats.push(t);
            }
        }
        for r in rmw_features {
            if !feats.contains(r) {
                feats.push(r.clone());
            }
        }
        format!(
            "{name} = {{ path = \"{path}\", default-features = false, features = {} }}\n",
            toml_string_array(&feats)
        )
    }
}

/// Phase 248 C5b (RFC-0031) — the board-crate RMW feature(s) the declared RMW
/// lowers to: `["rmw-zenoh"]` for a single-RMW build, the union (e.g.
/// `["rmw-zenoh", "rmw-cyclonedds"]`) for a bridge. Empty when no RMW is
/// declared (`rmw = "none"`/unset). The board crate owns linking + registering
/// the concrete backend behind this feature (C5a), so this is the Rust lowering
/// target — replacing the old `nros/rmw-X` umbrella feature.
fn board_rmw_features(build: &PlanBuildOptions) -> Vec<String> {
    rmw_set(build).iter().map(|r| format!("rmw-{r}")).collect()
}

/// Phase 252 (issue 0072) — the capability-axis features that lower to the BOARD
/// crate's forwarding feature (target 3 of the RFC-0031 capability generalization).
/// A declared `[safety]` axis lowers to the board's `safety-e2e` feature only when
/// the board advertises support (`capability_features` in its descriptor); a board
/// that does not is **skipped + warned**, never a Cargo error. Empty when no
/// capability axis is declared (byte-identical to pre-252).
fn board_capability_features(
    plan: &NrosPlan,
    board: &crate::orchestration::board_descriptor::BoardDescriptor,
) -> Vec<String> {
    let mut feats = Vec::new();
    if plan.safety.is_some()
        && let Some(bf) = crate::orchestration::capability("safety").and_then(|c| c.backend_feature)
    {
        if board.capability_features.iter().any(|f| f == bf) {
            feats.push(bf.to_string());
        } else {
            eprintln!(
                "warning: board {:?} does not declare the '{bf}' capability feature; \
                 [safety] enables the validation surface but NOT backend CRC on this \
                 board (issue 0072)",
                board.names
            );
        }
    }
    feats
}

fn render_platform_dependencies(options: &GenerateOptions, plan: &NrosPlan) -> String {
    let Some(workspace) = workspace_from_nros_path(&options.nros_path) else {
        return String::new();
    };
    let Some(p) = profile(&plan.build) else {
        return String::new();
    };
    // Board-crate dependency — name + path come from the descriptor, so no
    // `nros-board-*` / `packages/boards/...` literal lives in the CLI. An
    // RtosOwned board (NuttX) takes a plain path dep: its transports land in
    // the RTOS defconfig, not the board `Config`, so no transport-feature
    // merge. Crate-less host boards (posix / zephyr / orin-spe) have no
    // `board_crate` and emit their deps below.
    //
    // Phase 248 C5b (RFC-0031) — the declared RMW lowers to the BOARD crate's
    // `rmw-X` feature here (the board self-links + registers the concrete
    // backend, C5a), not to an `nros/rmw-X` feature. Crate-less boards
    // (native/posix, zephyr) have no board crate to carry it yet; those still
    // link the backend via the direct `nros-rmw-*` dep + explicit `register()`
    // in `register_backends()` (board-driven lowering for them is a follow-up —
    // see the C5b notes / RFC-0031).
    let rmw_feats = board_rmw_features(&plan.build);
    // Phase 252 (issue 0072) — capability axes (`[safety]`) lower to the board
    // crate's forwarding feature, gated on the board advertising support.
    let cap_feats = board_capability_features(plan, &p);
    let board_line = match (p.board_crate.as_deref(), p.crate_path_rel()) {
        (Some(name), Some(rel)) => {
            let path = path_for_template(&workspace.join(rel));
            if p.net_stack == NetStack::RtosOwned {
                // RtosOwned (NuttX): transports go to the RTOS defconfig (no
                // transport-feature merge), but the board still selects + links
                // the RMW backend via its `rmw-X` feature.
                let mut feats = rmw_feats.clone();
                feats.extend(cap_feats.iter().cloned());
                if feats.is_empty() {
                    format!("{name} = {{ path = \"{path}\" }}\n")
                } else {
                    format!(
                        "{name} = {{ path = \"{path}\", features = {} }}\n",
                        toml_string_array(&feats)
                    )
                }
            } else {
                let mut base: Vec<String> = p.board_features.clone();
                base.extend(cap_feats.iter().cloned());
                let feats: Vec<&str> = base.iter().map(String::as_str).collect();
                board_dep(name, &path, &feats, &rmw_feats, &plan.build)
            }
        }
        _ => String::new(),
    };
    // Per-platform direct crates.io deps that must be visible at the generated
    // package's crate root (entry-point proc macros, panic handlers, log
    // transports) plus the crate-less boards' platform deps.
    let extra = match p.platform {
        // esp-hal's `#[main]` + esp-backtrace's panic handler + esp-bootloader's
        // `esp_app_desc!()`. The chip feature (`esp32c3` / `esp32s3`) gates each.
        PlatformKind::Esp32 => {
            let chip = p.chip.as_deref().unwrap_or("esp32c3");
            format!(
                "esp-hal = {{ version = \"~1.0.0\", features = [\"{chip}\", \"unstable\"] }}\n\
                 esp-backtrace = {{ version = \"~0.18.0\", features = [\"{chip}\", \"panic-handler\", \"println\"] }}\n\
                 esp-bootloader-esp-idf = {{ version = \"~0.4.0\", features = [\"{chip}\"] }}\n",
            )
        }
        // defmt's `timestamp!` + panic-probe's panic handler + defmt-rtt.
        PlatformKind::Stm32 => {
            "panic-probe = { version = \"0.3\", features = [\"print-defmt\"] }\n\
             defmt = \"0.3\"\n\
             defmt-rtt = \"0.4\"\n"
                .to_string()
        }
        // Cortex-M `no_std` panic handler + QEMU semihosting exit.
        PlatformKind::Freertos | PlatformKind::BareMetal => {
            "panic-semihosting = { version = \"0.6\", features = [\"exit\"] }\n".to_string()
        }
        // Posix has no board crate — the C-port platform shim is the dep.
        PlatformKind::Posix => format!(
            "nros-platform-cffi = {{ path = \"{}\", default-features = false, features = [\"posix-c-port\"] }}\n",
            path_for_template(&workspace.join("packages/core/nros-platform-cffi")),
        ),
        // zephyr-lang-rust integration: the `zephyr` crate (logger, kconfig,
        // POSIX shims). Kernel / RMW / nros C runtime link at the CMake layer.
        // Phase 248 C5c — crate-less zephyr has no board crate to carry the
        // platform, so it brings `nros-platform[platform-zephyr]` directly
        // (resolves `ConcretePlatform`/the cffi link anchor); the `nros`
        // umbrella stays platform-agnostic.
        PlatformKind::Zephyr => format!(
            "zephyr = \"0.1.0\"\nlog = \"0.4\"\n\
             nros-platform = {{ path = \"{}\", default-features = false, features = [\"{}\"] }}\n",
            path_for_template(&workspace.join("packages/core/nros-platform")),
            p.platform_feature,
        ),
        _ => String::new(),
    };
    format!("{board_line}{extra}")
}

/// Canonical RMW name (`zenoh` / `xrce` / `cyclonedds`) from any accepted
/// spelling. Delegates to the single alias table in `rmw_resolver` (Phase
/// 227.3 — one source of truth, no drift).
fn normalize_rmw(rmw: &str) -> Option<&'static str> {
    crate::orchestration::rmw_resolver::canonical_rmw(rmw)
}

/// Phase 173.5 — the set of canonical RMW backends the build links: the
/// union of every `[[transport]].rmw` (falling back to `build.rmw` when
/// a transport omits it), deduped. With no transports declared it is
/// just `build.rmw` — so single-RMW builds are byte-identical.
fn rmw_set(build: &PlanBuildOptions) -> Vec<&'static str> {
    let mut set: Vec<&'static str> = Vec::new();
    let mut raw: Vec<&str> = if build.transports.is_empty() {
        vec![build.rmw.as_str()]
    } else {
        build
            .transports
            .iter()
            .map(|t| t.rmw.as_deref().unwrap_or(build.rmw.as_str()))
            .collect()
    };
    // Phase 255 Wave 5 — fold in the cross-RMW `[[bridge]]` link set
    // (`system.toml`), the SSoT replacement for the `[[transport]].rmw` overlay
    // multi-RMW path. Empty ⇒ no change ⇒ single-RMW build byte-identical.
    raw.extend(build.bridged_rmws.iter().map(String::as_str));
    for r in raw {
        if let Some(n) = normalize_rmw(r)
            && !set.contains(&n)
        {
            set.push(n);
        }
    }
    set
}

/// Cargo dep line(s) for one canonical RMW backend.
fn render_one_backend(
    workspace: &Path,
    build: &PlanBuildOptions,
    rmw: &str,
    safety: bool,
) -> String {
    match rmw {
        "zenoh" => format!(
            "nros-rmw-zenoh = {{ path = \"{}\", default-features = false, features = {} }}\n",
            path_for_template(&workspace.join("packages/zpico/nros-rmw-zenoh")),
            toml_string_array(&backend_features(build, "zenoh", safety)),
        ),
        "xrce" => format!(
            "nros-rmw-xrce-cffi = {{ path = \"{}\", default-features = false, features = {} }}\n",
            path_for_template(&workspace.join("packages/xrce/nros-rmw-xrce-cffi")),
            toml_string_array(&backend_features(build, "xrce", safety)),
        ),
        // Phase 169 (nano-ros 2026-05-19) — dust-dds retired; the
        // generic "dds" / "rmw-dds" / "rmw-dds-cffi" tokens are no
        // longer wired up. Cyclone is the DDS backend and is
        // selected via "cyclonedds" only (see nano-ros Phase 169.5).
        "cyclonedds" => "# Cyclone DDS is a CMake/C++ project — no Rust shim crate.\n\
             # Consumers select it via NANO_ROS_RMW=cyclonedds at the CMake\n\
             # layer (nros-c / nros-cpp). The generated Cargo.toml leaves\n\
             # the DDS slot empty; the staticlib is linked into the binary\n\
             # by the CMake glue alongside `corrosion_link_libraries`.\n"
            .to_string(),
        _ => String::new(),
    }
}

fn render_backend_dependencies(options: &GenerateOptions, plan: &NrosPlan) -> String {
    let Some(workspace) = workspace_from_nros_path(&options.nros_path) else {
        return String::new();
    };
    // Phase 173.5 — emit a dep for every RMW the transports bind to
    // (bridge mode links 2+). Single-RMW (or no `[[transport]]`) emits
    // exactly one, byte-identical to before.
    let safety = plan.safety.is_some();
    rmw_set(&plan.build)
        .iter()
        .map(|rmw| render_one_backend(&workspace, &plan.build, rmw, safety))
        .collect()
}

fn workspace_from_nros_path(nros_path: &Path) -> Option<PathBuf> {
    nros_path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map(Path::to_path_buf)
}

fn backend_features(build: &PlanBuildOptions, backend: &str, safety: bool) -> Vec<String> {
    let mut features = Vec::new();
    if uses_std(build) {
        features.push("std".to_string());
    }
    if let Some(platform) = platform_feature(build) {
        features.push(platform.to_string());
    }
    // Phase 126.M4 — `link-tcp` / `link-udp-unicast` feature gates were
    // deleted from zpico-sys (CLAUDE.md "Key Patterns": "vendor always
    // compiles those transports; locator picks at runtime"). nros-rmw-zenoh
    // now only exposes `link-tls` + `link-custom`. Plain TCP/UDP is
    // unconditional — no per-backend feature needed.
    //
    // Phase 250 (issue 0072) — a declared `[safety]` axis must reach the
    // BACKEND's own feature (the CRC attach on publish + validate on receive live
    // there), not just `nros/safety-e2e` on the entry — Cargo features don't
    // propagate upward. The capability registry (phase-252) is the SSoT for the
    // feature name + which backends carry it (zenoh today; xrce / cyclonedds have
    // no such feature, so the axis no-ops there).
    if safety {
        let cap = crate::orchestration::capability("safety").expect("safety capability");
        if cap.backend_supports(backend) {
            features.push(
                cap.backend_feature
                    .expect("safety backend feature")
                    .to_string(),
            );
        }
    }
    features
}

fn write_if_changed(path: &Path, contents: &str) -> Result<()> {
    if fs::read_to_string(path).ok().as_deref() == Some(contents) {
        return Ok(());
    }
    fs::write(path, contents).wrap_err_with(|| format!("failed to write {}", path.display()))
}

fn load_plan(path: &Path) -> Result<NrosPlan> {
    let raw =
        fs::read_to_string(path).wrap_err_with(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&raw).wrap_err_with(|| format!("failed to parse {}", path.display()))
}

fn generated_default_features(
    build: &PlanBuildOptions,
    managed_lifecycle: bool,
    param_persistence: bool,
    param_services: bool,
    safety: bool,
    has_bridges: bool,
) -> Vec<String> {
    let mut features = Vec::new();
    if uses_std(build) {
        features.push("std".to_string());
    }
    // Phase 172 — a `[[bridge]]` plan's generated `register_bridges` uses the
    // `nros-bridge` origin codec (`nros::bridge::{parse,encode}_bridge_origin`),
    // gated behind `nros/bridge`.
    if has_bridges {
        features.push("nros/bridge".to_string());
    }
    // Phase 172.A — a `[lifecycle]` plan needs the REP-2002 services on the
    // executor (`nros/lifecycle-services` → `nros-node/lifecycle-services`).
    if managed_lifecycle {
        features.push("nros/lifecycle-services".to_string());
    }
    // Parameter services (`nros/param-services`) — the declarative runtime
    // lazily registers the 6 ROS 2 param services on the first `declare_parameter`
    // (node_runtime `EntityKind::Parameter`), so external tools can query/update
    // a node's parameters. Pulled by either:
    //   * Phase 172.H — a `[param_persistence]` plan (also attaches the store), or
    //   * Phase 250 (Wave 3) — a declared `[param_services]` axis: the param
    //     SERVER on its own (no persistence). The user writes normal ROS
    //     `declare_parameter`/`get_parameter` in node source; this axis only
    //     toggles whether the external query/update services are compiled in.
    // The `nros/<feat>` umbrella-feature name comes from the capability registry
    // (phase-252 — the lowering SSoT, so RMW + the capability axes share one
    // table). `[param_persistence]` also pulls the param server (it additionally
    // attaches a store).
    if param_persistence || param_services {
        let feat = crate::orchestration::capability("param_services")
            .expect("param_services capability")
            .nros_feature;
        features.push(format!("nros/{feat}"));
    }
    // Phase 250 (Wave 1) — a declared `[safety]` block compiles the E2E
    // message-integrity capability (CRC + sequence gap/dup) into the generated
    // entry via its `nros/<feat>` umbrella feature. Kept a compile feature
    // (not a runtime always-on path) so embedded only pays the arena/CRC code
    // size when the axis is selected.
    if safety {
        let feat = crate::orchestration::capability("safety")
            .expect("safety capability")
            .nros_feature;
        features.push(format!("nros/{feat}"));
    }
    // Phase 173.2 — `nros/<feature>` + the per-platform local aliases
    // (which gate the platform-specific Cargo deps + cfg) both come from
    // the single `profile()` descriptor. (Since Phase 173.2b the
    // `src/main.rs` entry is selected by `render_main` from
    // `profile().entry`, not by these feature aliases.) ESP32/STM32
    // carry only their own alias (`platform-esp32-qemu` /
    // `platform-stm32`), NOT the `platform-bare-metal` alias.
    if let Some(p) = profile(build) {
        // Phase 248 C5c — the `nros` umbrella is fully platform-AGNOSTIC: it
        // carries only the `platform-cffi`/`rmw-cffi` vtable, never a
        // `platform-X` feature. Concrete platform selection lowers to a
        // dependency, not an `nros` feature:
        //   * board-backed entries  → the board crate brings
        //     `nros-platform/platform-X` (C5a/C5b);
        //   * crate-less posix      → `nros-platform-cffi[posix-c-port]`
        //     (emitted by `render_platform_dependencies`) supplies the
        //     `nros_platform_*` symbols; nros-node selects the impl at runtime;
        //   * crate-less zephyr/orin → a direct `nros-platform[platform-X]`
        //     dep (also emitted by `render_platform_dependencies`).
        // So nothing pushes `nros/platform-X` here anymore.
        for alias in &p.local_aliases {
            features.push(alias.to_string());
        }
    }
    if uses_rmw_cffi(&build.rmw) {
        features.push("nros/rmw-cffi".to_string());
        features.push("nros-orchestration/rmw-cffi".to_string());
        if let Some(rmw) = rmw_backend_feature(&build.rmw) {
            features.push(format!("nros/{rmw}"));
        }
    }
    for feature in build
        .features
        .iter()
        .filter_map(|feature| generated_feature(feature))
    {
        features.push(feature);
    }
    dedup(features)
}

fn uses_std(build: &PlanBuildOptions) -> bool {
    // Zephyr (zephyr-lang-rust) is no_std + alloc even on `native_sim`, whose
    // Cargo target triple is a host (`x86_64-*-linux`). Without this guard the
    // triple heuristic below wrongly enables `std`, and the generated entry
    // crate fails to build against the no_std Zephyr target (`can't find crate
    // for std`). Other RTOS sims (e.g. ThreadX-on-Linux) are genuinely
    // Linux-hosted processes and keep the triple-driven `std`.
    if matches!(
        profile(build).map(|p| p.entry_kind),
        Some(EntryKind::ZephyrStaticlib)
    ) {
        return false;
    }
    matches!(build.board.as_str(), "native" | "posix")
        || build.target.contains("linux")
        || build.target.contains("darwin")
        || build.target.contains("apple")
        || build.target.contains("windows")
        || build.target.contains("freebsd")
}

/// W.5.6 — true if the plan has ≥1 std rust executable instance, i.e. the tick
/// machinery (`TICK_ENTRIES` + `GenActionExec` + `run_tick_loop`) is emitted and
/// the spin entrypoints route through the manual spin-once + tick loop.
fn has_shared_instance(plan: &NrosPlan) -> bool {
    uses_std(&plan.build)
        && plan.instances.iter().any(|instance| {
            rust_executable_component_path(plan, instance).is_some()
                && instance_has_executable_callback(instance)
        })
}

/// W.5.11 — true if the plan has ≥1 **no_std** rust executable action server, so
/// the no_std action execution path (`tick_{idx}` module fns + `GenActionExec` +
/// `run_tick_loop_nostd`) is emitted and the no_std spin entrypoints route
/// through it.
fn has_no_std_action(plan: &NrosPlan) -> bool {
    !uses_std(&plan.build)
        && plan.instances.iter().any(|instance| {
            rust_executable_component_path(plan, instance).is_some()
                && instance.nodes.iter().any(|node| {
                    node.entities
                        .iter()
                        .any(|e| matches!(e, PlanEntity::ActionServer { .. }))
                })
        })
}

// ============================================================================
// Phase 195.C — board profile resolved from workspace descriptors
// ----------------------------------------------------------------------------
// `profile()` no longer hardcodes any board layout. It loads every
// `<workspace>/packages/boards/*/nros-board.toml` (via `BoardCatalog`) and
// resolves the `(board, target)` pair to a `BoardDescriptor`. The render
// bodies read crate names, paths, the `.cargo/config.toml` body and the
// entry-point template from that data, so adding a board with an existing
// platform kind is a descriptor file with no CLI edit.
// ============================================================================

/// Resolve a build's `(board, target)` to its board descriptor, loading the
/// catalog from the workspace recorded on `build` (set at generate time).
/// `None` when the workspace is unknown or no descriptor matches.
fn profile(build: &PlanBuildOptions) -> Option<BoardDescriptor> {
    let workspace = build.workspace_root.as_ref()?;
    let catalog = BoardCatalog::load(workspace).ok()?;
    catalog.resolve(&build.board, &build.target).cloned()
}

fn platform_feature(build: &PlanBuildOptions) -> Option<String> {
    profile(build).map(|p| p.platform_feature)
}

fn generated_feature(feature: &str) -> Option<String> {
    // Phase 126.M4 — `nros/rmw-{zenoh,xrce,dds}-cffi` feature names were
    // dropped in Phase 128.C ("RMW-blind init + drop rmw-*-cffi features").
    // Backend selection now happens at link time via the linker-section
    // walker inside `Executor::open`, driven by the per-backend `path`
    // dep in the generated Cargo.toml. The generator collapses the old
    // per-RMW `cffi` feature aliases to plain `nros/rmw-cffi` (the only
    // C-FFI feature `nros` still exposes).
    match feature {
        "std" => Some("std".to_string()),
        "rmw-cffi"
        | "rmw-zenoh"
        | "rmw-zenoh-cffi"
        | "rmw-xrce"
        | "rmw-xrce-cffi"
        | "rmw-cyclonedds"
        | "rmw-cyclonedds-cffi" => Some("nros/rmw-cffi".to_string()),
        feature if feature.starts_with("nros/") || feature.starts_with("nros-orchestration/") => {
            Some(feature.to_string())
        }
        _ => None,
    }
}

fn uses_rmw_cffi(rmw: &str) -> bool {
    !matches!(rmw, "" | "none")
}

fn rmw_backend_feature(rmw: &str) -> Option<&'static str> {
    // Phase 126.M4 — see `generated_feature`. Per-RMW `cffi` features
    // collapsed to the single `rmw-cffi` umbrella; backend dispatch is
    // section-walker based.
    match rmw {
        "zenoh" | "rmw-zenoh" | "rmw-zenoh-cffi" => Some("rmw-cffi"),
        "xrce" | "rmw-xrce" | "rmw-xrce-cffi" => Some("rmw-cffi"),
        "cyclonedds" | "rmw-cyclonedds" | "rmw-cyclonedds-cffi" => Some("rmw-cffi"),
        "cffi" | "rmw-cffi" => None,
        "" | "none" => None,
        _ => None,
    }
}

fn dedup(features: Vec<String>) -> Vec<String> {
    features
        .into_iter()
        .fold(Vec::new(), |mut deduped, feature| {
            if !deduped.contains(&feature) {
                deduped.push(feature);
            }
            deduped
        })
}

fn toml_string_array(values: &[String]) -> String {
    let entries = values
        .iter()
        .map(|value| format!("{:?}", value))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{entries}]")
}

fn render_generated_tables(plan: &NrosPlan) -> String {
    let schema = format!("nano-ros/plan/v{}", plan.version);
    let callback_count = plan
        .instances
        .iter()
        .map(|instance| instance.callbacks.len())
        .sum::<usize>();
    let max_nodes = plan
        .instances
        .iter()
        .map(|instance| instance.nodes.len())
        .sum::<usize>();
    let max_sched_contexts = plan.sched_contexts.len() + 1;
    let max_parameters = plan
        .instances
        .iter()
        .map(|instance| instance.parameters.len())
        .sum::<usize>();
    let max_interfaces = plan.interfaces.len();

    let mut out = String::new();
    out.push_str("#[allow(unused_imports)]\n");
    out.push_str("use nros_orchestration::{CallbackBindingSpec, CapacitySpec, ComponentLanguage, NodeSpec, PlanId, SchedClassSpec, SchedContextSpec, SystemSpec};\n");
    out.push_str("#[allow(unused_imports)]\n");
    out.push_str("use nros_orchestration::{CallbackHandleTable, ComponentSpec, InstanceSpec, ParameterSpec, ParameterValue};\n");
    out.push_str("#[allow(unused_imports)]\n");
    out.push_str("use nros_orchestration::{DeadlinePolicySpec, PrioritySpec};\n\n");
    out.push_str(&format!(
        "pub const CALLBACK_COUNT: usize = {callback_count};\n"
    ));
    out.push_str(&format!(
        "pub const SCHED_CONTEXT_COUNT: usize = {};\n\n",
        plan.sched_contexts.len()
    ));
    // Phase 173.5 — the locator from the first `[[transport]]` that
    // declares one. The board entry prefers it over the board
    // `Config`'s default; hosted entries keep using env (ZENOH_LOCATOR)
    // so runtime override still works. `None` ⇒ no transport locator ⇒
    // board/env default unchanged.
    let transport_locator = plan
        .build
        .transports
        .iter()
        .find_map(|t| t.locator.as_deref());
    out.push_str(&format!(
        "pub const TRANSPORT_LOCATOR: ::core::option::Option<&str> = {};\n\n",
        match transport_locator {
            Some(loc) => format!("::core::option::Option::Some({loc:?})"),
            None => "::core::option::Option::None".to_string(),
        }
    ));
    // Multi-session builds bake a `SESSION_SPECS` table consumed by
    // `Executor::open_multi`; single-session builds use `Executor::open` and
    // never reference it. Two sources: bridge mode (≥2 transports — one spec
    // per transport's rmw+locator+domain, Phase 173.5) and multi-domain mode
    // (≥2 distinct node domains — one spec per domain, same rmw+locator,
    // Phase 172.K.5).
    if let Some(domains) = multi_domain_sessions(plan) {
        let rmw = normalize_rmw(&plan.build.rmw).unwrap_or(plan.build.rmw.as_str());
        let locator = plan
            .build
            .transports
            .iter()
            .find_map(|t| t.locator.as_deref())
            .unwrap_or("");
        out.push_str(&format!(
            "pub static SESSION_SPECS: [nros::SessionSpec<'static>; {}] = [\n",
            domains.len()
        ));
        for d in &domains {
            out.push_str(&format!(
                "    nros::SessionSpec::new({rmw:?}, {locator:?}).domain_id({d}),\n"
            ));
        }
        out.push_str("];\n\n");
    } else if plan.build.is_bridge() {
        out.push_str(&format!(
            "pub static SESSION_SPECS: [nros::SessionSpec<'static>; {}] = [\n",
            plan.build.transports.len()
        ));
        for t in &plan.build.transports {
            let rmw = t.rmw.as_deref().unwrap_or(plan.build.rmw.as_str());
            let canonical = normalize_rmw(rmw).unwrap_or(rmw);
            let locator = t.locator.as_deref().unwrap_or("");
            // Phase 172 WP-B — a transport's `domain` joins its session to a
            // distinct ROS domain (multi-domain in-binary); absent ⇒ default 0.
            let domain = match t.domain {
                Some(d) => format!(".domain_id({d})"),
                None => String::new(),
            };
            out.push_str(&format!(
                "    nros::SessionSpec::new({canonical:?}, {locator:?}){domain},\n"
            ));
        }
        out.push_str("];\n\n");
    }
    // Phase 173.5 — write the nros.toml transport IP / baud into the
    // board `Config` (NanoRosOwned). The board entry calls this on a
    // `Config::default()` before `run`, so `init_hardware` brings up the
    // NIC / UART with the configured values.
    if emits_transport_config_override(plan) {
        out.push_str("pub fn apply_transport_config<C: nros::BoardTransportConfig>(c: &mut C) {\n");
        for call in transport_config_setter_calls(&plan.build) {
            out.push_str(&call);
            out.push('\n');
        }
        out.push_str("}\n\n");
    }
    render_backend_register_fn(&mut out, plan);
    render_lifecycle_fn(&mut out, plan);
    render_param_persistence_fn(&mut out, plan);
    render_native_component_ffi(&mut out, plan);
    render_components(&mut out, plan);
    render_instances(&mut out, plan);
    render_nodes(&mut out, plan);
    render_parameters(&mut out, plan);
    out.push_str(&format!(
        "pub static SCHED_CONTEXTS: [SchedContextSpec; {}] = [\n",
        plan.sched_contexts.len()
    ));
    for sc in &plan.sched_contexts {
        out.push_str(&render_sched_context(sc));
    }
    out.push_str("];\n\n");
    let bindings = collect_callback_bindings(plan);
    out.push_str(&format!(
        "pub static CALLBACK_BINDINGS: [CallbackBindingSpec; {}] = [\n",
        bindings.len()
    ));
    for (callback_index, sched_context_index) in bindings {
        out.push_str(&format!(
            "    CallbackBindingSpec {{ callback_index: {callback_index}, sched_context_index: {sched_context_index} }},\n"
        ));
    }
    out.push_str("];\n\n");
    out.push_str(&format!(
        "pub static SYSTEM: SystemSpec = SystemSpec {{ schema: {schema:?}, plan_id: PlanId({plan_id}), capacities: CapacitySpec {{ max_nodes: {max_nodes}, max_callbacks: {callback_count}, max_sched_contexts: {max_sched_contexts}, max_parameters: {max_parameters}, max_interfaces: {max_interfaces} }}, components: &COMPONENTS, instances: &INSTANCES, nodes: &NODES, parameters: &PARAMETERS, sched_contexts: &SCHED_CONTEXTS, callback_bindings: &CALLBACK_BINDINGS }};\n\n",
        plan_id = stable_plan_id(plan),
    ));
    out.push_str("struct GeneratedNodeRuntime<'a> {\n");
    out.push_str("    executor: &'a mut nros::Executor,\n");
    out.push_str("    instance: &'static InstanceSpec,\n");
    out.push_str("}\n\n");
    out.push_str("impl nros::ComponentNodeRuntime for GeneratedNodeRuntime<'_> {\n");
    out.push_str(
        "    type NodeHandle = <nros::Executor as nros::ComponentNodeRuntime>::NodeHandle;\n\n",
    );
    out.push_str("    fn build_component_node(&mut self, id: nros::NodeId<'_>, options: nros::NodeOptions<'_>) -> nros::ComponentResult<Self::NodeHandle> {\n");
    out.push_str("        let planned = NODES.iter().find(|node| node.instance_id == self.instance.id && node.source_node == id.as_str());\n");
    out.push_str(
        "        let name = planned.map(|node| node.node_name).unwrap_or(options.name);\n",
    );
    out.push_str("        let namespace = planned.map(|node| node.namespace).unwrap_or(options.namespace);\n");
    out.push_str("        let domain_id = planned.and_then(|node| node.domain_id).unwrap_or(options.domain_id);\n");
    if multi_domain_sessions(plan).is_some() {
        // Phase 172.K.5 — route the node to the session opened for its domain
        // (one SESSION_SPEC per distinct domain); fall back to the primary.
        out.push_str("        let session_idx = SESSION_SPECS.iter().position(|s| s.domain_id == domain_id).unwrap_or(0) as u8;\n");
        out.push_str("        self.executor.node_builder(name).namespace(namespace).session_idx(session_idx).build().map_err(|_| nros::ComponentError::Runtime)\n");
    } else {
        out.push_str("        self.executor.node_builder(name).namespace(namespace).domain_id(domain_id).build().map_err(|_| nros::ComponentError::Runtime)\n");
    }
    out.push_str("    }\n");
    out.push_str("}\n\n");
    // Subscriber placeholders now use the Phase 189 builder closure
    // (no C-fn-ptr noop). Services/actions keep their C-fn-ptr noops
    // until their builders land (M3).
    out.push_str("#[allow(dead_code)]\nunsafe extern \"C\" fn noop_raw_service(_req: *const u8, _req_len: usize, _resp: *mut u8, _resp_cap: usize, resp_len: *mut usize, _context: *mut core::ffi::c_void) -> bool {\n");
    out.push_str("    if !resp_len.is_null() { unsafe { *resp_len = 0; } }\n");
    out.push_str("    true\n");
    out.push_str("}\n");
    out.push_str("#[allow(dead_code)]\nunsafe extern \"C\" fn noop_raw_goal(_goal_id: *const nros::GoalId, _goal_data: *const u8, _goal_len: usize, _context: *mut core::ffi::c_void) -> nros::GoalResponse { nros::GoalResponse::AcceptAndDefer }\n");
    out.push_str("#[allow(dead_code)]\nunsafe extern \"C\" fn noop_raw_cancel(_goal_id: *const nros::GoalId, _status: nros::GoalStatus, _context: *mut core::ffi::c_void) -> nros::CancelResponse { nros::CancelResponse::Rejected }\n");
    out.push_str("#[allow(dead_code)]\nunsafe extern \"C\" fn noop_raw_accepted(_goal_id: *const nros::GoalId, _context: *mut core::ffi::c_void) {}\n\n");
    // W.5.6 — tick machinery (std, emitted only when the plan has a rust
    // executable instance). `TICK_ENTRIES` holds one boxed tick closure per
    // shared instance (populated during registration); `run_tick_loop` drains it
    // each spin. `GenActionExec` is the runtime `ActionExecutor` the tick `ctx`
    // delegates complete-goal / publish-feedback to, resolving the action handle
    // by the source entity id and driving it through the live executor.
    // `TICK_ENTRIES` (std boxed closures) is the std tick registry only.
    if has_shared_instance(plan) {
        out.push_str(
            "thread_local! {\n    \
             static TICK_ENTRIES: ::core::cell::RefCell<::std::vec::Vec<::std::boxed::Box<dyn FnMut(&mut nros::Executor)>>> = ::core::cell::RefCell::new(::std::vec::Vec::new());\n\
             }\n\n",
        );
    }
    // `GenActionExec` is the runtime `ActionExecutor` both the std tick closures
    // and the no_std `tick_{idx}` delegate to (no std-only constructs, so it is
    // shared by both paths).
    if has_shared_instance(plan) || has_no_std_action(plan) {
        // M-F.4.a — the `executor` slot is `*mut Executor` (not `&mut`) so this
        // backend can coexist with `GenClientDispatch` (which also needs mutable
        // executor access) inside the same `TickCtx`. The substrate `TickCtx`
        // serializes calls between the two `&mut dyn` backends — no reentrant
        // access — and the tick closure drops both backends together, so the
        // pointer never outlives the executor borrow.
        out.push_str(
            "struct GenActionExec<'e> {\n    \
             executor: *mut nros::Executor,\n    \
             handles: &'e [(&'static str, nros::ActionServerRawHandle)],\n\
             }\n",
        );
        out.push_str("impl GenActionExec<'_> {\n");
        out.push_str(
            "    fn handle(&self, action_entity: &str) -> nros::ComponentResult<nros::ActionServerRawHandle> {\n        \
             self.handles.iter().find(|(e, _)| *e == action_entity).map(|(_, h)| *h).ok_or(nros::ComponentError::Runtime)\n    \
             }\n}\n",
        );
        out.push_str("impl nros::ActionExecutor for GenActionExec<'_> {\n");
        out.push_str(
            "    fn complete_goal_raw(&mut self, action_entity: &str, goal_id: &nros::GoalId, status: nros::GoalStatus, result: &[u8]) -> nros::ComponentResult<()> {\n        \
             let handle = self.handle(action_entity)?;\n        \
             let executor = unsafe { &mut *self.executor };\n        \
             handle.complete_goal_raw(executor, goal_id, status, result);\n        \
             Ok(())\n    \
             }\n",
        );
        out.push_str(
            "    fn publish_feedback_raw(&mut self, action_entity: &str, goal_id: &nros::GoalId, feedback: &[u8]) -> nros::ComponentResult<()> {\n        \
             let handle = self.handle(action_entity)?;\n        \
             let executor = unsafe { &mut *self.executor };\n        \
             handle.publish_feedback_raw(executor, goal_id, feedback).map_err(|_| nros::ComponentError::Runtime)\n    \
             }\n",
        );
        out.push_str(
            "    fn for_each_active_goal(&self, action_entity: &str, visit: &mut dyn FnMut(&nros::GoalId, nros::GoalStatus)) {\n        \
             if let Ok(handle) = self.handle(action_entity) {\n            \
             let executor = unsafe { &*self.executor };\n            \
             handle.for_each_active_goal(executor, |g| visit(&g.goal_id, g.status));\n        \
             }\n    \
             }\n}\n\n",
        );
    }
    // M-F.4.a — `GenClientDispatch` is the runtime `ClientDispatch` the std tick
    // closures delegate service-client `call_raw` + action-client `send_goal_raw`
    // to. Mirrors `GenActionExec`: resolves the client handle by source entity id,
    // drives it through the live executor. Tick-only (clients need `&mut Executor`).
    //
    // The substrate `TickCtx::new(pubs, actions, clients)` holds both action +
    // client backends as `&mut dyn` simultaneously, so they can't both hold
    // `&mut Executor`. We stash the executor as `*mut Executor` (raw pointer)
    // shared by reference between the two; each method reborrows it as
    // `&mut Executor` inside, and the substrate `TickCtx` API serializes calls
    // (no reentrancy: a `call_raw` runs to completion before `complete_goal_raw`
    // gets a chance, etc.). Tick closure builds both backends + drops them at
    // scope end, so the pointer never outlives the executor.
    if has_shared_instance(plan) {
        out.push_str(
            "struct GenClientDispatch<'e> {\n    \
             executor: *mut nros::Executor,\n    \
             services: &'e [(&'static str, nros::HandleId)],\n    \
             actions: &'e [(&'static str, usize)],\n\
             }\n",
        );
        out.push_str("impl GenClientDispatch<'_> {\n");
        out.push_str(
            "    fn service(&self, entity: &str) -> nros::ComponentResult<nros::HandleId> {\n        \
             self.services.iter().find(|(e, _)| *e == entity).map(|(_, h)| *h).ok_or(nros::ComponentError::Runtime)\n    \
             }\n",
        );
        out.push_str(
            "    fn action_entry(&self, entity: &str) -> nros::ComponentResult<usize> {\n        \
             self.actions.iter().find(|(e, _)| *e == entity).map(|(_, ei)| *ei).ok_or(nros::ComponentError::Runtime)\n    \
             }\n}\n",
        );
        out.push_str("impl nros::component::ClientDispatch for GenClientDispatch<'_> {\n");
        // M-F.4.a — service-client `call_raw`: send the request through the
        // arena's `RmwServiceClient`, then loop `spin_once` + `try_recv_reply_raw`
        // until the reply lands or we exhaust the iteration cap. Mirrors the
        // (deprecated) `ServiceClientTrait::call_raw` shape but routes the wait
        // through the executor so callbacks on other handles keep dispatching.
        out.push_str(
            "    fn call_raw(&mut self, service_entity: &str, request_cdr: &[u8], response_buf: &mut [u8]) -> nros::ComponentResult<usize> {\n        \
             let hid = self.service(service_entity)?;\n        \
             use nros::ServiceClientTrait;\n        \
             {\n            \
             let executor = unsafe { &mut *self.executor };\n            \
             let entry = unsafe { executor.service_client_entry_mut(hid.0) }.ok_or(nros::ComponentError::Runtime)?;\n            \
             entry.handle.send_request_raw(request_cdr).map_err(|_| nros::ComponentError::Runtime)?;\n        \
             }\n        \
             // Bounded wait — caps total time to keep the tick loop responsive.\n        \
             for _ in 0..200 {\n            \
             let executor = unsafe { &mut *self.executor };\n            \
             executor.spin_once(::core::time::Duration::from_millis(10));\n            \
             let entry = unsafe { executor.service_client_entry_mut(hid.0) }.ok_or(nros::ComponentError::Runtime)?;\n            \
             match entry.handle.try_recv_reply_raw(response_buf) {\n                \
             Ok(Some(len)) => return Ok(len),\n                \
             Ok(None) => continue,\n                \
             Err(_) => return Err(nros::ComponentError::Runtime),\n            \
             }\n        \
             }\n        \
             Err(nros::ComponentError::Runtime)\n    \
             }\n",
        );
        out.push_str(
            "    fn send_goal_raw(&mut self, action_entity: &str, goal_cdr: &[u8]) -> nros::ComponentResult<nros::GoalId> {\n        \
             let entry_index = self.action_entry(action_entity)?;\n        \
             let executor = unsafe { &mut *self.executor };\n        \
             let core = unsafe { executor.action_client_core_mut(entry_index) }.ok_or(nros::ComponentError::Runtime)?;\n        \
             core.send_goal_raw(goal_cdr).map_err(|_| nros::ComponentError::Runtime)\n    \
             }\n}\n\n",
        );
    }
    out.push_str("pub fn instantiate_components(executor: &mut nros::Executor, handles: &mut CallbackHandleTable<CALLBACK_COUNT>) -> Result<(), nros::NodeError> {\n");
    out.push_str("    for instance in INSTANCES.iter() {\n");
    out.push_str("        let mut node_runtime = GeneratedNodeRuntime { executor, instance };\n");
    out.push_str("        let mut runtime = nros::ComponentRuntimeAdapter::<_, MAX_NODES, MAX_ENTITIES, CALLBACK_COUNT>::new(&mut node_runtime);\n");
    out.push_str("        match instance.component_id {\n");
    for component in &plan.components {
        if matches!(component.language.as_str(), "rust" | "Rust") {
            if let Some(path) = rust_component_type_path(&component.id) {
                out.push_str(&format!(
                    "            {id:?} => nros::register_component::<{path}>(&mut runtime).map_err(|_| nros::NodeError::NotInitialized)?,\n",
                    id = component.id,
                ));
            }
        } else {
            let fn_name = native_register_fn_name(&component.id);
            out.push_str(&format!(
                "            {id:?} => unsafe {{ {fn_name}(&mut node_runtime) }}?,\n",
                id = component.id,
            ));
        }
    }
    out.push_str("            _ => return Err(nros::NodeError::NotInitialized),\n");
    out.push_str("        }\n");
    out.push_str("    }\n");
    out.push_str("    instantiate_callback_handles(executor, handles)?;\n");
    out.push_str("    Ok(())\n");
    out.push_str("}\n");
    let regs = render_callback_registrations(plan);
    // W.5.11 — module-level items the no_std action tick path needs (ctx struct +
    // `static mut`s + decision trampolines + `tick_{idx}`), emitted at module
    // scope so `run_tick_loop` can reach them.
    for item in &regs.module_items {
        out.push_str(item);
    }
    out.push_str("\nfn instantiate_callback_handles(executor: &mut nros::Executor, handles: &mut CallbackHandleTable<CALLBACK_COUNT>) -> Result<(), nros::NodeError> {\n");
    for line in &regs.inline {
        out.push_str(line);
    }
    out.push_str("    Ok(())\n");
    out.push_str("}\n");
    render_entry_lib_fns(&mut out, plan, &regs.no_std_tick_idxs);
    out
}

/// Phase 172 WP-B — emit the generated entry lib's Rust-native API: the
/// `build_executor` openers and `register_all`. These are the units the
/// entry-lib C ABI wraps; the per-platform entry (and a future vendor C caller)
/// only calls them + spins.
///
/// * `build_executor(config)` — register backends + `Executor::open(config)`.
/// * `build_executor_bridge()` — backends + `Executor::open_multi(SESSION_SPECS)`
///   (emitted only in bridge mode).
/// * `register_all(executor)` — the full post-open wiring on an already-opened
///   executor: sched contexts → instantiate components → bind callbacks →
///   lifecycle → parameter persistence.
fn render_entry_lib_fns(out: &mut String, plan: &NrosPlan, no_std_tick_idxs: &[usize]) {
    out.push_str(
        "\npub fn build_executor(config: &nros::ExecutorConfig<'_>) -> Result<nros::Executor, nros::NodeError> {\n",
    );
    out.push_str("    register_backends();\n");
    out.push_str("    nros::Executor::open(config)\n");
    out.push_str("}\n");
    // Emitted for any multi-session build — a bridge (≥2 transports) or
    // multi-domain (≥2 distinct node domains, Phase 172.K.5).
    if is_multi_session(plan) {
        out.push_str(
            "\npub fn build_executor_bridge() -> Result<nros::Executor, nros::NodeError> {\n",
        );
        out.push_str("    register_backends();\n");
        out.push_str("    nros::Executor::open_multi(&SESSION_SPECS)\n");
        out.push_str("}\n");
    }
    render_register_all_fn(out, plan);
    render_register_bridges_fn(out, plan);
    // W.5.6 — the manual spin-once + per-instance tick loop (std). The spin
    // entrypoints route through this (instead of plain `spin_blocking`) so each
    // spin runs every component's `tick` between dispatch, where the executor is
    // free for action complete/feedback ops. Registration must run first
    // (`register_all` populates `TICK_ENTRIES`).
    if has_shared_instance(plan) {
        out.push_str(
            "\npub fn run_tick_loop(executor: &mut nros::Executor) -> Result<(), nros::NodeError> {\n",
        );
        out.push_str("    loop {\n");
        out.push_str("        if executor.is_halted() {\n            break;\n        }\n");
        out.push_str("        executor.spin_once(::core::time::Duration::from_millis(10));\n");
        out.push_str("        TICK_ENTRIES.with(|__t| {\n");
        out.push_str("            for __entry in __t.borrow_mut().iter_mut() {\n");
        out.push_str("                __entry(executor);\n");
        out.push_str("            }\n");
        out.push_str("        });\n");
        out.push_str("    }\n");
        out.push_str("    Ok(())\n");
        out.push_str("}\n");
    }
    // W.5.11 — the no_std action execution loop. `is_halted`/`spin_blocking` are
    // std-only and there's no `thread_local`/alloc, so this is an infinite
    // `spin_once` + per-action `tick_{idx}` loop (mirrors `spin_default`'s
    // never-returns shape); each `tick_{idx}` reads its module-level `static mut`
    // ctx + handle and drives feedback/result via `GenActionExec`.
    if !no_std_tick_idxs.is_empty() {
        out.push_str("\npub fn run_tick_loop_nostd(executor: &mut nros::Executor) -> ! {\n");
        out.push_str("    loop {\n");
        out.push_str("        executor.spin_once(::core::time::Duration::from_millis(50));\n");
        for idx in no_std_tick_idxs {
            out.push_str(&format!("        tick_{idx}(executor);\n"));
        }
        out.push_str("    }\n");
        out.push_str("}\n");
    }
}

/// Emit `register_all` (see [`render_entry_lib_fns`]).
fn render_register_all_fn(out: &mut String, plan: &NrosPlan) {
    out.push_str(
        "\npub fn register_all(executor: &mut nros::Executor) -> Result<(), nros::NodeError> {\n",
    );
    out.push_str("    let mut callback_handles = CallbackHandleTable::<CALLBACK_COUNT>::new();\n");
    out.push_str(
        "    let mut sched_context_ids = [executor.default_sched_context_id(); SCHED_CONTEXT_COUNT + 1];\n",
    );
    out.push_str("    for (index, spec) in SCHED_CONTEXTS.iter().copied().enumerate() {\n");
    out.push_str(
        "        sched_context_ids[index + 1] = executor.create_sched_context(spec.to_nros_node())?;\n",
    );
    out.push_str("    }\n");
    out.push_str("    instantiate_components(executor, &mut callback_handles)?;\n");
    out.push_str("    for binding in CALLBACK_BINDINGS.iter().copied() {\n");
    out.push_str(
        "        let handle = callback_handles.get(binding.callback_index).ok_or(nros::NodeError::NotInitialized)?;\n",
    );
    out.push_str(
        "        let sched_context = sched_context_ids.get(binding.sched_context_index).copied().ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n",
    );
    out.push_str("        executor.bind_handle_to_sched_context(handle, sched_context)?;\n");
    out.push_str("    }\n");
    out.push_str("    apply_lifecycle(executor)?;\n");
    out.push_str("    apply_param_persistence(executor)?;\n");
    if !plan.bridges.is_empty() {
        out.push_str("    register_bridges(executor)?;\n");
    }
    out.push_str("    Ok(())\n");
    out.push_str("}\n");
}

/// Resolve a forwarded topic's interface (type name + hash) from the plan's
/// declared entities — the topic must be a publisher/subscriber `resolved_name`
/// somewhere in the plan (the "resolve from interfaces" model). `None` ⇒ the
/// topic isn't declared by any component, which [`validate_bridges`] rejects.
fn resolve_topic_interface<'a>(
    plan: &'a NrosPlan,
    topic: &str,
) -> Option<&'a super::schema::InterfaceRef> {
    plan.instances
        .iter()
        .flat_map(|instance| instance.nodes.iter())
        .flat_map(|node| node.entities.iter())
        .find_map(|entity| match entity {
            super::plan::PlanEntity::Publisher {
                resolved_name,
                interface,
                ..
            }
            | super::plan::PlanEntity::Subscriber {
                resolved_name,
                interface,
                ..
            } if resolved_name == topic => Some(interface),
            _ => None,
        })
}

/// Map a bridge endpoint to its `SESSION_SPECS` slot index. SESSION_SPECS for a
/// bridge build is one entry per `plan.build.transports` (same order); an
/// endpoint matches the transport with the same canonical rmw + domain +
/// locator. `None` ⇒ the endpoint names a session the build didn't open.
fn bridge_endpoint_session_idx(
    plan: &NrosPlan,
    ep: &super::plan::PlanBridgeEndpoint,
) -> Option<usize> {
    let ep_rmw = normalize_rmw(&ep.rmw).unwrap_or(ep.rmw.as_str());
    plan.build.transports.iter().position(|t| {
        let t_rmw = t.rmw.as_deref().unwrap_or(plan.build.rmw.as_str());
        let t_rmw = normalize_rmw(t_rmw).unwrap_or(t_rmw);
        t_rmw == ep_rmw
            && t.domain.unwrap_or(0) == ep.domain
            && t.locator.as_deref().unwrap_or("") == ep.locator.as_deref().unwrap_or("")
    })
}

/// Validate every `[[bridge]]` resolves before codegen: each forwarded topic
/// must be a declared interface, and each `connect` endpoint must map to an
/// opened session. Returns a clear deploy-time error otherwise (the infallible
/// renderer then re-resolves with the same helpers, guaranteed to succeed).
fn validate_bridges(plan: &NrosPlan) -> Result<()> {
    for bridge in &plan.bridges {
        if bridge.connect.len() < 2 {
            bail!(
                "bridge `{}` connects {} session(s); a bridge needs ≥2",
                bridge.name,
                bridge.connect.len()
            );
        }
        for ep in &bridge.connect {
            if bridge_endpoint_session_idx(plan, ep).is_none() {
                bail!(
                    "bridge `{}` endpoint (rmw={}, domain={}, locator={:?}) matches no opened \
                     session — declare a matching `[[transport]]` in the bridge build",
                    bridge.name,
                    ep.rmw,
                    ep.domain,
                    ep.locator
                );
            }
        }
        for topic in &bridge.topics {
            if resolve_topic_interface(plan, topic).is_none() {
                bail!(
                    "bridge `{}` forwards topic `{}`, but no component declares it — the type is \
                     resolved from the plan's interfaces (wildcards are not supported)",
                    bridge.name,
                    topic
                );
            }
        }
    }
    Ok(())
}

/// Emit `register_bridges` — the Phase 172 topic-forwarding relay. For each
/// `[[bridge]]`: one bridge node per endpoint session (bound via
/// `session_idx`), then per forwarded topic, per ordered endpoint pair `(i→j)`,
/// a generic publisher on `j` plus a generic+`MessageInfo` subscription on `i`
/// whose callback re-publishes through it. Echo/loop suppression: every forward
/// stamps the bridge's `bridge_origin`; a subscription drops samples already
/// carrying its own bridge's origin (the `nros-bridge` codec). The node-centric
/// builder is the Phase 189.M1 surface; resolution is pre-checked by
/// [`validate_bridges`], so the `expect`s here cannot fire.
fn render_register_bridges_fn(out: &mut String, plan: &NrosPlan) {
    if plan.bridges.is_empty() {
        return;
    }
    out.push_str(
        "\npub fn register_bridges(executor: &mut nros::Executor) -> Result<(), nros::NodeError> {\n",
    );
    for (bi, bridge) in plan.bridges.iter().enumerate() {
        out.push_str(&format!("    // bridge {:?}\n", bridge.name));
        out.push_str(&format!(
            "    let origin_b{bi}: &[u8] = {:?}.as_bytes();\n",
            bridge.name
        ));
        // One bridge node per endpoint session.
        for (ei, ep) in bridge.connect.iter().enumerate() {
            let idx = bridge_endpoint_session_idx(plan, ep)
                .expect("validate_bridges checked endpoint→session");
            let node_name = format!("{}_ep{ei}", bridge.name);
            out.push_str(&format!(
                "    let node_b{bi}_ep{ei} = executor.node_builder({node_name:?}).session_idx({idx}).build()?;\n"
            ));
        }
        for (ti, topic) in bridge.topics.iter().enumerate() {
            let interface =
                resolve_topic_interface(plan, topic).expect("validate_bridges checked topic");
            let ty = interface_type_name(interface);
            let hash = interface_type_hash(interface);
            out.push_str(&format!("    // topic {topic:?} : {ty} / {hash}\n"));
            let n = bridge.connect.len();
            for i in 0..n {
                for j in 0..n {
                    if i == j {
                        continue;
                    }
                    let pub_var = format!("pub_b{bi}_{i}_{j}_t{ti}");
                    out.push_str(&format!(
                        "    let {pub_var} = executor.node_mut(node_b{bi}_ep{j}).publisher({topic:?}).generic({ty:?}, {hash:?}).build()?;\n"
                    ));
                    out.push_str(&format!(
                        "    executor.node_mut(node_b{bi}_ep{i}).subscription({topic:?}).generic({ty:?}, {hash:?}).message_info().build(move |payload: &[u8], info: &nros::RawMessageInfo| {{\n"
                    ));
                    out.push_str(&format!(
                        "        if let Some(o) = nros::bridge::parse_bridge_origin(info.attachment()) {{ if o == origin_b{bi} {{ return; }} }}\n"
                    ));
                    out.push_str("        let mut att = [0u8; 64];\n");
                    out.push_str(&format!(
                        "        let n = nros::bridge::encode_bridge_origin(origin_b{bi}, &mut att);\n"
                    ));
                    out.push_str(&format!(
                        "        let _ = {pub_var}.publish_raw_with_attachment(payload, &att[..n]);\n"
                    ));
                    out.push_str("    })?;\n");
                }
            }
        }
    }
    out.push_str("    Ok(())\n");
    out.push_str("}\n");
}

fn render_native_component_ffi(out: &mut String, plan: &NrosPlan) {
    let native_components = plan
        .components
        .iter()
        .filter(|component| !matches!(component.language.as_str(), "rust" | "Rust"))
        .collect::<Vec<_>>();
    if native_components.is_empty() {
        return;
    }

    out.push_str("use core::ffi::{c_char, c_void, CStr};\n\n");
    out.push_str("#[repr(C)]\nstruct NrosCComponentNodeOptions { name: *const c_char, namespace_: *const c_char, domain_id: u32 }\n");
    out.push_str("#[repr(C)]\nstruct NrosCComponentNode { stable_id: *const c_char, runtime_handle: *mut c_void, context: *mut NrosCComponentContext }\n");
    out.push_str("#[repr(C)]\nstruct NrosCComponentEntityDescriptor { stable_id: *const c_char, node_id: *const c_char, kind: i32, source_name: *const c_char, type_name: *const c_char, type_hash: *const c_char, callback_id: *const c_char }\n");
    out.push_str("#[repr(C)]\nstruct NrosCComponentContextOps { create_node: Option<unsafe extern \"C\" fn(*mut c_void, *const c_char, *const NrosCComponentNodeOptions, *mut NrosCComponentNode) -> i32>, create_entity: Option<unsafe extern \"C\" fn(*mut c_void, *const NrosCComponentEntityDescriptor) -> i32>, record_callback_effect: Option<unsafe extern \"C\" fn(*mut c_void, *const c_char, i32, *const c_char) -> i32> }\n");
    out.push_str("#[repr(C)]\nstruct NrosCComponentContext { user_data: *mut c_void, ops: *const NrosCComponentContextOps }\n\n");
    out.push_str("const NROS_RET_OK: i32 = 0;\nconst NROS_RET_INVALID_ARGUMENT: i32 = -3;\n\n");
    out.push_str("static NROS_C_COMPONENT_OPS: NrosCComponentContextOps = NrosCComponentContextOps { create_node: Some(nros_c_component_create_node), create_entity: Some(nros_c_component_create_entity), record_callback_effect: Some(nros_c_component_record_callback_effect) };\n\n");
    out.push_str("unsafe extern \"C\" fn nros_c_component_create_node(user_data: *mut c_void, stable_id: *const c_char, options: *const NrosCComponentNodeOptions, out_node: *mut NrosCComponentNode) -> i32 {\n");
    out.push_str("    if user_data.is_null() || stable_id.is_null() || options.is_null() || out_node.is_null() { return NROS_RET_INVALID_ARGUMENT; }\n");
    out.push_str(
        "    let runtime = unsafe { &mut *(user_data as *mut GeneratedNodeRuntime<'_>) };\n",
    );
    out.push_str("    let stable_id = match unsafe { c_str_to_str(stable_id) } { Some(value) => value, None => return NROS_RET_INVALID_ARGUMENT };\n");
    out.push_str("    let options = unsafe { &*options };\n");
    out.push_str("    if options.name.is_null() || options.namespace_.is_null() { return NROS_RET_INVALID_ARGUMENT; }\n");
    out.push_str("    let name = match unsafe { c_str_to_str(options.name) } { Some(value) => value, None => return NROS_RET_INVALID_ARGUMENT };\n");
    out.push_str("    let namespace = match unsafe { c_str_to_str(options.namespace_) } { Some(value) => value, None => return NROS_RET_INVALID_ARGUMENT };\n");
    out.push_str("    let options = nros::NodeOptions::new(name).namespace(namespace).domain_id(options.domain_id);\n");
    out.push_str("    match nros::ComponentNodeRuntime::build_component_node(runtime, nros::NodeId(stable_id), options) { Ok(_) => { unsafe { (*out_node).stable_id = core::ptr::null(); (*out_node).runtime_handle = core::ptr::null_mut(); (*out_node).context = core::ptr::null_mut(); } NROS_RET_OK }, Err(_) => NROS_RET_INVALID_ARGUMENT }\n");
    out.push_str("}\n\n");
    out.push_str("unsafe extern \"C\" fn nros_c_component_create_entity(_user_data: *mut c_void, _descriptor: *const NrosCComponentEntityDescriptor) -> i32 { NROS_RET_OK }\n");
    out.push_str("unsafe extern \"C\" fn nros_c_component_record_callback_effect(_user_data: *mut c_void, _callback_id: *const c_char, _kind: i32, _entity_id: *const c_char) -> i32 { NROS_RET_OK }\n\n");
    out.push_str("unsafe fn c_str_to_str<'a>(ptr: *const c_char) -> Option<&'a str> { unsafe { CStr::from_ptr(ptr) }.to_str().ok() }\n\n");
    out.push_str("unsafe extern \"C\" {\n");
    for component in &native_components {
        out.push_str(&format!(
            "    #[link_name = {symbol:?}]\n    fn {fn_name}(context: *mut NrosCComponentContext) -> i32;\n",
            symbol = component.component,
            fn_name = native_symbol_fn_name(&component.id),
        ));
    }
    out.push_str("}\n\n");
    for component in &native_components {
        out.push_str(&format!(
            "unsafe fn {fn_name}(runtime: &mut GeneratedNodeRuntime<'_>) -> Result<(), nros::NodeError> {{\n    let mut context = NrosCComponentContext {{ user_data: runtime as *mut _ as *mut c_void, ops: &NROS_C_COMPONENT_OPS }};\n    let status = unsafe {{ {symbol_fn}(&mut context) }};\n    if status == NROS_RET_OK {{ Ok(()) }} else {{ Err(nros::NodeError::NotInitialized) }}\n}}\n\n",
            fn_name = native_register_fn_name(&component.id),
            symbol_fn = native_symbol_fn_name(&component.id),
        ));
    }
}

fn render_backend_register_fn(out: &mut String, plan: &NrosPlan) {
    out.push_str("pub fn register_backends() {\n");
    // Phase 173.5 — register every RMW the transports bind to (bridge
    // mode registers 2+ before `Executor::open_multi`). Single-RMW emits
    // one call, byte-identical.
    for rmw in rmw_set(&plan.build) {
        match rmw {
            "zenoh" => out.push_str("    let _ = nros_rmw_zenoh::register();\n"),
            "xrce" => out.push_str("    let _ = nros_rmw_xrce_cffi::register();\n"),
            // Cyclone DDS is a CMake/C++ project with no Rust shim;
            // registration happens through the C ABI at the CMake layer
            // (NANO_ROS_RMW=cyclonedds). No Rust call emitted.
            _ => {}
        }
    }
    out.push_str("}\n\n");
}

/// Phase 172.A — emit `apply_lifecycle`, called from `run_executor` after the
/// callbacks are bound. Unmanaged plans get a no-op (so the build needs no
/// `lifecycle-services` feature and stays byte-equivalent in behaviour); a
/// `[lifecycle]` plan registers the REP-2002 services on the executor and
/// drives the node to its boot `autostart` state.
fn render_lifecycle_fn(out: &mut String, plan: &NrosPlan) {
    out.push_str(
        "pub fn apply_lifecycle(executor: &mut nros::Executor) -> Result<(), nros::NodeError> {\n",
    );
    match &plan.lifecycle {
        None => {
            out.push_str("    let _ = executor;\n    Ok(())\n");
        }
        Some(lifecycle) => {
            out.push_str("    executor.register_lifecycle_services()?;\n");
            // Drive the boot autostart policy on the freshly-registered state
            // machine. No transition callbacks are registered, so each
            // transition takes the default-success path (REP-2002 skeleton);
            // component-provided transition hooks are a later increment.
            let transitions: &[&str] = match lifecycle.autostart {
                LifecycleAutostart::None => &[],
                LifecycleAutostart::Configure => &["Configure"],
                LifecycleAutostart::Active => &["Configure", "Activate"],
            };
            if !transitions.is_empty() {
                out.push_str("    if let Some(sm) = executor.lifecycle_state_machine_mut() {\n");
                out.push_str("        unsafe {\n");
                for t in transitions {
                    out.push_str(&format!(
                        "            let _ = sm.trigger_transition(nros::LifecycleTransition::{t});\n",
                    ));
                }
                out.push_str("        }\n    }\n");
            }
            out.push_str("    Ok(())\n");
        }
    }
    out.push_str("}\n\n");
}

/// Phase 172.H — emit `apply_param_persistence`, called from `run_executor`
/// after `apply_lifecycle`. Plans without `[param_persistence]` get a no-op (no
/// param services, byte-equivalent). A `[param_persistence]` plan registers the
/// parameter services, declares the plan's parameters as defaults, then attaches
/// the persistence backend (which overlays any persisted overrides at boot and
/// flushes runtime `set_parameters` changes from the spin loop).
fn render_param_persistence_fn(out: &mut String, plan: &NrosPlan) {
    out.push_str(
        "pub fn apply_param_persistence(executor: &mut nros::Executor) -> Result<(), nros::NodeError> {\n",
    );
    match &plan.param_persistence {
        None => {
            out.push_str("    let _ = executor;\n    Ok(())\n");
        }
        Some(pp) if pp.backend == "file" => {
            out.push_str("    executor.register_parameter_services()?;\n");
            out.push_str("    for spec in PARAMETERS.iter() {\n");
            out.push_str("        let value = match spec.value {\n");
            out.push_str("            ParameterValue::Bool(b) => nros::ParameterValue::Bool(b),\n");
            out.push_str(
                "            ParameterValue::I64(i) => nros::ParameterValue::Integer(i),\n",
            );
            out.push_str(
                "            ParameterValue::F64(f) => nros::ParameterValue::Double(f),\n",
            );
            out.push_str(
                "            ParameterValue::Str(s) => nros::ParameterValue::from_string(s).unwrap_or_default(),\n",
            );
            out.push_str("        };\n");
            out.push_str("        executor.declare_parameter(spec.name, value);\n");
            out.push_str("    }\n");
            out.push_str(&format!(
                "    executor.enable_parameter_persistence_with(nros::FileParamStore::new({:?}))?;\n",
                pp.path
            ));
            out.push_str("    Ok(())\n");
        }
        Some(pp) => {
            // Only the hosted file backend exists today; an unknown backend is
            // a config error surfaced at build time rather than silently
            // dropping persistence.
            out.push_str(&format!(
                "    let _ = executor;\n    compile_error!(\"unsupported param_persistence backend: {}\");\n    #[allow(unreachable_code)] Ok(())\n",
                pp.backend.escape_default()
            ));
        }
    }
    out.push_str("}\n\n");
}

fn render_components(out: &mut String, plan: &NrosPlan) {
    out.push_str(&format!(
        "pub static COMPONENTS: [ComponentSpec; {}] = [\n",
        plan.components.len()
    ));
    for component in &plan.components {
        out.push_str(&format!(
            "    ComponentSpec {{ id: {id:?}, package: {package:?}, symbol: {symbol:?}, language: ComponentLanguage::{language} }},\n",
            id = component.id,
            package = component.package,
            symbol = component.component,
            language = component_language(&component.language),
        ));
    }
    out.push_str("];\n\n");
}

fn render_instances(out: &mut String, plan: &NrosPlan) {
    out.push_str(&format!(
        "pub static INSTANCES: [InstanceSpec; {}] = [\n",
        plan.instances.len()
    ));
    let mut parameter_start = 0usize;
    for instance in &plan.instances {
        let parameter_len = instance.parameters.len();
        let node_name = instance
            .nodes
            .first()
            .map(|node| node.resolved_name.as_str())
            .unwrap_or(instance.launch_name.as_str());
        out.push_str(&format!(
            "    InstanceSpec {{ id: {id:?}, component_id: {component:?}, node_name: {node_name:?}, namespace: {namespace:?}, domain_id: None, parameter_start: {parameter_start}, parameter_len: {parameter_len} }},\n",
            id = instance.id,
            component = instance.component,
            namespace = instance.namespace,
        ));
        parameter_start += parameter_len;
    }
    out.push_str("];\n\n");
}

/// Phase 172.K.5 — the distinct ROS domains the plan's nodes span (sorted),
/// when there is more than one (so a session per domain is needed). `None` ⇒
/// a single domain, or a bridge build (whose sessions come from transports
/// instead). A node with no `domain_id` uses the default domain `0`.
fn multi_domain_sessions(plan: &NrosPlan) -> Option<Vec<u32>> {
    if plan.build.is_bridge() {
        return None;
    }
    let mut domains: Vec<u32> = plan
        .instances
        .iter()
        .flat_map(|instance| instance.nodes.iter())
        .map(|node| node.domain_id.unwrap_or(0))
        .collect();
    domains.sort_unstable();
    domains.dedup();
    (domains.len() > 1).then_some(domains)
}

/// A build opens multiple RMW sessions (`SESSION_SPECS` + `open_multi`) when it
/// bridges (≥2 transports) or spans ≥2 ROS domains (Phase 172.K.5).
fn is_multi_session(plan: &NrosPlan) -> bool {
    plan.build.is_bridge() || multi_domain_sessions(plan).is_some()
}

fn render_nodes(out: &mut String, plan: &NrosPlan) {
    let node_count = plan
        .instances
        .iter()
        .map(|instance| instance.nodes.len())
        .sum::<usize>();
    out.push_str(&format!("pub const MAX_NODES: usize = {node_count};\n"));
    let max_entities = plan
        .instances
        .iter()
        .map(|instance| {
            instance
                .nodes
                .iter()
                .map(|node| node.entities.len())
                .sum::<usize>()
        })
        .max()
        .unwrap_or(0);
    out.push_str(&format!(
        "pub const MAX_ENTITIES: usize = {max_entities};\n"
    ));
    out.push_str(&format!("pub static NODES: [NodeSpec; {node_count}] = [\n"));
    for instance in &plan.instances {
        for node in &instance.nodes {
            let node_name = final_node_name(&node.resolved_name, &node.namespace);
            // Phase 172.K.5 — carry the node's assigned domain (from a
            // `[[domain]]` group); `build_component_node` maps it to the right
            // `SESSION_SPECS` slot for multi-domain builds.
            let domain_id = match node.domain_id {
                Some(d) => format!("Some({d})"),
                None => "None".to_string(),
            };
            out.push_str(&format!(
                "    NodeSpec {{ instance_id: {instance_id:?}, node_id: {node_id:?}, source_node: {source_node:?}, node_name: {node_name:?}, namespace: {namespace:?}, domain_id: {domain_id} }},\n",
                instance_id = instance.id,
                node_id = node.id,
                source_node = node.source_node,
                namespace = node.namespace,
            ));
        }
    }
    out.push_str("];\n\n");
}

fn render_parameters(out: &mut String, plan: &NrosPlan) {
    let rendered_parameters = plan
        .instances
        .iter()
        .flat_map(|instance| {
            instance.parameters.iter().filter_map(move |parameter| {
                render_parameter_value(&parameter.value).map(|value| {
                    format!(
                        "    ParameterSpec {{ instance_id: {instance_id:?}, name: {name:?}, value: {value} }},\n",
                        instance_id = instance.id,
                        name = parameter.name,
                    )
                })
            })
        })
        .collect::<Vec<_>>();
    out.push_str(&format!(
        "pub static PARAMETERS: [ParameterSpec; {}] = [\n",
        rendered_parameters.len()
    ));
    for parameter in rendered_parameters {
        out.push_str(&parameter);
    }
    out.push_str("];\n\n");
}

/// Output of [`render_callback_registrations`]: the inline body of
/// `instantiate_callback_handles`, plus (W.5.11) module-level items the no_std
/// action tick path needs (struct/statics/trampolines/tick fns can't be
/// fn-local, since the separate `run_tick_loop` must reach them) and the list of
/// no_std action tick-fn indices `run_tick_loop` drives.
#[derive(Default)]
struct CallbackRegistrations {
    inline: Vec<String>,
    module_items: Vec<String>,
    no_std_tick_idxs: Vec<usize>,
}

/// Phase 211.H — build the `QosSettings` expression for a generated
/// subscription, baking the plan's `qos_overrides` for `(topic, subscription)`
/// at GENERATION time. Generated entities go through the executor's
/// `register_subscription_*` path (not `NodeHandle::create_*`), which doesn't
/// consult the runtime override table — so the merge is done here in codegen.
/// Component-created entities (typed entry) use the runtime table instead. The
/// base matches the historical generated default (`default().keep_last(1)`);
/// each matching override appends the corresponding builder call. Unknown
/// policy/value pairs are skipped (forward-compat), never emitted as junk.
fn render_sub_qos_expr(instance: &PlanInstance, topic: &str) -> String {
    let mut expr = String::from("nros::QosSettings::default().keep_last(1)");
    for ovr in &instance.qos_overrides {
        if ovr.topic != topic || ovr.role != "subscription" {
            continue;
        }
        match (ovr.policy.as_str(), &ovr.value) {
            ("reliability", ParameterValue::String(s)) if s == "reliable" => {
                expr.push_str(".reliable()")
            }
            ("reliability", ParameterValue::String(s)) if s == "best_effort" => {
                expr.push_str(".best_effort()")
            }
            ("durability", ParameterValue::String(s)) if s == "volatile" => {
                expr.push_str(".volatile()")
            }
            ("durability", ParameterValue::String(s)) if s == "transient_local" => {
                expr.push_str(".transient_local()")
            }
            ("history", ParameterValue::String(s)) if s == "keep_all" => {
                expr.push_str(".keep_all()")
            }
            ("depth", ParameterValue::Integer(n)) if *n >= 0 => {
                expr.push_str(&format!(".keep_last({n})"));
            }
            _ => {}
        }
    }
    expr
}

fn render_callback_registrations(plan: &NrosPlan) -> CallbackRegistrations {
    let mut out = Vec::new();
    let mut module_items: Vec<String> = Vec::new();
    let mut no_std_tick_idxs: Vec<usize> = Vec::new();
    let mut callback_index = 0usize;
    let std_build = uses_std(&plan.build);
    for (inst_idx, instance) in plan.instances.iter().enumerate() {
        // W.5.7 — a rust executable component on a std target shares one `State`
        // across all of the instance's callbacks via `Rc<RefCell>` (the per-instance
        // shared prelude); no_std keeps the per-callback move/noop path (shared
        // no_std state needs a `'static`, tracked as W.5.8).
        let comp_path = rust_executable_component_path(plan, instance);
        let shared = std_build && comp_path.is_some();
        let shared_ticks = shared && instance_has_executable_callback(instance);
        if shared_ticks {
            emit_shared_prelude(
                &mut out,
                inst_idx,
                comp_path.as_deref().expect("shared implies rust component"),
                instance,
            );
        }
        // W.5.6 — (source entity id, action handle var) for this instance's
        // action servers, captured into the per-instance tick entry below.
        let mut inst_actions: Vec<(String, usize)> = Vec::new();
        for callback in &instance.callbacks {
            // The component matches the *source* CallbackId it declared
            // (`cb_timer`), not the plan-prefixed id.
            let cb_id = callback.source_callback.as_str();
            match find_callback_entity(instance, callback.id.as_str(), cb_id) {
                Some((_node_id, PlanEntity::Timer { period_ms, .. })) => {
                    if shared {
                        emit_shared_timer(
                            &mut out,
                            callback_index,
                            inst_idx,
                            comp_path.as_deref().unwrap(),
                            cb_id,
                            *period_ms,
                        );
                    } else if let Some(comp_path) = &comp_path {
                        emit_executable_timer(
                            &mut out,
                            callback_index,
                            comp_path,
                            cb_id,
                            *period_ms,
                            instance,
                        );
                    } else {
                        out.push(format!(
                            "    let handle_{callback_index} = executor.register_timer(nros::TimerDuration::from_millis({period_ms}), || {{}})?;\n"
                        ));
                        out.push(format!(
                            "    handles.set({callback_index}, handle_{callback_index}).map_err(|_| nros::NodeError::InvalidSchedContextBinding)?;\n"
                        ));
                    }
                }
                Some((
                    node_id,
                    PlanEntity::Subscriber {
                        resolved_name,
                        interface,
                        ..
                    },
                )) => {
                    if shared {
                        emit_shared_subscription(
                            &mut out,
                            callback_index,
                            inst_idx,
                            comp_path.as_deref().unwrap(),
                            cb_id,
                            node_id,
                            resolved_name,
                            &interface_type_name(interface),
                            &interface_type_hash(interface),
                            instance,
                        );
                    } else if let Some(comp_path) = &comp_path {
                        emit_executable_subscription(
                            &mut out,
                            callback_index,
                            comp_path,
                            cb_id,
                            node_id,
                            resolved_name,
                            &interface_type_name(interface),
                            &interface_type_hash(interface),
                            instance,
                        );
                    } else {
                        out.push(format!(
                            "    let node_{callback_index} = NODES.iter().find(|node| node.node_id == {node_id:?}).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
                        ));
                        out.push(format!(
                            "    let node_handle_{callback_index} = executor.node_id_by_name(node_{callback_index}.node_name, node_{callback_index}.namespace).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
                        ));
                        // Phase 211.H — bake qos_overrides for this topic+subscription.
                        let qos_expr = render_sub_qos_expr(instance, resolved_name);
                        out.push(format!(
                            "    let handle_{callback_index} = executor.node_mut(node_handle_{callback_index}).subscription({topic:?}).generic({type_name:?}, {type_hash:?}).qos({qos_expr}).rx_buffer::<1024>().build(|_data: &[u8]| {{}})?;\n",
                            topic = resolved_name,
                            type_name = interface_type_name(interface),
                            type_hash = interface_type_hash(interface),
                        ));
                        out.push(format!(
                            "    handles.set({callback_index}, handle_{callback_index}).map_err(|_| nros::NodeError::InvalidSchedContextBinding)?;\n"
                        ));
                    }
                }
                Some((
                    node_id,
                    PlanEntity::ServiceServer {
                        resolved_name,
                        interface,
                        ..
                    },
                )) => {
                    // Service bodies need a captured ctx (raw service callbacks
                    // can't close over one): std uses the shared `Box::leak`'d Rc
                    // ctx; no_std (W.5.8) a function-local `static mut`; native
                    // C/C++ keeps the noop.
                    if shared {
                        emit_shared_service(
                            &mut out,
                            callback_index,
                            inst_idx,
                            comp_path.as_deref().unwrap(),
                            cb_id,
                            node_id,
                            resolved_name,
                            &interface_type_name(interface),
                            &interface_type_hash(interface),
                        );
                    } else if let Some(comp_path) = &comp_path {
                        emit_static_service(
                            &mut out,
                            callback_index,
                            comp_path,
                            cb_id,
                            node_id,
                            resolved_name,
                            &interface_type_name(interface),
                            &interface_type_hash(interface),
                            instance,
                        );
                    } else {
                        out.push(format!(
                            "    let node_{callback_index} = NODES.iter().find(|node| node.node_id == {node_id:?}).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
                        ));
                        out.push(format!(
                            "    let node_handle_{callback_index} = executor.node_id_by_name(node_{callback_index}.node_name, node_{callback_index}.namespace).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
                        ));
                        out.push(format!(
                            "    let handle_{callback_index} = executor.register_service_raw_sized_on::<1024, 1024>(node_handle_{callback_index}, {service:?}, {type_name:?}, {type_hash:?}, nros::QosSettings::services_default(), noop_raw_service, core::ptr::null_mut())?;\n",
                            service = resolved_name,
                            type_name = interface_type_name(interface),
                            type_hash = interface_type_hash(interface),
                        ));
                        out.push(format!(
                            "    handles.set({callback_index}, handle_{callback_index}).map_err(|_| nros::NodeError::InvalidSchedContextBinding)?;\n"
                        ));
                    }
                }
                Some((
                    node_id,
                    PlanEntity::ActionServer {
                        source_entity,
                        resolved_name,
                        interface,
                        ..
                    },
                )) => {
                    // Goal/cancel decision bodies: std shared `Box::leak`'d ctx;
                    // no_std (W.5.8) a function-local `static mut`; native keeps the
                    // noop. Execution (feedback/result) rides the W.5.6 tick hook
                    // (std); no_std action execution is a follow-up.
                    if shared {
                        emit_shared_action(
                            &mut out,
                            callback_index,
                            inst_idx,
                            comp_path.as_deref().unwrap(),
                            cb_id,
                            node_id,
                            resolved_name,
                            &interface_type_name(interface),
                            &interface_type_hash(interface),
                        );
                        // Keep the action handle reachable from the tick entry so
                        // the component's `tick` can complete goals / publish
                        // feedback (keyed by the *source* entity id it declared).
                        inst_actions.push((source_entity.clone(), callback_index));
                    } else if let Some(comp_path) = &comp_path {
                        emit_static_action(
                            &mut out,
                            &mut module_items,
                            callback_index,
                            comp_path,
                            cb_id,
                            node_id,
                            resolved_name,
                            source_entity,
                            &interface_type_name(interface),
                            &interface_type_hash(interface),
                            instance,
                        );
                        // W.5.11 — drive this action's `tick` from the no_std loop.
                        no_std_tick_idxs.push(callback_index);
                    } else {
                        out.push(format!(
                            "    let node_{callback_index} = NODES.iter().find(|node| node.node_id == {node_id:?}).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
                        ));
                        out.push(format!(
                            "    let node_handle_{callback_index} = executor.node_id_by_name(node_{callback_index}.node_name, node_{callback_index}.namespace).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
                        ));
                        out.push(format!(
                            "    let action_{callback_index} = executor.register_action_server_raw_sized::<1024, 1024, 1024, 4>(nros::RawActionServerSpec {{ node_id: Some(node_handle_{callback_index}), action_name: {action:?}, type_name: {type_name:?}, type_hash: {type_hash:?}, qos: nros::QosSettings::services_default(), goal_callback: noop_raw_goal, cancel_callback: noop_raw_cancel, accepted_callback: Some(noop_raw_accepted), context: core::ptr::null_mut() }})?;\n",
                            action = resolved_name,
                            type_name = interface_type_name(interface),
                            type_hash = interface_type_hash(interface),
                        ));
                        out.push(format!(
                            "    handles.set({callback_index}, action_{callback_index}.handle_id()).map_err(|_| nros::NodeError::InvalidSchedContextBinding)?;\n"
                        ));
                    }
                }
                _ => {
                    out.push(format!(
                        "    return Err(nros::NodeError::NotInitialized); // unsupported generated callback: {:?}\n",
                        callback.id
                    ));
                }
            }
            callback_index += 1;
        }
        // M-F.4.a — register the instance's service-client + action-client
        // handles inline (after the callback loop, before the tick entry that
        // captures them). Only emitted on the std/shared path: clients live on
        // the executor + need `Rc<RefCell>`-style tick wiring. Each handle gets a
        // local var keyed by stable client entity id, fed into `GenClientDispatch`
        // via the `__tick_sclients_i{inst}` / `__tick_aclients_i{inst}` arrays.
        let mut inst_service_clients: Vec<(String, usize)> = Vec::new();
        let mut inst_action_clients: Vec<(String, usize)> = Vec::new();
        if shared_ticks {
            for (sc_idx, (source_entity, service_name, type_name, type_hash, node_id)) in
                instance_service_clients(instance).into_iter().enumerate()
            {
                out.push(format!(
                    "    let scnode_i{inst_idx}_{sc_idx} = NODES.iter().find(|node| node.node_id == {node_id:?}).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
                ));
                out.push(format!(
                    "    let scnh_i{inst_idx}_{sc_idx} = executor.node_id_by_name(scnode_i{inst_idx}_{sc_idx}.node_name, scnode_i{inst_idx}_{sc_idx}.namespace).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
                ));
                out.push(format!(
                    "    let scli_i{inst_idx}_{sc_idx} = executor.register_service_client_raw_sized_on::<1024>(scnh_i{inst_idx}_{sc_idx}, {service_name:?}, {type_name:?}, {type_hash:?}, nros::QosSettings::services_default(), None, core::ptr::null_mut())?;\n"
                ));
                inst_service_clients.push((source_entity, inst_service_clients.len()));
                // Re-key the local var name so the array can reference it. We use
                // a stable name pattern `scli_{key}` where key encodes inst+seq.
            }
            for (ac_idx, (source_entity, action_name, type_name, type_hash, node_id)) in
                instance_action_clients(instance).into_iter().enumerate()
            {
                out.push(format!(
                    "    let acnode_i{inst_idx}_{ac_idx} = NODES.iter().find(|node| node.node_id == {node_id:?}).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
                ));
                out.push(format!(
                    "    let acnh_i{inst_idx}_{ac_idx} = executor.node_id_by_name(acnode_i{inst_idx}_{ac_idx}.node_name, acnode_i{inst_idx}_{ac_idx}.namespace).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
                ));
                out.push(format!(
                    "    let acli_i{inst_idx}_{ac_idx} = executor.register_action_client_raw_sized::<1024, 1024, 1024>(nros::RawActionClientSpec {{ node_id: Some(acnh_i{inst_idx}_{ac_idx}), action_name: {action_name:?}, type_name: {type_name:?}, type_hash: {type_hash:?}, goal_response_callback: None, feedback_callback: None, result_callback: None, context: core::ptr::null_mut() }})?;\n"
                ));
                inst_action_clients.push((source_entity, inst_action_clients.len()));
            }
        }
        // W.5.6 — register the instance's per-spin tick entry (it shares the same
        // `Rc<RefCell<State>>` the callbacks mutate). Emitted for every shared
        // instance: action components drive feedback/result here, timer/sub-only
        // components may do periodic work (default `tick` is a no-op).
        if shared_ticks {
            emit_tick_entry(
                &mut out,
                inst_idx,
                comp_path.as_deref().expect("shared implies rust component"),
                &inst_actions,
                &inst_service_clients,
                &inst_action_clients,
            );
        }
    }
    CallbackRegistrations {
        inline: out,
        module_items,
        no_std_tick_idxs,
    }
}

/// W.5.6 + M-F.4.a — push the instance's tick closure into `TICK_ENTRIES`. The
/// closure shares the instance's `Rc<RefCell<State>>` + resolver, and owns three
/// arrays so `GenActionExec` / `GenClientDispatch` can resolve handles by source
/// entity id: action-server handles (`actions`), service-client handles
/// (`service_clients`), action-client handles (`action_clients`).
/// `run_tick_loop` invokes every entry each spin.
fn emit_tick_entry(
    out: &mut Vec<String>,
    inst: usize,
    comp_path: &str,
    actions: &[(String, usize)],
    service_clients: &[(String, usize)],
    action_clients: &[(String, usize)],
) {
    let n_act = actions.len();
    let arr_act = actions
        .iter()
        .map(|(entity, ci)| format!("({entity:?}, action_{ci})"))
        .collect::<Vec<_>>()
        .join(", ");
    out.push(format!(
        "    let __tick_actions_i{inst}: [(&'static str, nros::ActionServerRawHandle); {n_act}] = [{arr_act}];\n"
    ));
    let n_sc = service_clients.len();
    let arr_sc = service_clients
        .iter()
        .map(|(entity, ci)| format!("({entity:?}, scli_i{inst}_{ci})"))
        .collect::<Vec<_>>()
        .join(", ");
    out.push(format!(
        "    let __tick_sclients_i{inst}: [(&'static str, nros::HandleId); {n_sc}] = [{arr_sc}];\n"
    ));
    let n_ac = action_clients.len();
    let arr_ac = action_clients
        .iter()
        .map(|(entity, ci)| format!("({entity:?}, acli_i{inst}_{ci}.entry_index())"))
        .collect::<Vec<_>>()
        .join(", ");
    out.push(format!(
        "    let __tick_aclients_i{inst}: [(&'static str, usize); {n_ac}] = [{arr_ac}];\n"
    ));
    out.push("    {\n".to_string());
    out.push(format!(
        "        let __st_i{inst} = ::std::rc::Rc::clone(&state_i{inst});\n"
    ));
    out.push(format!(
        "        let __rv_i{inst} = ::std::rc::Rc::clone(&resolver_i{inst});\n"
    ));
    out.push(
        "        TICK_ENTRIES.with(|__t| __t.borrow_mut().push(::std::boxed::Box::new(move |__executor: &mut nros::Executor| {\n"
            .to_string(),
    );
    out.push(
        "            let __exec_ptr: *mut nros::Executor = __executor as *mut nros::Executor;\n"
            .to_string(),
    );
    out.push(format!(
        "            let mut __ae = GenActionExec {{ executor: __exec_ptr, handles: &__tick_actions_i{inst} }};\n"
    ));
    out.push(format!(
        "            let mut __cd = GenClientDispatch {{ executor: __exec_ptr, services: &__tick_sclients_i{inst}, actions: &__tick_aclients_i{inst} }};\n"
    ));
    out.push(format!(
        "            let mut __tc = nros::TickCtx::new(__rv_i{inst}.as_ref(), &mut __ae, &mut __cd);\n"
    ));
    out.push(format!(
        "            <{comp_path} as nros::ExecutableComponent>::tick(&mut *__st_i{inst}.borrow_mut(), &mut __tc);\n"
    ));
    out.push("        })));\n".to_string());
    out.push("    }\n".to_string());
}

/// W.5.3 — the rust component type path for an instance whose component is a
/// rust component (so it may impl `ExecutableComponent`); `None` for native
/// (C/C++) components, which keep the noop path.
fn rust_executable_component_path(plan: &NrosPlan, instance: &PlanInstance) -> Option<String> {
    let comp = plan
        .components
        .iter()
        .find(|c| c.id == instance.component)?;
    if !matches!(comp.language.as_str(), "rust" | "Rust") {
        return None;
    }
    rust_component_type_path(&comp.id)
}

/// M-F.4.a — every service-client entity declared across an instance's nodes,
/// as `(entity_id, service_name, type_name, type_hash, owning_node_id)`. These
/// are the service clients the tick `ClientDispatch::call_raw` can invoke.
fn instance_service_clients(
    instance: &PlanInstance,
) -> Vec<(String, String, String, String, String)> {
    let mut clients = Vec::new();
    for node in &instance.nodes {
        for entity in &node.entities {
            if let PlanEntity::ServiceClient {
                source_entity,
                resolved_name,
                interface,
                ..
            } = entity
            {
                // Key on the *source* entity id (`cli_add_two`), not the
                // plan-prefixed id (`adder_1/cli_add_two`): the component body
                // invokes via the `EntityId` it declared in `register`.
                clients.push((
                    source_entity.clone(),
                    resolved_name.clone(),
                    interface_type_name(interface),
                    interface_type_hash(interface),
                    node.id.clone(),
                ));
            }
        }
    }
    clients
}

/// M-F.4.a — every action-client entity declared across an instance's nodes,
/// as `(entity_id, action_name, type_name, type_hash, owning_node_id)`. These
/// are the action clients the tick `ClientDispatch::send_goal_raw` can invoke.
fn instance_action_clients(
    instance: &PlanInstance,
) -> Vec<(String, String, String, String, String)> {
    let mut clients = Vec::new();
    for node in &instance.nodes {
        for entity in &node.entities {
            if let PlanEntity::ActionClient {
                source_entity,
                resolved_name,
                interface,
                ..
            } = entity
            {
                clients.push((
                    source_entity.clone(),
                    resolved_name.clone(),
                    interface_type_name(interface),
                    interface_type_hash(interface),
                    node.id.clone(),
                ));
            }
        }
    }
    clients
}

/// W.5.3 — every publisher entity declared across an instance's nodes, as
/// `(entity_id, topic, type_name, type_hash, owning_node_id)`. These are the
/// publishers a callback's `CallbackCtx` can publish through.
fn instance_publishers(instance: &PlanInstance) -> Vec<(String, String, String, String, String)> {
    let mut pubs = Vec::new();
    for node in &instance.nodes {
        for entity in &node.entities {
            if let PlanEntity::Publisher {
                source_entity,
                resolved_name,
                interface,
                ..
            } = entity
            {
                // Key on the *source* entity id (`pub_chatter`), not the
                // plan-prefixed id (`talker_1/pub_chatter`): the component body
                // publishes via the `EntityId` it declared in `register`.
                pubs.push((
                    source_entity.clone(),
                    resolved_name.clone(),
                    interface_type_name(interface),
                    interface_type_hash(interface),
                    node.id.clone(),
                ));
            }
        }
    }
    pubs
}

/// Emit `struct Resolver{key}` + its `PublisherResolver` impl (publish_raw
/// dispatches on the source entity id). Shared by the per-callback prelude (no_std)
/// and the per-instance shared prelude (std). With no publishers the params are
/// prefixed to stay warning-clean.
fn emit_resolver_struct(
    out: &mut Vec<String>,
    key: &str,
    pubs: &[(String, String, String, String, String)],
) {
    let fields = pubs
        .iter()
        .enumerate()
        .map(|(i, _)| format!("p{i}: nros::EmbeddedRawPublisher"))
        .collect::<Vec<_>>()
        .join(", ");
    out.push(format!("    struct Resolver{key} {{ {fields} }}\n"));
    out.push(format!(
        "    impl nros::PublisherResolver for Resolver{key} {{\n"
    ));
    if pubs.is_empty() {
        out.push(
            "        fn publish_raw(&self, _entity_id: &str, _data: &[u8]) -> nros::ComponentResult<()> {\n"
                .to_string(),
        );
        out.push("            Err(nros::ComponentError::Runtime)\n        }\n    }\n".to_string());
    } else {
        out.push(
            "        fn publish_raw(&self, entity_id: &str, data: &[u8]) -> nros::ComponentResult<()> {\n"
                .to_string(),
        );
        out.push("            match entity_id {\n".to_string());
        for (i, (entity_id, ..)) in pubs.iter().enumerate() {
            out.push(format!(
                "                {entity_id:?} => self.p{i}.publish_raw(data).map_err(|_| nros::ComponentError::Runtime),\n"
            ));
        }
        out.push("                _ => Err(nros::ComponentError::Runtime),\n".to_string());
        out.push("            }\n        }\n    }\n".to_string());
    }
}

/// Emit the publisher builders `p{i}_{key}` for an instance's publishers.
fn emit_publisher_builders(
    out: &mut Vec<String>,
    key: &str,
    pubs: &[(String, String, String, String, String)],
) {
    for (i, (_entity_id, topic, type_name, type_hash, node_id)) in pubs.iter().enumerate() {
        out.push(format!(
            "    let pubnode_{key}_{i} = NODES.iter().find(|node| node.node_id == {node_id:?}).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
        ));
        out.push(format!(
            "    let pubnh_{key}_{i} = executor.node_id_by_name(pubnode_{key}_{i}.node_name, pubnode_{key}_{i}.namespace).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
        ));
        out.push(format!(
            "    let p{i}_{key} = executor.node_mut(pubnh_{key}_{i}).publisher({topic:?}).generic({type_name:?}, {type_hash:?}).build()?;\n"
        ));
    }
}

/// W.5.3 — per-callback prelude (no_std executable path): a `Resolver{idx}`
/// owning the instance's publishers + a fresh component `State`, both moved into
/// the timer/sub dispatch closure (no statics, no_std-clean). Each callback owns
/// its own state — the *std* path shares one state across callbacks via
/// `emit_shared_prelude` (W.5.7). `state_mut` controls the `state{idx}`
/// mutability: timer/sub capture by `move` and mutate in place, so the binding
/// must be `mut`.
fn emit_executable_prelude(
    out: &mut Vec<String>,
    idx: usize,
    comp_path: &str,
    instance: &PlanInstance,
    state_mut: bool,
) {
    let pubs = instance_publishers(instance);
    let key = idx.to_string();
    emit_resolver_struct(out, &key, &pubs);
    emit_publisher_builders(out, &key, &pubs);
    let init = pubs
        .iter()
        .enumerate()
        .map(|(i, _)| format!("p{i}: p{i}_{idx}"))
        .collect::<Vec<_>>()
        .join(", ");
    out.push(format!(
        "    let resolver{idx} = Resolver{idx} {{ {init} }};\n"
    ));
    let mut_kw = if state_mut { "mut " } else { "" };
    out.push(format!(
        "    let {mut_kw}state{idx} = <{comp_path} as nros::ExecutableComponent>::init();\n"
    ));
}

/// W.5.7 — per-instance shared prelude (std/alloc). Builds the instance's
/// publishers once into a `Resolveri{inst}`, then wraps both the resolver and the
/// component `State` in `Rc` so every callback on the instance shares one state:
/// timer/sub clone the `Rc` into their move-closure; service/action clone it into
/// the leaked ctx. `RefCell` gives interior mutability across the shared borrows
/// (the executor spins single-threaded, so the borrows never overlap).
fn emit_shared_prelude(
    out: &mut Vec<String>,
    inst: usize,
    comp_path: &str,
    instance: &PlanInstance,
) {
    let pubs = instance_publishers(instance);
    let key = format!("i{inst}");
    emit_resolver_struct(out, &key, &pubs);
    emit_publisher_builders(out, &key, &pubs);
    let init = pubs
        .iter()
        .enumerate()
        .map(|(i, _)| format!("p{i}: p{i}_i{inst}"))
        .collect::<Vec<_>>()
        .join(", ");
    out.push(format!(
        "    let resolver_i{inst} = ::std::rc::Rc::new(Resolveri{inst} {{ {init} }});\n"
    ));
    out.push(format!(
        "    let state_i{inst} = ::std::rc::Rc::new(::core::cell::RefCell::new(<{comp_path} as nros::ExecutableComponent>::init()));\n"
    ));
}

/// True if the instance has at least one timer/sub/service/action callback (so
/// the shared prelude's `state_i{inst}`/`resolver_i{inst}` are actually used).
fn instance_has_executable_callback(instance: &PlanInstance) -> bool {
    instance.callbacks.iter().any(|cb| {
        matches!(
            find_callback_entity(instance, cb.id.as_str(), cb.source_callback.as_str()),
            Some((
                _,
                PlanEntity::Timer { .. }
                    | PlanEntity::Subscriber { .. }
                    | PlanEntity::ServiceServer { .. }
                    | PlanEntity::ActionServer { .. }
            ))
        )
    })
}

/// W.5.3 — a timer callback that runs a real `ExecutableComponent` body (empty
/// payload). Single-callback-per-state model; shared state is a follow-up.
fn emit_executable_timer(
    out: &mut Vec<String>,
    idx: usize,
    comp_path: &str,
    callback_id: &str,
    period_ms: u64,
    instance: &PlanInstance,
) {
    emit_executable_prelude(out, idx, comp_path, instance, true);
    out.push(format!(
        "    let handle_{idx} = executor.register_timer(nros::TimerDuration::from_millis({period_ms}), move || {{\n"
    ));
    out.push(format!(
        "        let mut cb_ctx = nros::CallbackCtx::new(&[], &resolver{idx});\n"
    ));
    out.push(format!(
        "        <{comp_path} as nros::ExecutableComponent>::on_callback(&mut state{idx}, nros::Callback::__from_id(nros::CallbackId::new({callback_id:?})), &mut cb_ctx);\n"
    ));
    out.push("    })?;\n".to_string());
    out.push(format!(
        "    handles.set({idx}, handle_{idx}).map_err(|_| nros::NodeError::InvalidSchedContextBinding)?;\n"
    ));
}

/// W.5.3 — a subscription callback that runs a real `ExecutableComponent` body.
/// The build closure receives the message CDR `data`, which becomes the
/// `CallbackCtx` payload (`ctx.message::<M>()` in the body).
#[allow(clippy::too_many_arguments)]
fn emit_executable_subscription(
    out: &mut Vec<String>,
    idx: usize,
    comp_path: &str,
    callback_id: &str,
    node_id: &str,
    topic: &str,
    type_name: &str,
    type_hash: &str,
    instance: &PlanInstance,
) {
    out.push(format!(
        "    let subnode_{idx} = NODES.iter().find(|node| node.node_id == {node_id:?}).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
    ));
    out.push(format!(
        "    let subnh_{idx} = executor.node_id_by_name(subnode_{idx}.node_name, subnode_{idx}.namespace).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
    ));
    emit_executable_prelude(out, idx, comp_path, instance, true);
    // Phase 211.H — bake the plan's qos_overrides for this topic+subscription.
    let qos_expr = render_sub_qos_expr(instance, topic);
    out.push(format!(
        "    let handle_{idx} = executor.node_mut(subnh_{idx}).subscription({topic:?}).generic({type_name:?}, {type_hash:?}).qos({qos_expr}).rx_buffer::<1024>().build(move |data: &[u8]| {{\n"
    ));
    out.push(format!(
        "        let mut cb_ctx = nros::CallbackCtx::new(data, &resolver{idx});\n"
    ));
    out.push(format!(
        "        <{comp_path} as nros::ExecutableComponent>::on_callback(&mut state{idx}, nros::Callback::__from_id(nros::CallbackId::new({callback_id:?})), &mut cb_ctx);\n"
    ));
    out.push("    })?;\n".to_string());
    out.push(format!(
        "    handles.set({idx}, handle_{idx}).map_err(|_| nros::NodeError::InvalidSchedContextBinding)?;\n"
    ));
}

/// W.5.7 — a timer callback that shares the instance's `Rc<RefCell<State>>`
/// (`state_i{inst}`) + `Rc<Resolveri{inst}>`. Clones the `Rc`s into the
/// move-closure; dispatch borrows the shared state mutably (single-threaded spin
/// ⇒ no overlap).
fn emit_shared_timer(
    out: &mut Vec<String>,
    idx: usize,
    inst: usize,
    comp_path: &str,
    callback_id: &str,
    period_ms: u64,
) {
    out.push(format!(
        "    let state_cb{idx} = ::std::rc::Rc::clone(&state_i{inst});\n"
    ));
    out.push(format!(
        "    let resolver_cb{idx} = ::std::rc::Rc::clone(&resolver_i{inst});\n"
    ));
    out.push(format!(
        "    let handle_{idx} = executor.register_timer(nros::TimerDuration::from_millis({period_ms}), move || {{\n"
    ));
    out.push(format!(
        "        let mut cb_ctx = nros::CallbackCtx::new(&[], resolver_cb{idx}.as_ref());\n"
    ));
    out.push(format!(
        "        <{comp_path} as nros::ExecutableComponent>::on_callback(&mut *state_cb{idx}.borrow_mut(), nros::Callback::__from_id(nros::CallbackId::new({callback_id:?})), &mut cb_ctx);\n"
    ));
    out.push("    })?;\n".to_string());
    out.push(format!(
        "    handles.set({idx}, handle_{idx}).map_err(|_| nros::NodeError::InvalidSchedContextBinding)?;\n"
    ));
}

/// W.5.7 — a subscription callback sharing the instance state (payload = CDR
/// `data`). Mirrors `emit_shared_timer`.
#[allow(clippy::too_many_arguments)]
fn emit_shared_subscription(
    out: &mut Vec<String>,
    idx: usize,
    inst: usize,
    comp_path: &str,
    callback_id: &str,
    node_id: &str,
    topic: &str,
    type_name: &str,
    type_hash: &str,
    instance: &PlanInstance,
) {
    out.push(format!(
        "    let subnode_{idx} = NODES.iter().find(|node| node.node_id == {node_id:?}).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
    ));
    out.push(format!(
        "    let subnh_{idx} = executor.node_id_by_name(subnode_{idx}.node_name, subnode_{idx}.namespace).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
    ));
    out.push(format!(
        "    let state_cb{idx} = ::std::rc::Rc::clone(&state_i{inst});\n"
    ));
    out.push(format!(
        "    let resolver_cb{idx} = ::std::rc::Rc::clone(&resolver_i{inst});\n"
    ));
    // Phase 211.H — bake the plan's qos_overrides for this topic+subscription.
    let qos_expr = render_sub_qos_expr(instance, topic);
    out.push(format!(
        "    let handle_{idx} = executor.node_mut(subnh_{idx}).subscription({topic:?}).generic({type_name:?}, {type_hash:?}).qos({qos_expr}).rx_buffer::<1024>().build(move |data: &[u8]| {{\n"
    ));
    out.push(format!(
        "        let mut cb_ctx = nros::CallbackCtx::new(data, resolver_cb{idx}.as_ref());\n"
    ));
    out.push(format!(
        "        <{comp_path} as nros::ExecutableComponent>::on_callback(&mut *state_cb{idx}.borrow_mut(), nros::Callback::__from_id(nros::CallbackId::new({callback_id:?})), &mut cb_ctx);\n"
    ));
    out.push("    })?;\n".to_string());
    out.push(format!(
        "    handles.set({idx}, handle_{idx}).map_err(|_| nros::NodeError::InvalidSchedContextBinding)?;\n"
    ));
}

/// W.5.7 — a service callback that runs a real `ExecutableComponent` body.
/// Raw service callbacks are non-capturing `extern "C"` fn pointers, so the
/// shared `Rc<RefCell<State>>` + `Rc<Resolveri{inst}>` clones live behind a
/// `Box::leak`'d `'static` ctx the trampoline reads (std/alloc — `uses_std`-gated).
/// Body reads the request via `ctx.message::<Req>()`, writes via `ctx.reply::<Reply, N>()`.
#[allow(clippy::too_many_arguments)]
fn emit_shared_service(
    out: &mut Vec<String>,
    idx: usize,
    inst: usize,
    comp_path: &str,
    callback_id: &str,
    node_id: &str,
    service: &str,
    type_name: &str,
    type_hash: &str,
) {
    out.push(format!(
        "    struct SvcCtx{idx} {{ state: ::std::rc::Rc<::core::cell::RefCell<<{comp_path} as nros::ExecutableComponent>::State>>, resolver: ::std::rc::Rc<Resolveri{inst}> }}\n"
    ));
    out.push(format!(
        "    unsafe extern \"C\" fn svc_tramp_{idx}(req: *const u8, req_len: usize, resp: *mut u8, resp_cap: usize, resp_len: *mut usize, ctx: *mut core::ffi::c_void) -> bool {{\n"
    ));
    out.push(format!(
        "        let sctx = unsafe {{ &*(ctx as *const SvcCtx{idx}) }};\n"
    ));
    out.push(
        "        let req_slice = unsafe { core::slice::from_raw_parts(req, req_len) };\n"
            .to_string(),
    );
    out.push(
        "        let resp_slice = unsafe { core::slice::from_raw_parts_mut(resp, resp_cap) };\n"
            .to_string(),
    );
    out.push("        let mut written = 0usize;\n".to_string());
    out.push(
        "        let mut cb_ctx = nros::CallbackCtx::with_reply(req_slice, sctx.resolver.as_ref(), resp_slice, &mut written);\n"
            .to_string(),
    );
    out.push(format!(
        "        <{comp_path} as nros::ExecutableComponent>::on_callback(&mut *sctx.state.borrow_mut(), nros::Callback::__from_id(nros::CallbackId::new({callback_id:?})), &mut cb_ctx);\n"
    ));
    out.push("        unsafe { *resp_len = written; }\n".to_string());
    out.push("        true\n    }\n".to_string());
    out.push(format!(
        "    let svcctx{idx}: *mut core::ffi::c_void = ::std::boxed::Box::into_raw(::std::boxed::Box::new(SvcCtx{idx} {{ state: ::std::rc::Rc::clone(&state_i{inst}), resolver: ::std::rc::Rc::clone(&resolver_i{inst}) }})) as *mut core::ffi::c_void;\n"
    ));
    out.push(format!(
        "    let svcnode_{idx} = NODES.iter().find(|node| node.node_id == {node_id:?}).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
    ));
    out.push(format!(
        "    let svcnh_{idx} = executor.node_id_by_name(svcnode_{idx}.node_name, svcnode_{idx}.namespace).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
    ));
    out.push(format!(
        "    let handle_{idx} = executor.register_service_raw_sized_on::<1024, 1024>(svcnh_{idx}, {service:?}, {type_name:?}, {type_hash:?}, nros::QosSettings::services_default(), svc_tramp_{idx}, svcctx{idx})?;\n"
    ));
    out.push(format!(
        "    handles.set({idx}, handle_{idx}).map_err(|_| nros::NodeError::InvalidSchedContextBinding)?;\n"
    ));
}

/// W.5.5 + W.5.7 — an action server whose goal/cancel **decisions** run a real
/// `ExecutableComponent` body (execution — feedback/result — rides the W.5.6 tick
/// hook). Goal/cancel are non-capturing `extern "C"` fn-ptrs, so the shared
/// `Rc<RefCell<State>>` + `Rc<Resolveri{inst}>` clones live behind a `Box::leak`'d
/// `'static` `ActionCtx` (std/alloc — `uses_std`-gated). Body sets accept/reject via
/// `set_goal_response` / `set_cancel_response`; the same callback id serves both
/// phases (the ctx sink kind disambiguates). Accepted stays the noop until the tick.
#[allow(clippy::too_many_arguments)]
fn emit_shared_action(
    out: &mut Vec<String>,
    idx: usize,
    inst: usize,
    comp_path: &str,
    callback_id: &str,
    node_id: &str,
    action: &str,
    type_name: &str,
    type_hash: &str,
) {
    out.push(format!(
        "    struct ActionCtx{idx} {{ state: ::std::rc::Rc<::core::cell::RefCell<<{comp_path} as nros::ExecutableComponent>::State>>, resolver: ::std::rc::Rc<Resolveri{inst}> }}\n"
    ));
    // goal-decision trampoline
    out.push(format!(
        "    unsafe extern \"C\" fn goal_tramp_{idx}(_goal_id: *const nros::GoalId, goal_data: *const u8, goal_len: usize, ctx: *mut core::ffi::c_void) -> nros::GoalResponse {{\n"
    ));
    out.push(format!(
        "        let actx = unsafe {{ &*(ctx as *const ActionCtx{idx}) }};\n"
    ));
    out.push(
        "        let goal_slice = unsafe { core::slice::from_raw_parts(goal_data, goal_len) };\n"
            .to_string(),
    );
    out.push("        let mut resp = nros::GoalResponse::Reject;\n".to_string());
    out.push(
        "        let mut cb_ctx = nros::CallbackCtx::with_goal_decision(goal_slice, actx.resolver.as_ref(), &mut resp);\n"
            .to_string(),
    );
    out.push(format!(
        "        <{comp_path} as nros::ExecutableComponent>::on_callback(&mut *actx.state.borrow_mut(), nros::Callback::__from_id(nros::CallbackId::new({callback_id:?})), &mut cb_ctx);\n"
    ));
    out.push("        resp\n    }\n".to_string());
    // cancel-decision trampoline
    out.push(format!(
        "    unsafe extern \"C\" fn cancel_tramp_{idx}(_goal_id: *const nros::GoalId, _status: nros::GoalStatus, ctx: *mut core::ffi::c_void) -> nros::CancelResponse {{\n"
    ));
    out.push(format!(
        "        let actx = unsafe {{ &*(ctx as *const ActionCtx{idx}) }};\n"
    ));
    out.push("        let mut resp = nros::CancelResponse::Rejected;\n".to_string());
    out.push(
        "        let mut cb_ctx = nros::CallbackCtx::with_cancel_decision(&[], actx.resolver.as_ref(), &mut resp);\n"
            .to_string(),
    );
    out.push(format!(
        "        <{comp_path} as nros::ExecutableComponent>::on_callback(&mut *actx.state.borrow_mut(), nros::Callback::__from_id(nros::CallbackId::new({callback_id:?})), &mut cb_ctx);\n"
    ));
    out.push("        resp\n    }\n".to_string());
    out.push(format!(
        "    let actctx{idx}: *mut core::ffi::c_void = ::std::boxed::Box::into_raw(::std::boxed::Box::new(ActionCtx{idx} {{ state: ::std::rc::Rc::clone(&state_i{inst}), resolver: ::std::rc::Rc::clone(&resolver_i{inst}) }})) as *mut core::ffi::c_void;\n"
    ));
    out.push(format!(
        "    let actnode_{idx} = NODES.iter().find(|node| node.node_id == {node_id:?}).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
    ));
    out.push(format!(
        "    let actnh_{idx} = executor.node_id_by_name(actnode_{idx}.node_name, actnode_{idx}.namespace).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
    ));
    out.push(format!(
        "    let action_{idx} = executor.register_action_server_raw_sized::<1024, 1024, 1024, 4>(nros::RawActionServerSpec {{ node_id: Some(actnh_{idx}), action_name: {action:?}, type_name: {type_name:?}, type_hash: {type_hash:?}, qos: nros::QosSettings::services_default(), goal_callback: goal_tramp_{idx}, cancel_callback: cancel_tramp_{idx}, accepted_callback: Some(noop_raw_accepted), context: actctx{idx} }})?;\n"
    ));
    out.push(format!(
        "    handles.set({idx}, action_{idx}.handle_id()).map_err(|_| nros::NodeError::InvalidSchedContextBinding)?;\n"
    ));
}

/// W.5.8 — a service callback on a **no_std** rust executable component. Same
/// real dispatch as `emit_shared_service`, but the `(state, resolver)` context
/// lives in a function-local `static mut` (no `Box::leak`/alloc); the trampoline
/// reads it via `addr_of_mut!`. Per-callback state (no_std doesn't share — that
/// is the W.5.7 std path); the executor spins single-threaded so the `static mut`
/// access is sound.
#[allow(clippy::too_many_arguments)]
fn emit_static_service(
    out: &mut Vec<String>,
    idx: usize,
    comp_path: &str,
    callback_id: &str,
    node_id: &str,
    service: &str,
    type_name: &str,
    type_hash: &str,
    instance: &PlanInstance,
) {
    emit_executable_prelude(out, idx, comp_path, instance, false);
    out.push(format!(
        "    struct SvcCtx{idx} {{ state: <{comp_path} as nros::ExecutableComponent>::State, resolver: Resolver{idx} }}\n"
    ));
    out.push(format!(
        "    static mut SVC_CTX_{idx}: Option<SvcCtx{idx}> = None;\n"
    ));
    out.push(format!(
        "    unsafe extern \"C\" fn svc_tramp_{idx}(req: *const u8, req_len: usize, resp: *mut u8, resp_cap: usize, resp_len: *mut usize, _ctx: *mut core::ffi::c_void) -> bool {{\n"
    ));
    out.push(format!(
        "        let sctx = match unsafe {{ (*core::ptr::addr_of_mut!(SVC_CTX_{idx})).as_mut() }} {{ Some(s) => s, None => {{ unsafe {{ *resp_len = 0; }} return true; }} }};\n"
    ));
    out.push(
        "        let req_slice = unsafe { core::slice::from_raw_parts(req, req_len) };\n"
            .to_string(),
    );
    out.push(
        "        let resp_slice = unsafe { core::slice::from_raw_parts_mut(resp, resp_cap) };\n"
            .to_string(),
    );
    out.push("        let mut written = 0usize;\n".to_string());
    out.push(
        "        let mut cb_ctx = nros::CallbackCtx::with_reply(req_slice, &sctx.resolver, resp_slice, &mut written);\n"
            .to_string(),
    );
    out.push(format!(
        "        <{comp_path} as nros::ExecutableComponent>::on_callback(&mut sctx.state, nros::Callback::__from_id(nros::CallbackId::new({callback_id:?})), &mut cb_ctx);\n"
    ));
    out.push("        unsafe { *resp_len = written; }\n".to_string());
    out.push("        true\n    }\n".to_string());
    out.push(format!(
        "    unsafe {{ SVC_CTX_{idx} = Some(SvcCtx{idx} {{ state: state{idx}, resolver: resolver{idx} }}); }}\n"
    ));
    out.push(format!(
        "    let svcnode_{idx} = NODES.iter().find(|node| node.node_id == {node_id:?}).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
    ));
    out.push(format!(
        "    let svcnh_{idx} = executor.node_id_by_name(svcnode_{idx}.node_name, svcnode_{idx}.namespace).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
    ));
    out.push(format!(
        "    let handle_{idx} = executor.register_service_raw_sized_on::<1024, 1024>(svcnh_{idx}, {service:?}, {type_name:?}, {type_hash:?}, nros::QosSettings::services_default(), svc_tramp_{idx}, core::ptr::null_mut())?;\n"
    ));
    out.push(format!(
        "    handles.set({idx}, handle_{idx}).map_err(|_| nros::NodeError::InvalidSchedContextBinding)?;\n"
    ));
}

/// W.5.8 + W.5.11 — an action server on a **no_std** rust executable component.
/// The `(state, resolver)` context + the registered handle live in *module-level*
/// `static mut`s (no alloc, no `thread_local`) so both the goal/cancel decision
/// trampolines (W.5.8) and the per-spin `tick_{idx}` (W.5.11, called from the
/// no_std `run_tick_loop`) can reach them — a function-local static can't be seen
/// from the separate spin loop. The decision bodies run via the trampolines; the
/// execution body (feedback/result) runs in `tick_{idx}` over a `GenActionExec`.
/// `module` collects the module-level items; `out` the inline registration.
#[allow(clippy::too_many_arguments)]
fn emit_static_action(
    out: &mut Vec<String>,
    module: &mut Vec<String>,
    idx: usize,
    comp_path: &str,
    callback_id: &str,
    node_id: &str,
    action: &str,
    source_entity: &str,
    type_name: &str,
    type_hash: &str,
    instance: &PlanInstance,
) {
    let pubs = instance_publishers(instance);
    let key = idx.to_string();
    // --- module-level items ---
    emit_resolver_struct(module, &key, &pubs);
    module.push(format!(
        "struct ActionCtx{idx} {{ state: <{comp_path} as nros::ExecutableComponent>::State, resolver: Resolver{idx} }}\n"
    ));
    module.push(format!(
        "static mut ACT_CTX_{idx}: Option<ActionCtx{idx}> = None;\n"
    ));
    module.push(format!(
        "static mut ACT_HANDLE_{idx}: Option<nros::ActionServerRawHandle> = None;\n"
    ));
    // goal-decision trampoline
    module.push(format!(
        "unsafe extern \"C\" fn goal_tramp_{idx}(_goal_id: *const nros::GoalId, goal_data: *const u8, goal_len: usize, _ctx: *mut core::ffi::c_void) -> nros::GoalResponse {{\n    \
         let actx = match unsafe {{ (*core::ptr::addr_of_mut!(ACT_CTX_{idx})).as_mut() }} {{ Some(s) => s, None => return nros::GoalResponse::Reject }};\n    \
         let goal_slice = unsafe {{ core::slice::from_raw_parts(goal_data, goal_len) }};\n    \
         let mut resp = nros::GoalResponse::Reject;\n    \
         let mut cb_ctx = nros::CallbackCtx::with_goal_decision(goal_slice, &actx.resolver, &mut resp);\n    \
         <{comp_path} as nros::ExecutableComponent>::on_callback(&mut actx.state, nros::Callback::__from_id(nros::CallbackId::new({callback_id:?})), &mut cb_ctx);\n    \
         resp\n}}\n"
    ));
    // cancel-decision trampoline
    module.push(format!(
        "unsafe extern \"C\" fn cancel_tramp_{idx}(_goal_id: *const nros::GoalId, _status: nros::GoalStatus, _ctx: *mut core::ffi::c_void) -> nros::CancelResponse {{\n    \
         let actx = match unsafe {{ (*core::ptr::addr_of_mut!(ACT_CTX_{idx})).as_mut() }} {{ Some(s) => s, None => return nros::CancelResponse::Rejected }};\n    \
         let mut resp = nros::CancelResponse::Rejected;\n    \
         let mut cb_ctx = nros::CallbackCtx::with_cancel_decision(&[], &actx.resolver, &mut resp);\n    \
         <{comp_path} as nros::ExecutableComponent>::on_callback(&mut actx.state, nros::Callback::__from_id(nros::CallbackId::new({callback_id:?})), &mut cb_ctx);\n    \
         resp\n}}\n"
    ));
    // per-spin execution tick (W.5.11). M-F.4.a — no_std path has no client
    // backend (clients need spin-based call_raw / alloc); use an inline stub
    // that errors so the substrate's 3-arg `TickCtx::new` is satisfied. A no_std
    // codegen-side client backend is a follow-up.
    module.push(format!(
        "fn tick_{idx}(executor: &mut nros::Executor) {{\n    \
         struct __NoStdClients;\n    \
         impl nros::component::ClientDispatch for __NoStdClients {{\n        \
         fn call_raw(&mut self, _: &str, _: &[u8], _: &mut [u8]) -> nros::ComponentResult<usize> {{ Err(nros::ComponentError::Runtime) }}\n        \
         fn send_goal_raw(&mut self, _: &str, _: &[u8]) -> nros::ComponentResult<nros::GoalId> {{ Err(nros::ComponentError::Runtime) }}\n    \
         }}\n    \
         let actx = match unsafe {{ (*core::ptr::addr_of_mut!(ACT_CTX_{idx})).as_mut() }} {{ Some(s) => s, None => return }};\n    \
         let handle = match unsafe {{ *core::ptr::addr_of!(ACT_HANDLE_{idx}) }} {{ Some(h) => h, None => return }};\n    \
         let __handles: [(&'static str, nros::ActionServerRawHandle); 1] = [({source_entity:?}, handle)];\n    \
         let __exec_ptr: *mut nros::Executor = executor as *mut nros::Executor;\n    \
         let mut __ae = GenActionExec {{ executor: __exec_ptr, handles: &__handles }};\n    \
         let mut __cd = __NoStdClients;\n    \
         let mut __tc = nros::TickCtx::new(&actx.resolver, &mut __ae, &mut __cd);\n    \
         <{comp_path} as nros::ExecutableComponent>::tick(&mut actx.state, &mut __tc);\n}}\n"
    ));
    // --- inline registration ---
    emit_publisher_builders(out, &key, &pubs);
    let init = pubs
        .iter()
        .enumerate()
        .map(|(i, _)| format!("p{i}: p{i}_{idx}"))
        .collect::<Vec<_>>()
        .join(", ");
    out.push(format!(
        "    unsafe {{ ACT_CTX_{idx} = Some(ActionCtx{idx} {{ state: <{comp_path} as nros::ExecutableComponent>::init(), resolver: Resolver{idx} {{ {init} }} }}); }}\n"
    ));
    out.push(format!(
        "    let actnode_{idx} = NODES.iter().find(|node| node.node_id == {node_id:?}).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
    ));
    out.push(format!(
        "    let actnh_{idx} = executor.node_id_by_name(actnode_{idx}.node_name, actnode_{idx}.namespace).ok_or(nros::NodeError::InvalidSchedContextBinding)?;\n"
    ));
    out.push(format!(
        "    let action_{idx} = executor.register_action_server_raw_sized::<1024, 1024, 1024, 4>(nros::RawActionServerSpec {{ node_id: Some(actnh_{idx}), action_name: {action:?}, type_name: {type_name:?}, type_hash: {type_hash:?}, qos: nros::QosSettings::services_default(), goal_callback: goal_tramp_{idx}, cancel_callback: cancel_tramp_{idx}, accepted_callback: Some(noop_raw_accepted), context: core::ptr::null_mut() }})?;\n"
    ));
    out.push(format!(
        "    unsafe {{ ACT_HANDLE_{idx} = Some(action_{idx}); }}\n"
    ));
    out.push(format!(
        "    handles.set({idx}, action_{idx}.handle_id()).map_err(|_| nros::NodeError::InvalidSchedContextBinding)?;\n"
    ));
}

fn find_callback_entity<'a>(
    instance: &'a PlanInstance,
    callback_id: &str,
    source_callback: &str,
) -> Option<(&'a str, &'a PlanEntity)> {
    let mut callback_entities = Vec::new();
    for node in &instance.nodes {
        for entity in &node.entities {
            if entity_callback_id(entity).is_some_and(|entity_callback| {
                entity_callback == callback_id || entity_callback == source_callback
            }) {
                return Some((node.id.as_str(), entity));
            }
            if entity_callback_id(entity).is_some() {
                callback_entities.push((node.id.as_str(), entity));
            }
        }
    }
    if let Some(entity) = callback_entities.iter().copied().find(|(_, entity)| {
        matches!(entity, PlanEntity::Timer { .. }) && source_callback.contains("timer")
    }) {
        return Some(entity);
    }
    if let Some(entity) = callback_entities.iter().copied().find(|(_, entity)| {
        matches!(entity, PlanEntity::Subscriber { .. })
            && (source_callback.contains("message") || source_callback.contains("sub"))
    }) {
        return Some(entity);
    }
    if let Some(entity) = callback_entities
        .iter()
        .copied()
        .find(|(_, entity)| entity_matches_callback_text(entity, source_callback))
    {
        return Some(entity);
    }
    if callback_entities.len() == 1 {
        return callback_entities.first().copied();
    }
    None
}

fn entity_matches_callback_text(entity: &PlanEntity, source_callback: &str) -> bool {
    let text = match entity {
        PlanEntity::Publisher {
            source_entity,
            resolved_name,
            ..
        }
        | PlanEntity::Subscriber {
            source_entity,
            resolved_name,
            ..
        }
        | PlanEntity::ServiceServer {
            source_entity,
            resolved_name,
            ..
        }
        | PlanEntity::ServiceClient {
            source_entity,
            resolved_name,
            ..
        }
        | PlanEntity::ActionServer {
            source_entity,
            resolved_name,
            ..
        }
        | PlanEntity::ActionClient {
            source_entity,
            resolved_name,
            ..
        } => format!("{source_entity} {resolved_name}"),
        PlanEntity::Timer { source_entity, .. } => source_entity.clone(),
    };
    source_callback
        .trim_start_matches("cb_")
        .split('_')
        .filter(|token| token.len() > 2)
        .any(|token| text.contains(token))
}

fn entity_callback_id(entity: &PlanEntity) -> Option<&str> {
    match entity {
        PlanEntity::Subscriber {
            source_entity,
            callback,
            ..
        }
        | PlanEntity::Timer {
            source_entity,
            callback,
            ..
        }
        | PlanEntity::ServiceServer {
            source_entity,
            callback,
            ..
        }
        | PlanEntity::ActionServer {
            source_entity,
            callback,
            ..
        } => callback.as_deref().or(Some(source_entity.as_str())),
        _ => None,
    }
}

fn interface_type_name(interface: &super::schema::InterfaceRef) -> String {
    let (namespace, name) = split_interface_name(&interface.name);
    format!("{}::{}::dds_::{}_", interface.package, namespace, name)
}

fn interface_type_hash(interface: &super::schema::InterfaceRef) -> String {
    format!("{}/{}", interface.package, interface.name)
}

fn split_interface_name(name: &str) -> (&str, &str) {
    name.split_once('/').unwrap_or(("msg", name))
}

fn render_sched_context(sc: &PlanSchedContext) -> String {
    format!(
        "    SchedContextSpec {{ id: {id:?}, class: SchedClassSpec::{class}, priority: PrioritySpec::{priority}, period_us: {period}, budget_us: {budget}, deadline_us: {deadline}, deadline_policy: DeadlinePolicySpec::{deadline_policy}, os_pri: {os_pri}, tt_window_offset_us: {tt_offset}, tt_window_duration_us: {tt_duration} }},\n",
        id = sc.id,
        class = sched_class(&sc.class),
        priority = priority(sc.priority),
        period = option_ms_to_us(sc.period_ms),
        budget = option_ms_to_us(sc.budget_ms),
        deadline = option_ms_to_us(sc.deadline_ms),
        deadline_policy = deadline_policy(&sc.deadline_policy),
        os_pri = sc.priority.unwrap_or(0),
        tt_offset = "None",
        tt_duration = option_ms_to_us(match sc.class {
            SchedClass::TimeTriggered => sc.period_ms,
            _ => None,
        }),
    )
}

// ── Phase 256 W4.2 — direct, µs-lossless tier → SchedContextSpec lowering ──
// `nros generate` resolves scheduling TIERS itself (decision c, issue 0082) and
// emits the runtime `SchedContextSpec` straight from a `ResolvedTier` — bypassing
// the ms-based `PlanSchedContext` so µs precision is preserved and the i64→u8 /
// double-enum conversions collapse to one each.

// NOTE: `#[allow(dead_code)]` is INTERIM — these are wired into `generate_package`
// in the next W4.2 step (load NrosConfig → resolve_tiers → emit from tiers); the
// unit test exercises them meanwhile.
#[allow(dead_code)]
type ResolvedTier = crate::orchestration::tier_resolver::ResolvedTier;

/// `[tiers.<n>].class` string → the plan `SchedClass`. `None` ⇒ `RealTime` (a
/// plain priority tier). Unknown → a clear error (validated at codegen).
#[allow(dead_code)]
fn tier_sched_class(class: Option<&str>) -> Result<SchedClass> {
    match class {
        None | Some("real_time") => Ok(SchedClass::RealTime),
        Some("best_effort") => Ok(SchedClass::BestEffort),
        Some("time_triggered") => Ok(SchedClass::TimeTriggered),
        Some("interrupt") => Ok(SchedClass::Interrupt),
        Some(other) => bail!(
            "[tiers.<name>].class: unknown scheduling class {other:?} \
             (expected best_effort | real_time | time_triggered | interrupt)"
        ),
    }
}

#[allow(dead_code)]
/// `[tiers.<n>].deadline_policy` string → the plan `DeadlinePolicy`. `None` ⇒
/// `Ignore`. Unknown → a clear error.
fn tier_deadline_policy(policy: Option<&str>) -> Result<DeadlinePolicy> {
    match policy {
        None | Some("ignore") => Ok(DeadlinePolicy::Ignore),
        Some("warn") => Ok(DeadlinePolicy::Warn),
        Some("skip") => Ok(DeadlinePolicy::Skip),
        Some("fault") => Ok(DeadlinePolicy::Fault),
        Some(other) => bail!(
            "[tiers.<name>].deadline_policy: unknown {other:?} \
             (expected ignore | warn | skip | fault)"
        ),
    }
}

#[allow(dead_code)]
/// `Option<u64>` µs → the literal a `SchedContextSpec` `Option<u32>` field wants,
/// **without** the ms round-trip `option_ms_to_us` does (tiers are already µs).
fn option_u64_us(value: Option<u64>) -> String {
    match value.and_then(|us| u32::try_from(us).ok()) {
        Some(us) => format!("Some({us})"),
        None => "None".to_string(),
    }
}

#[allow(dead_code)]
/// Emit a `SchedContextSpec` literal directly from a `ResolvedTier` (W4.2). The
/// RTOS numeric `priority: i64` clamps to `os_pri: u8` (Zephyr negative coop
/// priorities are the bake/C path, not generate's Rust runtime) and buckets the
/// `PrioritySpec`; the EDF fields carry µs-for-µs.
fn render_sched_context_from_tier(t: &ResolvedTier) -> Result<String> {
    let class = tier_sched_class(t.class.as_deref())?;
    let policy = tier_deadline_policy(t.deadline_policy.as_deref())?;
    let os_pri: u8 = t.priority.clamp(0, u8::MAX as i64) as u8;
    let tt_duration = if matches!(class, SchedClass::TimeTriggered) {
        option_u64_us(t.period_us)
    } else {
        "None".to_string()
    };
    Ok(format!(
        "    SchedContextSpec {{ id: {id:?}, class: SchedClassSpec::{class}, priority: PrioritySpec::{prio}, period_us: {period}, budget_us: {budget}, deadline_us: {deadline}, deadline_policy: DeadlinePolicySpec::{dp}, os_pri: {os_pri}, tt_window_offset_us: None, tt_window_duration_us: {tt} }},\n",
        id = t.name,
        class = sched_class(&class),
        prio = priority(Some(os_pri)),
        period = option_u64_us(t.period_us),
        budget = option_u64_us(t.budget_us),
        deadline = option_u64_us(t.deadline_us),
        dp = deadline_policy(&policy),
        tt = tt_duration,
    ))
}

fn collect_callback_bindings(plan: &NrosPlan) -> Vec<(usize, usize)> {
    let mut bindings = Vec::new();
    let mut callback_index = 0usize;
    for instance in &plan.instances {
        for callback in &instance.callbacks {
            let sched_context_index = plan
                .sched_contexts
                .iter()
                .position(|context| context.id == callback.sched_context)
                .map(|index| index + 1)
                .unwrap_or(0);
            bindings.push((callback_index, sched_context_index));
            callback_index += 1;
        }
    }
    bindings
}

fn component_language(raw: &str) -> &'static str {
    match raw {
        "rust" | "Rust" => "Rust",
        "c" | "C" => "C",
        "cpp" | "c++" | "Cpp" => "Cpp",
        _ => "Rust",
    }
}

fn rust_crate_name(component_id: &str) -> Option<&str> {
    component_id
        .split("::")
        .next()
        .filter(|name| !name.is_empty())
}

fn rust_component_type_path(component_id: &str) -> Option<String> {
    let mut parts = component_id.split("::").filter(|part| !part.is_empty());
    let crate_name = parts.next()?;
    let module = parts.next()?;
    Some(format!("{crate_name}::{module}::Component"))
}

fn native_register_fn_name(component_id: &str) -> String {
    format!("register_native_component_{}", rust_ident(component_id))
}

fn native_symbol_fn_name(component_id: &str) -> String {
    format!("nros_native_symbol_{}", rust_ident(component_id))
}

fn rust_ident(raw: &str) -> String {
    let mut ident = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    while ident.contains("__") {
        ident = ident.replace("__", "_");
    }
    if ident
        .chars()
        .next()
        .is_none_or(|ch| !ch.is_ascii_alphabetic() && ch != '_')
    {
        ident.insert(0, '_');
    }
    ident
}

fn final_node_name(resolved_name: &str, namespace: &str) -> String {
    let trimmed = resolved_name.trim_matches('/');
    if trimmed.is_empty() {
        return "node".to_string();
    }
    let namespace = namespace.trim_matches('/');
    if !namespace.is_empty()
        && let Some(stripped) = trimmed.strip_prefix(namespace)
    {
        let stripped = stripped.trim_matches('/');
        if !stripped.is_empty() {
            return stripped.to_string();
        }
    }
    trimmed
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(trimmed)
        .to_string()
}

fn render_parameter_value(value: &ParameterValue) -> Option<String> {
    match value {
        ParameterValue::Bool(value) => Some(format!("ParameterValue::Bool({value})")),
        ParameterValue::Integer(value) => Some(format!("ParameterValue::I64({value})")),
        ParameterValue::Float(value) => Some(format!("ParameterValue::F64({value:?})")),
        ParameterValue::String(value) => Some(format!("ParameterValue::Str({value:?})")),
        _ => None,
    }
}

fn sched_class(class: &SchedClass) -> &'static str {
    match class {
        SchedClass::BestEffort => "BestEffort",
        SchedClass::RealTime => "Fifo",
        SchedClass::TimeTriggered => "Fifo",
        SchedClass::Interrupt => "Fifo",
    }
}

fn priority(priority: Option<u8>) -> &'static str {
    match priority {
        Some(0..=63) => "BestEffort",
        Some(64..=191) => "Normal",
        Some(_) => "Critical",
        None => "Normal",
    }
}

fn deadline_policy(policy: &DeadlinePolicy) -> &'static str {
    match policy {
        DeadlinePolicy::Ignore => "Activated",
        DeadlinePolicy::Warn => "Activated",
        DeadlinePolicy::Skip => "Activated",
        DeadlinePolicy::Fault => "Activated",
    }
}

fn option_ms_to_us(value: Option<u64>) -> String {
    match value
        .and_then(|ms| ms.checked_mul(1_000))
        .and_then(|us| u32::try_from(us).ok())
    {
        Some(us) => format!("Some({us})"),
        None => "None".to_string(),
    }
}

fn stable_plan_id(plan: &NrosPlan) -> u32 {
    let mut hash = 0x811c9dc5u32;
    for byte in plan.system.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x01000193);
    }
    hash
}

#[cfg(test)]
mod net_fragment_tests {
    use super::*;
    use crate::orchestration::plan::PlanTransport;

    /// Phase 256 W4.2 — the direct `ResolvedTier → SchedContextSpec` lowering is
    /// µs-native (no ms round-trip), clamps `priority: i64 → os_pri: u8` + buckets
    /// the `PrioritySpec`, and maps `class`/`deadline_policy` strings to the runtime
    /// enums. Time-triggered carries the period into the TT window.
    #[test]
    fn render_sched_context_from_tier_is_us_native() {
        let tier = ResolvedTier {
            name: "control".to_string(),
            priority: 80,
            stack_bytes: Some(8192),
            spin_period_us: Some(1000),
            preempt_threshold: None,
            sched_class: None,
            class: Some("time_triggered".to_string()),
            period_us: Some(20_000),
            budget_us: Some(5_000),
            deadline_us: Some(18_000),
            deadline_policy: Some("fault".to_string()),
            core: Some(1),
            members: vec![("control_node".to_string(), "loop".to_string())],
        };
        let s = render_sched_context_from_tier(&tier).unwrap();
        assert!(s.contains("id: \"control\""), "{s}");
        // time_triggered → Fifo class; µs carried directly (NOT ÷1000 then ×1000).
        assert!(s.contains("class: SchedClassSpec::Fifo"), "{s}");
        assert!(s.contains("period_us: Some(20000)"), "{s}");
        assert!(s.contains("budget_us: Some(5000)"), "{s}");
        assert!(s.contains("deadline_us: Some(18000)"), "{s}");
        // priority 80 → os_pri 80, Normal bucket (64..=191).
        assert!(s.contains("os_pri: 80"), "{s}");
        assert!(s.contains("priority: PrioritySpec::Normal"), "{s}");
        // tt window duration = period for time_triggered.
        assert!(s.contains("tt_window_duration_us: Some(20000)"), "{s}");

        // Unknown class → a clear error, not a silent default.
        let bad = ResolvedTier {
            class: Some("bogus".to_string()),
            ..tier
        };
        assert!(render_sched_context_from_tier(&bad).is_err());
    }

    /// Workspace root for tests — a hermetic fixture workspace bundled in the
    /// crate (Phase 195.C). It carries `packages/boards/*/nros-board.toml` so
    /// `profile()` resolves boards without the nano-ros superproject present
    /// (the CLI ships from a separate repo; its CI has no real `packages/boards`).
    fn test_workspace_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/board-workspace")
    }

    fn build_with(transports: Vec<PlanTransport>) -> PlanBuildOptions {
        let mut build: PlanBuildOptions = serde_json::from_value(serde_json::json!({
            "target": "x", "board": "native", "rmw": "zenoh",
            "profile": "release", "features": [], "cfg": {}
        }))
        .unwrap();
        build.transports = transports;
        build.workspace_root = Some(test_workspace_root());
        build
    }

    /// Phase 211.H — the generated subscription QoS expr bakes the plan's
    /// qos_overrides for the matching topic+subscription, and ignores
    /// publisher-side / other-topic overrides.
    #[test]
    fn render_sub_qos_expr_bakes_matching_overrides() {
        let inst: PlanInstance = serde_json::from_value(serde_json::json!({
            "id": "demo.sub.0",
            "component": "demo::sub",
            "package": "demo",
            "executable": "sub",
            "launch_name": "/sub",
            "namespace": "/",
            "remaps": [],
            "nodes": [],
            "callbacks": [],
            "parameters": [],
            "sched_bindings": [],
            "trace": { "launch_record_entity": "record://x", "source_metadata": "" },
            "qos_overrides": [
                { "topic": "/chatter", "role": "subscription", "policy": "reliability",
                  "value": "best_effort", "source": {"kind": "launch", "artifact": "launch"} },
                { "topic": "/chatter", "role": "subscription", "policy": "depth",
                  "value": 5, "source": {"kind": "launch", "artifact": "launch"} },
                { "topic": "/chatter", "role": "publisher", "policy": "reliability",
                  "value": "reliable", "source": {"kind": "launch", "artifact": "launch"} },
                { "topic": "/other", "role": "subscription", "policy": "durability",
                  "value": "transient_local", "source": {"kind": "launch", "artifact": "launch"} }
            ]
        }))
        .unwrap();

        // Matching topic+subscription → best_effort + depth(5); publisher and
        // /other overrides excluded.
        let expr = render_sub_qos_expr(&inst, "/chatter");
        assert_eq!(
            expr,
            "nros::QosSettings::default().keep_last(1).best_effort().keep_last(5)"
        );

        // A topic with no overrides → the base default expr unchanged.
        assert_eq!(
            render_sub_qos_expr(&inst, "/untouched"),
            "nros::QosSettings::default().keep_last(1)"
        );
    }

    #[test]
    fn profile_section_fans_out_optimize_intent() {
        let mut b = build_with(vec![]);
        // Default (no optimize) → no [profile], cargo's default release.
        assert_eq!(render_profile_section(&b), "");
        b.optimize = Some("size".to_string());
        let s = render_profile_section(&b);
        assert!(s.contains("[profile.release]"), "{s}");
        assert!(s.contains("opt-level = \"z\""), "{s}");
        assert!(s.contains("lto = \"fat\""), "{s}");
        assert!(s.contains("strip = true"), "{s}");
        b.optimize = Some("speed".to_string());
        assert!(render_profile_section(&b).contains("opt-level = 3"));
        b.optimize = Some("balanced".to_string());
        assert!(render_profile_section(&b).contains("opt-level = \"s\""));
        // Unknown intent is inert (no profile), not an error.
        b.optimize = Some("bogus".to_string());
        assert_eq!(render_profile_section(&b), "");
    }

    /// Phase 255 Wave 5 — `rmw_set` folds the `bridged_rmws` link set (from a
    /// cross-RMW `[[bridge]]`) into the board-feature set, deduped against the
    /// single `build.rmw`. Empty `bridged_rmws` ⇒ single-RMW set unchanged.
    #[test]
    fn rmw_set_unions_bridged_rmws() {
        let mut b = build_with(vec![]);
        // Single-RMW build, no bridges → just `build.rmw`.
        assert_eq!(rmw_set(&b), vec!["zenoh"]);

        // A cross-RMW bridge adds cyclonedds; zenoh stays deduped.
        b.bridged_rmws = vec!["zenoh".to_string(), "cyclonedds".to_string()];
        assert_eq!(rmw_set(&b), vec!["zenoh", "cyclonedds"]);
        assert_eq!(
            board_rmw_features(&b),
            vec!["rmw-zenoh".to_string(), "rmw-cyclonedds".to_string()]
        );
    }

    #[test]
    fn inject_env_var_merges_or_appends_link_ip() {
        // No [env] → append a new table.
        let body = "[build]\ntarget = \"thumbv7m-none-eabi\"\n".to_string();
        let out = inject_env_var(body, "NROS_LINK_IP", "0");
        assert!(out.contains("[env]\nNROS_LINK_IP = \"0\""), "{out}");

        // Existing [env] → insert into it (key right after the header).
        let body = "[env]\nFOO = \"1\"\n\n[build]\ntarget = \"x\"\n".to_string();
        let out = inject_env_var(body, "NROS_LINK_IP", "0");
        assert!(
            out.contains("[env]\nNROS_LINK_IP = \"0\"\nFOO = \"1\""),
            "{out}"
        );
        // FOO untouched, single [env] table.
        assert_eq!(out.matches("[env]").count(), 1, "{out}");

        // Already present → idempotent (no duplicate).
        let body = "[env]\nNROS_LINK_IP = \"0\"\n".to_string();
        let out = inject_env_var(body.clone(), "NROS_LINK_IP", "0");
        assert_eq!(out.matches("NROS_LINK_IP").count(), 1, "{out}");

        // Empty body → just the [env] table.
        let out = inject_env_var(String::new(), "NROS_LINK_IP", "0");
        assert_eq!(out, "[env]\nNROS_LINK_IP = \"0\"\n");
    }

    #[test]
    fn drops_ip_link_only_for_serial_can_only_builds() {
        use crate::orchestration::plan::{PlanTransport, TransportKind};
        let mk = |k: &str| -> PlanTransport {
            serde_json::from_value(serde_json::json!({ "kind": k })).unwrap()
        };
        let _ = TransportKind::Serial; // keep the import meaningful
        let mut b = build_with(vec![]);
        assert!(!b.drops_ip_link(), "empty transports keep board default");
        b.transports = vec![mk("serial")];
        assert!(b.drops_ip_link(), "serial-only drops IP");
        b.transports = vec![mk("serial"), mk("can")];
        assert!(b.drops_ip_link(), "serial+can drops IP");
        b.transports = vec![mk("ethernet")];
        assert!(!b.drops_ip_link(), "ethernet keeps IP");
        b.transports = vec![mk("serial"), mk("ethernet")];
        assert!(!b.drops_ip_link(), "any IP transport keeps IP");
        b.transports = vec![mk("wifi")];
        assert!(!b.drops_ip_link(), "wifi keeps IP");
    }

    #[test]
    fn build_cargo_overrides_merge_over_optimize_baseline() {
        use serde_json::json;
        let mut b = build_with(vec![]);

        // Override alone (no `optimize`) still renders a profile.
        b.optimize = None;
        b.cargo = Some(PlanCargoOverrides {
            opt_level: Some(json!("z")),
            ..Default::default()
        });
        let s = render_profile_section(&b);
        assert!(s.contains("[profile.release]"), "{s}");
        assert!(s.contains("opt-level = \"z\""), "{s}");

        // The motivating case (acceptance b, Rust side): size baseline, but keep
        // debuginfo — `debug = true` + `strip = false` override the size fields
        // *in place* while the rest of the size profile stays.
        b.optimize = Some("size".to_string());
        b.cargo = Some(PlanCargoOverrides {
            debug: Some(json!(true)),
            strip: Some(json!(false)),
            ..Default::default()
        });
        let s = render_profile_section(&b);
        assert!(s.contains("opt-level = \"z\""), "{s}"); // baseline kept
        assert!(s.contains("lto = \"fat\""), "{s}");
        assert!(s.contains("strip = false"), "{s}"); // replaced in place
        assert!(s.contains("debug = true"), "{s}"); // appended
        // `strip` appears once (replaced, not duplicated).
        assert_eq!(s.matches("strip =").count(), 1, "{s}");

        // Numeric opt-level renders bare; string stays quoted.
        b.optimize = None;
        b.cargo = Some(PlanCargoOverrides {
            opt_level: Some(json!(3)),
            codegen_units: Some(json!(16)),
            lto: Some(json!("thin")),
            ..Default::default()
        });
        let s = render_profile_section(&b);
        assert!(s.contains("opt-level = 3"), "{s}");
        assert!(s.contains("codegen-units = 16"), "{s}");
        assert!(s.contains("lto = \"thin\""), "{s}");

        // Out-of-shape JSON (array) is dropped, not panicked, leaving baseline.
        b.optimize = Some("balanced".to_string());
        b.cargo = Some(PlanCargoOverrides {
            opt_level: Some(json!(["nonsense"])),
            ..Default::default()
        });
        let s = render_profile_section(&b);
        assert!(s.contains("opt-level = \"s\""), "{s}"); // balanced baseline intact
    }

    fn eth(ip: &str) -> PlanTransport {
        PlanTransport {
            kind: TransportKind::Ethernet,
            id: None,
            ip: Some(ip.to_string()),
            ssid: None,
            password: None,
            mac: None,
            gateway: None,
            interfaces: Vec::new(),
            device: None,
            baudrate: None,
            rmw: None,
            locator: None,
            domain: None,
        }
    }

    #[test]
    fn mac_and_gateway_emit_setter_calls() {
        // Phase 172.J — ethernet mac + gateway → set_mac / set_gateway.
        let mut t = eth("10.0.2.50/24");
        t.mac = Some("02:00:00:00:00:01".to_string());
        t.gateway = Some("10.0.2.2".to_string());
        let calls = transport_config_setter_calls(&build_with(vec![t]));
        assert!(
            calls
                .iter()
                .any(|c| c.contains("c.set_ipv4([10, 0, 2, 50], 24)")),
            "{calls:?}"
        );
        assert!(
            calls
                .iter()
                .any(|c| c.contains("c.set_mac([0x02, 0x00, 0x00, 0x00, 0x00, 0x01])")),
            "{calls:?}"
        );
        assert!(
            calls
                .iter()
                .any(|c| c.contains("c.set_gateway([10, 0, 2, 2])")),
            "{calls:?}"
        );
    }

    #[test]
    fn malformed_mac_emits_nothing() {
        // Bad mac → no set_mac (the parser returns None; ip still emits).
        let mut t = eth("10.0.2.50/24");
        t.mac = Some("zz:zz".to_string());
        let calls = transport_config_setter_calls(&build_with(vec![t]));
        assert!(!calls.iter().any(|c| c.contains("set_mac")), "{calls:?}");
        assert!(calls.iter().any(|c| c.contains("set_ipv4")), "{calls:?}");
    }

    #[test]
    fn prefix_to_netmask_converts_common_prefixes() {
        assert_eq!(prefix_to_netmask("24").as_deref(), Some("255.255.255.0"));
        assert_eq!(prefix_to_netmask("16").as_deref(), Some("255.255.0.0"));
        assert_eq!(prefix_to_netmask("8").as_deref(), Some("255.0.0.0"));
        assert_eq!(prefix_to_netmask("0").as_deref(), Some("0.0.0.0"));
        assert_eq!(prefix_to_netmask("33"), None);
    }

    #[test]
    fn ipv4_to_hex_packs_octets() {
        assert_eq!(ipv4_to_hex("10.0.2.50").as_deref(), Some("0x0a000232"));
        assert_eq!(ipv4_to_hex("255.255.255.0").as_deref(), Some("0xffffff00"));
        assert_eq!(ipv4_to_hex("10.0.2"), None);
        assert_eq!(ipv4_to_hex("10.0.2.999"), None);
    }

    #[test]
    fn zephyr_fragment_empty_without_transport() {
        assert!(zephyr_net_fragment(&build_with(vec![])).is_empty());
    }

    fn plan_with_param_persistence(pp: Option<serde_json::Value>) -> NrosPlan {
        use crate::orchestration::schema::PLAN_VERSION;
        let mut plan = serde_json::json!({
            "version": PLAN_VERSION,
            "system": "demo",
            "trace": {
                "system_config": "nros.toml",
                "launch_record": "r.json",
                "generated_by": "test"
            },
            "components": [], "instances": [], "interfaces": [], "sched_contexts": [],
            "build": {
                "target": "x86_64-unknown-linux-gnu", "board": "native", "rmw": "zenoh",
                "profile": "release", "features": [], "cfg": {}
            }
        });
        if let Some(pp) = pp {
            plan.as_object_mut()
                .unwrap()
                .insert("param_persistence".into(), pp);
        }
        let mut plan: NrosPlan = serde_json::from_value(plan).expect("plan parses");
        plan.build.workspace_root = Some(test_workspace_root());
        plan
    }

    fn timer_entity(id: &str, source_entity: &str, callback: Option<&str>) -> PlanEntity {
        let mut value = serde_json::json!({
            "role": "timer",
            "id": id,
            "source_entity": source_entity,
            "period_ms": 100,
            "trace": {
                "source_artifact": { "artifact": "source.rs", "line": null, "column": null },
                "manifest_endpoint": null
            }
        });
        if let Some(callback) = callback {
            value
                .as_object_mut()
                .expect("timer entity is an object")
                .insert("callback".to_string(), serde_json::json!(callback));
        }
        serde_json::from_value(value).expect("timer entity parses")
    }

    #[test]
    fn generated_plan_entity_id_is_not_callback_fallback() {
        let entity = timer_entity("launch.0/entity.7", "heartbeat_timer", None);
        assert_eq!(entity_callback_id(&entity), Some("heartbeat_timer"));
        assert_ne!(entity_callback_id(&entity), Some("launch.0/entity.7"));
        assert!(
            !entity_matches_callback_text(&entity, "cb_entity"),
            "generated plan id text must not drive callback matching"
        );
    }

    #[test]
    fn explicit_generated_callback_id_can_still_bind_entity() {
        let instance: PlanInstance = serde_json::from_value(serde_json::json!({
            "id": "launch.0",
            "component": "demo::talker",
            "package": "demo",
            "executable": "talker",
            "launch_name": "talker",
            "namespace": "/",
            "remaps": [],
            "nodes": [{
                "id": "launch.0/node.0",
                "source_node": "talker_node",
                "resolved_name": "/talker",
                "namespace": "/",
                "entities": [{
                    "role": "timer",
                    "id": "launch.0/entity.0",
                    "source_entity": "tick_timer",
                    "callback": "launch.0/callback.0",
                    "period_ms": 100,
                    "trace": {
                        "source_artifact": { "artifact": "source.rs", "line": null, "column": null },
                        "manifest_endpoint": null
                    }
                }]
            }],
            "callbacks": [{
                "id": "launch.0/callback.0",
                "source_callback": "cb_tick",
                "group": "default",
                "sched_context": "default_executor",
                "source": { "artifact": "source.rs", "line": null, "column": null }
            }],
            "parameters": [],
            "sched_bindings": [],
            "trace": { "launch_record_entity": "launch.json#0", "source_metadata": "source-metadata.json" }
        }))
        .expect("instance parses");

        let callback = &instance.callbacks[0];
        let (node_id, entity) = find_callback_entity(
            &instance,
            callback.id.as_str(),
            callback.source_callback.as_str(),
        )
        .expect("generated callback id binds entity");
        assert_eq!(node_id, "launch.0/node.0");
        assert!(matches!(entity, PlanEntity::Timer { .. }));
    }

    #[test]
    fn param_persistence_none_renders_noop() {
        // 172.H — no [param_persistence] ⇒ no-op fn, no param services.
        let mut out = String::new();
        render_param_persistence_fn(&mut out, &plan_with_param_persistence(None));
        assert!(out.contains("pub fn apply_param_persistence"), "{out}");
        assert!(out.contains("let _ = executor;"), "{out}");
        assert!(!out.contains("register_parameter_services"), "{out}");
        // And a None plan must not pull the param-services feature.
        let feats = generated_default_features(
            &plan_with_param_persistence(None).build,
            false,
            false,
            false,
            false,
            false,
        );
        assert!(
            !feats.iter().any(|f| f == "nros/param-services"),
            "{feats:?}"
        );
    }

    #[test]
    fn param_persistence_file_renders_declare_and_enable() {
        // 172.H — a file backend registers services, declares params, attaches
        // the FileParamStore at the configured path, and pulls param-services.
        let plan = plan_with_param_persistence(Some(serde_json::json!({
            "backend": "file", "path": "/var/lib/nros/params.store"
        })));
        let mut out = String::new();
        render_param_persistence_fn(&mut out, &plan);
        assert!(
            out.contains("executor.register_parameter_services()?;"),
            "{out}"
        );
        assert!(
            out.contains("executor.declare_parameter(spec.name, value);"),
            "{out}"
        );
        assert!(
            out.contains(
                "executor.enable_parameter_persistence_with(nros::FileParamStore::new(\"/var/lib/nros/params.store\"))?;"
            ),
            "{out}"
        );
        assert!(out.contains("nros::ParameterValue::Integer(i)"), "{out}");

        let feats = generated_default_features(&plan.build, false, true, false, false, false);
        assert!(
            feats.iter().any(|f| f == "nros/param-services"),
            "{feats:?}"
        );
    }

    #[test]
    fn safety_axis_lowers_to_nros_feature() {
        // Phase 250 Wave 1 — a declared `[safety]` block (plan.safety = Some)
        // pulls `nros/safety-e2e`; absent ⇒ it must not, keeping non-safety
        // plans byte-identical.
        let plan = plan_with_param_persistence(None);
        let off = generated_default_features(&plan.build, false, false, false, false, false);
        assert!(
            !off.iter().any(|f| f == "nros/safety-e2e"),
            "safety off must not pull the feature: {off:?}"
        );
        let on = generated_default_features(&plan.build, false, false, false, true, false);
        assert!(
            on.iter().any(|f| f == "nros/safety-e2e"),
            "safety on must pull the feature: {on:?}"
        );
    }

    fn board_with_caps(caps: &[&str]) -> crate::orchestration::board_descriptor::BoardDescriptor {
        use crate::orchestration::board_descriptor::*;
        BoardDescriptor {
            names: vec!["fake-board".into()],
            platform: PlatformKind::Freertos,
            target: None,
            toolchain: Toolchain::Stable,
            platform_feature: "platform-freertos".into(),
            local_aliases: vec![],
            link_kind: LinkKind::None,
            entry_kind: EntryKind::BoardRun,
            net_stack: NetStack::NanorosOwned,
            chip: None,
            board_crate: Some("nros-board-fake".into()),
            crate_path: None,
            board_features: vec![],
            capability_features: caps.iter().map(|s| s.to_string()).collect(),
            cargo_config: None,
            entry: None,
            target_contains: None,
            capabilities: None,
        }
    }

    #[test]
    fn board_capability_features_gated_on_advertisement() {
        // Phase 252 / issue 0072 — `[safety]` lowers to the board feature ONLY when
        // the board advertises `safety-e2e`; else skip (warn), never a Cargo error.
        let mut plan = plan_with_param_persistence(None);

        // No [safety] → nothing, regardless of advertisement.
        plan.safety = None;
        assert!(board_capability_features(&plan, &board_with_caps(&["safety-e2e"])).is_empty());

        // [safety] + board advertises → board feature emitted.
        plan.safety = Some(crate::orchestration::plan::PlanSafety { crc: true });
        assert_eq!(
            board_capability_features(&plan, &board_with_caps(&["safety-e2e"])),
            vec!["safety-e2e".to_string()]
        );

        // [safety] + board does NOT advertise → skipped (no Cargo error).
        assert!(board_capability_features(&plan, &board_with_caps(&[])).is_empty());
    }

    #[test]
    fn safety_axis_reaches_zenoh_backend_feature() {
        // Issue 0072 — a declared `[safety]` must also enable the BACKEND's own
        // safety-e2e (the CRC path), not just `nros/safety-e2e`. Native board-less
        // builds carry a direct `nros-rmw-zenoh` dep via `backend_features`.
        let build = plan_with_param_persistence(None).build;
        // Safety on → zenoh dep carries safety-e2e.
        assert!(
            backend_features(&build, "zenoh", true)
                .iter()
                .any(|f| f == "safety-e2e"),
            "zenoh backend must carry safety-e2e when [safety] is declared"
        );
        // Safety off → it must not (byte-identical to pre-0072).
        assert!(
            !backend_features(&build, "zenoh", false)
                .iter()
                .any(|f| f == "safety-e2e"),
            "safety off must not pull the backend feature"
        );
        // xrce has no safety-e2e CRC path → never pulled, even with safety on.
        assert!(
            !backend_features(&build, "xrce", true)
                .iter()
                .any(|f| f == "safety-e2e"),
            "xrce has no safety-e2e feature; the axis no-ops there"
        );
    }

    #[test]
    fn param_services_axis_lowers_to_nros_feature() {
        // Phase 250 Wave 3 — a declared `[param_services]` axis (plan.param_services
        // = Some) pulls `nros/param-services` on its own (no persistence); absent ⇒
        // it must not (unless `[param_persistence]` does), keeping plans byte-identical.
        let plan = plan_with_param_persistence(None);
        // param_persistence=false, param_services=false → no feature.
        let off = generated_default_features(&plan.build, false, false, false, false, false);
        assert!(
            !off.iter().any(|f| f == "nros/param-services"),
            "param-services off must not pull the feature: {off:?}"
        );
        // param_persistence=false, param_services=true → feature pulled.
        let on = generated_default_features(&plan.build, false, false, true, false, false);
        assert!(
            on.iter().any(|f| f == "nros/param-services"),
            "param-services on must pull the feature: {on:?}"
        );
    }

    #[test]
    fn entry_lib_idents_and_c_abi_header() {
        // 172 WP-B — sanitizers + the directly-emitted C ABI header.
        assert_eq!(crate_ident("nros-e2e-generated"), "nros_e2e_generated");
        assert_eq!(crate_ident("a.b-c"), "a_b_c");
        let plan = plan_with_param_persistence(None); // system = "demo"
        assert_eq!(system_ident(&plan), "demo");
        let header = render_entry_header(&plan);
        assert!(
            header.contains("typedef struct NrosExecutor NrosExecutor;")
                && header.contains("} NrosConfig;"),
            "{header}"
        );
        assert!(
            header.contains("NrosExecutor *nros_demo_build_executor(const NrosConfig *cfg);")
                && header.contains("int32_t nros_demo_register_all(NrosExecutor *executor);")
                && header.contains("void nros_demo_destroy(NrosExecutor *executor);"),
            "{header}"
        );
        // Config lowering: the lib applies the param override (param > env > baked).
        let lib = render_entry_lib_rs(&plan);
        assert!(
            lib.contains("pub struct NrosConfig")
                && lib.contains("config = config.domain_id(cfg.domain_id as u32)")
                && lib.contains("config.locator = locator"),
            "{lib}"
        );
        // The std-hosted native plan emits the entry lib (lib + staticlib),
        // with the C ABI + its alloc box, and is NOT no_std.
        assert!(emits_entry_lib(&plan), "native std-hosted ⇒ entry lib");
        assert!(!lib.starts_with("#![no_std]"), "hosted lib is std:\n{lib}");
        assert!(
            lib.contains("extern crate alloc;"),
            "hosted C ABI boxes via alloc"
        );
        assert!(
            render_lib_section(&plan, "nros-e2e-generated")
                .contains("crate-type = [\"lib\", \"staticlib\"]"),
            "entry-lib crate-type"
        );
    }

    #[test]
    fn entry_lib_board_shape_is_no_std_without_c_abi() {
        // A board (no_std, no allocator) entry lib is `#![no_std]`, exposes the
        // Rust API (`register_all`), and omits the C ABI + alloc (the board
        // `self` shim calls `register_all` directly).
        let mut plan = plan_with_param_persistence(None);
        plan.build.board = "baremetal".to_string();
        plan.build.target = "thumbv7m-none-eabi".to_string();
        let lib = render_entry_lib_rs(&plan);
        assert!(
            lib.starts_with("#![no_std]\n"),
            "board lib is no_std:\n{lib}"
        );
        assert!(
            lib.contains(
                "pub use nros_generated::{SYSTEM, TRANSPORT_LOCATOR, build_executor, register_all};"
            ),
            "board lib exposes the Rust API:\n{lib}"
        );
        assert!(
            !lib.contains("extern crate alloc"),
            "no alloc on bare-metal:\n{lib}"
        );
        assert!(
            !lib.contains("pub struct NrosConfig"),
            "no C ABI on board self:\n{lib}"
        );
        assert!(
            !lib.contains("nros_demo_build_executor"),
            "no C-ABI fns:\n{lib}"
        );
    }

    #[test]
    fn bridge_routes_through_entry_lib_via_build_executor_bridge() {
        // A bridge (>1 transport) routes through the entry lib: the lib
        // re-exports `build_executor_bridge` and the shim opens via it (no
        // per-run ExecutorConfig — sessions come from baked SESSION_SPECS).
        let mut plan = plan_with_param_persistence(None);
        plan.build = build_with(vec![
            serde_json::from_value(serde_json::json!({ "kind": "ethernet", "rmw": "zenoh" }))
                .unwrap(),
            serde_json::from_value(serde_json::json!({ "kind": "ethernet", "rmw": "cyclonedds" }))
                .unwrap(),
        ]);
        assert!(plan.build.is_bridge(), "two transports ⇒ bridge");
        assert!(
            emits_entry_lib(&plan),
            "bridge routes through the entry lib"
        );

        let lib = render_entry_lib_rs(&plan);
        assert!(
            lib.contains(", build_executor_bridge}"),
            "bridge lib re-exports build_executor_bridge:\n{lib}"
        );

        let opts = GenerateOptions {
            package_name: "demo".into(),
            output_dir: PathBuf::from("/x"),
            plan_path: PathBuf::from("/x/nros-plan.json"),
            nros_path: PathBuf::from("/n"),
            nros_orchestration_path: PathBuf::from("/no"),
            component_workspace: None,
        };
        let shim = render_hosted_shim_main(&opts, &plan);
        assert!(
            shim.contains("build_executor_bridge()") && !shim.contains("from_env"),
            "bridge shim opens via build_executor_bridge, no per-run config:\n{shim}"
        );
    }

    #[test]
    fn session_specs_emit_per_transport_domain() {
        // 172 WP-B — a bridge's SESSION_SPECS carry each transport's domain
        // (multi-domain in-binary); a transport without `domain` stays default.
        use crate::orchestration::schema::PLAN_VERSION;
        let plan: NrosPlan = serde_json::from_value(serde_json::json!({
            "version": PLAN_VERSION, "system": "s",
            "trace": { "system_config": "nros.toml", "launch_record": "r", "generated_by": "t" },
            "components": [], "instances": [], "interfaces": [], "sched_contexts": [],
            "build": {
                "target": "x86_64-unknown-linux-gnu", "board": "native", "rmw": "zenoh",
                "profile": "release", "features": [], "cfg": {},
                "transports": [
                    { "kind": "ethernet", "rmw": "zenoh", "locator": "tcp/a:7447" },
                    { "kind": "ethernet", "rmw": "zenoh", "locator": "tcp/b:7447", "domain": 5 }
                ]
            }
        }))
        .expect("bridge plan parses");
        assert!(plan.build.is_bridge());
        let tables = render_generated_tables(&plan);
        assert!(tables.contains("pub static SESSION_SPECS"), "{tables}");
        assert!(
            tables.contains("nros::SessionSpec::new(\"zenoh\", \"tcp/a:7447\"),"),
            "default-domain transport has no .domain_id:\n{tables}"
        );
        assert!(
            tables.contains("nros::SessionSpec::new(\"zenoh\", \"tcp/b:7447\").domain_id(5)"),
            "domain-5 transport emits .domain_id(5):\n{tables}"
        );
    }

    /// Build a bridge plan: two zenoh transports (domains 0 + 5), a `[[bridge]]`
    /// connecting them forwarding `topics`, and (optionally) an instance whose
    /// node publishes each topic so it resolves to an interface.
    fn bridge_plan(topics: &[&str], declare: bool) -> NrosPlan {
        use crate::orchestration::schema::PLAN_VERSION;
        let entities: Vec<_> = if declare {
            topics
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    serde_json::json!({
                        "role": "publisher", "id": format!("e{i}"), "source_entity": format!("e{i}"),
                        "resolved_name": t,
                        "interface": { "package": "std_msgs", "name": "msg/Int32", "kind": "message" },
                        "qos": { "reliability": "reliable", "durability": "volatile",
                                 "history": "keep_last", "depth": 10, "deadline_ms": null,
                                 "lifespan_ms": null, "liveliness": "automatic",
                                 "liveliness_lease_duration_ms": null, "extensions": {} },
                        "trace": { "source_artifact": { "artifact": "x", "line": null, "column": null },
                                   "manifest_endpoint": null }
                    })
                })
                .collect()
        } else {
            vec![]
        };
        let instances = if declare {
            serde_json::json!([{
                "id": "i0", "component": "c", "package": "p", "executable": "e",
                "launch_name": "n", "namespace": "/", "remaps": [],
                "nodes": [{ "id": "n0", "source_node": "n0", "resolved_name": "/n",
                            "namespace": "/", "entities": entities }],
                "callbacks": [], "parameters": [], "sched_bindings": [],
                "trace": { "launch_record_entity": "x", "source_metadata": "y" }
            }])
        } else {
            serde_json::json!([])
        };
        serde_json::from_value(serde_json::json!({
            "version": PLAN_VERSION, "system": "s",
            "trace": { "system_config": "nros.toml", "launch_record": "r", "generated_by": "t" },
            "components": [], "instances": instances, "interfaces": [], "sched_contexts": [],
            "bridges": [{ "name": "gw", "connect": [
                { "rmw": "zenoh", "domain": 0, "locator": "tcp/a:7447" },
                { "rmw": "zenoh", "domain": 5, "locator": "tcp/b:7447" }
            ], "topics": topics }],
            "build": {
                "target": "x86_64-unknown-linux-gnu", "board": "native", "rmw": "zenoh",
                "profile": "release", "features": [], "cfg": {},
                "transports": [
                    { "kind": "ethernet", "rmw": "zenoh", "locator": "tcp/a:7447" },
                    { "kind": "ethernet", "rmw": "zenoh", "locator": "tcp/b:7447", "domain": 5 }
                ]
            }
        }))
        .expect("bridge plan parses")
    }

    #[test]
    fn validate_bridges_rejects_undeclared_topic() {
        // A forwarded topic no component declares ⇒ clear error (no wildcards).
        let plan = bridge_plan(&["/ghost"], false);
        let err = validate_bridges(&plan).expect_err("undeclared topic must error");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("/ghost") && msg.contains("no component declares"),
            "{msg}"
        );
        // Declared ⇒ passes.
        assert!(validate_bridges(&bridge_plan(&["/chatter"], true)).is_ok());
    }

    #[test]
    fn register_bridges_emits_relay() {
        let plan = bridge_plan(&["/chatter"], true);
        let tables = render_generated_tables(&plan);
        // register_bridges fn + per-endpoint bridge nodes bound to their session.
        assert!(tables.contains("pub fn register_bridges"), "{tables}");
        assert!(
            tables.contains("node_builder(\"gw_ep0\").session_idx(0)"),
            "{tables}"
        );
        assert!(
            tables.contains("node_builder(\"gw_ep1\").session_idx(1)"),
            "{tables}"
        );
        // The relay: generic publisher + generic+message_info subscription, with
        // bridge_origin echo suppression and attachment re-stamping.
        assert!(
            tables.contains(".publisher(\"/chatter\").generic("),
            "{tables}"
        );
        assert!(
            tables.contains(".subscription(\"/chatter\").generic(")
                && tables.contains(".message_info().build(move |payload"),
            "{tables}"
        );
        assert!(
            tables.contains("nros::bridge::parse_bridge_origin(info.attachment())"),
            "{tables}"
        );
        assert!(
            tables.contains("publish_raw_with_attachment(payload"),
            "{tables}"
        );
        // register_all calls it.
        assert!(tables.contains("register_bridges(executor)?;"), "{tables}");
    }

    #[test]
    fn zephyr_fragment_static_ip_and_dhcp() {
        let stat = zephyr_net_fragment(&build_with(vec![eth("10.0.2.50/24")]));
        assert!(stat.contains("CONFIG_NET_CONFIG_SETTINGS=y"));
        assert!(stat.contains("CONFIG_NET_CONFIG_MY_IPV4_ADDR=\"10.0.2.50\""));
        assert!(stat.contains("CONFIG_NET_CONFIG_MY_IPV4_NETMASK=\"255.255.255.0\""));

        let dhcp = zephyr_net_fragment(&build_with(vec![eth("dhcp")]));
        assert!(dhcp.contains("CONFIG_NET_DHCPV4=y"));
        assert!(!dhcp.contains("MY_IPV4_ADDR"));
    }
}
