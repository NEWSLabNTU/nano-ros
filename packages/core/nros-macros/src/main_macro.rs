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

use std::path::{Path, PathBuf};

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
                        KvValue::Args(_) => {
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
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!(
                            "unknown `nros::main!` argument `{other}` \
                             (expected one of: board, launch, args)"
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
}

impl Parse for KeyValue {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let key: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        // Try the array form first (args = [...]) — `Expr::parse`
        // would also handle it, but extracting the tuple values is
        // cleaner via a dedicated branch.
        if input.peek(syn::token::Bracket) {
            let content;
            syn::bracketed!(content in input);
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
    let board_path = match &args.board {
        Some(p) => p.clone(),
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
            board_path_for(&deploy).ok_or_else(|| {
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
            })?
        }
    };

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

    let register_calls = pkg_idents.iter().map(|ident| {
        quote! {
            ::#ident::register(runtime)?;
        }
    });

    let expanded = quote! {
        // Phase 212.N.9 — rebuild-tracking workaround. Stable Rust
        // proc-macros can't use `proc_macro::tracked_path::path()`;
        // anonymous `const _: &[u8] = include_bytes!(...)` items are
        // tracked by cargo's `include_bytes!` and force a recompile
        // when any tracked file changes.
        #( #tracked_consts )*

        fn main() {
            let outcome = <#board_path as ::nros::__macro_support::nros_platform::BoardEntry>::run(
                |runtime: &mut ::nros::__macro_support::nros_platform::RuntimeCtx<'_>|
                    -> ::core::result::Result<
                        (),
                        ::nros::__macro_support::nros_platform::RuntimeError,
                    >
                {
                    #( #register_calls )*
                    ::core::result::Result::Ok(())
                },
            );
            if let ::core::result::Result::Err(e) = outcome {
                ::std::eprintln!("{}: {}", ::core::env!("CARGO_PKG_NAME"), e);
                ::std::process::exit(1);
            }
        }
    };

    Ok(expanded)
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
        "nuttx" | "qemu-arm-nuttx" => "::nros_board_qemu_arm_nuttx::QemuArmNuttx",
        "esp32" => "::nros_board_esp32::Esp32",
        "zephyr" => "::nros_board_zephyr::Zephyr",
        _ => return None,
    };
    syn::parse_str::<SynPath>(path_str).ok()
}

fn known_boards_csv() -> &'static str {
    "native, freertos, threadx-linux, threadx-qemu-riscv64, nuttx, esp32, zephyr"
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
