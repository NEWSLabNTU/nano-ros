//! Stage 4c — generate the entry package (phase-383 W3.b, RFC-0065 D4).
//!
//! **This is the headline claim of the phase**: "the entry stops being
//! hand-written". Everything else generates a *build file*; this generates the
//! program.
//!
//! ## What an entry actually is
//!
//! Measured across the tree before writing any of this: every Rust entry source
//! is **≤ 6 non-comment lines**, every *embedded* C/C++ entry has **zero**
//! source files, and a native C entry is two lines. The Rust ones look like:
//!
//! ```ignore
//! #![no_std]
//! #![no_main]
//! use panic_semihosting as _;
//! nros::main!(panic = "own", launch = "demo_bringup");
//! ```
//!
//! Three parts, and all three are derivable:
//!
//! | part | derived from |
//! | --- | --- |
//! | the `no_std` / `no_main` shell | the board's `entry_kind` |
//! | the board boilerplate (`use panic_semihosting as _;`, `esp_app_desc!()`) | the board descriptor's `entry.crate_root_extra` |
//! | `nros::main!(launch = …, args = …)` | the image |
//!
//! The second is the one worth pointing at: `crate_root_extra` already carries
//! each board's shell verbatim, so this emitter needs **no per-board knowledge
//! of its own**. Adding a board adds a descriptor, not a branch here.
//!
//! ## Why `nros::main!` and not the expanded form
//!
//! `nros codegen entry` can emit the fully expanded entry, and using it here
//! would be a mistake. The macro reads the launch XML **at expansion time**, so
//! a generated entry that calls it keeps its derivation LIVE: add a node to the
//! launch file and the next compile picks it up. Expanding here would freeze
//! the node set into a file regenerated only when the builder decides to.
//!
//! It is also what makes `nros materialize` cheap (D5): the materialised
//! package is the same six lines, and the part a user would want to keep
//! editing is the shell — not a hundred lines of expansion.
//!
//! ## Paths are relative, always
//!
//! A generated entry sits in `build/<coord>/` and depends on crates in the
//! nano-ros checkout and node packages in the user's workspace — three trees.
//! Every path between them is computed relative (W3.c); an absolute one is a
//! manifest that only builds on the machine that wrote it.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use super::paths::relative_or_err;
use crate::orchestration::board_descriptor::{BoardDescriptor, EntryKind};

/// Everything the emitter needs. Assembled by the caller so this module does
/// no discovery of its own and stays testable without a workspace.
#[derive(Debug, Clone)]
pub struct EntrySpec {
    /// `[image.<id>]` key — names the package (`<id>_entry`).
    pub image_id: String,
    /// The `deploy` token recorded in `[package.metadata.nros.entry]`, which
    /// `nros check` and the macro's board resolution both read.
    pub deploy: String,
    /// `"<bringup>:<file.launch.xml>"`, or just `"<bringup>"` for its default.
    pub launch: String,
    /// Launch arguments bound at resolve time — how an image selects a machine.
    pub args: BTreeMap<String, String>,
    /// RFC-0077 panic policy, already validated. `None` leaves the macro's own
    /// default in place.
    pub panic: Option<String>,
    /// Node packages the launch file names, as (crate name, directory).
    pub nodes: Vec<(String, PathBuf)>,
    /// The nano-ros checkout, for `nros` / board / platform crate paths.
    pub nano_ros_root: PathBuf,
    /// The generated selection facade's directory, when one exists.
    pub facade_dir: Option<PathBuf>,
    /// Dependencies the BRINGUP implies, as `name = { … }` manifest lines.
    ///
    /// A declaration in `system.toml` can make the macro emit code that needs a
    /// crate no node package brings: `[[bridge]]` makes it call
    /// `nros_bridge::run_from_config_str`, and without the dep the generated
    /// entry fails with `cannot find module or crate` several frames inside the
    /// macro. The hand-written bridge entries listed `nros-bridge` by hand;
    /// nothing else in the graph would have told the emitter.
    ///
    /// Manifest lines rather than a structured type, for the same reason as
    /// `crate_root_deps`: a dependency spec is already a small language and
    /// re-modelling it here would be a second grammar to keep in step.
    pub bringup_deps: Vec<String>,
}

