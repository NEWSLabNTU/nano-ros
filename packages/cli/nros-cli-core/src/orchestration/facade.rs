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
/// IS the deploy declaration on the Rust side (`nros-board-linux` supplies the
/// `LinuxBoard` ZST that `nros::main!` resolves `deploy = "native"` onto).
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

/// The RMW the IMAGE that names `entry_name` declares, if there is one.
///
/// Issue 0831. `[image.<id>].rmw` used to reach exactly one thing on the cargo
/// driver — `coordinate()`, which names the build DIRECTORY — while the backend
/// came from here, off `[system] rmw`. So `[image.native_cyclonedds]` produced
/// `build/posix-cyclonedds/…` containing a zenoh binary: measured 0 occurrences
/// of "cyclone" and 1916 of "zenoh" in the artifact. A directory named for a
/// backend it does not contain reads as coverage, and two of tier 2's fourteen
/// coordinates were exactly that.
///
/// The facade is what selects the backend (through the board crate's `rmw-*`
/// feature), and it is already keyed per ENTRY. An image names an entry —
/// `package_name(image_id)` — so per-entry IS per-image, and the fix is to read
/// the RMW from the image rather than from the system header.
///
/// Matched FORWARD, `package_name(id) == entry_name`, never by stripping
/// `_entry` off the name: that function replaces `-`, `.` and `/` with `_`, so
/// the reverse is ambiguous and would silently pick the wrong image for an id
/// containing any of them.
///
/// `[image_defaults]` is folded in first, so an image inherits the workspace's
/// RMW exactly as the builder resolves it. `None` means no image names this
/// entry — a hand-written entry, or an unmigrated workspace — and the caller
/// falls back to `[system] rmw`, which is what those have always used.
fn image_rmw(entry_name: &str, sys: &SystemToml) -> Option<String> {
    let base = sys.image_defaults.clone().unwrap_or_default();
    sys.image
        .iter()
        .find(|(id, _)| crate::builder::entry::package_name(id) == entry_name)
        .and_then(|(_, img)| img.with_base(&base).rmw)
}

