//! Phase 212.N.9 — `nros::main!()` proc-macro implementation.
//!
//! Replaces the legacy Entry pkg `build.rs + include!()` shape with a
//! one-line `main.rs`. Four forms (per design-doc §11.6):
//!
//! ```ignore
//! nros::main!();                                          // single-node self-bringup
//! nros::main!(board = NativeBoard);                       // single-node, explicit board
//! nros::main!(launch = "demo_bringup");                   // multi-node, default launch
//! nros::main!(launch = "demo_bringup:sim.launch.xml");    // multi-node, explicit file
//! nros::main!(board = X, launch = "Y:Z.xml", args = [("k", "v")]);
//! ```
//!
//! Form 1 reads `[package.metadata.nros.entry] deploy = "<board>"` from
//! the Entry pkg's own `Cargo.toml` and maps the board key to a board
//! crate via a small lookup table. Forms 3+ resolve the bringup pkg
//! through the N.10 workspace pkg-index, walk the N.11 launch.xml
//! parser, and emit one `<pkg_ident>::register(runtime)?;` call per
//! `<node pkg=… exec=…>` entry.
//!
//! ## Rebuild-correctness workaround
//!
//! Stable Rust proc-macros can't use the unstable
//! `proc_macro::tracked_path::path()` API. Instead we emit
//! `const _: &[u8] = include_bytes!("/abs/path");` for every file the
//! macro read (launch.xml, every `package.xml` the pkg-index walked,
//! the bringup's `system.toml`). Cargo's `include_bytes!` is tracked,
//! so touching any of these files invalidates the Entry pkg's
//! compilation cache.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use nros_orchestration_ir::{
    CallbackGroupDecl, NodeOverride, ResolvedTierTable, TierDef, resolve_tiers,
};
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{
    Expr, ExprLit, Ident, Lit, LitStr, Path as SynPath, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
};

/// Parsed `nros::main!(...)` argument set.
///
/// Each form maps to a different population of these fields:
///   - Form 1 (no args): all `None`.
///   - Form 2 (`board = X`): `board = Some(X)`, launch / args `None`.
///   - Form 3 (`launch = "Y"`): `launch = Some("Y")`, board derived
///     from `[package.metadata.nros.entry] deploy`.
///   - Form 4 (board + launch + args): all populated.
#[derive(Default)]
struct MainArgs {
    /// Explicit board ident — used verbatim when supplied. None
    /// triggers the `Cargo.toml [package.metadata.nros.entry]
    /// deploy = "<board>"` lookup.
    board: Option<SynPath>,
    /// `"<bringup_pkg>"` or `"<bringup_pkg>:<file.launch.xml>"`. None
    /// triggers the single-node self-bringup path (emit
    /// `<this_pkg>::register(runtime)?;`).
    launch: Option<LitStr>,
    /// `args = [("k", "v"), ...]`. Forwarded to the launch parser as
    /// argument overrides.
    args: Vec<(String, String)>,
    /// Phase 211.F — `host = "<id>"`: multi-host partition. When set, keep only
    /// launch nodes whose `<node machine="…">` equals `<id>` plus all unhosted
    /// (shared) nodes — mirrors `nros codegen entry --host` / `Plan::for_host`.
    /// `None` = single-host / unfiltered (every node). A per-host Entry pkg
    /// passes its target host so a multi-host launch bakes one runnable entry
    /// per host (CLI/macro parity, now that nros-macros builds against the
    /// in-tree nros-cli-core carrying `NodeSpec.machine`).
    host: Option<String>,
    /// Phase 216.B.4 — `custom_tasks = [adc_sample, ui_redraw]`. Each
    /// ident becomes an extra `#[task]` trampoline inside the
    /// generated `mod __nros_app` body when the dispatched framework
    /// is RTIC. `None` = not supplied (the default — no extra tasks).
    /// `Some(vec![])` = supplied as an empty list (still no extra
    /// tasks, but the key was present — distinguished so the
    /// OwnedSpin/Embassy misuse error fires even on `[]`).
    custom_tasks: Option<Vec<Ident>>,
    /// Span of the `custom_tasks` key, retained for diagnostics when
    /// rejecting the key under a non-RTIC framework.
    custom_tasks_span: Option<Span>,
}

impl Parse for MainArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut out = MainArgs::default();
        if input.is_empty() {
            return Ok(out);
        }
        // Parse `key = value` pairs, comma-separated. Allow a trailing
        // comma — matches every other Rust attribute-arg syntax.
        let pairs: Punctuated<KeyValue, Token![,]> = Punctuated::parse_terminated(input)?;
        for KeyValue { key, value } in pairs {
            match key.to_string().as_str() {
                "board" => {
                    let path = match value {
                        KvValue::Path(p) => p,
                        KvValue::Str(s) => {
                            return Err(syn::Error::new(
                                s.span(),
                                "expected board ident (e.g. `NativeBoard`), got string literal",
                            ));
                        }
                        KvValue::Args(_) | KvValue::IdentList(_) => {
                            return Err(syn::Error::new(
                                key.span(),
                                "`board = ` takes a type path, not a list",
                            ));
                        }
                    };
                    out.board = Some(path);
                }
                "launch" => {
                    let s = match value {
                        KvValue::Str(s) => s,
                        _ => {
                            return Err(syn::Error::new(
                                key.span(),
                                "`launch = ` takes a string literal",
                            ));
                        }
                    };
                    out.launch = Some(s);
                }
                "host" => {
                    let s = match value {
                        KvValue::Str(s) => s,
                        _ => {
                            return Err(syn::Error::new(
                                key.span(),
                                "`host = ` takes a string literal (the target machine id)",
                            ));
                        }
                    };
                    out.host = Some(s.value());
                }
                "args" => {
                    let list = match value {
                        KvValue::Args(pairs) => pairs,
                        _ => {
                            return Err(syn::Error::new(
                                key.span(),
                                "`args = ` takes a list of `(\"key\", \"value\")` tuples",
                            ));
                        }
                    };
                    out.args = list;
                }
                "custom_tasks" => {
                    // Phase 216.B.4 — `custom_tasks = [ident, ident,
                    // ...]`. Stored even when empty so the framework-
                    // dispatch error fires regardless of list length.
                    let idents = match value {
                        KvValue::IdentList(v) => v,
                        _ => {
                            return Err(syn::Error::new(
                                key.span(),
                                "`custom_tasks = ` takes a list of fn idents, \
                                 e.g. `custom_tasks = [adc_sample, ui_redraw]`",
                            ));
                        }
                    };
                    out.custom_tasks = Some(idents);
                    out.custom_tasks_span = Some(key.span());
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "unknown `nros::main!` argument `{other}` \
                             (expected one of: board, launch, host, args, custom_tasks)"
                        ),
                    ));
                }
            }
        }
        Ok(out)
    }
}

struct KeyValue {
    key: Ident,
    value: KvValue,
}

enum KvValue {
    /// `board = NativeBoard` / `board = ::nros_board_native::NativeBoard`
    Path(SynPath),
    /// `launch = "demo_bringup:sim.launch.xml"`
    Str(LitStr),
    /// `args = [("use_sim", "true"), ...]`
    Args(Vec<(String, String)>),
    /// Phase 216.B.4 — `custom_tasks = [adc_sample, ui_redraw, ...]`.
    /// Empty list is valid (parses to `vec![]`).
    IdentList(Vec<Ident>),
}

impl Parse for KeyValue {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let key: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        // Try the array form first — two shapes share the bracket:
        // `args = [("k","v"), ...]` (tuple-string pairs) and
        // `custom_tasks = [foo, bar]` (bare idents, Phase 216.B.4).
        // Dispatch on the key name so each form's parser sees only
        // its expected token shape (cleaner diagnostics).
        if input.peek(syn::token::Bracket) {
            let content;
            syn::bracketed!(content in input);
            if key == "custom_tasks" {
                // Bare-ident list; empty `[]` parses to `vec![]`.
                let idents: Punctuated<Ident, Token![,]> = Punctuated::parse_terminated(&content)?;
                let collected: Vec<Ident> = idents.into_iter().collect();
                return Ok(KeyValue {
                    key,
                    value: KvValue::IdentList(collected),
                });
            }
            let pairs: Punctuated<TupleStrPair, Token![,]> =
                Punctuated::parse_terminated(&content)?;
            let collected = pairs
                .into_iter()
                .map(|p| (p.k.value(), p.v.value()))
                .collect();
            return Ok(KeyValue {
                key,
                value: KvValue::Args(collected),
            });
        }
        // A bare string literal -> KvValue::Str. Anything else -> Path.
        if input.peek(LitStr) {
            let s: LitStr = input.parse()?;
            return Ok(KeyValue {
                key,
                value: KvValue::Str(s),
            });
        }
        let path: SynPath = input.parse()?;
        Ok(KeyValue {
            key,
            value: KvValue::Path(path),
        })
    }
}

struct TupleStrPair {
    k: LitStr,
    v: LitStr,
}

impl Parse for TupleStrPair {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let content;
        syn::parenthesized!(content in input);
        let k: LitStr = content.parse()?;
        content.parse::<Token![,]>()?;
        let v: LitStr = content.parse()?;
        Ok(TupleStrPair { k, v })
    }
}