/// Board facts the emitter needs, lifted out of [`BoardDescriptor`] so callers
/// can construct one in a test without a catalog.
#[derive(Debug, Clone)]
pub struct BoardFacts {
    pub entry_kind: EntryKind,
    /// Board crate to depend on; `None` for crate-less host boards.
    pub board_crate: Option<String>,
    /// Workspace-relative path to that crate.
    pub crate_path: Option<String>,
    pub board_features: Vec<String>,
    /// `nros/<feature>` this board selects — `platform-posix`, `platform-zephyr`.
    pub platform_feature: String,
    /// Verbatim crate-root items: the panic-crate `use`, `esp_app_desc!()`.
    pub crate_root_extra: String,
    /// Manifest lines for the crates [`Self::crate_root_extra`] names. Travels
    /// WITH it — a crate-root `use` whose crate is not a dependency is a
    /// generated entry that does not compile.
    pub crate_root_deps: Vec<String>,
}

/// The board crate `nros::main!` will emit a reference to, if any.
///
/// **Read from `board_path_for`, NOT from the descriptor's `board_crate`, and
/// the difference is load-bearing.** Those two disagree: the `linux` descriptor
/// declares no `board_crate` ("crate-less host board"), while
/// `nros_orchestration_ir::board_path_for("native")` returns
/// `::nros_board_linux::LinuxBoard` — which the macro emits, and which
/// therefore must be in scope or the generated entry does not compile. The
/// hand-written `native_entry` depends on `nros-board-linux` for exactly this
/// reason, and the same holds for zephyr.
///
/// So: the macro's mapping is authoritative here, because the macro's output is
/// what has to build. Two sources for one fact is this repository's named
/// defect class; this function picks the one that is checkable by compilation.
/// Tries each key in order and takes the first that resolves. The mapping is
/// keyed on the DEPLOY token (`native`, `freertos`, `zephyr`) — not on the
/// board name, which is why `linux` alone misses and `native` hits. An image id
/// is the natural first candidate (it IS the deploy key), with the board and
/// platform as fallbacks for an image named something else entirely.
#[must_use]
pub fn macro_board_crate(candidates: &[&str]) -> Option<String> {
    for key in candidates {
        if key.is_empty() {
            continue;
        }
        if let Some(path) = nros_orchestration_ir::board_path_for(key) {
            // `::nros_board_linux::LinuxBoard` → `nros_board_linux`
            //   → `nros-board-linux`
            let krate = path.trim_start_matches(':').split("::").next()?;
            return Some(krate.replace('_', "-"));
        }
    }
    None
}

/// The `deploy` token to record, from the same candidate list as the crate.
///
/// **These two must agree, and that is the whole point of sharing the search.**
/// `deploy` is what `nros::main!` looks up in its own board table at expansion
/// time; the crate is what must be in scope for the path that lookup returns.
/// Writing the image id unconditionally worked only while an image happened to
/// be NAMED after a board — `[image.native]` does, `[image.native_service_server]`
/// does not, and the generated entry failed with "unknown board
/// `native_service_server` in `[package.metadata.nros.entry] deploy`". A
/// hand-written entry never hit this because a human wrote `deploy = "native"`
/// and named the package whatever they liked.
///
/// Falls back to the first candidate so the error, when nothing resolves, still
/// names what the author wrote rather than an empty string.
#[must_use]
pub fn macro_deploy_token(candidates: &[&str]) -> String {
    for key in candidates {
        if !key.is_empty() && nros_orchestration_ir::board_path_for(key).is_some() {
            return (*key).to_string();
        }
    }
    candidates
        .iter()
        .find(|k| !k.is_empty())
        .map_or_else(String::new, |k| (*k).to_string())
}

impl BoardFacts {
    /// Lift from a descriptor, taking the board crate from the MACRO's mapping.
    ///
    /// `candidates` are deploy-key spellings to try, most specific first —
    /// typically `[image_id, board, platform]`.
    #[must_use]
    pub fn from_descriptor_for(d: &BoardDescriptor, candidates: &[&str]) -> Self {
        let mut f = Self::from_descriptor(d);
        if let Some(k) = macro_board_crate(candidates) {
            // The descriptor's crate_path only applies to ITS crate name; a
            // different crate takes the conventional location.
            if f.board_crate.as_deref() != Some(k.as_str()) {
                f.crate_path = None;
            }
            f.board_crate = Some(k);
        }
        f
    }

