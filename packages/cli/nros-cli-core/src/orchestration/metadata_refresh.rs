//! phase-307 W2 — the producer trigger.
//!
//! [`metadata_build`](super::metadata_build) can produce a component's
//! `source-metadata.json`, and W1 made every shipping Rust Node pkg a
//! candidate. Neither is worth anything to a bake while the only way to run it
//! is a user typing `nros metadata --build`: a bake cannot depend on an input
//! that may or may not exist and may or may not describe the current source.
//!
//! This module is the ordering guarantee. `nros ws sync` calls
//! [`refresh_stale_sidecars`] as its last step — after interface codegen and
//! after the `[patch.crates-io]` tables are written, because the harness
//! compiles the Node pkg for real and its generated interface deps resolve
//! only through those patches.
//!
//! **Staleness is content-addressed, not mtime-based.** Every sidecar carries a
//! [`SourceMetadataProvenance`] digest of the sources it was derived from; a
//! refresh recomputes that digest and rebuilds only on a mismatch. mtimes were
//! rejected deliberately: a `git pull`, rebase, or `git stash pop` rewrites
//! tracked files without changing their content, and an mtime-keyed cache
//! reads STALE for the entire tree afterwards — the "fixture mtime treadmill"
//! this repo already pays for elsewhere. A digest is immune, and it also lets
//! a consumer tell a fresh sidecar from museum data without re-running
//! anything.

use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use eyre::{Result, WrapErr};

use super::{
    metadata_build::{MetadataBuildOptions, build_metadata, host_triple, probe_blocker},
    source_metadata::{ComponentLanguage, SourceMetadata, SourceMetadataProvenance},
    workspace::{ComponentDeclaration, Workspace},
};

/// Directory names never hashed as component source: build output and the
/// sidecar dir itself (hashing the output would make the digest self-
/// referential).
const SKIPPED_DIRS: &[&str] = &["target", "build", "generated", "metadata", "node_modules"];

#[derive(Debug, Default, PartialEq, Eq)]
pub struct RefreshReport {
    /// Sidecars produced or updated.
    pub rebuilt: Vec<PathBuf>,
    /// Sidecars whose digest already matched the sources.
    pub fresh: Vec<PathBuf>,
    /// Components skipped because no producer exists for their language yet
    /// (C/C++ — phase-307 W3). Reported, never silently dropped.
    pub unsupported: Vec<String>,
}

impl RefreshReport {
    pub fn total(&self) -> usize {
        self.rebuilt.len() + self.fresh.len()
    }
}

