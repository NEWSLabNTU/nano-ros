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
            let workspace_root = nros_build::pkg_index::detect_workspace_root(&manifest_dir)
                .map_err(|e| {
                    syn::Error::new(
                        launch_lit.span(),
                        format!("nros::main!: detect_workspace_root: {e}"),
                    )
                })?;
            let pkg_index =
                nros_build::pkg_index::build_pkg_index(&workspace_root).map_err(|e| {
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
            let desc = nros_build::launch_parser::parse_launch_file(
                &launch_path,
                &pkg_index,
                &arg_overrides,
            )
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

                // Phase 228.G — collect the node instance + its declared
                // callback groups for tier resolution. The instance name keys
                // the group map (RFC-0032 §7); when the launch `<node>` has no
                // explicit name, fall back to the executable name.
                let instance = node
                    .name
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| node.exec.clone());
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
        .map(|ident| {
            quote! {
                ::#ident::register(runtime)?;
            }
        })
        .collect();
    // Node count for the Zephyr framework boot banner (literal baked at
    // expansion time so the runtime body needs no extra import).
    let num_register_calls = register_calls.len();

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
    let deploy_overlay_lit = match deploy_for_framework.as_deref() {
        Some(board_key) => read_deploy_overlay(&manifest_dir.join("Cargo.toml"), board_key),
        None => DeployOverlayLit::default(),
    };
    let deploy_overlay_ts = deploy_overlay_tokens(&deploy_overlay_lit);

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
                        #( #register_calls )*
                        ::core::result::Result::Ok(())
                    },
                )
            }
        }
        None => quote! {
            // Issue #48 cause 1 — `run_with_deploy` applies the deploy-metadata
            // overlay (locator / ip / gateway / domain) to the board's boot
            // config. The default trait body ignores the overlay and forwards
            // to `run`, so hosted / framework boards are byte-identical; the
            // FreeRTOS / bare-metal boards override it to stop the
            // `[deploy.<board>]` block being inert.
            <#board_path as ::nros::__macro_support::nros_platform::BoardEntry>::run_with_deploy(
                &#deploy_overlay_ts,
                |runtime: &mut ::nros::__macro_support::nros_platform::RuntimeCtx<'_>|
                    -> ::core::result::Result<
                        (),
                        ::nros::__macro_support::nros_platform::RuntimeError,
                    >
                {
                    #( #register_calls )*
                    #[cfg(not(target_os = "none"))]
                    __nros_hosted_spin_if_requested(runtime)?;
                    ::core::result::Result::Ok(())
                },
            )
        },
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

    // Phase 216.B.3 — framework-dispatched emit body. `OwnedSpin`
    // keeps the long-standing `fn __nros_entry_run + fn main` shape
    // (BoardEntry::run owns the spin loop). `Rtic` emits a
    // `#[rtic::app(...)]` skeleton that delegates to
    // `RticBoardEntry::init_hardware` from the framework-generated
    // `#[init]` body. `Embassy` is a hard error pointing at the
    // 216.C.3 sibling that lands the emit body.
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
                // Phase 244.D1 — install a board custom transport (e.g. the
                // XRCE-over-UART vtable) selected by `deploy.transport`, BEFORE
                // the RMW registers. XRCE's `set_custom_transport_ops` must
                // precede `register`; the default `setup_transport` is a no-op so
                // every other board/deploy is byte-identical.
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
                // Carrier / link-up gate. Use the same
                // `platform::zephyr::wait_for_network` wrapper the
                // single-node `nros::zephyr_component_main!` uses — it
                // exposes a real linkable symbol. (`ZephyrBoard::wait_link_up`
                // calls Zephyr's `net_if_is_up` / `k_msleep`, which are
                // `static inline` header functions with no link symbol, so
                // the native_sim final link fails with undefined references.)
                let _ = ::nros::platform::zephyr::wait_for_network(2000);

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
                let config = match BAKED_LOCATOR {
                    ::core::option::Option::Some(loc) if !loc.is_empty() => {
                        ::nros::ExecutorConfig::new(loc)
                            .node_name(::core::env!("CARGO_PKG_NAME"))
                    }
                    _ => ::nros::ExecutorConfig::default_const()
                        .node_name(::core::env!("CARGO_PKG_NAME")),
                };
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
                #( #register_calls )*
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
                        #( #register_calls )*
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
                        executor: <__NrosBoard as ::nros::__macro_support::nros_platform::RticBoardEntry>::Executor,
                        runtime: <__NrosBoard as ::nros::__macro_support::nros_platform::RticBoardEntry>::Runtime,
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
                        let (mut executor, runtime) =
                            <__NrosBoard as ::nros::__macro_support::nros_platform::RticBoardEntry>::init_hardware_with_deploy(
                                cx.device,
                                cx.core,
                                &#deploy_overlay_ts,
                            );
                        // Phase 216 final wave — per-Node dispatch
                        // registration. Each Node pkg's
                        // `<pkg>::register_dispatch(&mut executor)`
                        // (emitted by `nros::node!()`) builds the
                        // Node's `State` blob and pushes
                        // `(state, __nros_node_<pkg>_on_callback)`
                        // into the executor's dispatch-slot table.
                        // The `__nros_run` task's
                        // `executor.dispatch_callback(cb_id, ctx)`
                        // call (above) walks this table once per
                        // dequeued envelope.
                        #( #framework_register_dispatch_calls )*
                        __nros_run::spawn().unwrap();
                        (Shared {}, Local { executor, runtime })
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
                    #[task(local = [executor, runtime], priority = 1)]
                    async fn __nros_run(cx: __nros_run::Context) {
                        let executor = cx.local.executor;
                        // The board-side runtime owns the SPSC
                        // producer half. Today's collapse keeps it in
                        // `Local` for symmetry with the planned split
                        // — once `ExecutorNodeRuntime`-wrapped routing
                        // lands the runtime's `signal_callback` will
                        // be the producer-side bridge between executor
                        // callbacks and the SPSC consumer drained
                        // below.
                        let _runtime = cx.local.runtime;
                        let mut consumer =
                            #rtic_consumer()
                                .expect("RTIC dispatch consumer take");
                        loop {
                            let _ = executor.spin_once(
                                ::core::time::Duration::from_millis(1),
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
                                executor.dispatch_callback(cb.cb_id, cb.ctx_ptr);
                            }
                        }
                    }

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
                    // Sync `init_hardware` — see the
                    // `EmbassyBoardEntry` trait "Sync
                    // `init_hardware`" note; matches `RticBoardEntry`.
                    let (mut executor, runtime) =
                        <__NrosBoard as ::nros::__macro_support::nros_platform::EmbassyBoardEntry>::init_hardware(
                            spawner,
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

    let expanded = quote! {
        // Phase 212.N.9 — rebuild-tracking workaround. Stable Rust
        // proc-macros can't use `proc_macro::tracked_path::path()`;
        // anonymous `const _: &[u8] = include_bytes!(...)` items are
        // tracked by cargo's `include_bytes!` and force a recompile
        // when any tracked file changes.
        #( #tracked_consts )*

        #body_ts
    };

    Ok(expanded)
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
    quote! {
        ::nros::__macro_support::nros_platform::DeployOverlay {
            locator: #locator,
            ip: #ip,
            gateway: #gateway,
            netmask: #netmask,
            domain_id: #domain_id,
            transport: #transport,
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

/// Map a board key from `[package.metadata.nros.entry] deploy = "X"`
/// to the tier-1 board crate's ZST type path. Adding a new board
/// requires a workspace-wide edit anyway; the table stays local.
///
/// Per the Phase 212.N.3 tier-1 board crates inventory (run
/// `ls packages/boards/`):
fn board_path_for(deploy: &str) -> Option<SynPath> {
    let path_str = match deploy {
        "native" | "posix" => "::nros_board_native::NativeBoard",
        "freertos" | "freertos-qemu-mps2-an385" | "qemu-arm-freertos" => {
            "::nros_board_mps2_an385_freertos::Mps2An385"
        }
        "threadx-linux" => "::nros_board_threadx_linux::ThreadxLinux",
        "threadx-qemu-riscv64" | "qemu-riscv64-threadx" => {
            "::nros_board_threadx_qemu_riscv64::ThreadxQemuRiscv64"
        }
        "nuttx" | "qemu-arm-nuttx" => "::nros_board_nuttx_qemu_arm::QemuArmVirt",
        // Phase 225.O — CI-runnable ESP32-C3 QEMU (OpenETH) board. esp32
        // is its own framework (esp-riscv-rt's `_start` requires the
        // esp-hal entry registration), routed via `framework_for` to the
        // `Framework::Esp32` emit shape (`#[esp_hal::main]`). The board
        // ZST impls the real-runtime `BoardEntry`. (The WiFi-only `"esp32"`
        // board was dropped 2026-06-14 — untestable in any emulator, see
        // phase-244 D2.)
        "esp32-qemu" | "qemu-esp32-baremetal" => "::nros_board_esp32_qemu::Esp32QemuEntry",
        // Phase 225.P — Zephyr is its own framework (RTOS owns `main`).
        // `ZephyrBoard` impls `NetworkWait` only (NOT `BoardEntry`); the
        // macro routes `deploy = "zephyr"` through `framework_for` to the
        // `Framework::Zephyr` emit shape (`rust_main` staticlib export),
        // and uses this board path solely for the link-up gate.
        "zephyr" => "::nros_board_zephyr::ZephyrBoard",
        // Phase 216.B.3 — RTIC + STM32F4 framework-owned-spin board.
        // The board ZST impls `RticBoardEntry` (not `BoardEntry`);
        // the macro routes through `framework_for(deploy)` below to
        // pick the `#[rtic::app(...)]` emit shape instead of the
        // direct-exec `BoardEntry::run` shape.
        "rtic-stm32f4" => "::nros_board_rtic_stm32f4::RticStm32F4",
        "rtic-mps2-an385" | "qemu-rtic-mps2-an385" => "::nros_board_rtic_mps2_an385::RticMps2An385",
        // Phase 244.D1 — pure bare-metal (no-RTOS) MPS2-AN385 direct-exec board.
        // OwnedSpin framework + a `#[cortex_m_rt::entry]` reset emit (see
        // `is_baremetal_cortexm_deploy`): the board ZST impls the new
        // `nros_platform::BoardEntry` (behind its `board-entry` feature) and
        // drives boot → executor → spin inline on the reset thread. Distinct
        // from `rtic-mps2-an385`, which routes through the RTIC framework emit.
        "qemu-mps2-an385" | "mps2-an385" => "::nros_board_mps2_an385::Mps2An385",
        // Phase 244.C5 — pure bare-metal (no-RTOS) STM32F4 direct-exec board.
        // Same shape as `mps2-an385`: the board ZST impls `nros_platform::BoardEntry`
        // (behind its `board-entry` feature) + a `#[cortex_m_rt::entry]` reset emit
        // (`is_baremetal_cortexm_deploy`). Distinct from `rtic-stm32f4` /
        // `embassy-stm32f4`, which route through their framework emit shapes.
        "stm32f4" => "::nros_board_stm32f4::Stm32F4",
        // Phase 216.C.3 — Embassy + STM32F4 framework-owned-spin board.
        // Same dispatch story as `rtic-stm32f4`: the board ZST impls
        // `EmbassyBoardEntry` (not `BoardEntry`); the macro routes
        // through `framework_for(deploy)` to pick the
        // `#[embassy_executor::main]` emit shape.
        "embassy-stm32f4" => "::nros_board_embassy_stm32f4::EmbassyStm32F4",
        _ => return None,
    };
    syn::parse_str::<SynPath>(path_str).ok()
}

fn known_boards_csv() -> &'static str {
    "native, freertos, threadx-linux, threadx-qemu-riscv64, nuttx, esp32-qemu, zephyr, \
     rtic-stm32f4, rtic-mps2-an385, qemu-mps2-an385, stm32f4, embassy-stm32f4"
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
}

fn rtic_board_spec_for(deploy: &str) -> Option<RticBoardSpec> {
    let (device, dispatchers, consumer) = match deploy {
        "rtic-stm32f4" => (
            "stm32f4xx_hal::pac",
            &["USART1", "USART2"][..],
            "::nros_board_rtic_stm32f4::take_dispatch_consumer",
        ),
        "rtic-mps2-an385" | "qemu-rtic-mps2-an385" => (
            "mps2_an385_pac",
            &["UARTRX0", "UARTTX0"][..],
            "::nros_board_rtic_mps2_an385::take_dispatch_consumer",
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
