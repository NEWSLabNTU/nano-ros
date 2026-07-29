//! phase-315 W1 — the Rust selection facade.
//!
//! # Why this exists
//!
//! `system.toml` declares three axes for the whole system — ROS edition, RMW,
//! capability list — and C/C++ derive all three: the user writes
//! `find_package(nano_ros)` + `nano_ros_add_executable(...)` and names no
//! feature anywhere. Rust restated every one of them by hand, in the entry's
//! `Cargo.toml`, where the copies were free to disagree with the declaration.
//!
//! The mismatches are not equally loud. A transport-tier or capability
//! disagreement is a build error. An **edition** disagreement is not: codegen
//! bakes jazzy `type_hash`es while the runtime speaks humble, so the image
//! links, boots, and simply fails to interoperate (RFC-0056). That is the
//! failure this module removes.
//!
//! # Why a generated crate rather than a rewrite
//!
//! Cargo resolves features from **manifests**, before any nano-ros code runs.
//! A proc-macro can observe a mismatch — phase-314 made `nros::main!` assert on
//! one — but it cannot repair it, and `.cargo/config.toml` has no way to
//! express features. So the derivation has to reach cargo as a manifest.
//!
//! It could have reached cargo by rewriting the user's `Cargo.toml`.
//! Deliberately rejected: `nros sync` does not edit user-authored files, and a
//! sync that silently rewrites a hand-maintained manifest is the kind of thing
//! that survives right up until it clobbers an edit someone cared about.
//! Generated selection belongs in generated code.
//!
//! So: a tiny generated crate that depends on `nros` and the board crate with
//! the derived features. The entry depends on the facade; cargo's feature
//! unification carries the selection to the same `nros` / board packages the
//! entry already names. The facade has no code — the dependency edge IS the
//! payload.
//!
//! This is not a new mechanism. It is the shape the C++ path has used since
//! `nros_synth_runtime_umbrella` (`cmake/NanoRosRuntimeCrate.cmake`), which
//! generates `nros_ws_runtime` with `nros-cpp = { features = [...] }` for
//! exactly this reason, and is precisely why a C++ consumer names no features.
//! W1 gives Rust the twin it never had.

use eyre::{Result, WrapErr};
use std::path::{Path, PathBuf};

use crate::orchestration::cargo_metadata_schema::SystemToml;

/// A generated facade, as written to disk.
#[derive(Debug, Clone)]
pub struct Facade {
    /// Entry package the facade selects for.
    pub entry: String,
    /// Directory holding the generated `Cargo.toml` + `src/lib.rs`.
    pub dir: PathBuf,
    /// Cargo package name — what the entry depends on.
    pub crate_name: String,
    /// The derived `nros` feature list, for logging and for W5's gate.
    pub nros_features: Vec<String>,
    /// The derived board-crate feature list.
    pub board_features: Vec<String>,
    /// The derived feature list for a direct `nros-rmw-*` backend dep, if any.
    pub backend_features: Vec<String>,
    /// True iff the on-disk manifest actually changed.
    pub changed: bool,
}

/// The runtime deps a facade must re-declare in order to attach features to
/// them. Discovered from the ENTRY's own manifest rather than from a
/// deploy→board table, because no such table exists in the CLI: the board dep
/// IS the deploy declaration on the Rust side (`nros-board-native` supplies the
/// `NativeBoard` ZST that `nros::main!` resolves `deploy = "native"` onto).
/// Inventing a second mapping here would be a fourth copy of a selection that
/// phase-314 spent its whole length collapsing to one.
#[derive(Debug, Default)]
struct RuntimeDeps {
    /// Absolute path to the `nros` umbrella crate, from the entry's path dep.
    nros: Option<PathBuf>,
    /// Absolute path + package name of the `nros-board-*` crate.
    board: Option<(String, PathBuf)>,
    /// Absolute path + package name of a DIRECT `nros-rmw-*` backend dep.
    ///
    /// Some entries (the Zephyr ones) depend on the backend crate directly
    /// rather than only through the board. That dep needs the edition too, and
    /// this is not bookkeeping: the backend's `keyexpr` module cfg-gates the
    /// RIHS01 type-hash tail on `ros-iron`/`ros-jazzy` itself. Miss the forward
    /// and a jazzy build keeps the humble `TypeHashNotSupported` placeholder on
    /// the wire while everything compiles — the precise failure mode this phase
    /// exists to remove, reintroduced one dep to the left.
    rmw_backend: Option<(String, PathBuf)>,
}