/// Entry point — emits the `fn main()` body. Errors surface as
/// `compile_error!()` spans pointing at the macro invocation.
pub fn expand(input: TokenStream) -> TokenStream {
    let args = parse_macro_input!(input as MainArgs);
    match build_main(args) {
        Ok(ts) => ts.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

/// Errors carry a span — when we have a relevant token, attach it; when
/// the failure is environmental (Cargo.toml missing, launch parse
/// failed), fall back to `Span::call_site()`.
type MacroResult<T> = std::result::Result<T, syn::Error>;

fn build_main(args: MainArgs) -> MacroResult<proc_macro2::TokenStream> {
    // CARGO_MANIFEST_DIR is set by cargo when compiling proc-macro
    // consumers; if missing we fail loud — proc-macros without a
    // manifest dir would have no way to find Cargo.toml or workspace
    // root.
    let manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR").ok_or_else(|| {
        syn::Error::new(
            Span::call_site(),
            "nros::main!: CARGO_MANIFEST_DIR not set (cargo must drive the build)",
        )
    })?;
    let manifest_dir = PathBuf::from(manifest_dir);

    // List of files that participated in the macro's decision so we
    // can emit `include_bytes!` rebuild stamps below. Always
    // canonicalised so the paths survive cargo's relocation tricks.
    let mut tracked: Vec<PathBuf> = Vec::new();

    // --- Board resolution ---
    // `deploy` is `Some(...)` only when the macro had to read the
    // Entry pkg's `[package.metadata.nros.entry] deploy = "..."` key
    // (form 1). When the user passes `board = X` directly we have no
    // deploy string and default to the `OwnedSpin` framework — RTIC
    // / Embassy require the `deploy = "rtic-stm32f4"` / `deploy =
    // "embassy-stm32f4"` opt-in for now.
    let (board_path, deploy_for_framework): (SynPath, Option<String>) = match &args.board {
        Some(p) => (p.clone(), None),
        None => {
            let cargo_toml = manifest_dir.join("Cargo.toml");
            tracked.push(cargo_toml.clone());
            let deploy = read_entry_deploy(&cargo_toml).map_err(|e| {
                syn::Error::new(
                    Span::call_site(),
                    format!(
                        "nros::main!: failed to read `[package.metadata.nros.entry] deploy` \
                         from `{}`: {e}\n  Hint: add `[package.metadata.nros.entry] deploy = \
                         \"<board>\"` (e.g. `\"native\"`, `\"freertos\"`, `\"zephyr\"`) \
                         to your Cargo.toml, or pass `board = MyBoard` to the macro.",
                        cargo_toml.display()
                    ),
                )
            })?;
            let resolved = board_path_for(&deploy).ok_or_else(|| {
                syn::Error::new(
                    Span::call_site(),
                    format!(
                        "nros::main!: unknown board `{deploy}` in \
                         `[package.metadata.nros.entry] deploy`. \
                         Known boards: {}.\n  Pass `board = <YourBoardZst>` explicitly \
                         if your board crate is not in the default table.",
                        known_boards_csv()
                    ),
                )
            })?;
            (resolved, Some(deploy))
        }
    };
    let framework = match deploy_for_framework.as_deref() {
        Some(d) => framework_for(d),
        None => Framework::OwnedSpin,
    };

    // Phase 216.B.4 — `custom_tasks = [...]` only applies to the
    // RTIC emit. OwnedSpin / Embassy have no `mod __nros_app { ... }`
    // body for the macro to splice into, so flag misuse early with a
    // span pointing at the key.
    if args.custom_tasks.is_some() && framework != Framework::Rtic {
        let span = args.custom_tasks_span.unwrap_or_else(Span::call_site);
        let framework_label = match framework {
            Framework::OwnedSpin => "OwnedSpin",
            Framework::Embassy => "Embassy",
            Framework::Zephyr => "Zephyr",
            Framework::Esp32 => "Esp32",
            Framework::Rtic => unreachable!(),
        };
        return Err(syn::Error::new(
            span,
            format!(
                "nros::main!: `custom_tasks = [...]` is only valid for the RTIC framework \
                 (current framework: {framework_label}). \
                 RTIC splices each ident as a `#[task]` inside the generated `mod __nros_app` \
                 body; other frameworks have no equivalent splice point."
            ),
        ));
    }

    // --- Phase 228.G — per-tier resolution state (RFC-0032 §6) ---
    // Populated only in the launch arm (where bringup_dir + node pkgs are in
    // scope). `node_groups` maps a node *instance* name → its declared callback
    // groups; `node_instances` is every launch node name (for the
    // instance-identity check vs `[[component]]`). `resolved_tiers` stays `None`
    // unless `system.toml` declares `[tiers.*]`.
    let mut node_groups: BTreeMap<String, Vec<CallbackGroupDecl>> = BTreeMap::new();
    let mut node_instances: Vec<String> = Vec::new();
    let mut resolved_tiers: Option<ResolvedTierTable> = None;
    // Phase 264 W2 — `[lifecycle]` boot autostart from `system.toml` (launch arm only).
    let mut lifecycle_code: Option<u8> = None;
    // Phase 264 W4b — `[param_services]` declared in `system.toml` (launch arm only) →
    // register the ROS 2 param services + seed the volatile store from the baked params.
    let mut param_services_enabled = false;
    // phase-267 W1c/C4 — when `system.toml` declares a `[[bridge]]` AND `nros sync`
    // has generated `<bringup>/nros-bridge.toml`, the entry is a cross-RMW bridge:
    // the macro emits a `run_from_config_str(include_str!(<that file>))` main
    // instead of the ordinary register/spin entry. Holds the absolute path.
    let mut bridge_config_path: Option<PathBuf> = None;
    // Issue 0106 — the RMW backends a `[[bridge]]` Entry uses, read from
    // `system.toml` (resolved via `[[bridge]]` endpoints + `[[domain]]` rmws).
    // The macro emits `nros_rmw_<x>::register()` for each so the linker doesn't
    // dead-strip the backend's self-register `.init_array` ctor (the data-driven
    // `run_from_config` path references no backend symbol on its own).
    let mut bridge_rmws: Vec<String> = Vec::new();
    // Phase 264 W4a — per-node launch `<param name=… value=…/>` initials, parallel to
    // `pkg_idents` (launch arm only). Each entry is the baked `(name, value)` slice that
    // seeds the node's `NodeContext::param` at register time (RFC-0004 §10). Empty in the
    // self-bringup arm (no launch file → no `<param>`).
    let mut node_param_bakes: Vec<Vec<(String, String)>> = Vec::new();
    // Phase 268 W1 — per-node launch `<node name= namespace=>` identity, parallel to
    // `node_param_bakes` (launch arm only). Each entry is the baked `(name, namespace)`
    // pair that `ExecutorSink::create_node` uses instead of the `NodeOptions` default
    // (RFC-0046). Empty (`None`) in the self-bringup arm.
    let mut node_identity_bakes: Vec<(String, String)> = Vec::new();

    // --- Launch resolution → list of <pkg_ident> register calls ---
    let pkg_idents: Vec<Ident> = match &args.launch {
        None => {
            // Form 1 / Form 2 — single-node self-bringup. Use this
            // pkg's own name as the register target. The codegen-
            // emitted ident is snake_case; we read CARGO_PKG_NAME and
            // sanitise via the same `-` → `_` rule the existing
            // `nros::node!()` macro uses (see `lib.rs::sanitize_pkg_name_for_symbol`).
            //
            // The bin target depends on the lib target of the same
            // pkg via Cargo's automatic `extern crate <my_pkg>;`,
            // so `<my_pkg>::register(runtime)?;` resolves at build
            // time.
            let pkg_name = std::env::var("CARGO_PKG_NAME").map_err(|_| {
                syn::Error::new(Span::call_site(), "nros::main!: CARGO_PKG_NAME not set")
            })?;
            let crate_ident = pkg_to_crate_ident(&pkg_name);
            // Form 1 self-bringup is opt-in: the user's lib crate
            // must expose a `pub fn register(runtime)`. If this
            // Entry pkg is binary-only (no companion lib), the
            // emitted `<this_pkg>::register(...)` will fail to
            // compile with a clear error; we don't try to detect
            // that here.
            vec![Ident::new(&crate_ident, Span::call_site())]
        }
        Some(launch_lit) => {
            let launch_value = launch_lit.value();
            // Walk the workspace from the Entry pkg's manifest dir.
            let workspace_root =
                nros_pkg_index::detect_workspace_root(&manifest_dir).map_err(|e| {
                    syn::Error::new(
                        launch_lit.span(),
                        format!("nros::main!: detect_workspace_root: {e}"),
                    )
                })?;
            let pkg_index = nros_pkg_index::build_pkg_index(&workspace_root).map_err(|e| {
                syn::Error::new(
                    launch_lit.span(),
                    format!("nros::main!: build_pkg_index: {e}"),
                )
            })?;
            // Track every package.xml the index walked.
            for (_, pkg_dir) in pkg_index.pkgs() {
                tracked.push(pkg_dir.join("package.xml"));
            }

            // Split `"bringup_pkg[:file.launch.xml]"`.
            let (bringup_name, file_override) = match launch_value.split_once(':') {
                Some((b, f)) => (b.trim().to_string(), Some(f.trim().to_string())),
                None => (launch_value.trim().to_string(), None),
            };
            if bringup_name.is_empty() {
                return Err(syn::Error::new(
                    launch_lit.span(),
                    "nros::main!: empty bringup pkg name in `launch = \"...\"`",
                ));
            }
            let bringup_dir = pkg_index
                .resolve_pkg(&bringup_name)
                .map_err(|e| syn::Error::new(launch_lit.span(), format!("nros::main!: {e}")))?;

            // Phase 264 W2 — read `[lifecycle]` so the macro can emit the REP-2002
            // service registration + autostart (mirrors the bake). Unconditional
            // (independent of the launch-file override below). Phase 264 W4b — also
            // read `[param_services]` so the macro can emit the parameter-service
            // registration + store seeding.
            {
                let st = bringup_dir.join("system.toml");
                if st.exists() {
                    tracked.push(st.clone());
                    lifecycle_code = read_lifecycle_autostart(&st);
                    param_services_enabled = read_param_services_enabled(&st);
                    if read_has_bridge(&st) {
                        let cfg = bringup_dir.join("nros-bridge.toml");
                        if cfg.exists() {
                            tracked.push(cfg.clone());
                            bridge_config_path = Some(cfg);
                            // Issue 0106 — collect the bridge's RMW backends so
                            // the entry force-registers them (anti dead-strip).
                            bridge_rmws = read_bridge_rmws(&st);
                        }
                    }
                }
            }

            // Resolve the launch file. If no override, consult
            // `system.toml::[system] default_launch` (default
            // `system.launch.xml`).
            let launch_filename = match file_override {
                Some(s) => s,
                None => {
                    let system_toml = bringup_dir.join("system.toml");
                    if system_toml.exists() {
                        tracked.push(system_toml.clone());
                        read_default_launch(&system_toml)
                            .map_err(|e| {
                                syn::Error::new(
                                    launch_lit.span(),
                                    format!("nros::main!: parse `{}`: {e}", system_toml.display()),
                                )
                            })?
                            .unwrap_or_else(|| "system.launch.xml".to_string())
                    } else {
                        "system.launch.xml".to_string()
                    }
                }
            };
            let launch_path = bringup_dir.join("launch").join(&launch_filename);
            tracked.push(launch_path.clone());
            if !launch_path.exists() {
                return Err(syn::Error::new(
                    launch_lit.span(),
                    format!(
                        "nros::main!: launch file not found: `{}`",
                        launch_path.display()
                    ),
                ));
            }

            // Parse the launch file via N.11.
            let arg_overrides: Vec<(String, String)> = args.args.clone();
            let desc =
                nros_launch_parser::parse_launch_file(&launch_path, &pkg_index, &arg_overrides)
                    .map_err(|e| {
                        syn::Error::new(
                            launch_lit.span(),
                            format!(
                                "nros::main!: parse launch file `{}`: {e}",
                                launch_path.display()
                            ),
                        )
                    })?;

            // Walk every `<node>` (top-level + inside groups) and
            // resolve to its Rust crate ident.
            let mut node_specs = Vec::new();
            for n in &desc.nodes {
                node_specs.push(n.clone());
            }
            for g in &desc.groups {
                for n in &g.nodes {
                    node_specs.push(n.clone());
                }
            }

            // Phase 211.F — multi-host partition: when `host = "<id>"` is set,
            // keep only this host's nodes (`<node machine="<id>">`) + all
            // unhosted (shared) nodes. Mirrors `nros codegen entry --host` /
            // `Plan::for_host`, giving the macro path parity so a per-host Entry
            // pkg (`nros::main!(launch = …, host = "<id>")`) bakes a runnable
            // single-host slice of a multi-host launch.
            if let Some(host) = args.host.as_deref() {
                node_specs.retain(|n| match &n.machine {
                    Some(m) => m == host,
                    None => true,
                });
                if node_specs.is_empty() {
                    return Err(syn::Error::new(
                        launch_lit.span(),
                        format!(
                            "nros::main!(host = {host:?}): no nodes for this host in `{}` \
                             (no `<node machine={host:?}>` and no unhosted/shared node)",
                            launch_path.display()
                        ),
                    ));
                }
            }

            let mut idents = Vec::new();
            for node in node_specs {
                // Look up the node pkg in the index, then derive the
                // Rust crate ident from its `package.xml::<name>`.
                let node_pkg_dir = pkg_index.resolve_pkg(&node.pkg).map_err(|e| {
                    syn::Error::new(
                        launch_lit.span(),
                        format!("nros::main!: node pkg `{}`: {e}", node.pkg),
                    )
                })?;
                tracked.push(node_pkg_dir.join("package.xml"));
                // Is this a Rust pkg? Check for `Cargo.toml`. If
                // missing, surface a clear error — C++ Node pkgs are
                // out of scope for v1.
                let cargo_toml = node_pkg_dir.join("Cargo.toml");
                if !cargo_toml.exists() {
                    return Err(syn::Error::new(
                        launch_lit.span(),
                        format!(
                            "nros::main!: node pkg `{}` has no Cargo.toml at `{}`. \
                             C++ Node pkgs are not yet supported in Rust Entry pkgs.",
                            node.pkg,
                            node_pkg_dir.display()
                        ),
                    ));
                }
                let crate_ident = pkg_to_crate_ident(&node.pkg);
                idents.push(Ident::new(&crate_ident, Span::call_site()));

                // Phase 264 W4a — bake this node's launch `<param>` initials (RFC-0004 §10).
                // Parallel to `idents`: index i's params seed pkg `idents[i]`'s NodeContext.
                node_param_bakes.push(
                    node.params
                        .iter()
                        .map(|p| (p.name.clone(), p.value.clone()))
                        .collect(),
                );

                // Phase 228.G — collect the node instance + its declared
                // callback groups for tier resolution. The instance name keys
                // the group map (RFC-0032 §7); when the launch `<node>` has no
                // explicit name, fall back to the executable name.
                let instance = node
                    .name
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| node.exec.clone());

                // Phase 268 W1 — bake the launch `<node name= namespace=>` identity
                // (RFC-0046). The instance name is already resolved above (same logic as
                // the W4a param rail). Namespace: `node.namespace` when present and
                // non-empty; default `"/"` (ROS convention). `NodeSpec::namespace` is
                // `Option<String>` in the launch parser.
                {
                    let baked_name = instance.clone();
                    let baked_ns = node
                        .namespace
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .unwrap_or("/")
                        .to_string();
                    node_identity_bakes.push((baked_name, baked_ns));
                }
                let groups = read_node_callback_groups(&cargo_toml);
                if !groups.is_empty() {
                    node_groups.insert(instance.clone(), groups);
                }
                node_instances.push(instance);
            }

            // Phase 228.G — resolve the tier table from `system.toml` when it
            // declares `[tiers.*]`. No tiers → leave `resolved_tiers = None`
            // (the single-tier `BoardEntry::run` emit, byte-identical).
            let system_toml = bringup_dir.join("system.toml");
            if system_toml.exists() {
                let cfg = read_system_tier_config(&system_toml).map_err(|e| {
                    syn::Error::new(
                        launch_lit.span(),
                        format!(
                            "nros::main!: parse tiers in `{}`: {e}",
                            system_toml.display()
                        ),
                    )
                })?;
                if cfg.has_tiers {
                    tracked.push(system_toml.clone());
                    // Instance-identity (RFC-0032 §7): every launch node that
                    // carries callback groups must name a `[[component]]`.
                    let component_names: BTreeSet<&str> =
                        cfg.component_names.iter().map(String::as_str).collect();
                    for inst in node_groups.keys() {
                        if !component_names.contains(inst.as_str()) {
                            return Err(syn::Error::new(
                                launch_lit.span(),
                                format!(
                                    "nros::main!: launch node `{inst}` declares callback groups \
                                     but is not a `[[component]]` in `{}` (the launch \
                                     `<node name>` must match a `system.toml` component name).",
                                    system_toml.display()
                                ),
                            ));
                        }
                    }
                    let rtos = derive_target_rtos(deploy_for_framework.as_deref());
                    let table = resolve_tiers(
                        &cfg.tiers,
                        &cfg.node_overrides,
                        &component_names,
                        &node_groups,
                        &rtos,
                    )
                    .map_err(|e| {
                        syn::Error::new(
                            launch_lit.span(),
                            format!("nros::main!: tier resolution: {e}"),
                        )
                    })?;
                    resolved_tiers = Some(table);
                }
            }

            idents
        }
    };

    // De-duplicate the tracked list — pkg-index walks can revisit
    // a pkg dir's `package.xml` from multiple paths.
    tracked.sort();
    tracked.dedup();

    // --- Emit ---
    let tracked_consts = tracked.iter().filter_map(|p| {
        // Skip non-existent paths. The macro may have stuffed a
        // path that the filesystem doesn't surface (e.g. a missing
        // `package.xml` when build_pkg_index synthesised a dir).
        // `include_bytes!` on a missing path is a hard compile
        // error, so be defensive.
        if !p.exists() {
            return None;
        }
        let s = p.to_string_lossy().into_owned();
        let lit = LitStr::new(&s, Span::call_site());
        Some(quote! {
            const _: &[u8] = ::core::include_bytes!(#lit);
        })
    });

    let register_calls: Vec<proc_macro2::TokenStream> = pkg_idents
        .iter()
        .enumerate()
        .map(|(i, ident)| {
            // Phase 264 W4a — set `runtime.params` to this node's baked launch `<param>`
            // initials (a promoted `&'static` slice) before the register call, so the
            // node's `register`/`init` observes its launch values via `ctx.param(name)`.
            // Empty (self-bringup arm, or a node with no `<param>`) → reset to `&[]` so a
            // prior node's params never leak into the next register.
            let params = node_param_bakes.get(i).cloned().unwrap_or_default();
            let param_lits = params.iter().map(|(name, value)| {
                let n = LitStr::new(name, Span::call_site());
                let v = LitStr::new(value, Span::call_site());
                quote! { (#n, #v) }
            });
            // Phase 268 W1 — set `runtime.node_identity` to this node's launch
            // `<node name= namespace=>` identity before the register call (RFC-0046).
            // `None` in the self-bringup arm (no launch, `node_identity_bakes` empty) so
            // no identity leaks between components (same discipline as the params reset).
            let identity_emit = match node_identity_bakes.get(i) {
                Some((name, ns)) => {
                    let n = LitStr::new(name, Span::call_site());
                    let s = LitStr::new(ns, Span::call_site());
                    quote! {
                        runtime.node_identity = ::core::option::Option::Some((#n, #s));
                    }
                }
                None => quote! {
                    runtime.node_identity = ::core::option::Option::None;
                },
            };
            quote! {
                runtime.params = &[ #( #param_lits ),* ];
                #identity_emit
                ::#ident::register(runtime)?;
            }
        })
        .collect();
    // Node count for the Zephyr framework boot banner (literal baked at
    // expansion time so the runtime body needs no extra import).
    let num_register_calls = register_calls.len();

    // Phase 264 W2 — `[lifecycle]` wiring: when `system.toml` declares it, register
    // the REP-2002 services + drive boot autostart right after the per-node
    // `register` calls (the executor is built, the nodes are installed). No-op token
    // stream when absent. `apply_lifecycle` is a no-op unless the Entry enabled
    // `nros/lifecycle-services`, so this is inert without the feature.
    let lifecycle_call: proc_macro2::TokenStream = match lifecycle_code {
        Some(code) => quote! {
            runtime.apply_lifecycle(#code).map_err(
                |_| ::nros::__macro_support::nros_platform::RuntimeError::NodeRegister("lifecycle"),
            )?;
        },
        None => quote! {},
    };

    // Phase 264 W4b — `[param_services]` wiring: when `system.toml` declares it, register
    // the 6 ROS 2 parameter services + seed the volatile param store with the aggregate
    // of every node's launch-baked `<param>` initials, right after the per-node `register`
    // calls. No-op token stream when absent. `apply_param_services` is a no-op unless the
    // Entry enabled `nros/param-services`, so this is inert without the feature. The seed
    // values are the raw launch strings; the runtime infers each `ParameterValue` type.
    let param_services_call: proc_macro2::TokenStream = if param_services_enabled {
        let seed_lits = node_param_bakes.iter().flatten().map(|(name, value)| {
            let n = LitStr::new(name, Span::call_site());
            let v = LitStr::new(value, Span::call_site());
            quote! { (#n, #v) }
        });
        quote! {
            runtime.apply_param_services(&[ #( #seed_lits ),* ]).map_err(
                |_| ::nros::__macro_support::nros_platform::RuntimeError::NodeRegister("param_services"),
            )?;
        }
    } else {
        quote! {}
    };

    // Phase 228.G (RFC-0032 §5) — the OwnedSpin entry call. Multi-tier
    // (`[tiers.*]` present, more than the synthesized `default` tier) emits
    // `<Board>::run_tiers(TIERS, register-only-closure)`; the board owns the
    // per-tier spin. Single-tier / no tiers keeps the unchanged
    // `BoardEntry::run` path (`setup` owns the bounded hosted spin) — so the
    // emitted TU is byte-identical to pre-228 for every current example.
    // Issue #48 cause 1 — bake the deploy overlay from
    // `[package.metadata.nros.deploy.<board>]`. Only Form 1 (deploy key present)
    // has a board key to read; Form 2 (explicit `board = X`) gets an all-`None`
    // overlay, so `run_with_deploy` then behaves exactly like `run`.
    let mut deploy_overlay_lit = match deploy_for_framework.as_deref() {
        Some(board_key) => read_deploy_overlay(&manifest_dir.join("Cargo.toml"), board_key),
        None => DeployOverlayLit::default(),
    };
    // Issue #98 — a single-node launch names the primary session (the ROS graph
    // node name) after that node, instead of the board default `"node"`. With
    // multiple nodes they share one primary session, so naming it after one would
    // be wrong — per-node naming is the deferred multi-node piece, so leave the
    // overlay name unset and the board keeps `"node"`.
    if let [only] = node_instances.as_slice() {
        deploy_overlay_lit.node_name = Some(only.clone());
    }
    let deploy_overlay_ts = deploy_overlay_tokens(&deploy_overlay_lit);

    // Issue #129 (RFC-0031 C5b amendment) — explicit backend register for the
    // Zephyr framework arm. On every OwnedSpin board the BOARD's boot path owns
    // `nros_rmw_<x>::register()` (Phase 248 C5a); Zephyr has no BoardEntry and
    // its board crate is NetworkWait-only, so registration must come from the
    // entry itself. `.init_array` ctors are compiled out on `target_os = "none"`
    // (this includes native_sim's `x86_64-unknown-none`), so without this emit
    // the CFFI registry stays empty and `Executor::open` fails
    // `Transport(ConnectionFailed)` (NoBackend). The entry deps the concrete
    // backend under its `rmw-<x>` feature (C5b), keeping the crate + this call
    // resolvable; unknown rmw names emit nothing (data-driven fallbacks).
    let zephyr_rmw_register_ts: proc_macro2::TokenStream = deploy_for_framework
        .as_deref()
        .and_then(|board_key| read_deploy_rmw(&manifest_dir.join("Cargo.toml"), board_key))
        .and_then(|rmw| rmw_crate_ident(&rmw).map(|ident| (rmw, ident)))
        .map(|(rmw, crate_ident)| {
            let id = Ident::new(crate_ident, Span::call_site());
            // Gate on the entry's matching `rmw-<x>` cargo feature so an entry
            // built with a different `--features rmw-<y>` selection (the
            // Kconfig-driven multi-RMW shape) still compiles — the register
            // call cfg-outs together with the optional backend dep.
            let feature = format!("rmw-{rmw}");
            quote! {
                #[cfg(feature = #feature)]
                {
                    let _ = ::#id::register();
                }
            }
        })
        .unwrap_or_default();

    // W4b — bake the single-node boot identity into a `NROS_BOOT_CONFIG` static
    // placed in the `.nros_boot_config` linker section (bare-metal only; hosted
    // gets a plain `#[unsafe(no_mangle)] #[used]` static that is still referenceable).
    // The static is emitted once, before `body_ts`, so `&NROS_BOOT_CONFIG`
    // resolves in every framework arm's `deploy_overlay_ts` use site.
    let boot_config_static_ts = {
        let node_name_opt = match &deploy_overlay_lit.node_name {
            Some(s) => quote! { ::core::option::Option::Some(#s) },
            None => quote! { ::core::option::Option::None },
        };
        let locator_opt = match &deploy_overlay_lit.locator {
            Some(s) => quote! { ::core::option::Option::Some(#s) },
            None => quote! { ::core::option::Option::None },
        };
        let domain_opt = match deploy_overlay_lit.domain_id {
            Some(d) => quote! { ::core::option::Option::Some(#d) },
            None => quote! { ::core::option::Option::None },
        };
        quote! {
            #[used]
            #[cfg_attr(target_os = "none", unsafe(link_section = ".nros_boot_config"))]
            #[unsafe(no_mangle)]
            static NROS_BOOT_CONFIG: ::nros::BakedBootConfig =
                ::nros::BakedBootConfig::new(
                    #node_name_opt,
                    #locator_opt,
                    #domain_opt,
                    ::core::option::Option::None,
                );
        }
    };

    // Phase 244.D1 — `target_os = "none"` entry shape for the OwnedSpin
    // framework. FreeRTOS / threadx-linux have a C runtime that calls `main`,
    // so they keep the `extern "C" fn main`. A pure bare-metal Cortex-M image
    // has no C runtime; its reset vector needs a `#[cortex_m_rt::entry]`. Both
    // funnel through the shared `__nros_entry_run`.
    let none_entry_ts: proc_macro2::TokenStream =
        if is_baremetal_cortexm_deploy(deploy_for_framework.as_deref()) {
            quote! {
                #[cfg(target_os = "none")]
                #[::cortex_m_rt::entry]
                fn __nros_cortex_m_reset() -> ! {
                    // `run_with_deploy` loops forever on success (firmware
                    // lifetime) and exits via the board on spin error; reaching
                    // here means `setup()` returned `Err` before the spin loop.
                    let _ = __nros_entry_run();
                    loop {
                        ::cortex_m::asm::wfi();
                    }
                }
            }
        } else {
            quote! {
                #[cfg(target_os = "none")]
                #[unsafe(no_mangle)]
                pub extern "C" fn main() -> i32 {
                    match __nros_entry_run() {
                        ::core::result::Result::Ok(()) => 0,
                        ::core::result::Result::Err(_) => 1,
                    }
                }
            }
        };

    // phase-271 (issue #110) — per-entry executor sizing from the entry's own
    // `[package.metadata.nros.entry] max_callbacks`. `None` → default sizing
    // (byte-identical to pre-271); `Some` → emit the `_sized` board entry so the
    // hosted board opens via `Executor::open_sized`.
    let exec_sizing = read_entry_executor_sizing(&manifest_dir.join("Cargo.toml"));

    let multi_tier = resolved_tiers.as_ref().filter(|t| !t.is_single_tier());
    let entry_call: proc_macro2::TokenStream = match multi_tier {
        Some(table) => {
            let tiers_ts = tier_specs_tokens(table);
            quote! {
                <#board_path>::run_tiers(
                    // Issue #48 cause 1 — thread the deploy overlay into the
                    // multi-tier path too (firmware boards apply it to their boot
                    // `Config`; hosted boards ignore it).
                    &#deploy_overlay_ts,
                    #tiers_ts,
                    |runtime: &mut ::nros::__macro_support::nros_platform::RuntimeCtx<'_>|
                        -> ::core::result::Result<
                            (),
                            ::nros::__macro_support::nros_platform::RuntimeError,
                        >
                    {
                        // Register-only: the board sets each tier's
                        // `active_groups` filter and owns the spin loop.
                        // W4c — param services BEFORE the node registers, so the store
                        // exists when each cell captures it (cell → `ctx.parameter`).
                        #param_services_call
                        #( #register_calls )*
                        #lifecycle_call
                        ::core::result::Result::Ok(())
                    },
                )
            }
        }
        None => {
            // The register/spin closure — identical for the sized + unsized
            // board calls below.
            let setup_closure = quote! {
                |runtime: &mut ::nros::__macro_support::nros_platform::RuntimeCtx<'_>|
                    -> ::core::result::Result<
                        (),
                        ::nros::__macro_support::nros_platform::RuntimeError,
                    >
                {
                    // W4c — param services BEFORE the node registers, so the store
                    // exists when each cell captures it (cell → `ctx.parameter`).
                    #param_services_call
                    #( #register_calls )*
                    #lifecycle_call
                    #[cfg(not(target_os = "none"))]
                    __nros_hosted_spin_if_requested(runtime)?;
                    ::core::result::Result::Ok(())
                }
            };
            // Issue #48 cause 1 — `run_with_deploy{,_sized}` applies the
            // deploy-metadata overlay (locator / ip / gateway / domain) to the
            // board's boot config. The default trait bodies ignore the overlay +
            // sizing and forward to `run`, so hosted / framework boards are
            // byte-identical; the FreeRTOS / bare-metal boards override
            // `run_with_deploy`, and the posix board overrides
            // `run_with_deploy_sized` (phase-271, issue #110) to open at the
            // entry's declared `max_callbacks`.
            match exec_sizing {
                ::core::option::Option::Some((max_cbs, max_sc)) => quote! {
                    <#board_path as ::nros::__macro_support::nros_platform::BoardEntry>::run_with_deploy_sized(
                        &#deploy_overlay_ts,
                        #max_cbs,
                        #max_sc,
                        #setup_closure,
                    )
                },
                ::core::option::Option::None => quote! {
                    <#board_path as ::nros::__macro_support::nros_platform::BoardEntry>::run_with_deploy(
                        &#deploy_overlay_ts,
                        #setup_closure,
                    )
                },
            }
        }
    };

    // Phase 216 final wave — Node-pkg registration for framework
    // targets (RTIC + Embassy). The OwnedSpin branch keeps its
    // existing `<pkg>::register(runtime)?;` flow inside the
    // `BoardEntry::run` closure; the framework branches instead emit
    // `<pkg>::register_dispatch(&mut executor)?;` calls into the
    // generated `#[init]` body, which push the per-Node
    // `(state, on_callback)` pair into the `Executor`'s dispatch-slot
    // registry that the framework dispatch task walks.
    //
    // Source of truth: `[package.metadata.nros.entry] node_pkgs =
    // ["pkg_a", "pkg_b"]` in the Entry pkg's `Cargo.toml`. When the
    // key is absent we fall back to the Entry pkg's own name with a
    // conventional `_entry` suffix stripped — covers the common
    // self-bringup shape where a single Node pkg `foo_pkg` has a
    // sibling Entry pkg `foo_pkg_entry` (though the framework
    // examples today don't follow this — they always declare
    // `node_pkgs = [...]` explicitly).
    let framework_node_pkg_idents: Vec<Ident> = match framework {
        // OwnedSpin (incl. NuttX) + Zephyr + Esp32 all register via the
        // launch-resolved `register_calls` (the `RuntimeCtx`-based
        // `<pkg>::register` flow), NOT the RTIC/Embassy
        // `register_dispatch` splice.
        Framework::OwnedSpin | Framework::Zephyr | Framework::Esp32 => Vec::new(),
        Framework::Rtic | Framework::Embassy => {
            let cargo_toml = manifest_dir.join("Cargo.toml");
            let from_metadata = read_entry_node_pkgs(&cargo_toml).map_err(|e| {
                syn::Error::new(
                    Span::call_site(),
                    format!(
                        "nros::main!: failed to read `[package.metadata.nros.entry] node_pkgs` \
                         from `{}`: {e}",
                        cargo_toml.display()
                    ),
                )
            })?;
            let names: Vec<String> = match from_metadata {
                Some(v) => v,
                None => {
                    // Self-bringup fallback: strip `_entry` suffix
                    // from the Entry pkg's own name.
                    let pkg_name = std::env::var("CARGO_PKG_NAME").map_err(|_| {
                        syn::Error::new(Span::call_site(), "nros::main!: CARGO_PKG_NAME not set")
                    })?;
                    let stripped = pkg_name
                        .strip_suffix("_entry")
                        .or_else(|| pkg_name.strip_suffix("-entry"))
                        .unwrap_or(&pkg_name);
                    vec![stripped.to_string()]
                }
            };
            names
                .into_iter()
                .map(|n| Ident::new(&pkg_to_crate_ident(&n), Span::call_site()))
                .collect()
        }
    };
    let framework_register_dispatch_calls: Vec<proc_macro2::TokenStream> =
        framework_node_pkg_idents
            .iter()
            .map(|ident| {
                quote! {
                    ::#ident::register_dispatch(&mut executor)
                        .expect("nros::main!: register_dispatch — executor dispatch-slot table full");
                }
            })
            .collect();
    // Phase 289 (#178) — Rtic entity registration. `register_dispatch` only
    // installs the on_callback trampoline into the dispatch-slot table; it
    // never runs `Node::register(ctx)` — so an RTIC image opened its session
    // but owned NO node/publisher/timer entities and published nothing (the
    // phase-216 B.3 "per-Node register wiring" follow-up). Route through the
    // same owned-spin `<pkg>::register(&mut RuntimeCtx)` seam every other
    // board uses (`install_node_typed`: entities + tick registry + the
    // component table `dispatch_callback` scans).
    let framework_register_entity_calls: Vec<proc_macro2::TokenStream> = framework_node_pkg_idents
        .iter()
        .map(|ident| {
            quote! {
                ::#ident::register(&mut __nros_rt)
                    .expect("nros::main!: Node register failed on the RTIC entry");
            }
        })
        .collect();

    // Phase 216.B.3 — framework-dispatched emit body. `OwnedSpin`
    // keeps the long-standing `fn __nros_entry_run + fn main` shape
    // (BoardEntry::run owns the spin loop). `Rtic` emits a
    // `#[rtic::app(...)]` skeleton that delegates to
    // `RticBoardEntry::init_hardware` from the framework-generated
    // `#[init]` body. `Embassy` is a hard error pointing at the
    // 216.C.3 sibling that lands the emit body.
    // issue #128 (half 2) — the Zephyr arm's spin-or-tiers tail. Multi-tier
    // systems route through `ZephyrBoard::run_tiers` (one k_thread per tier
    // over the one shared session, RFC-0015 Model 1); single-tier keeps the
    // plain register+spin scaffold, byte-identical to pre-#128-half-2.
    let zephyr_body_tail: proc_macro2::TokenStream = match multi_tier {
        Some(table) => {
            let tiers_ts = tier_specs_tokens(table);
            quote! {
                return ::nros_board_zephyr::ZephyrBoard::run_tiers(
                    &config,
                    #tiers_ts,
                    |runtime: &mut ::nros::__macro_support::nros_platform::RuntimeCtx<'_>|
                        -> ::core::result::Result<
                            (),
                            ::nros::__macro_support::nros_platform::RuntimeError,
                        >
                    {
                        // Same per-tier closure sequence as the OwnedSpin
                        // multi-tier path: param services BEFORE the node
                        // registers (the store must exist when each cell
                        // captures it), lifecycle AFTER; the board sets each
                        // tier's `active_groups` filter and owns the spin.
                        #param_services_call
                        #( #register_calls )*
                        #lifecycle_call
                        ::core::result::Result::Ok(())
                    },
                );
            }
        }
        None => quote! {
            let executor = match ::nros::Executor::open(&config) {
                ::core::result::Result::Ok(executor) => executor,
                ::core::result::Result::Err(e) => {
                    ::log::error!("nros: zephyr entry — executor open failed: {:?}", e);
                    return ::core::result::Result::Err(
                        ::nros::__macro_support::nros_platform::RuntimeError::Spin,
                    );
                }
            };
            let mut node_runtime = ::nros::ExecutorNodeRuntime::from_executor(executor);
            let mut ctx = ::nros::__macro_support::nros_platform::RuntimeCtx::with_runtime(
                &mut node_runtime,
            );
            let runtime = &mut ctx;
            // Issue #128 — OwnedSpin parity for the capability emits. Param
            // services BEFORE the node registers (the store must exist when
            // each cell captures it — W4c), lifecycle AFTER (the executor is
            // built, the nodes are installed). Both are inert token streams
            // when system.toml doesn't declare them, and no-ops without the
            // `nros/param-services` / `nros/lifecycle-services` features, so
            // plain pub/sub Zephyr entries are byte-identical to pre-#128.
            #param_services_call
            #( #register_calls )*
            #lifecycle_call
            ::log::info!(
                "nros: zephyr workspace entry up ({} nodes)",
                #num_register_calls
            );

            // Forever-spin: native_sim is `no_std`, so the OwnedSpin
            // `NROS_ENTRY_*` bounded path (which needs `std::time`) does
            // not apply. The runtime drives the launch node set (the
            // talker's timer publishes); the workspace E2E observes
            // delivery from an external listener and stops the process.
            loop {
                let _ = runtime.runtime.spin_once(10);
            }
        },
    };

    let body_ts: proc_macro2::TokenStream = match framework {
        Framework::OwnedSpin => quote! {
            // Phase 213.C follow-up — emit two cfg-gated entry shapes so
            // both hosted (POSIX / NuttX / threadx-linux) and embedded
            // (FreeRTOS / bare-metal `target_os = "none"`) targets resolve
            // a working `main`. The shared body is factored into a private
            // `__nros_entry_run` returning `Result` so neither arm
            // duplicates the closure logic.
            fn __nros_entry_run() -> ::core::result::Result<
                (),
                ::nros::__macro_support::nros_platform::RuntimeError,
            > {
                // Phase 244.D1 — custom-transport install seam. Install a
                // board custom transport (e.g. the XRCE-over-UART vtable)
                // selected by `deploy.transport`, BEFORE the RMW registers.
                // XRCE's `set_custom_transport_ops` must precede `register`.
                // The default `setup_transport` is a no-op so boards with a
                // built-in transport are byte-identical. This call is always
                // emitted (it is not dead code): see `BoardEntry::setup_transport`
                // for the design rationale and the current override in
                // `nros-board-mps2-an385` (`xrce-transport` feature).
                #[cfg(target_os = "none")]
                <#board_path as ::nros::__macro_support::nros_platform::BoardEntry>::setup_transport(
                    &#deploy_overlay_ts,
                );
                // Phase 249 P1 — the RMW backend register is OWNED BY THE BOARD
                // (Phase 248 C5a: each board's boot path calls its linked
                // `nros_rmw_<x>::register()`, gated on the board's `rmw-<x>` feature),
                // ordered after `setup_transport` inside the board's `run`. The former
                // `#[cfg(target_os="none")] ::nros::__register_linked_rmw()` emit here
                // was a no-op vestige of the retired linkme-walk path — removed.
                #entry_call
            }

            // `#[allow(dead_code)]` — the multi-tier `run_tiers` entry path
            // (Phase 228.G) owns its own spin and never calls these, so they
            // are unused in that emit; harmless in the single-tier path.
            #[cfg(not(target_os = "none"))]
            #[allow(dead_code)]
            fn __nros_env_usize(name: &str, default: usize) -> usize {
                ::std::env::var(name)
                    .ok()
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(default)
            }

            #[cfg(not(target_os = "none"))]
            #[allow(dead_code)]
            fn __nros_hosted_spin_if_requested(
                runtime: &mut ::nros::__macro_support::nros_platform::RuntimeCtx<'_>,
            ) -> ::core::result::Result<
                (),
                ::nros::__macro_support::nros_platform::RuntimeError,
            > {
                let total_ms = __nros_env_usize("NROS_ENTRY_SPIN_MS", 0);
                if total_ms == 0 {
                    return ::core::result::Result::Ok(());
                }

                let step_ms = __nros_env_usize("NROS_ENTRY_SPIN_STEP_MS", 10)
                    .clamp(1, total_ms.max(1));
                let expect_messages = __nros_env_usize("NROS_ENTRY_EXPECT_MESSAGE_CALLBACKS", 0);
                let deadline = ::std::time::Instant::now()
                    + ::std::time::Duration::from_millis(total_ms as u64);

                loop {
                    runtime
                        .runtime
                        .spin_once(step_ms as u32)
                        .map_err(|_| ::nros::__macro_support::nros_platform::RuntimeError::Spin)?;

                    let (callbacks, messages) = runtime.runtime.observed_callback_counts();
                    if expect_messages > 0 && messages >= expect_messages {
                        ::std::println!(
                            "nros: hosted spin complete callbacks={callbacks} message_callbacks={messages}"
                        );
                        return ::core::result::Result::Ok(());
                    }

                    if ::std::time::Instant::now() >= deadline {
                        ::std::println!(
                            "nros: hosted spin complete callbacks={callbacks} message_callbacks={messages}"
                        );
                        if expect_messages > 0 {
                            return ::core::result::Result::Err(
                                ::nros::__macro_support::nros_platform::RuntimeError::Spin,
                            );
                        }
                        return ::core::result::Result::Ok(());
                    }
                }
            }

            #[cfg(not(target_os = "none"))]
            fn main() {
                if let ::core::result::Result::Err(e) = __nros_entry_run() {
                    ::std::eprintln!("{}: {}", ::core::env!("CARGO_PKG_NAME"), e);
                    ::std::process::exit(1);
                }
            }

            // Phase 244.D1 — `target_os = "none"` entry: `extern "C" fn main`
            // for C-runtime boards (FreeRTOS / threadx-linux), or a
            // `#[cortex_m_rt::entry]` reset for pure bare-metal Cortex-M.
            #none_entry_ts
        },
        // Phase 225.P — Zephyr framework. The RTOS owns boot + the C
        // `main`; the Rust staticlib exports `rust_main`, which
        // `zephyr-lang-rust`'s `rust_cargo_application()` invokes after
        // kernel + net init. There is NO `BoardEntry::run` and NO Rust
        // `fn main` (Zephyr forbids it). The launch file remains the
        // single source of truth for the node set: `register_calls`
        // registers each launch-named Node pkg, identical to the
        // native/freertos OwnedSpin shape — only the boot/spin scaffold
        // differs. Generalises the single-node
        // `nros::zephyr_component_main!` body to N launch-named nodes.
        Framework::Zephyr => quote! {
            // `rust_main` is the only entry symbol Zephyr links (the RTOS
            // owns boot + the C `main`). native_sim builds for
            // `x86_64-unknown-none` — a `no_std` target — so `std` is
            // unavailable; observability goes through the `log` facade,
            // routed to Zephyr's logger. Errors can't cross the C ABI, so
            // they are logged and the `Result` is dropped here.
            #[unsafe(no_mangle)]
            pub extern "C" fn rust_main() {
                // SAFETY: `set_logger` is callable once post-kernel-init.
                unsafe { let _ = ::zephyr::set_logger(); }
                let _ = __nros_zephyr_entry_run();
            }

            fn __nros_zephyr_entry_run() -> ::core::result::Result<
                (),
                ::nros::__macro_support::nros_platform::RuntimeError,
            > {
                // Carrier / link-up gate. Use the `nros_platform::zephyr::
                // wait_network` C-symbol wrapper (Phase 248 C7 step 1 — relocated
                // from `nros::platform::zephyr`) — it exposes a real linkable
                // symbol. (`ZephyrBoard::wait_link_up` calls Zephyr's
                // `net_if_is_up` / `k_msleep`, which are `static inline` header
                // functions with no link symbol, so the native_sim final link
                // fails with undefined references.)
                let _ = ::nros_platform::zephyr::wait_network(2000);

                // Issue #129 (RFC-0031 C5b amendment) — explicit backend register.
                // Zephyr has no BoardEntry boot path to own it (the C5a home) and
                // `.init_array` ctors are compiled out on `target_os = "none"`, so
                // the codegen emits the register from the entry's own backend dep
                // (`rmw-<x> = ["dep:nros-rmw-<x>"]`). Without it the CFFI registry
                // is empty and `Executor::open` fails Transport(ConnectionFailed).
                #zephyr_rmw_register_ts

                // Phase 249 P1 — RMW register is board-owned (Phase 248 C5a); the
                // backend-agnostic `nros` crate cannot register (no backend dep), so
                // the former no-op `::nros::__register_linked_rmw()` emit is removed.

                // Open the executor + wrap it in the dispatch runtime, then
                // register each launch-named Node pkg through a `RuntimeCtx`
                // — exactly the `<pkg>::register(runtime)?` flow the native
                // entry uses, so the launch file stays the single source of
                // truth.
                // Locator: `default_const()` = EMPTY locator → zenoh-pico
                // multicast scouting, which fails on native_sim NSOS (no
                // multicast; the offload layer never even issues a
                // `connect()`). The Zephyr target is `no_std`, so the
                // hosted `from_env()` is unavailable — bake the locator at
                // compile time via `option_env!("NROS_LOCATOR")`. The Entry
                // `build.rs` re-exports `CONFIG_NROS_ZENOH_LOCATOR` (the
                // same Kconfig the C API path consumes) into that env, so
                // Kconfig is the single source of truth for both languages.
                const BAKED_LOCATOR: ::core::option::Option<&str> =
                    ::core::option_env!("NROS_LOCATOR");
                // #166 / phase-286 W1 — native_sim test parallelism. The test
                // harness launches the image with `-testargs --nros-locator=<loc>`
                // and starts a per-test zenohd on that ephemeral port; preferring
                // it over the compile-time bake lets every test dial a DISTINCT
                // router, retiring the shared-baked-port serialization of the
                // ws-entry lane. Provided by `nros-platform-zephyr` (argv-backed,
                // process lifetime); returns NULL on real embedded → the bake
                // stands. Mirrors `nros::zephyr_component_main!`.
                unsafe extern "C" {
                    fn nros_runtime_locator_override() -> *const ::core::ffi::c_char;
                }
                let runtime_locator: ::core::option::Option<&str> = {
                    let p = unsafe { nros_runtime_locator_override() };
                    if p.is_null() {
                        ::core::option::Option::None
                    } else {
                        match unsafe { ::core::ffi::CStr::from_ptr(p) }.to_str() {
                            ::core::result::Result::Ok(s) if !s.is_empty() => {
                                ::core::option::Option::Some(s)
                            }
                            _ => ::core::option::Option::None,
                        }
                    }
                };
                let effective_locator = runtime_locator.or(match BAKED_LOCATOR {
                    ::core::option::Option::Some(loc) if !loc.is_empty() => {
                        ::core::option::Option::Some(loc)
                    }
                    _ => ::core::option::Option::None,
                });
                let config = match effective_locator {
                    ::core::option::Option::Some(loc) => {
                        ::nros::ExecutorConfig::new(loc)
                            .node_name(::core::env!("CARGO_PKG_NAME"))
                    }
                    _ => ::nros::ExecutorConfig::default_const()
                        .node_name(::core::env!("CARGO_PKG_NAME")),
                };
                // issue #128 (half 2) — spin-or-tiers tail: multi-tier
                // systems route through `ZephyrBoard::run_tiers`; single-tier
                // keeps the plain single-executor register+spin body.
                #zephyr_body_tail
            }
        },
        // Phase 225.O — ESP32-C3 (esp-hal) framework. esp-riscv-rt's
        // `_start` registers + jumps to the esp-hal entry, so the boot
        // symbol must be a `#[::esp_hal::main] fn main() -> !` — the
        // `OwnedSpin` bare `extern "C" fn main` does not boot. The board
        // ZST's real-runtime `BoardEntry::run` builds the `Config`,
        // brings up hardware + transport, opens the executor, registers
        // each launch-named Node pkg through the `RuntimeCtx` closure
        // (identical `<pkg>::register(runtime)?` flow to native/freertos
        // — the launch file stays the single source of truth), and spins
        // forever (`run` never returns on ESP32). The trailing `loop` is
        // defensive (satisfies `-> !`; unreachable in a working build).
        // The Entry crate provides the panic handler (`esp-backtrace`)
        // and app descriptor (`esp_app_desc!`).
        Framework::Esp32 => quote! {
            #[::esp_hal::main]
            fn main() -> ! {
                let _ = __nros_esp32_entry_run();
                #[allow(clippy::empty_loop)]
                loop {
                    ::core::hint::spin_loop();
                }
            }

            fn __nros_esp32_entry_run() -> ::core::result::Result<
                (),
                ::nros::__macro_support::nros_platform::RuntimeError,
            > {
                // Phase 244.D2 — `run_with_deploy` (not `run`) so the
                // `[package.metadata.nros.deploy.<board>]` overlay (locator / ip /
                // domain) reaches `BoardEntry::run_with_deploy`; with `run` the
                // overlay was inert and both esp32 nodes used the board-default
                // net. Boards without an override fall back to `run` via the
                // default trait body, so non-overlay esp32 builds are unchanged.
                <#board_path as ::nros::__macro_support::nros_platform::BoardEntry>::run_with_deploy(
                    &#deploy_overlay_ts,
                    |runtime: &mut ::nros::__macro_support::nros_platform::RuntimeCtx<'_>|
                        -> ::core::result::Result<
                            (),
                            ::nros::__macro_support::nros_platform::RuntimeError,
                        >
                    {
                        // Issue #128 — OwnedSpin parity: param services before
                        // the registers, lifecycle after. Inert without the
                        // system.toml declarations / cargo features.
                        #param_services_call
                        #( #register_calls )*
                        #lifecycle_call
                        ::core::result::Result::Ok(())
                    },
                )
            }
        },
        Framework::Rtic => {
            let deploy = deploy_for_framework.as_deref().ok_or_else(|| {
                syn::Error::new(
                    Span::call_site(),
                    "nros::main!: RTIC framework requires `[package.metadata.nros.entry] deploy`",
                )
            })?;
            let rtic_spec = rtic_board_spec_for(deploy).ok_or_else(|| {
                syn::Error::new(
                    Span::call_site(),
                    format!("nros::main!: missing RTIC board spec for deploy `{deploy}`"),
                )
            })?;
            let rtic_device = rtic_spec.device_path;
            let rtic_dispatchers = rtic_spec.dispatchers;
            let rtic_consumer = rtic_spec.dispatch_consumer_path;
            // Phase 289 (#178) — optional periodic-tick hardware task. The
            // `binds` route wires the handler through RTIC's real vector
            // table (the same mechanism the dispatchers use) — a board-crate
            // `#[exception] SysTick` does NOT get wired (verified in #178).
            let rtic_tick_ts: proc_macro2::TokenStream = match rtic_spec.tick_irq {
                Some(tick_irq) => quote! {
                    /// Phase 289 — periodic tick. Priority 2 so it PREEMPTS
                    /// the priority-1 `__nros_run` task: its whole job is
                    /// waking the `wfi` inside that task's connect/poll
                    /// busy-waits. The board acknowledges the IRQ in
                    /// `on_tick` (an unacknowledged flag is an IRQ storm).
                    #[task(binds = #tick_irq, priority = 2)]
                    fn __nros_tick(_cx: __nros_tick::Context) {
                        <__NrosBoard as ::nros::__macro_support::nros_platform::RticBoardEntry>::on_tick();
                    }
                },
                None => quote! {},
            };

            // Phase 216.B.3 SKELETON — `#[rtic::app(...)]` module that
            // delegates boot to `RticBoardEntry::init_hardware`. The
            // full body (the `__nros_spin` + `__nros_dispatch` software
            // tasks + per-Node register/spawn wiring) lands in a
            // 216.B.3 follow-up. Today's emit only needs to compile so
            // the route through `framework_for(deploy)` is observable
            // — runtime use surfaces the board crate's `todo!()` in
            // `init_hardware` (intentional).
            //
            // Phase 216.B.4 adds the `custom_tasks = [...]` splice:
            // each user-listed ident `f` becomes a thin `#[task]`
            // trampoline that awaits `super::<f>_impl(cx).await`. The
            // user supplies the impl fn (and its `Context` type-alias
            // arg — RTIC generates `<f>::Context` from the task ident)
            // outside the macro; the macro just declares the task.
            //
            // Hardcoded `dispatchers = [USART1, USART2]` matches
            // `RticStm32F4::DISPATCHERS`. A follow-up reads the const
            // from the board crate at macro-expansion time (requires a
            // build-graph fs round-trip we want to defer).
            //
            // The `#![no_std]`/`#![no_main]` inner attrs are NOT emitted
            // here — the Entry pkg's `main.rs` already declares those
            // at file scope (see the talker-rtic example). Emitting
            // them inside the macro would double-declare.
            let custom_task_items: Vec<proc_macro2::TokenStream> = args
                .custom_tasks
                .as_deref()
                .unwrap_or(&[])
                .iter()
                .map(|f| {
                    // Sibling impl fn name: `<f>_impl`. Defined at
                    // module scope outside the `mod __nros_app` body
                    // by the user — the trampoline reaches it via
                    // `super::<f>_impl`.
                    let impl_ident = Ident::new(&format!("{}_impl", f), f.span());
                    quote! {
                        #[task(priority = 1)]
                        async fn #f(cx: #f::Context) {
                            super::#impl_ident(cx).await;
                        }
                    }
                })
                .collect();

            quote! {
                use #board_path as __NrosBoard;

                #[::rtic::app(
                    device = #rtic_device,
                    dispatchers = [#( #rtic_dispatchers ),*]
                )]
                mod __nros_app {
                    use super::*;

                    // rtic 2.3.0 asserts `Send` on late `#[local]` resources
                    // (initialized in `#[init]`, claimed by a task at a
                    // different priority). The `rmw-cffi` `Executor<'static>`
                    // holds a raw `*mut CffiSession`, so it is `!Send`. These
                    // RTIC boards are single-core cortex-m built with
                    // `critical-section-single-core`: the executor/runtime
                    // never cross cores, so the `Send` requirement is a
                    // structural formality. Wrap the resources in a cell that
                    // is unconditionally `Send` to satisfy the bound.
                    struct __NrosLocalCell<T>(T);
                    // SAFETY: single-core cortex-m — no other core can observe
                    // this value; RTIC serializes access via priority ceilings.
                    unsafe impl<T> ::core::marker::Send for __NrosLocalCell<T> {}

                    #[shared]
                    struct Shared {}

                    /// Phase 216.B.3 follow-up — stashes the
                    /// `(Executor, Runtime)` pair returned by
                    /// `RticBoardEntry::init_hardware` so the
                    /// `__nros_spin` / `__nros_dispatch` software
                    /// tasks can take ownership through the RTIC
                    /// `local = [<field>]` attribute. The assoc-type
                    /// projection keeps the macro emit board-agnostic
                    /// — every `RticBoardEntry` impl picks its own
                    /// concrete `Executor` / `Runtime` types.
                    #[local]
                    struct Local {
                        // #178 — the executor's zenoh session open is a BLOCKING
                        // connect (smoltcp poll loop needs the timer + RX IRQ). RTIC
                        // runs `#[init]` with interrupts masked, so opening there
                        // deadlocks the handshake. Instead `#[init]` stashes the
                        // board's `Boot` carrier (hardware already up, no network
                        // I/O) and the `__nros_run` task opens the executor on its
                        // first poll — after `init` returns and interrupts unmask.
                        // `Option` so the task can move the `Boot` out of `#[local]`.
                        boot: __NrosLocalCell<::core::option::Option<<__NrosBoard as ::nros::__macro_support::nros_platform::RticBoardEntry>::Boot>>,
                        runtime: __NrosLocalCell<<__NrosBoard as ::nros::__macro_support::nros_platform::RticBoardEntry>::Runtime>,
                    }

                    #[init]
                    fn init(cx: init::Context) -> (Shared, Local) {
                        // Phase 216.B.3 follow-up — board bring-up
                        // hands back the `(Executor, Runtime)` pair;
                        // we stash both into `Local` and spawn the two
                        // software tasks that will drive them. The
                        // `run_plan(&mut runtime)` per-Node
                        // register-dispatch-slot call still belongs to
                        // a separate follow-up (the trampoline
                        // registration story spans the macro +
                        // `nros::node!()` emit + the runtime trait
                        // surface; same deferred story as the Embassy
                        // sibling in the C.3 follow-up).
                        // Phase 244.D1 — thread the `[deploy.<board>]` overlay
                        // into the RTIC `#[init]` so each Entry pkg pins its own
                        // ip / locator (the default impl ignores it, so boards
                        // without a baked net Config are unchanged).
                        //
                        // #178 — `init_hardware_with_deploy` now brings up the
                        // hardware and returns a `Boot` carrier WITHOUT opening the
                        // executor (no blocking network I/O here — interrupts are
                        // masked). The `__nros_run` task calls `open_executor(boot)`
                        // + `register_dispatch` once interrupts are live.
                        let (boot, runtime) =
                            <__NrosBoard as ::nros::__macro_support::nros_platform::RticBoardEntry>::init_hardware_with_deploy(
                                cx.device,
                                cx.core,
                                &#deploy_overlay_ts,
                            );
                        __nros_run::spawn().unwrap();
                        (
                            Shared {},
                            Local {
                                boot: __NrosLocalCell(::core::option::Option::Some(boot)),
                                runtime: __NrosLocalCell(runtime),
                            },
                        )
                    }

                    /// RTIC run task — collapsed `__nros_spin` +
                    /// `__nros_dispatch` (Phase 216.B.3 follow-up).
                    ///
                    /// The earlier split-task shape had `__nros_spin`
                    /// own the `Executor` half and `__nros_dispatch`
                    /// own the `Runtime` half. RTIC `#[local]` fields
                    /// are claimed by a single task (the
                    /// `local = [<field>]` attribute is exclusive),
                    /// and the dispatch task needs the executor to
                    /// drive the per-Node trampolines that run inside
                    /// the executor's spin loop. Collapsing into one
                    /// task that owns both fields side-steps the
                    /// exclusivity rule and gives the spin / dequeue
                    /// loop a single coherent borrow.
                    ///
                    /// Body:
                    ///   1. Claim the board's SPSC consumer half via
                    ///      `take_dispatch_consumer()` (stashed by
                    ///      `RticBoardEntry::init_hardware`).
                    ///   2. Drive `executor.spin_once(small_dur)`
                    ///      — small budget so the dequeue loop runs
                    ///      between executor iterations.
                    ///   3. Drain whatever the SPSC has for this
                    ///      cycle. Today each dequeued envelope is
                    ///      dropped with a TODO: per-Node trampoline
                    ///      routing needs an `ExecutorNodeRuntime`-
                    ///      wrapped sink that the macro emit hasn't
                    ///      plumbed yet (the `dispatch_callback`
                    ///      entry on `ExecutorNodeRuntime` is wired
                    ///      separately; the trampoline registry that
                    ///      pairs `cb_id` → Node pkg is the next 216
                    ///      follow-up — likely via `linkme`).
                    ///
                    /// Splitting the tasks back apart once the
                    /// `ExecutorNodeRuntime` sink is plumbed (so the
                    /// spin task can run at lower priority than the
                    /// dispatch task) is a separate follow-up.
                    #[task(local = [boot, runtime], priority = 1)]
                    async fn __nros_run(cx: __nros_run::Context) {
                        // Phase 289 (#178 layer 2) — install the board's
                        // idle-yield hooks (e.g. `wfi` on the busy-wait
                        // sites) now that interrupts are unmasked and the
                        // tick IRQ is armed. MUST precede `open_executor`:
                        // the blocking connect below is the busy-wait the
                        // yield exists for.
                        <__NrosBoard as ::nros::__macro_support::nros_platform::RticBoardEntry>::on_interrupts_live();
                        // #178 — open the executor HERE, not in `#[init]`.
                        // `Executor::open` performs the blocking zenoh-pico
                        // session open (a TCP connect driven by the smoltcp poll
                        // loop, which needs the timer tick + RX IRQ). This task
                        // runs after `#[init]` returns and interrupts are unmasked,
                        // so the handshake can complete; opening in `#[init]`
                        // (interrupts masked) deadlocks it.
                        let executor =
                            <__NrosBoard as ::nros::__macro_support::nros_platform::RticBoardEntry>::open_executor(
                                cx.local.boot.0.take().expect("RTIC boot carrier already taken"),
                            );
                        // Phase 289 (#178) — wrap the executor in the SAME
                        // `ExecutorNodeRuntime` every owned-spin board uses and
                        // run each Node pkg's full `register()` (entities +
                        // tick registry + component table) against it. The
                        // earlier `register_dispatch`-only wiring installed the
                        // on_callback trampoline but never created the node /
                        // publisher / timer entities, so the image opened its
                        // session and then published nothing.
                        let mut __nros_crt =
                            ::nros::node_runtime::ExecutorNodeRuntime::from_executor(executor);
                        {
                            let mut __nros_rt =
                                ::nros::__macro_support::nros_platform::RuntimeCtx::with_runtime(&mut __nros_crt);
                            #( #framework_register_entity_calls )*
                        }
                        // The board-side runtime owns the SPSC
                        // producer half. Today's collapse keeps it in
                        // `Local` for symmetry with the planned split
                        // — once `ExecutorNodeRuntime`-wrapped routing
                        // lands the runtime's `signal_callback` will
                        // be the producer-side bridge between executor
                        // callbacks and the SPSC consumer drained
                        // below.
                        let _runtime = &mut cx.local.runtime.0;
                        let mut consumer =
                            #rtic_consumer()
                                .expect("RTIC dispatch consumer take");
                        loop {
                            // Phase 289 — trait spin (`spin_once(ms)` +
                            // `run_ticks`), matching the owned-spin boards, so
                            // registered timers fire and service/action poll
                            // components tick.
                            let _ = ::nros::__macro_support::nros_platform::NodeDispatchRuntime::spin_once(
                                &mut __nros_crt,
                                10,
                            );
                            // Phase 216 final dispatch wiring — drain
                            // every envelope the board's SPSC has
                            // queued this cycle and forward each one
                            // through `Executor::dispatch_callback`.
                            // The layer-clean `(cb_id: &str,
                            // ctx_ptr: *mut c_void)` shape matches both
                            // the dequeued `SignaledCallback<'static>`
                            // payload and the executor's stable entry
                            // point — no type-translation gymnastics.
                            // The executor-side body is a no-op stub
                            // today; the per-Node trampoline registry
                            // (linkme / Phase 216 follow-up) fills it
                            // in by resolving `cb_id` →
                            // `__nros_node_<pkg>_on_callback` and
                            // invoking with the per-pkg state blob.
                            // What this commit closes is the gap
                            // where the dequeued envelope was being
                            // silently dropped with a TODO — values
                            // now flow into the executor's stable
                            // surface and the registry lands as a
                            // body fill, not a macro rewrite.
                            while let Some(envelope) = consumer.dequeue() {
                                let cb = envelope.into_inner();
                                // SAFETY: the board's `signal_callback` enqueues a
                                // `*mut CallbackCtx<'static>` (see the rtic board
                                // crates); single-core, drained on this task only.
                                let ctx = unsafe {
                                    &mut *(cb.ctx_ptr as *mut ::nros::CallbackCtx<'static>)
                                };
                                __nros_crt.dispatch_callback(cb.cb_id, ctx);
                            }
                        }
                    }

                    // Phase 289 (#178) — periodic tick hardware task
                    // (empty when the board spec declares no tick_irq).
                    #rtic_tick_ts

                    // Phase 216.B.4 — user-supplied `#[task]` trampolines.
                    // Each calls a sibling `<name>_impl` fn the user
                    // defines at module scope; signatures are kept
                    // simple to dodge cross-pkg type-alias plumbing.
                    #( #custom_task_items )*
                }
            }
        }
        Framework::Embassy => {
            // Phase 216.C.3 follow-up — sibling of the Rtic emit above.
            // Emits `#[embassy_executor::main] async fn main(spawner)`
            // that delegates to `EmbassyBoardEntry::init_hardware`
            // (which is **sync** — see `embassy_entry.rs`'s "Sync
            // `init_hardware`" note; matches `RticBoardEntry`) and
            // then spawns two `#[embassy_executor::task]` fns:
            //
            // - `__nros_spin_task(executor)` — long-lived task that
            //   drives the executor. The real body will dequeue from
            //   the board's `CALLBACK_CHANNEL` and invoke per-Node
            //   trampolines; today it parks on `Timer::after_secs` so
            //   the macro emit compiles standalone. The dequeue +
            //   trampoline-lookup integration lands alongside the
            //   B.3-equivalent RTIC `__nros_spin` body fill — the
            //   trampoline registration story spans the macro, the
            //   `nros::node!()` emit, and
            //   `register_dispatch_slot_dyn`, which is substantial
            //   integration work for a separate follow-up.
            //
            // - `__nros_dispatch_task(runtime)` — long-lived task that
            //   calls `runtime.spin_once(timeout_ms)` in a loop with an
            //   `embassy_time` yield between iterations. Same
            //   placeholder shape as the spin task — the
            //   `register_dispatch_slot_dyn(...)` registration call
            //   (the `run_plan(&mut runtime)` story) is the
            //   integration work deferred to the same follow-up.
            //
            // Task argument types resolve via the board's
            // `EmbassyBoardEntry::{Executor, Runtime}` associated
            // types; `#[embassy_executor::task]` doesn't accept
            // generic params, so we name them concretely through the
            // assoc-type projection.
            //
            // The `#![no_std]`/`#![no_main]` inner attrs are NOT
            // emitted here — the Entry pkg's `main.rs` already
            // declares those at file scope (mirrors the Rtic branch).
            quote! {
                use #board_path as __NrosBoard;

                /// Embassy run task — collapsed `__nros_spin_task` +
                /// `__nros_dispatch_task` (Phase 216.C.3 follow-up).
                ///
                /// `#[embassy_executor::task]` doesn't accept multiple
                /// generic params and the spin + dispatch loops need
                /// to share the `(Executor, Runtime)` pair so the
                /// per-callback routing (once plumbed via the
                /// `ExecutorNodeRuntime::dispatch_callback` sink in
                /// `packages/core/nros/src/node_runtime.rs`) can drain
                /// the board's static `CALLBACK_CHANNEL` between
                /// executor iterations. Collapsing the two tasks into
                /// one gives the spin + drain loops a single coherent
                /// borrow over both halves.
                ///
                /// Body:
                ///   1. Drive `executor.spin_once(small_dur)` — a
                ///      small budget so the loop can yield between
                ///      iterations.
                ///   2. Yield to the Embassy scheduler via
                ///      `Timer::after_millis(1)` so other tasks
                ///      (Ethernet driver, user-spawned tasks) run.
                ///
                /// What's still placeholder: a dequeue + dispatch
                /// step. `EmbassyRuntime` owns a `&'static` borrow
                /// of the board's private `CALLBACK_CHANNEL`, but
                /// no public accessor exposes the receiver half today
                /// — adding one is the next 216.C follow-up, paired
                /// with the per-Node trampoline registry (linkme) and
                /// the `ExecutorNodeRuntime`-wrapped sink the macro
                /// emit needs to plumb in order to call
                /// `dispatch_callback`. Splitting the tasks back
                /// apart once that lands is a separate follow-up.
                #[::embassy_executor::task]
                async fn __nros_run_task(
                    mut executor: <__NrosBoard as ::nros::__macro_support::nros_platform::EmbassyBoardEntry>::Executor,
                    runtime: <__NrosBoard as ::nros::__macro_support::nros_platform::EmbassyBoardEntry>::Runtime,
                ) {
                    loop {
                        let _ = executor.spin_once(
                            ::core::time::Duration::from_millis(1),
                        );
                        // Phase 216 final dispatch wiring — drain
                        // every envelope the board's
                        // `CALLBACK_CHANNEL` has queued this cycle
                        // and forward each one through
                        // `Executor::dispatch_callback`.
                        // `EmbassyRuntime::try_recv()` (Phase 216
                        // final, sibling of the RTIC SPSC `dequeue`
                        // path) is non-blocking so the spin loop
                        // keeps yielding even when no callback is
                        // signaled — the `Timer::after_millis(1)`
                        // below paces the executor poll without
                        // needing `embassy_futures::select` (not a
                        // current dep). The same layer-clean
                        // `(cb_id: &str, ctx_ptr: *mut c_void)` shape
                        // applies — see the RTIC sibling for the
                        // matching commentary. The executor-side
                        // dispatch body is a no-op stub today; the
                        // per-Node trampoline registry (linkme /
                        // Phase 216 follow-up) fills it in.
                        while let Some(envelope) = runtime.try_recv() {
                            let cb = envelope.into_inner();
                            executor.dispatch_callback(cb.cb_id, cb.ctx_ptr);
                        }
                        ::embassy_time::Timer::after_millis(1).await;
                    }
                }

                #[::embassy_executor::main]
                async fn main(spawner: ::embassy_executor::Spawner) {
                    // Sync `init_hardware_with_deploy` — see the
                    // `EmbassyBoardEntry` trait "Sync `init_hardware`"
                    // note; matches `RticBoardEntry`. Phase 244.D1 /
                    // issue #98 / RFC-0045 — threads the deploy overlay
                    // so the board can resolve the node name from
                    // `deploy.boot_config` (the default impl ignores it,
                    // so boards without a baked boot config are
                    // unchanged).
                    let (mut executor, runtime) =
                        <__NrosBoard as ::nros::__macro_support::nros_platform::EmbassyBoardEntry>::init_hardware_with_deploy(
                            spawner,
                            &#deploy_overlay_ts,
                        );
                    // Phase 216 final wave — per-Node dispatch
                    // registration. Sibling of the RTIC `#[init]`
                    // splice above; same `register_dispatch(&mut
                    // executor)` shape, populating the executor's
                    // dispatch-slot table before the
                    // `__nros_run_task` is spawned to drain the
                    // board's `CALLBACK_CHANNEL`.
                    #( #framework_register_dispatch_calls )*
                    spawner.spawn(__nros_run_task(executor, runtime)).unwrap();
                }
            }
        }
    };

    // phase-267 W1c/C4 — a `[[bridge]]` system is a cross-RMW gateway: the entry
    // `include_str!`s the `nros-bridge.toml` `nros sync` generated and runs the
    // data-driven `nros_bridge::run_from_config_str` (open_multi + a PubSubBridge
    // per `[[bridge]]` + spin/pump). Replaces the ordinary register/spin body. The
    // Entry pkg deps `nros-bridge` (config feature) + both RMW backends.
    if let Some(cfg_path) = &bridge_config_path {
        let cfg_lit = cfg_path.to_string_lossy();
        let cfg_lit = cfg_lit.as_ref();
        // Issue 0106 — explicitly `register()` each bridge RMW backend so the
        // linker can't dead-strip its self-register `.init_array` ctor. The
        // `run_from_config` body references no backend symbol, so without this
        // the backend is dropped and `open_multi` fails `Transport(
        // InvalidArgument)` (null vtable). Mirrors the board boot path's
        // `nros_rmw_<x>::register()` (Phase 248 C5a). Unknown rmw names map to
        // nothing (the data-driven config still drives the actual open).
        let register_calls: Vec<proc_macro2::TokenStream> = bridge_rmws
            .iter()
            .filter_map(|rmw| rmw_crate_ident(rmw))
            .map(|crate_ident| {
                let id = Ident::new(crate_ident, Span::call_site());
                quote! { let _ = ::#id::register(); }
            })
            .collect();
        // phase-267 (non-flat types) — stage each cyclonedds-egress non-flat type's
        // Cyclone descriptor via a typed `register::<M>()` (reuses `M::FIELDS`, so
        // nested / array / sequence work without a flat schema). The Entry deps the
        // forwarded msg crate(s) + `nros-rmw-cyclonedds`. `let _ =` mirrors the
        // backend register: a failure surfaces downstream as the egress pub error.
        let typed_register_calls: Vec<proc_macro2::TokenStream> = read_register_types(cfg_path)
            .into_iter()
            .filter(|(_, rmw)| rmw == "cyclonedds")
            .filter_map(|(rust_path, _)| syn::parse_str::<SynPath>(&rust_path).ok())
            .map(|path| quote! { let _ = ::nros_rmw_cyclonedds::register::<#path>(); })
            .collect();
        let expanded = quote! {
            #( #tracked_consts )*
            fn main() -> ::core::result::Result<(), ::nros_bridge::ConfigError> {
                #( #register_calls )*
                #( #typed_register_calls )*
                ::nros_bridge::run_from_config_str(::core::include_str!(#cfg_lit))
            }
        };
        return Ok(expanded);
    }

    let expanded = quote! {
        // Phase 212.N.9 — rebuild-tracking workaround. Stable Rust
        // proc-macros can't use `proc_macro::tracked_path::path()`;
        // anonymous `const _: &[u8] = include_bytes!(...)` items are
        // tracked by cargo's `include_bytes!` and force a recompile
        // when any tracked file changes.
        #( #tracked_consts )*

        // W4b — baked boot-config static; emitted before the framework
        // body so `&NROS_BOOT_CONFIG` is in scope at every overlay use site.
        #boot_config_static_ts

        #body_ts
    };

    Ok(expanded)
}

/// Issue 0106 — map an `system.toml` rmw name to the backend crate ident the
/// Entry deps (so the macro can emit `nros_rmw_<x>::register()`). Mirrors the
/// orchestration codegen map (`generate.rs` ~2785). `None` for unknown names.
fn rmw_crate_ident(rmw: &str) -> Option<&'static str> {
    match rmw {
        "zenoh" => Some("nros_rmw_zenoh"),
        "cyclonedds" => Some("nros_rmw_cyclonedds_sys"),
        "xrce" => Some("nros_rmw_xrce_cffi"),
        _ => None,
    }
}

/// Issue 0106 — the set of RMW backends a `[[bridge]]` system uses, read from
/// `system.toml`: each `[[bridge]]` endpoint (`from`/`to`) is either an
/// `"<rmw>:<domain>"` literal or a bare `<domain>` name resolved through the
/// `[[domain]]` `rmw` field. De-duped, order-preserving. Best-effort (a parse
/// error / missing key yields an empty list — the data-driven config still
/// drives the open; this only affects the anti-dead-strip `register()` calls).
fn read_bridge_rmws(system_toml: &Path) -> Vec<String> {
    match std::fs::read_to_string(system_toml) {
        Ok(raw) => parse_bridge_rmws(&raw),
        Err(_) => Vec::new(),
    }
}

/// Pure half of [`read_bridge_rmws`] — parse the rmw set from `system.toml` text.
fn parse_bridge_rmws(raw: &str) -> Vec<String> {
    let Ok(v) = toml::from_str::<toml::Value>(raw) else {
        return Vec::new();
    };
    let domain_rmw = |name: &str| -> Option<String> {
        v.get("domain")
            .and_then(|d| d.as_array())?
            .iter()
            .find(|item| item.get("name").and_then(|n| n.as_str()) == Some(name))
            .and_then(|d| d.get("rmw"))
            .and_then(|r| r.as_str())
            .map(String::from)
    };
    let mut rmws: Vec<String> = Vec::new();
    if let Some(bridges) = v.get("bridge").and_then(|b| b.as_array()) {
        for bridge in bridges {
            for key in ["from", "to"] {
                let Some(ep) = bridge.get(key).and_then(|e| e.as_str()) else {
                    continue;
                };
                let rmw = match ep.split_once(':') {
                    Some((r, _)) => Some(r.to_string()),
                    None => domain_rmw(ep),
                };
                if let Some(r) = rmw
                    && !r.is_empty()
                    && !rmws.contains(&r)
                {
                    rmws.push(r);
                }
            }
        }
    }
    rmws
}

/// Phase 216 final wave — read `[package.metadata.nros.entry]
/// node_pkgs = ["pkg_a", "pkg_b"]` from `Cargo.toml`. Each entry names
/// a Node-pkg crate the Entry pkg depends on; the framework emit
/// (RTIC / Embassy) splices `<pkg>::register_dispatch(&mut executor)?;`
/// calls for each into the generated `#[init]` body.
///
/// Returns `Ok(None)` when the key is absent — callers fall back to
/// self-bringup (Entry pkg's own crate name minus the conventional
/// `_entry` suffix, when present).
fn read_entry_node_pkgs(cargo_toml: &Path) -> Result<Option<Vec<String>>, String> {
    let raw = std::fs::read_to_string(cargo_toml).map_err(|e| format!("read: {e}"))?;
    let v: toml::Value = toml::from_str(&raw).map_err(|e| format!("parse toml: {e}"))?;
    let arr = match v
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("nros"))
        .and_then(|n| n.get("entry"))
        .and_then(|e| e.get("node_pkgs"))
    {
        Some(a) => a,
        None => return Ok(None),
    };
    let list = arr
        .as_array()
        .ok_or_else(|| "`node_pkgs` must be an array of strings".to_string())?;
    let mut out = Vec::with_capacity(list.len());
    for item in list {
        let s = item
            .as_str()
            .ok_or_else(|| "`node_pkgs` entries must be strings".to_string())?;
        out.push(s.to_string());
    }
    Ok(Some(out))
}

/// Read `[package.metadata.nros.entry] deploy = "<board>"` from
/// `Cargo.toml`. The key is mandatory for form-1 (no-arg) invocations.
fn read_entry_deploy(cargo_toml: &Path) -> Result<String, String> {
    let raw = std::fs::read_to_string(cargo_toml).map_err(|e| format!("read: {e}"))?;
    let v: toml::Value = toml::from_str(&raw).map_err(|e| format!("parse toml: {e}"))?;
    let deploy = v
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("nros"))
        .and_then(|n| n.get("entry"))
        .and_then(|e| e.get("deploy"))
        .and_then(|d| d.as_str())
        .ok_or_else(|| {
            "missing `[package.metadata.nros.entry] deploy = \"<board>\"`".to_string()
        })?;
    Ok(deploy.to_string())
}

/// phase-271 (issue #110) — read the per-entry executor sizing from
/// `[package.metadata.nros.entry] max_callbacks = N` (+ optional
/// `max_sched_contexts = M`). Returns `Some((max_callbacks, max_sched_contexts))`
/// (`max_sched_contexts` defaulting to `0` = "board uses the build default") or
/// `None` when `max_callbacks` is absent (the executor opens at the build-time
/// default `MAX_CBS`/`ARENA_SIZE`). This is the per-entry, NOT workspace-global,
/// knob (issue #0110 fix-idea 2): a fat native entry declares its own callback
/// table size without a `[env] NROS_EXECUTOR_MAX_CBS` that bloats every lean
/// embedded entry in the same workspace. The hosted (posix) board applies it via
/// [`BoardEntry::run_with_deploy_sized`] → `Executor::open_sized`.
fn read_entry_executor_sizing(cargo_toml: &Path) -> Option<(usize, usize)> {
    let raw = std::fs::read_to_string(cargo_toml).ok()?;
    let v: toml::Value = toml::from_str(&raw).ok()?;
    let entry = v
        .get("package")?
        .get("metadata")?
        .get("nros")?
        .get("entry")?;
    let max_cbs = entry.get("max_callbacks")?.as_integer()?;
    if max_cbs <= 0 {
        return None;
    }
    let max_sc = entry
        .get("max_sched_contexts")
        .and_then(|x| x.as_integer())
        .filter(|n| *n > 0)
        .unwrap_or(0);
    Some((max_cbs as usize, max_sc as usize))
}

/// Issue #48 cause 1 — the deploy-overlay values read from the Entry pkg's
/// `[package.metadata.nros.deploy.<board>]` block. Every field is `Option`
/// (absent key → `None`), baked into a `DeployOverlay` const by
/// [`deploy_overlay_tokens`] and threaded through `BoardEntry::run_with_deploy`.
#[derive(Default)]
struct DeployOverlayLit {
    locator: Option<String>,
    ip: Option<[u8; 4]>,
    gateway: Option<[u8; 4]>,
    netmask: Option<[u8; 4]>,
    domain_id: Option<u32>,
    transport: Option<String>,
    /// Issue #98 — the ROS graph node name for the primary session, set from the
    /// launch file's single `<node name>` (only when the launch declares exactly
    /// one node). NOT read from `[deploy.*]`: it is a launch identity, threaded
    /// in by the caller after the launch is parsed.
    node_name: Option<String>,
}

/// Parse a dotted IPv4 string (`"10.0.2.15"`) into 4 octets. Returns `None`
/// on any malformed input so a bad deploy value silently keeps the board
/// default rather than baking garbage.
fn parse_ipv4_lit(s: &str) -> Option<[u8; 4]> {
    let mut out = [0u8; 4];
    let mut n = 0usize;
    for part in s.split('.') {
        if n >= 4 {
            return None;
        }
        out[n] = part.parse::<u8>().ok()?;
        n += 1;
    }
    if n == 4 { Some(out) } else { None }
}

/// Read `[package.metadata.nros.deploy.<board>]` from the Entry pkg's
/// `Cargo.toml`. Missing block / keys → all-`None` overlay (the firmware keeps
/// its compiled-in `Config::default()`). Only the network/locator/domain keys
/// are consumed here; `rmw` is handled elsewhere (feature/link wiring).
/// Issue #129 (RFC-0031 C5b amendment) — the entry's deploy RMW key
/// (`[package.metadata.nros.deploy.<board>].rmw`). The Zephyr framework arm
/// uses it to emit the explicit `::nros_rmw_<x>::register()` call: Zephyr has
/// no `BoardEntry` boot path to own registration (the FreeRTOS C5a home), the
/// board crate is NetworkWait-only, and `.init_array` ctors don't run on
/// `target_os = "none"` — so per the C5b amendment the ENTRY carries the
/// direct backend dep and codegen emits the register.
fn read_deploy_rmw(cargo_toml: &Path, board_key: &str) -> Option<String> {
    let raw = std::fs::read_to_string(cargo_toml).ok()?;
    let v = toml::from_str::<toml::Value>(&raw).ok()?;
    v.get("package")?
        .get("metadata")?
        .get("nros")?
        .get("deploy")?
        .get(board_key)?
        .get("rmw")?
        .as_str()
        .map(str::to_string)
}

fn read_deploy_overlay(cargo_toml: &Path, board_key: &str) -> DeployOverlayLit {
    let Ok(raw) = std::fs::read_to_string(cargo_toml) else {
        return DeployOverlayLit::default();
    };
    let Ok(v) = toml::from_str::<toml::Value>(&raw) else {
        return DeployOverlayLit::default();
    };
    let Some(block) = v
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("nros"))
        .and_then(|n| n.get("deploy"))
        .and_then(|d| d.get(board_key))
    else {
        return DeployOverlayLit::default();
    };
    DeployOverlayLit {
        locator: block
            .get("locator")
            .and_then(|x| x.as_str())
            .map(str::to_string),
        ip: block
            .get("ip")
            .and_then(|x| x.as_str())
            .and_then(parse_ipv4_lit),
        gateway: block
            .get("gateway")
            .and_then(|x| x.as_str())
            .and_then(parse_ipv4_lit),
        netmask: block
            .get("netmask")
            .and_then(|x| x.as_str())
            .and_then(parse_ipv4_lit),
        domain_id: block
            .get("domain_id")
            .and_then(|x| x.as_integer())
            .and_then(|i| u32::try_from(i).ok()),
        transport: block
            .get("transport")
            .and_then(|x| x.as_str())
            .map(str::to_string),
        // Issue #98 — not a `[deploy.*]` key; the caller fills this from the
        // parsed launch when it declares exactly one node.
        node_name: None,
    }
}

/// Bake a [`DeployOverlayLit`] into a `nros_platform::DeployOverlay` struct
/// literal (all fields `Option`, so the board overlays only the present ones).
fn deploy_overlay_tokens(lit: &DeployOverlayLit) -> proc_macro2::TokenStream {
    fn opt_ipv4(v: &Option<[u8; 4]>) -> proc_macro2::TokenStream {
        match v {
            Some([a, b, c, d]) => quote! { ::core::option::Option::Some([#a, #b, #c, #d]) },
            None => quote! { ::core::option::Option::None },
        }
    }
    let locator = match &lit.locator {
        Some(s) => quote! { ::core::option::Option::Some(#s) },
        None => quote! { ::core::option::Option::None },
    };
    let ip = opt_ipv4(&lit.ip);
    let gateway = opt_ipv4(&lit.gateway);
    let netmask = opt_ipv4(&lit.netmask);
    let domain_id = match lit.domain_id {
        Some(d) => quote! { ::core::option::Option::Some(#d) },
        None => quote! { ::core::option::Option::None },
    };
    let transport = match &lit.transport {
        Some(s) => quote! { ::core::option::Option::Some(#s) },
        None => quote! { ::core::option::Option::None },
    };
    let node_name = match &lit.node_name {
        Some(s) => quote! { ::core::option::Option::Some(#s) },
        None => quote! { ::core::option::Option::None },
    };
    quote! {
        ::nros::__macro_support::nros_platform::DeployOverlay {
            locator: #locator,
            ip: #ip,
            gateway: #gateway,
            netmask: #netmask,
            domain_id: #domain_id,
            transport: #transport,
            node_name: #node_name,
            boot_config: ::core::option::Option::Some(&NROS_BOOT_CONFIG),
        }
    }
}

/// Read `[system] default_launch = "<file>"` from a bringup pkg's
/// `system.toml`. Returns `Ok(None)` when the key is absent (caller
/// falls back to `"system.launch.xml"`).
fn read_default_launch(system_toml: &Path) -> Result<Option<String>, String> {
    let raw = std::fs::read_to_string(system_toml).map_err(|e| format!("read: {e}"))?;
    let v: toml::Value = toml::from_str(&raw).map_err(|e| format!("parse toml: {e}"))?;
    Ok(v.get("system")
        .and_then(|s| s.get("default_launch"))
        .and_then(|d| d.as_str())
        .map(str::to_string))
}

/// Phase 264 W2 — read `[lifecycle]` from `system.toml`. `None` ⇒ no block (the
/// macro emits no lifecycle wiring). `Some(code)` ⇒ block present; `code` is the
/// boot autostart (0 none, 1 configure, 2 active) the runtime applies after
/// registering the REP-2002 services. Mirrors the bake's `[lifecycle]` handling.
/// Best-effort: an unreadable / malformed system.toml yields `None` (the launch
/// resolution above already surfaces real parse errors).
fn read_lifecycle_autostart(system_toml: &Path) -> Option<u8> {
    let raw = std::fs::read_to_string(system_toml).ok()?;
    let v: toml::Value = toml::from_str(&raw).ok()?;
    let lifecycle = v.get("lifecycle")?;
    let code = match lifecycle.get("autostart").and_then(|a| a.as_str()) {
        Some("active") => 2,
        Some("configure") => 1,
        _ => 0, // "none" or absent — services registered, no boot transition
    };
    Some(code)
}

/// Phase 264 W4b — is `[param_services]` declared in `system.toml`? `true` ⇒ the macro
/// emits the parameter-service registration + store seeding (mirrors the bake's
/// `param_services` axis). Best-effort: an unreadable / malformed system.toml yields
/// `false` (the launch resolution above already surfaces real parse errors).
fn read_param_services_enabled(system_toml: &Path) -> bool {
    let Ok(raw) = std::fs::read_to_string(system_toml) else {
        return false;
    };
    let Ok(v) = toml::from_str::<toml::Value>(&raw) else {
        return false;
    };
    v.get("param_services").is_some()
}

/// phase-267 W1c/C4 — does `system.toml` declare a non-empty `[[bridge]]`? When it
/// does (and `nros sync` generated `nros-bridge.toml`), the macro emits a
/// cross-RMW bridge entry instead of the ordinary register/spin one. Best-effort:
/// an unreadable / malformed `system.toml` yields `false`.
fn read_has_bridge(system_toml: &Path) -> bool {
    let Ok(raw) = std::fs::read_to_string(system_toml) else {
        return false;
    };
    let Ok(v) = toml::from_str::<toml::Value>(&raw) else {
        return false;
    };
    v.get("bridge")
        .and_then(|b| b.as_array())
        .is_some_and(|a| !a.is_empty())
}

/// phase-267 (non-flat types) — read the `[[register_type]]` entries `nros sync`
/// emits into `nros-bridge.toml` for forwarded messages whose schema can't ride
/// the flat `fields` list. Each is `(rust_path, egress_rmw)`; the macro emits a
/// typed `register::<M>()` per cyclonedds egress so the runtime can stage the
/// Cyclone descriptor from `M::FIELDS` (arbitrary nesting). Best-effort: a parse
/// error yields an empty list.
fn read_register_types(bridge_toml: &Path) -> Vec<(String, String)> {
    let Ok(raw) = std::fs::read_to_string(bridge_toml) else {
        return Vec::new();
    };
    let Ok(v) = toml::from_str::<toml::Value>(&raw) else {
        return Vec::new();
    };
    let Some(arr) = v.get("register_type").and_then(|r| r.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|e| {
            let path = e.get("rust_path")?.as_str()?.to_string();
            let rmw = e.get("rmw")?.as_str()?.to_string();
            Some((path, rmw))
        })
        .collect()
}

// =============================================================================
// Phase 228.G — per-tier resolution inputs (RFC-0032 §6)
// =============================================================================

/// Read `[package.metadata.nros.node].callback_groups` from a node pkg's
/// `Cargo.toml`. Missing / malformed → empty (the node contributes no groups,
/// so it lands on the synthesized `default` tier).
fn read_node_callback_groups(cargo_toml: &Path) -> Vec<CallbackGroupDecl> {
    let Ok(raw) = std::fs::read_to_string(cargo_toml) else {
        return Vec::new();
    };
    let Ok(v) = toml::from_str::<toml::Value>(&raw) else {
        return Vec::new();
    };
    v.get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("nros"))
        .and_then(|n| n.get("node"))
        .and_then(|nd| nd.get("callback_groups"))
        .and_then(|cg| cg.clone().try_into::<Vec<CallbackGroupDecl>>().ok())
        .unwrap_or_default()
}

/// The tier-relevant slice of a bringup `system.toml`.
struct SystemTierConfig {
    tiers: BTreeMap<String, TierDef>,
    node_overrides: Vec<NodeOverride>,
    component_names: Vec<String>,
    has_tiers: bool,
}

/// Parse `[tiers.*]`, `[[node_overrides]]`, and `[[component]].name` from a
/// bringup `system.toml`. `has_tiers` gates the multi-tier emit — when no
/// `[tiers.*]` table exists the macro stays on the single-tier `BoardEntry::run`
/// path (byte-identical).
fn read_system_tier_config(system_toml: &Path) -> Result<SystemTierConfig, String> {
    let raw = std::fs::read_to_string(system_toml).map_err(|e| format!("read: {e}"))?;
    let v: toml::Value = toml::from_str(&raw).map_err(|e| format!("parse toml: {e}"))?;

    let tiers: BTreeMap<String, TierDef> = match v.get("tiers") {
        Some(t) => t
            .clone()
            .try_into()
            .map_err(|e| format!("`[tiers]`: {e}"))?,
        None => BTreeMap::new(),
    };
    let node_overrides: Vec<NodeOverride> = match v.get("node_overrides") {
        Some(o) => o
            .clone()
            .try_into()
            .map_err(|e| format!("`[[node_overrides]]`: {e}"))?,
        None => Vec::new(),
    };
    let component_names: Vec<String> = v
        .get("component")
        .and_then(|c| c.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| c.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    Ok(SystemTierConfig {
        has_tiers: !tiers.is_empty(),
        tiers,
        node_overrides,
        component_names,
    })
}

/// Map the resolved board deploy string to the RTOS key `resolve_tiers` expects
/// (picks the `[tiers.<name>.<rtos>]` sub-table). `None` (explicit `board = X`)
/// defaults to `posix` — the native dev target.
fn derive_target_rtos(deploy: Option<&str>) -> String {
    match deploy {
        Some(d) if d.contains("freertos") => "freertos",
        Some(d) if d.contains("threadx") => "threadx",
        Some(d) if d.contains("nuttx") => "nuttx",
        Some(d) if d.contains("zephyr") => "zephyr",
        _ => "posix",
    }
    .to_string()
}

/// Emit a `&[TierSpec]` literal from the resolved tier table (Phase 228.G,
/// RFC-0032 §5). `priority` is the raw per-RTOS value; `groups` is the tier's
/// distinct callback-group ids (the executor's `active_groups` filter).
fn tier_specs_tokens(table: &ResolvedTierTable) -> proc_macro2::TokenStream {
    let entries = table.tiers.iter().map(|t| {
        let name = &t.name;
        let mut groups: Vec<&str> = t.members.iter().map(|(_, g)| g.as_str()).collect();
        groups.sort();
        groups.dedup();
        let priority = t.priority;
        let stack_bytes = t.stack_bytes.unwrap_or(0) as usize;
        let spin_period_us = t.spin_period_us.unwrap_or(1000);
        quote! {
            ::nros::__macro_support::nros_platform::TierSpec {
                name: #name,
                groups: &[ #(#groups),* ],
                priority: #priority,
                stack_bytes: #stack_bytes,
                spin_period_us: #spin_period_us,
            }
        }
    });
    quote! { &[ #(#entries),* ] }
}

/// Map a board key from `[package.metadata.nros.entry] deploy = "X"` to the
/// tier-1 board crate's ZST type path.
///
/// The table is maintained in [`nros_orchestration_ir::board_path_for`]
/// (the single source of truth shared with the CLI codegen path). This
/// wrapper parses the returned string into a [`syn::Path`] for token
/// emission. Adding a new board requires editing the IR crate only.
fn board_path_for(deploy: &str) -> Option<SynPath> {
    let path_str = nros_orchestration_ir::board_path_for(deploy)?;
    syn::parse_str::<SynPath>(path_str).ok()
}

fn known_boards_csv() -> &'static str {
    "native, freertos, threadx-linux, threadx-qemu-riscv64, nuttx, nuttx-riscv, esp32-qemu, \
     zephyr, rtic-stm32f4, rtic-mps2-an385, qemu-mps2-an385, stm32f4, embassy-stm32f4"
}

/// Phase 244.D1 — does this deploy key name a pure bare-metal Cortex-M
/// direct-exec board? Such boards run `OwnedSpin` but, unlike the FreeRTOS /
/// threadx-linux `target_os = "none"` boards (whose C runtime calls `main`),
/// have no C runtime — the reset vector needs a `#[cortex_m_rt::entry]`. The
/// macro keys the entry-emit shape off this. RTIC bare-metal boards are NOT
/// here: they route through the RTIC framework, which owns its own entry.
fn is_baremetal_cortexm_deploy(deploy: Option<&str>) -> bool {
    matches!(deploy, Some("qemu-mps2-an385" | "mps2-an385" | "stm32f4"))
}

struct RticBoardSpec {
    device_path: SynPath,
    dispatchers: Vec<Ident>,
    dispatch_consumer_path: SynPath,
    /// Phase 289 (#178) — interrupt ident of the board's periodic tick
    /// timer. The macro emits a `#[task(binds = <tick_irq>, priority = 2)]`
    /// hardware task calling `RticBoardEntry::on_tick()`; the board arms the
    /// timer in `init_hardware` and acknowledges it in `on_tick`. The tick's
    /// job is waking the `wfi` idle-yield inside `__nros_run`'s blocking
    /// connect/poll busy-waits (priority 2 so it preempts the priority-1 run
    /// task). `None` = no tick task emitted (board runs without wfi-yield).
    tick_irq: Option<Ident>,
}

fn rtic_board_spec_for(deploy: &str) -> Option<RticBoardSpec> {
    let (device, dispatchers, consumer, tick_irq) = match deploy {
        "rtic-stm32f4" => (
            "stm32f4xx_hal::pac",
            &["USART1", "USART2"][..],
            "::nros_board_rtic_stm32f4::take_dispatch_consumer",
            Some("TIM2"),
        ),
        "rtic-mps2-an385" | "qemu-rtic-mps2-an385" => (
            "mps2_an385_pac",
            &["UARTRX0", "UARTTX0"][..],
            "::nros_board_rtic_mps2_an385::take_dispatch_consumer",
            Some("TIMER0"),
        ),
        _ => return None,
    };
    Some(RticBoardSpec {
        device_path: syn::parse_str::<SynPath>(device).ok()?,
        dispatchers: dispatchers
            .iter()
            .map(|name| Ident::new(name, Span::call_site()))
            .collect(),
        dispatch_consumer_path: syn::parse_str::<SynPath>(consumer).ok()?,
        tick_irq: tick_irq.map(|name| Ident::new(name, Span::call_site())),
    })
}

/// Phase 216.B.3 — boot-framework dispatch for `nros::main!()`.
///
/// Distinct from [`board_path_for`] (which only resolves the board
/// crate ZST). Frameworks are orthogonal to RMW + platform: each
/// board crate carries its own
/// `[package.metadata.nros.board] framework = "<f>"` knob (consumed
/// by 216.D.1 `nros check`). The macro keys off the Entry pkg's
/// `deploy = "..."` value directly to avoid a fs round-trip into the
/// board crate's manifest at proc-macro expansion time; the long-term
/// spec reads the board's manifest, but the skeleton hardcodes the
/// table to match the `board_path_for` row above.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Framework {
    /// Board owns the spin loop (`BoardEntry::run`). Default for
    /// every board key not explicitly routed below.
    OwnedSpin,
    /// RTIC framework owns the spin loop. The macro emits a
    /// `#[rtic::app(...)]` module + `#[init]` body that delegates to
    /// `RticBoardEntry::init_hardware`.
    Rtic,
    /// Embassy framework owns the spin loop. The macro emits a
    /// `#[embassy_executor::main] async fn main(spawner: Spawner)` body
    /// that delegates to `EmbassyBoardEntry::init_hardware`.
    Embassy,
    /// Phase 225.P — Zephyr RTOS owns boot + `main`. The macro emits a
    /// `#[unsafe(no_mangle)] pub extern "C" fn rust_main()` staticlib
    /// export (consumed by `zephyr-lang-rust`'s `rust_cargo_application`)
    /// that gates on `ZephyrBoard::wait_link_up`, opens an `Executor`,
    /// wraps it in `ExecutorNodeRuntime`, registers each launch-named
    /// Node pkg, then spins — bounded on hosted `native_sim`, forever
    /// otherwise. There is NO `BoardEntry::run` (Zephyr forbids a Rust
    /// `fn main`).
    Zephyr,
    /// Phase 225.O — ESP32-C3 (esp-hal). esp-riscv-rt's `_start` calls
    /// the esp-hal entry registration, so a bare `extern "C" fn main`
    /// (the `OwnedSpin` `target_os = "none"` shape) does not boot. The
    /// macro emits `#[::esp_hal::main] fn main() -> !` that delegates to
    /// the real-runtime `BoardEntry::run` (which never returns), then
    /// spins defensively. The Entry crate provides the panic handler
    /// (`esp-backtrace`) + app descriptor (`esp_app_desc!`).
    Esp32,
}

// Phase 225.O follow-up (known-issue #18) — NOTE on NuttX. NuttX does
// NOT get its own `Framework` variant: it rides `Framework::OwnedSpin`.
// The NuttX flat-build init task calls `CONFIG_INIT_ENTRYPOINT="nsh_main"`,
// but the board crate (`nros-board-nuttx-qemu-arm`'s `entry.rs`) already
// exports a `#[no_mangle] nsh_main` that runs `nsh_initialize()` (virtio
// FDT discovery + network bringup) and then calls the Rust `main`
// lang-start symbol. OwnedSpin emits exactly that `fn main()` (NuttX is
// `target_os = "nuttx"`, the `not(target_os = "none")` hosted arm), which
// delegates to `<QemuArmVirt as BoardEntry>::run`. So no NuttX-specific
// emit is needed; emitting our own `nsh_main` would both collide with
// the board's and skip the critical `nsh_initialize()` network bringup.

fn framework_for(deploy: &str) -> Framework {
    match deploy {
        "rtic-stm32f4" | "rtic-mps2-an385" | "qemu-rtic-mps2-an385" => Framework::Rtic,
        "embassy-stm32f4" => Framework::Embassy,
        "zephyr" => Framework::Zephyr,
        "esp32-qemu" | "qemu-esp32-baremetal" => Framework::Esp32,
        // NuttX ("nuttx" / "qemu-arm-nuttx") rides OwnedSpin — the board
        // crate's `nsh_main` bridges the kernel init task to the emitted
        // `fn main()`. See the `Framework` enum NOTE.
        _ => Framework::OwnedSpin,
    }
}

/// Sanitise a pkg name into a valid Rust crate ident. Cargo allows
/// `-` in pkg names; Rust idents don't. Matches the
/// `sanitize_pkg_name_for_symbol` rule the existing `nros::node!()`
/// macro uses, so the codegen + Entry-pkg sides round-trip.
fn pkg_to_crate_ident(pkg: &str) -> String {
    let mut out = String::with_capacity(pkg.len());
    for c in pkg.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    out
}

/// Silence the unused-import warning in proc_macro2 — Expr / ExprLit /
/// Lit are imported so future extensions (e.g. parsing
/// `args = vec![("k","v")]`) can reach them without re-importing.
#[allow(dead_code)]
fn _unused() {
    let _ = std::marker::PhantomData::<(Expr, ExprLit, Lit)>;
}

/// Phase 216.B.4 — parser-only unit tests for `custom_tasks = [...]`.
///
/// The full `cargo check`-driven round-trip (showing the RTIC splice
/// actually compiles against the stm32f4 board crate) needs a
/// thumbv7em-eabihf cross build; that lives in the 216.B.5 follow-up.
/// These tests pin the host-side syntax acceptance: the parser takes
/// `custom_tasks = [ident, ident, ...]` (and the empty `[]` form),
/// rejects malformed shapes, and round-trips ident order.
#[cfg(test)]
mod custom_tasks_parser_tests {
    use super::MainArgs;

    fn parse(src: &str) -> syn::Result<MainArgs> {
        syn::parse_str::<MainArgs>(src)
    }

    #[test]
    fn empty_list_is_accepted() {
        // `Some(vec![])` distinguishes "key supplied, list empty" from
        // "key not supplied" so the OwnedSpin / Embassy misuse error
        // can still fire on `[]`.
        let parsed = parse("custom_tasks = []").expect("parse empty list");
        let tasks = parsed.custom_tasks.expect("custom_tasks set");
        assert!(tasks.is_empty(), "expected empty Vec, got {tasks:?}");
        assert!(parsed.custom_tasks_span.is_some(), "span captured");
    }

    #[test]
    fn single_ident_is_accepted() {
        let parsed = parse("custom_tasks = [adc_sample]").expect("parse single");
        let tasks = parsed.custom_tasks.expect("custom_tasks set");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].to_string(), "adc_sample");
    }

    #[test]
    fn multi_ident_round_trips_in_order() {
        let parsed =
            parse("custom_tasks = [adc_sample, ui_redraw, watchdog]").expect("parse multi");
        let names: Vec<String> = parsed
            .custom_tasks
            .expect("custom_tasks set")
            .into_iter()
            .map(|i| i.to_string())
            .collect();
        assert_eq!(names, vec!["adc_sample", "ui_redraw", "watchdog"]);
    }

    #[test]
    fn trailing_comma_is_accepted() {
        let parsed =
            parse("custom_tasks = [adc_sample, ui_redraw,]").expect("parse trailing comma");
        assert_eq!(parsed.custom_tasks.expect("set").len(), 2);
    }

    #[test]
    fn combines_with_other_args() {
        let parsed = parse("board = ::nros_board_native::NativeBoard, custom_tasks = [foo, bar]")
            .expect("parse combined");
        assert!(parsed.board.is_some(), "board parsed");
        assert_eq!(parsed.custom_tasks.expect("custom_tasks set").len(), 2);
    }

    #[test]
    fn string_literal_in_list_is_rejected() {
        let err = match parse("custom_tasks = [\"adc_sample\"]") {
            Ok(_) => panic!("string literals must not parse as idents"),
            Err(e) => e,
        };
        let msg = err.to_string();
        // syn's default ident-parse error contains "expected identifier"
        // — pin on that rather than the syn version-specific wording so
        // a syn bump doesn't tip the test.
        assert!(
            msg.contains("expected") && msg.contains("identifier"),
            "diagnostic should mention identifier, got: {msg}"
        );
    }

    #[test]
    fn missing_brackets_falls_back_to_path_branch() {
        // Without brackets the parser drops into the Path branch and
        // stores a `KvValue::Path`, which the `custom_tasks` arm
        // rejects with its diagnostic. Pin on the message contents.
        let err = match parse("custom_tasks = adc_sample") {
            Ok(_) => panic!("bare ident must not parse as a list"),
            Err(e) => e,
        };
        let msg = err.to_string();
        assert!(
            msg.contains("custom_tasks") && msg.contains("list of fn idents"),
            "diagnostic should mention custom_tasks list, got: {msg}"
        );
    }
}

#[cfg(test)]
mod bridge_rmw_tests {
    use super::{parse_bridge_rmws, rmw_crate_ident};

    #[test]
    fn rmw_crate_ident_maps_known_backends() {
        assert_eq!(rmw_crate_ident("zenoh"), Some("nros_rmw_zenoh"));
        assert_eq!(
            rmw_crate_ident("cyclonedds"),
            Some("nros_rmw_cyclonedds_sys")
        );
        assert_eq!(rmw_crate_ident("xrce"), Some("nros_rmw_xrce_cffi"));
        assert_eq!(rmw_crate_ident("unknown"), None);
    }

    /// Endpoints resolved via `[[domain]]` rmw (the `demo_bringup` shape).
    #[test]
    fn parse_bridge_rmws_resolves_domains() {
        let toml = r#"
[[domain]]
name = "zen"
rmw = "zenoh"
id = 0

[[domain]]
name = "dds"
rmw = "cyclonedds"
id = 5

[[bridge]]
name = "gw"
from = "zenoh:zen"
to = "cyclonedds:dds"
"#;
        // "<rmw>:<domain>" literal form takes the prefix directly.
        assert_eq!(
            parse_bridge_rmws(toml),
            vec!["zenoh".to_string(), "cyclonedds".to_string()]
        );
    }

    /// Bare endpoint names resolve through the `[[domain]]` rmw field, and the
    /// result is de-duped + order-preserving.
    #[test]
    fn parse_bridge_rmws_bare_endpoints_dedup() {
        let toml = r#"
[[domain]]
name = "a"
rmw = "zenoh"

[[domain]]
name = "b"
rmw = "xrce"

[[bridge]]
from = "a"
to = "b"

[[bridge]]
from = "b"
to = "a"
"#;
        assert_eq!(
            parse_bridge_rmws(toml),
            vec!["zenoh".to_string(), "xrce".to_string()]
        );
    }

    #[test]
    fn parse_bridge_rmws_no_bridge_is_empty() {
        assert!(parse_bridge_rmws("[system]\nname = \"x\"\n").is_empty());
        assert!(parse_bridge_rmws("not valid toml {{{").is_empty());
    }
}

#[cfg(test)]
mod entry_sizing_tests {
    //! phase-271 (issue #110) — `[package.metadata.nros.entry] max_callbacks`
    //! parsing that drives the macro's `run_with_deploy_sized` emit.
    use super::read_entry_executor_sizing;
    use std::io::Write;

    fn write_tmp(name: &str, body: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("nros_macros_entry_sizing_{name}"));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("Cargo.toml");
        std::fs::File::create(&p)
            .unwrap()
            .write_all(body.as_bytes())
            .unwrap();
        p
    }

    #[test]
    fn absent_max_callbacks_is_none() {
        // No knob → macro emits the default `run_with_deploy` (byte-identical).
        let p = write_tmp(
            "absent",
            "[package.metadata.nros.entry]\ndeploy = \"native\"\n",
        );
        assert_eq!(read_entry_executor_sizing(&p), None);
    }

    #[test]
    fn max_callbacks_only_defaults_sc_to_zero() {
        // `0` sched-contexts means "board uses the build default".
        let p = write_tmp(
            "cbs_only",
            "[package.metadata.nros.entry]\nmax_callbacks = 12\n",
        );
        assert_eq!(read_entry_executor_sizing(&p), Some((12, 0)));
    }

    #[test]
    fn max_callbacks_and_sched_contexts() {
        let p = write_tmp(
            "both",
            "[package.metadata.nros.entry]\nmax_callbacks = 8\nmax_sched_contexts = 5\n",
        );
        assert_eq!(read_entry_executor_sizing(&p), Some((8, 5)));
    }

    #[test]
    fn nonpositive_max_callbacks_is_none() {
        let p = write_tmp("zero", "[package.metadata.nros.entry]\nmax_callbacks = 0\n");
        assert_eq!(read_entry_executor_sizing(&p), None);
    }
}