    /// Lift from a descriptor alone. Prefer [`Self::from_descriptor_for`],
    /// which also resolves the crate the macro will reference.
    #[must_use]
    pub fn from_descriptor(d: &BoardDescriptor) -> Self {
        Self {
            entry_kind: d.entry_kind,
            board_crate: d.board_crate.clone(),
            crate_path: d.crate_path.clone(),
            board_features: d.board_features.clone(),
            platform_feature: d.platform_feature.clone(),
            crate_root_extra: d
                .entry
                .as_ref()
                .map(|e| e.crate_root_extra.clone())
                .unwrap_or_default(),
            crate_root_deps: d
                .entry
                .as_ref()
                .map(|e| e.crate_root_deps.clone())
                .unwrap_or_default(),
        }
    }
}

/// Package name for an image's entry.
#[must_use]
pub fn package_name(image_id: &str) -> String {
    format!("{}_entry", image_id.replace(['-', '.', '/'], "_"))
}

/// Render `Cargo.toml`.
pub fn render_manifest(
    spec: &EntrySpec,
    board: &BoardFacts,
    entry_dir: &Path,
) -> Result<String, String> {
    let pkg = package_name(&spec.image_id);
    let mut out = String::new();
    out.push_str(&format!(
        "# GENERATED by `nros build` (phase-383 W3.b) — DO NOT EDIT.\n\
         #\n\
         # Regenerated from `[image.{}]` on every build. To change what this\n\
         # entry links, edit the launch file or the image declaration.\n\
         #\n\
         # To take ownership instead: `nros materialize {}` — one way, and the\n\
         # builder then leaves it alone.\n\n",
        spec.image_id, spec.image_id
    ));

    out.push_str("[package]\n");
    out.push_str(&format!("name = \"{pkg}\"\n"));
    // A fixed version, deliberately: this package is never published and never
    // depended on by version, and a version that moved would churn every
    // consumer's lockfile for no information.
    out.push_str("version = \"0.0.0\"\nedition = \"2024\"\npublish = false\n");
    out.push_str(&format!(
        "description = \"Generated entry for image `{}`.\"\n\n",
        spec.image_id
    ));

    match board.entry_kind {
        // Zephyr owns `main`, so the entry is a staticlib exporting `rust_main`
        // for `rust_cargo_application()`. `[[bin]]` here would produce a
        // Zephyr-forbidden Rust `fn main`.
        EntryKind::ZephyrStaticlib => {
            // `name = "rustapp"` is NOT a choice: zephyr-lang-rust's
            // `rust_cargo_application()` links `librustapp.a` by that fixed
            // name, so a lib named after the package silently produces an
            // archive the Zephyr build never finds. `rlib` rides alongside
            // because the workspace still needs to depend on it as a Rust
            // crate. Both facts read off the hand-written zephyr_entry.
            out.push_str("[lib]\n");
            out.push_str("name = \"rustapp\"\n");
            out.push_str("path = \"src/lib.rs\"\ncrate-type = [\"staticlib\", \"rlib\"]\n\n");
        }
        EntryKind::HostedMain | EntryKind::BoardRun => {
            out.push_str("[[bin]]\n");
            out.push_str(&format!("name = \"{pkg}\"\n"));
            out.push_str("path = \"src/main.rs\"\n\n");
        }
    }

    out.push_str("[package.metadata.nros.entry]\n");
    out.push_str(&format!("deploy = \"{}\"\n\n", spec.deploy));

    out.push_str("[dependencies]\n");

    // The selection facade carries the RMW, edition and capability features so
    // cargo unifies them onto `nros` and the board crate. Absent when the
    // workspace has not been synced.
    if let Some(facade) = &spec.facade_dir {
        let rel = relative_or_err(entry_dir, facade)?;
        out.push_str(&format!("{pkg}_nros_selection = {{ path = \"{rel}\" }}\n"));
    }

    // The umbrella names neither the platform nor the RMW — the board crate
    // brings both, and the facade unifies the axes (phase-248 C6a).
    let nros_rel = relative_or_err(entry_dir, &spec.nano_ros_root.join("packages/api/nros"))?;
    out.push_str(&format!(
        "nros = {{ path = \"{nros_rel}\", default-features = false, \
         features = [\"alloc\", \"rmw-cffi\", \"macros\"] }}\n"
    ));

    if let Some(krate) = &board.board_crate {
        let rel_path = board
            .crate_path
            .clone()
            .unwrap_or_else(|| format!("packages/boards/{krate}"));
        let rel = relative_or_err(entry_dir, &spec.nano_ros_root.join(&rel_path))?;
        let feats = if board.board_features.is_empty() {
            String::new()
        } else {
            format!(
                ", features = [{}]",
                board
                    .board_features
                    .iter()
                    .map(|f| format!("\"{f}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        out.push_str(&format!("{krate} = {{ path = \"{rel}\"{feats} }}\n"));
    }
    // The platform feature is named EXPLICITLY even when a board crate carries
    // it. The hand-written freertos and zephyr entries both do this, and the
    // reason is feature unification: a board crate's own default may not select
    // the platform when the entry's graph differs, and an unselected platform
    // is a link error a long way from its cause.
    let plat_rel = relative_or_err(
        entry_dir,
        &spec.nano_ros_root.join("packages/platform/nros-platform"),
    )?;
    out.push_str(&format!(
        "nros-platform = {{ path = \"{plat_rel}\", default-features = false, \
         features = [\"{}\"] }}\n",
        board.platform_feature
    ));

    // Whatever the board's crate-root items need. Emitted right before the node
    // packages so a reader sees it beside the `use` it serves — and emitted at
    // all because the descriptor is the only place that knows: the items are
    // verbatim board text (`use panic_semihosting as _;`), so this file cannot
    // infer the crate from them without parsing Rust.
    for dep in &board.crate_root_deps {
        out.push_str(dep);
        out.push('\n');
    }

    // What the BRINGUP's own declarations require — see `bringup_deps`.
    for dep in &spec.bringup_deps {
        out.push_str(dep);
        out.push('\n');
    }

    // Node packages, in launch order. Each is an rlib the macro's emitted
    // `register()` calls reach; `default-features = false` keeps a node from
    // dragging a platform choice into an entry that already made one.
    for (name, dir) in &spec.nodes {
        let rel = relative_or_err(entry_dir, dir)?;
        out.push_str(&format!(
            "{name} = {{ path = \"{rel}\", default-features = false }}\n"
        ));
    }
    Ok(out)
}

/// Render `src/main.rs` (or `src/lib.rs` for a Zephyr staticlib).
#[must_use]
pub fn render_source(spec: &EntrySpec, board: &BoardFacts) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "// GENERATED by `nros build` (phase-383 W3.b) — DO NOT EDIT.\n\
         //\n\
         // Regenerated from `[image.{}]`. The launch file is the source of\n\
         // truth for which nodes run; `nros::main!` reads it at COMPILE time,\n\
         // so adding a node there needs no change here.\n\
         //\n\
         // `nros materialize {}` takes ownership of this file, one way.\n\n",
        spec.image_id, spec.image_id
    ));

    // The shell. `no_std` for anything that is not a hosted main; `no_main`
    // only where the board supplies the entry symbol.
    match board.entry_kind {
        EntryKind::HostedMain => {}
        EntryKind::BoardRun => out.push_str("#![no_std]\n#![no_main]\n\n"),
        // Zephyr's C `main` calls into `rust_main`; there is no Rust main to
        // suppress, so `no_main` would be wrong here.
        EntryKind::ZephyrStaticlib => out.push_str("#![no_std]\n\n"),
    }

    // Board boilerplate, verbatim from the descriptor. This is why the emitter
    // needs no per-board branch: `use panic_semihosting as _;` and
    // `esp_app_desc!()` both arrive here without this file knowing either.
    if !board.crate_root_extra.is_empty() {
        out.push_str(board.crate_root_extra.trim_end());
        out.push_str("\n\n");
    }

    out.push_str("nros::main!(\n");
    if let Some(p) = &spec.panic {
        out.push_str(&format!("    panic = \"{p}\",\n"));
    }
    out.push_str(&format!("    launch = \"{}\",\n", spec.launch));
    if !spec.args.is_empty() {
        let pairs: Vec<String> = spec
            .args
            .iter()
            .map(|(k, v)| format!("(\"{k}\", \"{v}\")"))
            .collect();
        out.push_str(&format!("    args = [{}],\n", pairs.join(", ")));
    }
    out.push_str(");\n");
    out
}

/// Write the entry package under `parent`, returning its directory.
///
/// **Refuses to touch a materialised entry.** Once a user owns it, the builder
/// leaves it alone — that is the whole contract of D5's escape hatch, and
/// silently regenerating over someone's hand-written `main` would be the worst
/// failure this phase could ship.
pub fn write(spec: &EntrySpec, board: &BoardFacts, parent: &Path) -> Result<PathBuf, String> {
    let dir = parent.join(package_name(&spec.image_id));
    if super::materialize::is_materialized(&dir) {
        return Ok(dir);
    }
    let manifest = render_manifest(spec, board, &dir)?;
    let source = render_source(spec, board);
    let src_name = match board.entry_kind {
        EntryKind::ZephyrStaticlib => "lib.rs",
        _ => "main.rs",
    };

    std::fs::create_dir_all(dir.join("src"))
        .map_err(|e| format!("creating {}: {e}", dir.join("src").display()))?;
    write_if_changed(&dir.join("Cargo.toml"), &manifest)?;
    write_if_changed(&dir.join("src").join(src_name), &source)?;
    Ok(dir)
}

/// Write only when the content differs — an unchanged mtime keeps cargo from
/// rebuilding, and this repo's fixture-staleness rules make a gratuitous touch
/// expensive (CLAUDE.md's mtime treadmill).
fn write_if_changed(path: &Path, body: &str) -> Result<(), String> {
    if std::fs::read_to_string(path).ok().as_deref() == Some(body) {
        return Ok(());
    }
    std::fs::write(path, body).map_err(|e| format!("writing {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_board_the_macro_table_does_not_know_falls_through() {
        // This function answers for the RUST macro, whose board table is keyed
        // on deploy tokens like `freertos` and knows nothing of
        // `mps2-an385-freertos`. So a specific board id falls through to the
        // one the macro can actually resolve.
        //
        // The CMAKE entry must NOT use this: `nano_ros_add_executable(DEPLOY …)`
        // resolves against the board CATALOG, which does know the specific id,
        // and the hand-written entry said `DEPLOY mps2-an385-freertos`. Routing
        // it through here picked the generic `freertos` board and the mps2
        // board's lwIP glue was simply absent at link time —
        // `undefined reference to lwip_setsockopt` (phase-383 W10.a).
        assert_eq!(
            macro_deploy_token(&["mps2-an385-freertos", "freertos", "freertos"]),
            "freertos"
        );
    }

    #[test]
    fn the_deploy_token_falls_back_when_the_image_is_not_a_board_name() {
        // phase-383 W9.b, found by migrating `examples/workspaces/rust`.
        // `[image.native_service_server]` is a perfectly good image id and not a
        // board token; writing it as `deploy` made the generated entry fail with
        // "unknown board `native_service_server`". `[image.native]` hid it,
        // because there the id and the token coincide.
        assert_eq!(
            macro_deploy_token(&["native", "native_service_server", "posix"]),
            "native"
        );
        assert_eq!(macro_deploy_token(&["", "native_robot1", "posix"]), "posix");
    }

    #[test]
    fn the_deploy_token_and_the_board_crate_come_from_one_search() {
        // They must agree: `deploy` is what `nros::main!` looks up, and the
        // crate is what its answer needs in scope. Two searches could disagree,
        // and the disagreement is an entry that does not compile.
        let candidates = ["robot1", "native", "posix"];
        let token = macro_deploy_token(&candidates);
        let krate = macro_board_crate(&candidates).expect("resolves");
        let from_token = macro_board_crate(&[token.as_str()]).expect("resolves");
        assert_eq!(
            krate, from_token,
            "token {token} must select the same crate"
        );
    }

    use super::*;

    #[test]
    fn an_unresolvable_candidate_list_still_names_what_the_author_wrote() {
        assert_eq!(macro_deploy_token(&["", "nonesuch", ""]), "nonesuch");
    }

    fn spec() -> EntrySpec {
        EntrySpec {
            image_id: "native".to_string(),
            deploy: "native".to_string(),
            launch: "demo_bringup:system.launch.xml".to_string(),
            args: BTreeMap::new(),
            panic: None,
            nodes: vec![
                (
                    "talker_pkg".to_string(),
                    PathBuf::from("/ws/src/talker_pkg"),
                ),
                (
                    "listener_pkg".to_string(),
                    PathBuf::from("/ws/src/listener_pkg"),
                ),
            ],
            nano_ros_root: PathBuf::from("/nros"),
            facade_dir: Some(PathBuf::from("/ws/build/nros/nros-selection/native_entry")),
            bringup_deps: Vec::new(),
        }
    }

    fn hosted() -> BoardFacts {
        BoardFacts {
            entry_kind: EntryKind::HostedMain,
            board_crate: Some("nros-board-linux".to_string()),
            crate_path: None,
            board_features: Vec::new(),
            platform_feature: "platform-posix".to_string(),
            crate_root_extra: String::new(),
            crate_root_deps: Vec::new(),
        }
    }

    fn freertos() -> BoardFacts {
        BoardFacts {
            entry_kind: EntryKind::BoardRun,
            board_crate: Some("nros-board-mps2-an385-freertos".to_string()),
            crate_path: None,
            board_features: Vec::new(),
            platform_feature: "platform-freertos".to_string(),
            crate_root_extra: "use panic_semihosting as _;".to_string(),
            crate_root_deps: vec!["panic-semihosting = \"0.6\"".to_string()],
        }
    }

    const DIR: &str = "/ws/build/posix-zenoh/native_entry";

    #[test]
    fn a_hosted_entry_is_a_bin_with_no_no_std() {
        let m = render_manifest(&spec(), &hosted(), Path::new(DIR)).expect("renders");
        assert!(m.contains("[[bin]]"), "{m}");
        let s = render_source(&spec(), &hosted());
        assert!(!s.contains("no_std"), "a hosted main is std: {s}");
        assert!(s.contains("nros::main!("), "{s}");
    }

    #[test]
    fn a_board_run_entry_is_no_std_no_main() {
        let s = render_source(&spec(), &freertos());
        assert!(s.contains("#![no_std]"), "{s}");
        assert!(s.contains("#![no_main]"), "{s}");
    }

    #[test]
    fn a_zephyr_entry_is_a_staticlib_and_not_no_main() {
        // Zephyr's C main calls rust_main, so there is no Rust main to
        // suppress — `no_main` would be wrong, and `[[bin]]` would produce a
        // Zephyr-forbidden `fn main`.
        let mut b = hosted();
        b.entry_kind = EntryKind::ZephyrStaticlib;
        b.board_crate = None;
        b.platform_feature = "platform-zephyr".to_string();

        let m = render_manifest(&spec(), &b, Path::new(DIR)).expect("renders");
        assert!(m.contains("[lib]"), "{m}");
        assert!(m.contains("crate-type = [\"staticlib\", \"rlib\"]"), "{m}");
        assert!(
            m.contains("name = \"rustapp\""),
            "zephyr-lang-rust links librustapp.a by FIXED name; a lib named \
             after the package produces an archive it never finds: {m}"
        );
        assert!(!m.contains("[[bin]]"), "{m}");

        let s = render_source(&spec(), &b);
        assert!(s.contains("#![no_std]"), "{s}");
        assert!(!s.contains("no_main"), "Zephyr owns main: {s}");
    }

    #[test]
    fn board_boilerplate_comes_from_the_descriptor_not_from_this_file() {
        // The property that keeps this emitter board-agnostic: adding a board
        // adds a descriptor, never a branch here.
        let s = render_source(&spec(), &freertos());
        assert!(s.contains("use panic_semihosting as _;"), "{s}");

        let mut esp = freertos();
        esp.crate_root_extra =
            "use esp_backtrace as _;\nnros_board_esp32_qemu::esp_bootloader_esp_idf::esp_app_desc!();"
                .to_string();
        let s2 = render_source(&spec(), &esp);
        assert!(s2.contains("esp_app_desc!()"), "{s2}");
    }

    #[test]
    fn node_packages_from_the_launch_file_become_dependencies() {
        let m = render_manifest(&spec(), &hosted(), Path::new(DIR)).expect("renders");
        assert!(
            m.contains("talker_pkg = { path = \"../../../src/talker_pkg\""),
            "{m}"
        );
        assert!(m.contains("listener_pkg = "), "{m}");
        assert!(m.contains("default-features = false"), "{m}");
    }

    #[test]
    fn every_path_is_relative() {
        // W3.c — an absolute path is a manifest that only builds on the machine
        // that wrote it.
        let m = render_manifest(&spec(), &hosted(), Path::new(DIR)).expect("renders");
        // A quoted value starting with `/` is the actual defect. Checking for
        // the substring "/nros/" instead would fire on the CORRECT relative
        // path `../../../../nros/packages/api/nros` — the first version of this
        // assertion did exactly that.
        assert!(
            !m.contains("= \"/") && !m.contains("path = \"/"),
            "no dependency path may be absolute: {m}"
        );
        assert!(m.contains("../"), "relative paths present: {m}");
    }

    #[test]
    fn the_platform_feature_is_always_named_explicitly() {
        // Both hand-written embedded entries do this. A board crate's own
        // default may not select the platform once the entry's feature graph
        // differs, and an unselected platform is a link error far from its
        // cause.
        let m = render_manifest(&spec(), &hosted(), Path::new(DIR)).expect("renders");
        assert!(m.contains("nros-platform = "), "{m}");
        assert!(m.contains("\"platform-posix\""), "{m}");
    }

    #[test]
    fn the_board_crate_comes_from_the_macros_mapping_not_the_descriptor() {
        // The `linux` descriptor declares NO board_crate, but
        // board_path_for("native") returns ::nros_board_linux::LinuxBoard —
        // which the macro emits, so it must be in scope or the entry does not
        // compile. Two sources for one fact; this picks the checkable one.
        assert_eq!(
            macro_board_crate(&["native"]).as_deref(),
            Some("nros-board-linux")
        );
        assert_eq!(
            macro_board_crate(&["zephyr"]).as_deref(),
            Some("nros-board-zephyr")
        );
        assert_eq!(macro_board_crate(&["not-a-board"]), None);
        // The mapping is keyed on the DEPLOY token, not the board name, so
        // `linux` misses and the fallback chain is what saves it.
        assert_eq!(macro_board_crate(&["linux"]), None);
        assert_eq!(
            macro_board_crate(&["robot1", "linux", "posix"]).as_deref(),
            Some("nros-board-linux"),
            "an image named something else falls back to its platform"
        );
    }

    #[test]
    fn launch_args_reach_the_macro() {
        // How an image selects a MACHINE: native_entry_robot1 and _robot2
        // differ only here.
        let mut s = spec();
        s.args.insert("host".to_string(), "robot1".to_string());
        let src = render_source(&s, &hosted());
        assert!(src.contains("args = [(\"host\", \"robot1\")]"), "{src}");
    }

    #[test]
    fn a_panic_policy_is_forwarded_when_declared() {
        let mut s = spec();
        s.panic = Some("own".to_string());
        assert!(render_source(&s, &freertos()).contains("panic = \"own\""));
        // Absent leaves the macro's own default rather than inventing one.
        assert!(!render_source(&spec(), &freertos()).contains("panic ="));
    }

    #[test]
    fn the_package_name_is_derived_from_the_image() {
        assert_eq!(package_name("native"), "native_entry");
        // An image id may carry characters a crate name cannot.
        assert_eq!(package_name("esp32-qemu"), "esp32_qemu_entry");
    }

    #[test]
    fn generation_refuses_to_touch_a_materialized_entry() {
        // The worst failure this phase could ship is silently regenerating over
        // someone's hand-written main.
        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path();
        let dir = parent.join("native_entry");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src/main.rs"), "// MINE\n").unwrap();
        super::super::materialize::Stamp::current("native", "linux", "posix", "hosted-main")
            .write(&dir)
            .unwrap();

        let got = write(&spec(), &hosted(), parent).expect("returns the dir");
        assert_eq!(got, dir);
        assert_eq!(
            std::fs::read_to_string(dir.join("src/main.rs")).unwrap(),
            "// MINE\n",
            "a materialised entry must survive byte-for-byte"
        );
    }

    #[test]
    fn writing_twice_does_not_touch_the_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = write(&spec(), &hosted(), tmp.path()).expect("first");
        let m1 = std::fs::metadata(dir.join("Cargo.toml"))
            .unwrap()
            .modified()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(&spec(), &hosted(), tmp.path()).expect("second");
        let m2 = std::fs::metadata(dir.join("Cargo.toml"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(
            m1, m2,
            "unchanged content must not rewrite (mtime treadmill)"
        );
    }

    #[test]
    fn the_generated_files_say_how_to_take_ownership() {
        let m = render_manifest(&spec(), &hosted(), Path::new(DIR)).expect("renders");
        assert!(m.contains("DO NOT EDIT"), "{m}");
        assert!(
            m.contains("nros materialize native"),
            "names the escape: {m}"
        );
        assert!(render_source(&spec(), &hosted()).contains("nros materialize native"));
    }
}