/// Derive the selection for one entry and write its facade crate.
///
/// Returns `Ok(None)` when the package is not an entry, or when the entry
/// declares no path deps to attach features to (a registry-style manifest —
/// `nros sync`'s patch table handles those, and the facade would have nothing
/// to point at).
pub fn write_facade(
    entry_name: &str,
    entry_dir: &Path,
    entry_manifest: &Path,
    sys: &SystemToml,
    facade_root: &Path,
) -> Result<Option<Facade>> {
    let raw = std::fs::read_to_string(entry_manifest)
        .wrap_err_with(|| format!("facade: read {}", entry_manifest.display()))?;
    let manifest: toml::Value = toml::from_str(&raw)
        .wrap_err_with(|| format!("facade: parse {}", entry_manifest.display()))?;

    // Not an entry ⇒ nothing to select for. Node packages stay silent about all
    // three axes (phase-314 W3) and get their selection through unification.
    if manifest
        .get("package")
        .and_then(|p| p.get("metadata"))
        .and_then(|m| m.get("nros"))
        .and_then(|n| n.get("entry"))
        .is_none()
    {
        return Ok(None);
    }

    let deps = runtime_deps(&manifest, entry_dir);
    let Some(nros_path) = deps.nros.clone() else {
        return Ok(None);
    };

    // ---- the three declared axes ------------------------------------------
    let edition = sys.system.ros_edition()?;
    let rmw = crate::orchestration::rmw_resolver::resolve_rmw(&sys.system.rmw)
        .map_err(|e| eyre::eyre!("facade: {entry_name}: {e}"))?;

    // `nros` carries the edition and the capabilities; the BOARD crate carries
    // the RMW (phase-248 C5b — its `rmw-X` feature self-links and registers the
    // backend, and brings the concrete platform impl). Splitting them is not a
    // style choice: putting `rmw-zenoh` on `nros` selects nothing.
    let mut nros_features = vec![edition.cargo_feature().to_string()];
    for cap in cargo_nano_ros::capability_resolver::CAPABILITIES {
        if !sys.capability_enabled(cap.declared) {
            continue;
        }
        // `safety` is zenoh-only — the CRC path lives in that backend. Mirrors
        // the same guard in `nros_feature_set` (cmake), which warns rather than
        // silently dropping it.
        if !cap.backends_supporting.is_empty() && !cap.backend_supports(rmw.declared) {
            eprintln!(
                "ws sync: facade `{entry_name}`: capability `{}` declared but the \
                 `{}` RMW does not carry it — omitted.",
                cap.declared, rmw.declared,
            );
            continue;
        }
        nros_features.push(cap.nros_feature.to_string());
    }
    nros_features.sort();
    nros_features.dedup();

    // Only emit a feature the board crate actually DECLARES.
    //
    // W1 assumed "the board crate carries the RMW". That holds for 18 of the 23
    // board crates and is false for five — `nros-board-zephyr` declares only
    // `tiers` and `zephyr-edf`, because on Zephyr the RMW rides on the entry's
    // own `[features] rmw-zenoh` plus a direct `nros-rmw-zenoh` dep instead.
    // Emitting it unconditionally is not a silent mistake, it is a hard cargo
    // error that killed the whole zephyr fixture lane:
    //
    //     package `zephyr_entry_nros_selection` depends on `nros-board-zephyr`
    //     with feature `rmw-zenoh` but `nros-board-zephyr` does not have that
    //     feature. available features: tiers, zephyr-edf
    //
    // When the board has no such feature the facade stays silent about the RMW
    // for that dep — the entry's own selector is then the only one, which is
    // correct rather than a fallback.
    let board_features: Vec<String> = match deps.board.as_ref() {
        Some((_, path)) if crate_declares_feature(path, rmw.cargo_feature) => {
            vec![rmw.cargo_feature.to_string()]
        }
        _ => Vec::new(),
    };

    // A direct backend dep gets the edition (and any capability the backend
    // itself implements, e.g. safety-e2e's CRC path), NOT the `rmw-*` selector
    // — naming the backend crate already selects it.
    let mut backend_features = vec![edition.cargo_feature().to_string()];
    for cap in cargo_nano_ros::capability_resolver::CAPABILITIES {
        if !sys.capability_enabled(cap.declared) || !cap.backend_supports(rmw.declared) {
            continue;
        }
        if let Some(bf) = cap.backend_feature {
            backend_features.push(bf.to_string());
        }
    }
    backend_features.sort();
    backend_features.dedup();

    // ---- render ------------------------------------------------------------
    let crate_name = format!("{entry_name}_nros_selection");
    let dir = facade_root.join(entry_name);
    let body = render_manifest(
        &crate_name,
        &dir,
        &nros_path,
        &nros_features,
        deps.board.as_ref(),
        &board_features,
        deps.rmw_backend.as_ref(),
        &backend_features,
        entry_name,
        sys,
    );

    std::fs::create_dir_all(dir.join("src"))
        .wrap_err_with(|| format!("facade: create {}", dir.display()))?;
    let changed = write_if_changed(&dir.join("Cargo.toml"), &body)?;
    // An empty lib. The dependency edge is the entire payload; there is
    // deliberately no code here for anyone to start adding to.
    write_if_changed(
        &dir.join("src/lib.rs"),
        "//! Generated by `nros sync` (phase-315). Do not edit.\n\
         //!\n\
         //! This crate is intentionally empty. Its `Cargo.toml` carries the\n\
         //! cargo features derived from the bringup's `system.toml`; cargo's\n\
         //! feature unification is what delivers them.\n\
         #![no_std]\n",
    )?;

    Ok(Some(Facade {
        entry: entry_name.to_string(),
        dir,
        crate_name,
        nros_features,
        board_features,
        backend_features: if deps.rmw_backend.is_some() {
            backend_features
        } else {
            Vec::new()
        },
        changed,
    }))
}