/// Every RMW an image's binary must LINK, not just the one it defaults to.
///
/// The image's own `rmw` plus one per declared `[[domain]]`. A bridge is the
/// case: `examples/workspaces/bridge-cyclonedds` declares
///
/// ```toml
/// [[domain]] name = "zen" rmw = "zenoh"
/// [[domain]] name = "dds" rmw = "cyclonedds"
/// [[bridge]] name = "gw" from = "zenoh:zen" to = "cyclonedds:dds"
/// ```
///
/// so its one binary needs BOTH backends compiled in — it selects per domain at
/// run time rather than having a single default. The hand-written entry listed
/// the extra backend crates by hand, which is exactly the authored knowledge
/// RFC-0065 D4 says should be derived: the bringup already declares it.
///
/// No new syntax and no new registry field, because the board crate already
/// carries one `rmw-*` feature per backend and enabling several is additive —
/// `rmw-cyclonedds` pulls `nros-rmw-cyclonedds-sys`, which depends on
/// `nros-rmw-cyclonedds`; `rmw-xrce` pulls `nros-rmw-xrce-cffi`. Those are the
/// same crates the two hand-written bridge entries name.
///
/// Ordered and deduped so the emitted manifest is stable across machines.
fn image_backends(entry_name: &str, sys: &SystemToml) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(r) = image_rmw(entry_name, sys) {
        out.push(r);
    }
    for d in &sys.domains {
        out.push(d.rmw.clone());
    }
    out.sort();
    out.dedup();
    out
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
    let declared_rmw = image_rmw(entry_name, sys).unwrap_or_else(|| sys.system.rmw.clone());
    let rmw = crate::orchestration::rmw_resolver::resolve_rmw(&declared_rmw)
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
        // phase-347 W4 — "does this axis have a backend half at all" is now
        // `backend_feature.is_some()`; which backends carry it comes from their
        // descriptors, not a list in the registry.
        if cap.backend_feature.is_some() && !cap.backend_supports(rmw.declared) {
            eprintln!(
                "sync: facade `{entry_name}`: capability `{}` declared but the \
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
    //
    // Issue 0831 — and the reason the dep is rendered `default-features = false`
    // below. Naming `rmw-cyclonedds` while the board's `default = ["rmw-zenoh"]`
    // still applies gets BOTH: cargo unions features, it cannot subtract a
    // default (issue 0270). The image then carries two backends and the runtime
    // refuses to pick — `more than one RMW backend is registered and no
    // $NROS_RMW selector was set` — which is honest but is not a working image.
    //
    // So the defaults are carved out and re-supplied MINUS any `rmw-*`: the
    // board keeps its `ethernet` / `image-runtime`, and the RMW is named once,
    // by the facade.
    let board_features: Vec<String> = match deps.board.as_ref() {
        Some((_, path)) => {
            let mut f: Vec<String> = crate_default_features(path)
                .into_iter()
                .filter(|d| !d.starts_with("rmw-"))
                .collect();
            // One `rmw-*` per backend this image must LINK, not only its
            // default — see `image_backends`. A bridge needs two; every other
            // image resolves to exactly the one it already had.
            let mut wanted = vec![rmw.cargo_feature.to_string()];
            for extra in image_backends(entry_name, sys) {
                match crate::orchestration::rmw_resolver::resolve_rmw(&extra) {
                    Ok(r) => wanted.push(r.cargo_feature.to_string()),
                    // A domain naming an unknown RMW is the SYSTEM's error and
                    // is reported where the system is resolved; silently
                    // dropping it here would emit a facade missing a backend
                    // and fail at link with no mention of the declaration.
                    Err(e) => {
                        return Err(eyre::eyre!("facade: {entry_name}: {e}"));
                    }
                }
            }
            for cf in wanted {
                if crate_declares_feature(path, &cf) {
                    f.push(cf);
                }
            }
            f.sort();
            f.dedup();
            f
        }
        None => Vec::new(),
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
/// The crate's own `[features] default` list.
///
/// Sibling of [`crate_declares_feature`], and read for the same reason: the
/// board crate is the authority on what it declares, not a table here.
///
/// Used to CARVE OUT the board's default RMW without losing the rest of its
/// defaults (issue 0831, the shape issue 0270 recorded). Boards do not put the
/// same things there — `nros-board-linux` defaults to `["rmw-zenoh"]`, but
/// `nros-board-esp32-qemu` and `nros-board-mps2-an385` default to
/// `["ethernet", "rmw-zenoh"]` and the NuttX boards to `["image-runtime"]`,
/// which carries two lang items. A blanket `default-features = false` would
/// silently drop `ethernet` and the panic handler along with the backend.
fn crate_default_features(dir: &Path) -> Vec<String> {
    let Ok(raw) = std::fs::read_to_string(dir.join("Cargo.toml")) else {
        return Vec::new();
    };
    toml::from_str::<toml::Value>(&raw)
        .ok()
        .and_then(|v| {
            v.get("features")?.get("default")?.as_array().map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(ToString::to_string))
                    .collect()
            })
        })
        .unwrap_or_default()
}

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
                "{name} = {{ path = {:?}, default-features = false, features = [{}] }}\n",
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
/// (`../../../../../packages/api/nros`), and joining that onto the entry dir
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
    // issue 0562 — one spelling. This was one of four private copies of the
    // same skip-if-identical logic; the shared helper now owns it, and the
    // sites that never had a copy (the probe-cmake writers, providers.json)
    // get the behaviour for free.
    crate::atomic_file::atomic_write_reporting(dst, body.as_bytes())
        .wrap_err_with(|| format!("facade: write {}", dst.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rel_from_walks_up_then_down() {
        assert_eq!(
            rel_from(
                Path::new("/ws/build/nros-sync/facade/native_entry"),
                Path::new("/repo/packages/api/nros")
            ),
            "../../../../../repo/packages/api/nros"
        );
        assert_eq!(rel_from(Path::new("/a/b"), Path::new("/a/b/c")), "c");
    }

    #[test]
    fn normalize_collapses_the_entry_relative_hops() {
        // The exact shape a real entry produces: its `nros` dep is written
        // relative to itself, and joining leaves the hops embedded.
        assert_eq!(
            lexical_normalize(Path::new("/ws/src/native_entry/../../../packages/api/nros")),
            Path::new("/packages/api/nros")
        );
        assert_eq!(
            lexical_normalize(Path::new("/a/./b/../c")),
            Path::new("/a/c")
        );
    }
}