/// Refresh every stale source-metadata sidecar in the workspace.
///
/// `nano_ros` is the nano-ros checkout the harness path-depends on for `nros`;
/// without it no harness can be compiled and the whole step is skipped (a
/// user's workspace built against an installed SDK has no such path).
pub fn refresh_stale_sidecars(
    ws_root: &Path,
    nano_ros: Option<&Path>,
    verbose: bool,
) -> Result<RefreshReport> {
    let mut report = RefreshReport::default();
    let workspace = Workspace::discover(ws_root)?;
    let declarations = workspace.component_declarations()?;
    if declarations.is_empty() {
        return Ok(report);
    }
    let probe_root = ws_root.join("build").join("nros-metadata");
    // Issue 0286 — resolved once, not per component.
    let host = host_triple();
    // phase-313 — C/C++ components to probe in ONE workspace-scoped project.
    let mut cpp_batch: Vec<crate::orchestration::metadata_probe_cmake::CmakeProbeOptions> =
        Vec::new();

    for decl in &declarations {
        if decl.deploy_bound {
            // phase-308 — a self-contained standalone example (issue 0100)
            // declares its node AND its entry in one crate, so the crate deps
            // a board crate and cannot compile for the host. Probing it fails
            // on the board's inline asm; reported, never silently skipped.
            report.unsupported.push(format!(
                "{}::{} (deploy-bound: node + entry in one crate)",
                decl.config.package, decl.config.component
            ));
            continue;
        }
        if let Some(blocker) = probe_blocker(&decl.package_root, Some(host.as_str())) {
            // Issue 0286 — the probe cannot produce a runnable binary for this
            // component. Degrade to the sidecar-less bake (SystemModel bound)
            // instead of failing the whole build, which is what used to take
            // the nuttx fixture lane — and therefore `just ci` — down.
            report.unsupported.push(format!(
                "{}::{} ({blocker})",
                decl.config.package, decl.config.component
            ));
            continue;
        }
        if decl.config.language != ComponentLanguage::Rust {
            // phase-313 — C/C++ probes are COLLECTED here and run as one
            // batch below, so the workspace builds the runtime (and the ~1.2 GB
            // of nested sizes-probe cargo builds) once rather than once per
            // component.
            match cpp_probe_options(decl, nano_ros, &probe_root) {
                Ok(Some(opts)) => cpp_batch.push(opts),
                // Already current — no probe needed.
                Ok(None) => report.fresh.push(decl.source_metadata_path()),
                Err(why) => report.unsupported.push(format!(
                    "{}::{} ({why})",
                    decl.config.package, decl.config.component
                )),
            }
            continue;
        }
        let sidecar = decl.source_metadata_path();
        let digest = source_digest(&decl.package_root)?;
        if sidecar_is_fresh(&sidecar, &digest) {
            report.fresh.push(sidecar);
            continue;
        }
        let Some(nano_ros) = nano_ros else {
            // No harness is buildable. Not an error — a sidecar-less bake
            // falls back to the SystemModel bound — but never silently
            // pretend the sidecar is current.
            report.unsupported.push(format!(
                "{}::{} (no nano-ros path)",
                decl.config.package, decl.config.component
            ));
            continue;
        };
        if verbose {
            println!(
                "ws sync: metadata {}::{} — sources changed, rebuilding",
                decl.config.package, decl.config.component
            );
        }
        build_metadata(&build_options(decl, nano_ros, &probe_root))
            .wrap_err_with(|| format!("refresh source metadata for `{}`", decl.config.package))?;
        stamp_provenance(&sidecar, &digest)?;
        report.rebuilt.push(sidecar);
    }

    // phase-313 — one project, one configure, one runtime build; the BUILD is
    // still per target so an uncompilable component costs only its own sidecar.
    if !cpp_batch.is_empty() {
        let dir = crate::orchestration::metadata_probe_cmake::probe_dir_for_workspace(&probe_root);
        if verbose {
            println!(
                "ws sync: metadata — probing {} C/C++ component(s) in one project",
                cpp_batch.len()
            );
        }
        match crate::orchestration::metadata_probe_cmake::run_probes(&dir, &cpp_batch) {
            Ok(outcomes) => {
                for (o, opts) in outcomes.iter().zip(cpp_batch.iter()) {
                    match &o.result {
                        Ok(()) => {
                            // Stamp only on success; a failed probe leaves the
                            // previous sidecar (or none) untouched.
                            if let Ok(digest) = source_digest(&opts.package_dir) {
                                let _ = stamp_provenance(&opts.output_path, &digest);
                            }
                            report.rebuilt.push(opts.output_path.clone());
                        }
                        Err(why) => report
                            .unsupported
                            .push(format!("{}::{} ({why:#})", o.package, o.component)),
                    }
                }
            }
            // A configure failure is the whole project, not one component.
            Err(why) => {
                for opts in &cpp_batch {
                    report.unsupported.push(format!(
                        "{}::{} (probe project configure failed: {why:#})",
                        opts.package, opts.component
                    ));
                }
            }
        }
    }
    Ok(report)
}