/// Does the crate at `dir` declare `feature` in its `[features]` table?
///
/// Read from the manifest rather than assumed from the crate's name: the board
/// crates genuinely disagree about whether they own the RMW axis, and guessing
/// from a naming convention is what produced the zephyr breakage.
fn crate_declares_feature(dir: &Path, feature: &str) -> bool {
    let Ok(raw) = std::fs::read_to_string(dir.join("Cargo.toml")) else {
        // Unreadable manifest: emit nothing rather than emit something cargo
        // will reject. A missing feature is a build the user can still fix; a
        // bogus one fails resolution outright.
        return false;
    };
    toml::from_str::<toml::Value>(&raw)
        .ok()
        .and_then(|v| v.get("features").and_then(|f| f.as_table()).cloned())
        .is_some_and(|t| t.contains_key(feature))
}

/// Pull the `nros` and `nros-board-*` PATH deps out of the entry's manifest.
fn runtime_deps(manifest: &toml::Value, entry_dir: &Path) -> RuntimeDeps {
    let mut out = RuntimeDeps::default();
    let Some(table) = manifest.get("dependencies").and_then(|d| d.as_table()) else {
        return out;
    };
    for (name, spec) in table {
        let Some(rel) = spec.get("path").and_then(|p| p.as_str()) else {
            continue;
        };
        // Keep it lexical — `canonicalize` would resolve symlinked checkouts to
        // a path outside the workspace, and the manifest needs a path cargo can
        // read back relative to the facade.
        let abs = lexical_normalize(&entry_dir.join(rel));
        if name == "nros" {
            out.nros = Some(abs);
        } else if name.starts_with("nros-board-") {
            out.board = Some((name.clone(), abs));
        } else if name.starts_with("nros-rmw-") {
            out.rmw_backend = Some((name.clone(), abs));
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn render_manifest(
    crate_name: &str,
    facade_dir: &Path,
    nros_path: &Path,
    nros_features: &[String],
    board: Option<&(String, PathBuf)>,
    board_features: &[String],
    backend: Option<&(String, PathBuf)>,
    backend_features: &[String],
    entry_name: &str,
    sys: &SystemToml,
) -> String {
    let feat_list = |f: &[String]| {
        f.iter()
            .map(|s| format!("\"{s}\""))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut s = String::new();
    s.push_str(&format!(
        "# GENERATED by `nros sync` — do not edit, do not commit.\n\
         #\n\
         # Selection facade for entry `{entry_name}`, derived from the bringup\n\
         # `system.toml` of system `{}`:\n\
         #\n\
         #     ros_edition = {:?}\n\
         #     rmw         = {:?}\n\
         #     features    = {:?}\n\
         #\n\
         # The entry depends on this crate and names no features itself. Cargo\n\
         # unifies the features below onto the same `nros` / board packages the\n\
         # entry already depends on. Editing this file is pointless — the next\n\
         # `nros sync` overwrites it; edit `system.toml` instead.\n\n",
        sys.system.name,
        sys.system
            .ros_edition
            .clone()
            .unwrap_or_else(|| "humble (default)".into()),
        sys.system.rmw,
        sys.system.features,
    ));
    s.push_str(&format!(
        "[package]\n\
         name = \"{crate_name}\"\n\
         version = \"0.0.0\"\n\
         edition = \"2024\"\n\
         publish = false\n\n\
         [lib]\n\
         path = \"src/lib.rs\"\n\n\
         [dependencies]\n"
    ));
    s.push_str(&format!(
        "nros = {{ path = {:?}, default-features = false, features = [{}] }}\n",
        rel_from(facade_dir, nros_path),
        feat_list(nros_features),
    ));
    if let Some((name, path)) = board {
        // No features to contribute ⇒ omit the dep entirely. A bare path dep
        // would add an edge that changes nothing, and the facade exists only to
        // carry selection.
        if !board_features.is_empty() {
            s.push_str(&format!(
                "{name} = {{ path = {:?}, features = [{}] }}\n",
                rel_from(facade_dir, path),
                feat_list(board_features),
            ));
        }
    }
    if let Some((name, path)) = backend {
        s.push_str(&format!(
            "{name} = {{ path = {:?}, default-features = false, features = [{}] }}\n",
            rel_from(facade_dir, path),
            feat_list(backend_features),
        ));
    }
    s
}

/// Collapse `.` and `..` without touching the filesystem.
///
/// Needed because the entry's path deps are written relative to the ENTRY
/// (`../../../../../packages/core/nros`), and joining that onto the entry dir
/// leaves the `..`s embedded. Cargo reads such a path fine, but `rel_from`
/// below compares components to find the common prefix, and an un-collapsed
/// `..` makes every path look divergent — producing a correct-but-absurd
/// `../../../src/native_entry/../../../../../packages/...`.
fn lexical_normalize(p: &Path) -> PathBuf {
    let mut out: Vec<std::path::Component<'_>> = Vec::new();
    for c in p.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                // Only pop a real name; `/..` and a leading `..` stay put.
                if matches!(out.last(), Some(std::path::Component::Normal(_))) {
                    out.pop();
                } else {
                    out.push(c);
                }
            }
            other => out.push(other),
        }
    }
    out.iter().collect()
}

/// Path of `target` as seen from `base`, lexically (no filesystem access, so a
/// not-yet-created facade dir works).
fn rel_from(base: &Path, target: &Path) -> String {
    let b: Vec<_> = base.components().collect();
    let t: Vec<_> = target.components().collect();
    let common = b.iter().zip(&t).take_while(|(x, y)| x == y).count();
    let mut out: Vec<String> = std::iter::repeat_n("..".to_string(), b.len() - common).collect();
    out.extend(
        t[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().to_string()),
    );
    if out.is_empty() {
        ".".into()
    } else {
        out.join("/")
    }
}

/// Write only on change.
///
/// Not an optimisation. A churned mtime on a generated `Cargo.toml` re-triggers
/// every downstream cargo build, and this repo's whole fixture story is built
/// on mtimes — a generator that rewrites identical bytes every sync would stale
/// every fixture on every sync. Atomic (tmp + rename) so a killed sync cannot
/// leave a half-written manifest that cargo then parses.
fn write_if_changed(dst: &Path, body: &str) -> Result<bool> {
    if std::fs::read_to_string(dst).ok().as_deref() == Some(body) {
        return Ok(false);
    }
    let tmp = dst.with_extension(format!("nros-sync-tmp.{}", std::process::id()));
    std::fs::write(&tmp, body).wrap_err_with(|| format!("facade: write {}", tmp.display()))?;
    std::fs::rename(&tmp, dst)
        .wrap_err_with(|| format!("facade: rename into {}", dst.display()))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rel_from_walks_up_then_down() {
        assert_eq!(
            rel_from(
                Path::new("/ws/build/nros-sync/facade/native_entry"),
                Path::new("/repo/packages/core/nros")
            ),
            "../../../../../repo/packages/core/nros"
        );
        assert_eq!(rel_from(Path::new("/a/b"), Path::new("/a/b/c")), "c");
    }

    #[test]
    fn normalize_collapses_the_entry_relative_hops() {
        // The exact shape a real entry produces: its `nros` dep is written
        // relative to itself, and joining leaves the hops embedded.
        assert_eq!(
            lexical_normalize(Path::new(
                "/ws/src/native_entry/../../../packages/core/nros"
            )),
            Path::new("/packages/core/nros")
        );
        assert_eq!(
            lexical_normalize(Path::new("/a/./b/../c")),
            Path::new("/a/c")
        );
    }
}