/// phase-313 — build the probe options for one C/C++ component, or explain why
/// it cannot be probed.
///
/// `Ok(None)` = the sidecar is already current. `Err` = this component only;
/// the caller records it in the unsupported ledger and the bake falls back to
/// the SystemModel bound, which is what these components get today.
fn cpp_probe_options(
    decl: &ComponentDeclaration,
    nano_ros: Option<&Path>,
    probe_root: &Path,
) -> Result<Option<crate::orchestration::metadata_probe_cmake::CmakeProbeOptions>, String> {
    let Some(nano_ros) = nano_ros else {
        return Err("no nano-ros path".into());
    };
    let Some(class) = decl.class.clone() else {
        return Err("no declared CLASS — nothing to construct".into());
    };
    let Some(header) = decl.probe_header() else {
        return Err("no declared HEADER and none derivable from CLASS".into());
    };
    let Some(library_target) = decl.library_target.clone() else {
        return Err("no CMake library target to link".into());
    };

    let sidecar = decl.source_metadata_path();
    let digest = source_digest(&decl.package_root).map_err(|e| e.to_string())?;
    if sidecar_is_fresh(&sidecar, &digest) {
        return Ok(None);
    }

    let language = match decl.config.language {
        ComponentLanguage::C => "c",
        _ => "cpp",
    };
    Ok(Some(
        crate::orchestration::metadata_probe_cmake::CmakeProbeOptions {
            package: decl.config.package.clone(),
            component: decl.config.component.clone(),
            executable: decl
                .config
                .linkage
                .resolved_executable(&decl.config.component),
            class,
            header,
            language: language.to_string(),
            shape: decl.probe_shape().to_string(),
            library_target,
            package_dir: decl.package_root.clone(),
            nano_ros_workspace: nano_ros.to_path_buf(),
            output_path: sidecar,
            probe_dir: probe_root.to_path_buf(),
        },
    ))
}

fn build_options(
    decl: &ComponentDeclaration,
    nano_ros: &Path,
    probe_root: &Path,
) -> MetadataBuildOptions {
    let id = decl.config.component.clone();
    let name = id.rsplit("::").next().unwrap_or(&id).to_string();
    MetadataBuildOptions {
        component_id: id.clone(),
        class: decl.class.clone(),
        crate_name: decl.crate_name.clone(),
        package: decl.config.package.clone(),
        executable: Some(decl.config.linkage.resolved_executable(&name)),
        exported_symbol: Some(decl.config.linkage.resolved_exported_symbol(&name)),
        component: name,
        component_dir: decl.package_root.clone(),
        nano_ros_workspace: nano_ros.to_path_buf(),
        output_path: decl.source_metadata_path(),
        harness_dir: probe_root
            .join("metadata-probe")
            .join(id.replace("::", "__")),
    }
}

/// A sidecar is fresh iff it parses AND its recorded provenance digest matches
/// the sources on disk. An unparseable or unstamped sidecar is stale by
/// definition — that is the "museum data" case, and rebuilding is the only
/// answer that cannot be wrong.
fn sidecar_is_fresh(sidecar: &Path, digest: &str) -> bool {
    let Ok(raw) = std::fs::read_to_string(sidecar) else {
        return false;
    };
    let Ok(parsed) = serde_json::from_str::<SourceMetadata>(&raw) else {
        return false;
    };
    parsed.provenance.is_some_and(|p| p.inputs_digest == digest)
}

/// Record the digest the sidecar was derived from. Done CLI-side rather than in
/// the harness so the stamp is one implementation for every producer language
/// (W3's C/C++ probe gets it for free), and so the round-trip through
/// [`SourceMetadata`] doubles as schema validation of what the harness emitted.
fn stamp_provenance(sidecar: &Path, digest: &str) -> Result<()> {
    let raw =
        std::fs::read_to_string(sidecar).wrap_err_with(|| format!("read {}", sidecar.display()))?;
    let mut parsed: SourceMetadata = serde_json::from_str(&raw).wrap_err_with(|| {
        format!(
            "metadata harness emitted invalid JSON at {}",
            sidecar.display()
        )
    })?;
    parsed.provenance = Some(SourceMetadataProvenance {
        inputs_digest: digest.to_string(),
        generator: format!("nros {}", env!("CARGO_PKG_VERSION")),
    });
    let out = serde_json::to_string_pretty(&parsed)?;
    std::fs::write(sidecar, out).wrap_err_with(|| format!("write {}", sidecar.display()))
}

/// Content digest of every source file under a package root.
///
/// FNV-1a over `(relative path, bytes)` for each file in sorted order, mixed
/// with the CLI version (a harness change must invalidate every sidecar). No
/// hash crate is pulled in for this: the digest guards a rebuild decision, not
/// a security boundary, and a collision costs one stale sidecar that the W5
/// coverage gate would catch.
pub fn source_digest(package_root: &Path) -> Result<String> {
    let mut files = Vec::new();
    collect_sources(package_root, package_root, &mut files)?;
    files.sort();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mix = |bytes: &[u8], hash: &mut u64| {
        for b in bytes {
            *hash ^= u64::from(*b);
            *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    mix(env!("CARGO_PKG_VERSION").as_bytes(), &mut hash);
    for rel in &files {
        mix(rel.to_string_lossy().as_bytes(), &mut hash);
        let bytes = std::fs::read(package_root.join(rel))
            .wrap_err_with(|| format!("read {}", package_root.join(rel).display()))?;
        mix(&bytes, &mut hash);
    }
    Ok(format!("fnv1a64:{hash:016x}"))
}

fn collect_sources(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if SKIPPED_DIRS.contains(&name.as_ref()) {
                continue;
            }
            collect_sources(root, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            out.push(rel.to_path_buf());
        }
    }
    Ok(())
}

/// Newest mtime under a package root — diagnostics only (`nros ws status`).
/// Never a freshness input; see the module docs on why mtimes were rejected.
pub fn newest_source_mtime(package_root: &Path) -> Option<SystemTime> {
    let mut files = Vec::new();
    collect_sources(package_root, package_root, &mut files).ok()?;
    files
        .iter()
        .filter_map(|rel| {
            std::fs::metadata(package_root.join(rel))
                .ok()?
                .modified()
                .ok()
        })
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nros-md-refresh-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        dir
    }

    #[test]
    fn digest_is_stable_and_content_sensitive() {
        let dir = tmp("digest");
        std::fs::write(dir.join("src/lib.rs"), "fn a() {}").unwrap();
        let first = source_digest(&dir).unwrap();
        assert_eq!(first, source_digest(&dir).unwrap(), "stable across reads");
        std::fs::write(dir.join("src/lib.rs"), "fn b() {}").unwrap();
        assert_ne!(
            first,
            source_digest(&dir).unwrap(),
            "content change moves it"
        );
    }

    /// The whole point of content-addressing: rewriting a file with identical
    /// bytes (what `git pull` / `git stash pop` do to a whole tree) must NOT
    /// invalidate the sidecar.
    #[test]
    fn digest_ignores_mtime_churn() {
        let dir = tmp("mtime");
        std::fs::write(dir.join("src/lib.rs"), "fn a() {}").unwrap();
        let first = source_digest(&dir).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "fn a() {}").unwrap();
        assert_eq!(first, source_digest(&dir).unwrap());
    }

    /// Build output must not feed the digest — otherwise every build
    /// invalidates the sidecar that the build just produced.
    #[test]
    fn digest_skips_build_output_and_the_sidecar_dir() {
        let dir = tmp("skip");
        std::fs::write(dir.join("src/lib.rs"), "fn a() {}").unwrap();
        let before = source_digest(&dir).unwrap();
        std::fs::create_dir_all(dir.join("target/debug")).unwrap();
        std::fs::write(dir.join("target/debug/blob"), "junk").unwrap();
        std::fs::create_dir_all(dir.join("metadata")).unwrap();
        std::fs::write(dir.join("metadata/talker.json"), "{}").unwrap();
        assert_eq!(before, source_digest(&dir).unwrap());
    }

    #[test]
    fn a_missing_or_unstamped_sidecar_is_stale() {
        let dir = tmp("fresh");
        let sidecar = dir.join("metadata.json");
        assert!(!sidecar_is_fresh(&sidecar, "fnv1a64:0"), "missing ⇒ stale");
        std::fs::write(&sidecar, "not json").unwrap();
        assert!(!sidecar_is_fresh(&sidecar, "fnv1a64:0"), "garbage ⇒ stale");
    }
}
