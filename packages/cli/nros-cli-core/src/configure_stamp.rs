//! phase-530 W1 (RFC-0120 D2) -- the configure stamp.
//!
//! An unchanged `nros configure` is a no-op because it can PROVE nothing it
//! reads has moved. The stamp records two things: a digest of every input a
//! configure reads, and the environment it was configured with. [`is_current`]
//! answers "may I skip", and every `Stale` carries the one line saying why.
//!
//! Content-addressed throughout (never mtimes: a rebase touches every file).
use std::path::{Path, PathBuf};

use eyre::{Result, WrapErr};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::{
    atomic_file::atomic_write,
    cmd::entity_census::{CENSUS_DIR, Freshness, census_freshness},
    orchestration::metadata_refresh::stale_sidecar_components,
};

/// Path components a walk never enters: build output, generated trees,
/// cargo's target dir and git's own.
const SKIPPED_DIRS: &[&str] = &["build", "generated", "target", ".git"];
/// File names whose bytes are an input wherever they sit.
const INPUT_NAMES: &[&str] = &[
    "package.xml",
    "system.toml",
    "system.contract.yaml",
    "CMakeLists.txt",
    "Cargo.toml",
];
/// Interface definition extensions.
const INTERFACE_EXTS: &[&str] = &["msg", "srv", "action"];
/// Launch-file extensions, counted only under a `launch/` directory.
const LAUNCH_EXTS: &[&str] = &["xml", "py", "yaml", "yml"];
/// Environment variables whose VALUES are digest inputs. `NROS_MODEL_DIR`
/// moves where C1 writes the models (and where a build reads them).
const ENV_INPUTS: &[&str] = &[
    "AMENT_PREFIX_PATH",
    "NROS_INTERFACE_SEARCH_PATH",
    "NROS_MODEL_DIR",
];

/// The configure flags that change what configure WRITES, as digest inputs
/// (phase-530 W1). A configure with other output flags than the stamp's is
/// not the configure the stamp recorded. `--force`, `--check`, `--dry-run`,
/// `--verbose`, `--offline` and `--image` are not outputs: `--image` is the
/// stamp KEY, and the rest change whether or how loudly configure runs, not
/// what it writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OutputFlags {
    /// `(flag, value)`, every flag present, its default spelled as the parser
    /// would build it, so a default flag and an absent one digest alike.
    pairs: Vec<(&'static str, String)>,
    /// `--model-dir`: where C1 writes the models.
    pub(crate) model_dir: Option<PathBuf>,
}

impl OutputFlags {
    pub(crate) fn of(sync: &crate::cmd::ws::SyncArgs) -> Self {
        let path = |p: &Option<PathBuf>| {
            p.as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        };
        let base_paths: Vec<String> = sync
            .base_paths
            .iter()
            .map(|p| p.display().to_string())
            .collect();
        OutputFlags {
            pairs: vec![
                (
                    "--ros-edition",
                    sync.ros_edition.clone().unwrap_or_default(),
                ),
                ("--generated-base", sync.build_dir.display().to_string()),
                ("--model-dir", path(&sync.model_dir)),
                ("--no-metadata", sync.no_metadata.to_string()),
                ("--base-paths", base_paths.join("\0")),
                ("--nano-ros-path", path(&sync.nano_ros_path)),
                ("--nano-ros-root", path(&sync.nano_ros_root)),
            ],
            model_dir: sync.model_dir.clone(),
        }
    }

    /// Every output flag at its default: a configure with no flags.
    #[cfg(test)]
    pub(crate) fn defaults() -> Self {
        Self::of(&crate::cmd::configure::default_sync_args())
    }
}

/// What a configure recorded about itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Stamp {
    pub(crate) digest: String,
    pub(crate) codegen_version: String,
    pub(crate) ament_prefix: Option<String>,
    pub(crate) launch_resolve: Option<PathBuf>,
    /// Ruling R15: a SKIP stamp — the image's preflight refused it (this
    /// reason) when nobody named it. Current for a configure that does not
    /// name it either, so a host lacking one image's toolchain does not
    /// reconfigure on every build; stale for one that names it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) skipped: Option<String>,
    /// phase-530 W4 (RFC-0120 D2) — where this configure resolved the image's
    /// answer (`resolved.toml`, the seeds and their input records). A host
    /// configure finds it by the image's identity (`nros locate configured`),
    /// never by a path passed on its command line (the retired
    /// `NROS_RESOLVED_DIR`). `None` when the image resolved nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) resolved_dir: Option<PathBuf>,
}

/// Whether a recorded stamp still describes the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Currency {
    Current,
    /// One line: why the configure must run.
    Stale(String),
}

/// `project/build/nros/configured/<image>`; a `:` in a qualified id is `__`.
pub(crate) fn stamp_dir(project: &Path, image: &str) -> PathBuf {
    project
        .join("build")
        .join("nros")
        .join("configured")
        .join(image.replace(':', "__"))
}

/// The one lock every configure of this project takes.
/// Ruling R15: the stamp key of a project that declares no image (a
/// bringup-less workspace builds package by package). Not a legal image id,
/// so it cannot collide with one.
pub(crate) const PROJECT_KEY: &str = ".project";

pub(crate) fn lock_path(project: &Path) -> PathBuf {
    project
        .join("build")
        .join("nros")
        .join("configured")
        .join(".lock")
}

/// Ruling R9 (phase-530 W1): which image last wrote a leaf's SHARED output.
/// A leaf's images share one selection package and one `generated/` tree
/// (issue 1305), so an image's own stamp cannot say whether that output is
/// still its own.
fn leaf_selection_file(project: &Path) -> PathBuf {
    project
        .join("build")
        .join("nros")
        .join("configured")
        .join(".leaf-selection")
}

/// Record that `image` is about to (re)write the leaf's shared output.
/// Written BEFORE the writing starts, so a configure that fails half way
/// leaves every other image stale rather than one wrongly current.
pub(crate) fn record_leaf_selection(project: &Path, image: &str) -> Result<()> {
    let path = leaf_selection_file(project);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .wrap_err_with(|| format!("cannot create `{}`", dir.display()))?;
    }
    atomic_write(&path, &format!("{image}\n"))
}

fn leaf_selection(project: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(leaf_selection_file(project)).ok()?;
    Some(raw.trim().to_string()).filter(|s| !s.is_empty())
}

fn stamp_file(project: &Path, image: &str) -> PathBuf {
    stamp_dir(project, image).join("stamp.toml")
}

fn is_input(rel: &Path) -> bool {
    let Some(name) = rel.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if INPUT_NAMES.contains(&name) {
        return true;
    }
    let ext = rel.extension().and_then(|e| e.to_str()).unwrap_or("");
    if INTERFACE_EXTS.contains(&ext) {
        return true;
    }
    LAUNCH_EXTS.contains(&ext)
        && rel
            .parent()
            .into_iter()
            .flat_map(Path::components)
            .any(|c| c.as_os_str() == "launch")
}

/// Sorted repo-relative paths of every input file under `project`.
fn input_files(project: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let walk = WalkDir::new(project).follow_links(false).into_iter();
    let walk = walk.filter_entry(|e| {
        if e.depth() == 0 || !e.file_type().is_dir() {
            return true;
        }
        let skipped = e
            .file_name()
            .to_str()
            .is_some_and(|n| SKIPPED_DIRS.contains(&n));
        // A nested repository (worktree or submodule) is somebody else's
        // tree: `repo_walk.py`'s rule, stop at its `.git`.
        let nested_repo = e.path().join(".git").exists();
        !skipped && !nested_repo
    });
    for entry in walk.flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        if let Ok(rel) = entry.path().strip_prefix(project)
            && is_input(rel)
        {
            out.push(rel.to_path_buf());
        }
    }
    out.sort();
    out
}

fn codegen_version() -> String {
    crate::abi_guard::EMITTED_VERSION.to_string()
}

/// `sha256:<hex>` over the project's inputs, the digest-input env, the output
/// flags at their defaults, and the codegen version.
#[cfg(test)]
pub(crate) fn input_digest(project: &Path) -> Result<String> {
    input_digest_with_env(project, &real_env())
}

/// The digest-input variables as the process sees them.
fn real_env() -> Vec<(String, Option<String>)> {
    ENV_INPUTS
        .iter()
        .map(|v| ((*v).to_string(), std::env::var(v).ok()))
        .collect()
}

/// [`input_digest`] with the environment values injected, so a test needs no
/// process-env mutation.
#[cfg(test)]
pub(crate) fn input_digest_with_env(
    project: &Path,
    env: &[(String, Option<String>)],
) -> Result<String> {
    input_digest_with(project, env, &OutputFlags::defaults())
}

/// [`input_digest_with_env`] with the output flags injected too.
pub(crate) fn input_digest_with(
    project: &Path,
    env: &[(String, Option<String>)],
    flags: &OutputFlags,
) -> Result<String> {
    let mut h = Sha256::new();
    let mut feed = |tag: &str, bytes: &[u8]| {
        h.update(tag.as_bytes());
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(bytes);
    };
    for rel in input_files(project) {
        let abs = project.join(&rel);
        let bytes =
            std::fs::read(&abs).wrap_err_with(|| format!("cannot read `{}`", abs.display()))?;
        feed("file", rel.to_string_lossy().as_bytes());
        feed("body", &bytes);
    }
    for var in ENV_INPUTS {
        let val = env
            .iter()
            .find(|(n, _)| n == var)
            .and_then(|(_, v)| v.clone())
            .unwrap_or_default();
        feed(var, val.as_bytes());
    }
    for (flag, val) in &flags.pairs {
        feed(flag, val.as_bytes());
    }
    feed("codegen", codegen_version().as_bytes());
    Ok(format!("sha256:{:x}", h.finalize()))
}

/// The recorded stamp, or `None` if there is none or it cannot be read.
pub(crate) fn read(project: &Path, image: &str) -> Option<Stamp> {
    let raw = std::fs::read_to_string(stamp_file(project, image)).ok()?;
    toml::from_str(&raw).ok()
}

/// phase-530 W4 (RFC-0120 D2) — where `nros configure` put `image`'s answer,
/// located by the image's identity: the directory its stamp records. `Err`
/// names what to run when the image was not configured (no stamp, or a skip
/// stamp); `Ok(None)` when it was, and resolved nothing.
///
/// `image` is the stamp key: an image id for a leaf, `<bringup>:<id>` for a
/// workspace, where a bare id that names exactly one stamp is accepted too.
pub(crate) fn locate_resolved(project: &Path, image: &str) -> Result<(PathBuf, Option<PathBuf>)> {
    let key = stamp_key_for(project, image)?;
    let dir = stamp_dir(project, &key);
    let Some(stamp) = read(project, &key) else {
        eyre::bail!(
            "`{image}` in {} has not been configured (no {}): run `nros configure --image {image}` \
             there",
            project.display(),
            stamp_file(project, &key).display()
        );
    };
    if let Some(why) = &stamp.skipped {
        eyre::bail!(
            "`{image}` in {} was skipped by its last configure ({why}): run `nros configure \
             --image {image}` there",
            project.display()
        );
    }
    Ok((dir, stamp.resolved_dir.filter(|d| d.is_dir())))
}

/// The stamp key `image` names: itself when a stamp is recorded under it,
/// else the one `<bringup>:<image>` stamp whose id is `image`.
fn stamp_key_for(project: &Path, image: &str) -> Result<String> {
    if stamp_file(project, image).is_file() || image.contains(':') {
        return Ok(image.to_string());
    }
    let root = project.join("build").join("nros").join("configured");
    let suffix = format!("__{}", image.replace(':', "__"));
    let hits: Vec<String> = std::fs::read_dir(&root)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(&suffix) && root.join(n).join("stamp.toml").is_file())
        .collect();
    match hits.as_slice() {
        [one] => Ok(one.replacen("__", ":", 1)),
        _ => Ok(image.to_string()),
    }
}

/// Write the stamp atomically.
pub(crate) fn record(project: &Path, image: &str, stamp: &Stamp) -> Result<()> {
    let path = stamp_file(project, image);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .wrap_err_with(|| format!("cannot create `{}`", dir.display()))?;
    }
    let body = toml::to_string_pretty(stamp).wrap_err("cannot serialise the configure stamp")?;
    atomic_write(&path, &body)
}

/// Census files recorded under the project: beside a model, or under
/// `build/nros/census`.
fn recorded_censuses(project: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in [
        project.join("build").join("nros").join("models"),
        project.join("build").join(CENSUS_DIR),
    ] {
        for e in WalkDir::new(&root).into_iter().flatten() {
            let name = e.file_name().to_string_lossy();
            let census = e.file_type().is_file()
                && name.ends_with(".json")
                && (name.ends_with(".census.json") || root.ends_with(CENSUS_DIR));
            if census {
                out.push(e.into_path());
            }
        }
    }
    out.sort();
    out
}

/// `model_gate`'s refused-resolve markers under `build/nros/models` and,
/// when one is set, the models dir configure wrote into instead
/// (`--model-dir`, else `NROS_MODEL_DIR`). A models dir holds a model per
/// bringup at most one level down, so only that depth is read.
fn refused_model_markers(project: &Path, model_dir: Option<&Path>) -> Vec<PathBuf> {
    let ext = crate::model_gate::MARKER_EXT.trim_start_matches('.');
    let own = project.join("build").join("nros").join("models");
    let mut out: Vec<PathBuf> = Vec::new();
    for (root, depth) in std::iter::once((own.as_path(), usize::MAX))
        .chain(model_dir.filter(|d| *d != own).map(|d| (d, 2)))
    {
        out.extend(
            WalkDir::new(root)
                .max_depth(depth)
                .into_iter()
                .flatten()
                .filter(|e| e.file_type().is_file())
                .map(|e| e.into_path())
                .filter(|p| p.extension().and_then(|x| x.to_str()) == Some(ext)),
        );
    }
    out.sort();
    out
}

/// The models dir a configure with `flags` wrote into, when it is not the
/// project's own: `--model-dir`, else `NROS_MODEL_DIR` (sync's own order).
fn configured_model_dir(flags: &OutputFlags, env: &[(String, Option<String>)]) -> Option<PathBuf> {
    flags.model_dir.clone().or_else(|| {
        env.iter()
            .find(|(n, _)| n == "NROS_MODEL_DIR")
            .and_then(|(_, v)| v.clone())
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    })
}

/// The first configure OUTPUT a build of `image` reads that is gone
/// (phase-530 W1): a leaf's model, its `generated/<pkg>` path-deps
/// (`build::leaf_outputs_missing`, the check `nros build` itself refuses on)
/// and its selection package; a workspace image's model. Inputs alone cannot
/// see an `rm -rf generated`.
fn missing_output(project: &Path, image: &str, model_dir: Option<&Path>) -> Result<Option<String>> {
    use nros_orchestration_ir::leaf_system;
    if image == PROJECT_KEY {
        return Ok(None);
    }
    if image.contains(':') {
        return crate::cmd::build::workspace_model_missing(project, image, model_dir);
    }
    if !project.join(leaf_system::SYSTEM_TOML).is_file() {
        return Ok(None);
    }
    let has_components = leaf_system::read_image(project, image)
        .ok()
        .flatten()
        .is_some_and(|d| !d.components.is_empty());
    if let Some(what) = crate::cmd::build::leaf_outputs_missing(project, has_components)
        .into_iter()
        .next()
    {
        return Ok(Some(what));
    }
    if let Some(pkg) = crate::cmd::leaf_settings::leaf_package(project)
        && crate::cmd::leaf_settings::depends_on_selection(project, &pkg)
        && !project
            .join(leaf_system::LEAF_SELECTION_DIR)
            .join("Cargo.toml")
            .is_file()
    {
        return Ok(Some(format!(
            "the selection package `{}`",
            leaf_system::LEAF_SELECTION_DIR
        )));
    }
    Ok(None)
}

/// May a configure of `image` be skipped? Order of reasons: no stamp; digest
/// differs; a metadata sidecar's component sources moved; a recorded census is
/// stale. An image with no recorded census is not stale on that account.
///
/// `named`: whether the caller asked for this image by name. A SKIP stamp
/// (Ruling R15) is current only for a caller that did not.
pub(crate) fn is_current(
    project: &Path,
    image: &str,
    named: bool,
    flags: &OutputFlags,
) -> Result<Currency> {
    currency(
        project,
        image,
        named,
        &real_env(),
        flags,
        current_launch_resolver().as_deref(),
        &real_ament_moved,
        &real_preflight,
    )
}

/// Ruling R19 — would `image`'s toolchain preflight refuse it NOW? Stages 1–3
/// as a query (`build::preflight_query`): no census, nothing generated,
/// nothing written. `Ok(Some(reason))` still refuses; `Ok(None)` passes.
fn real_preflight(project: &Path, image: &str) -> Result<Option<String>> {
    let args = crate::cmd::configure::build_args(project, image, &None, false, false);
    match crate::cmd::build::preflight_query(&args) {
        Ok(()) => Ok(None),
        Err(e) => match e.downcast_ref::<crate::builder::preflight::PreflightRefusal>() {
            Some(r) => Ok(Some(r.summary())),
            None => Err(e),
        },
    }
}

/// The generated message crates whose recorded ament inputs moved (RFC-0120
/// D7, issue 1780): `ws`'s own comparison, never a second one.
fn real_ament_moved(project: &Path) -> Result<Vec<String>> {
    Ok(crate::cmd::ws::ament_moved_packages(project)?
        .into_iter()
        .map(|m| m.package)
        .collect())
}

/// The `nros-launch-resolve` this process resolves: the lookup
/// `ws::launch_resolver_path` (sync) makes. ONE spelling for the stamp's
/// writer ([`stamp_now`]) and its reader ([`is_current`]), so a recorded path
/// can never differ from the one compared against it (Ruling R6: a mismatch
/// reads "launch resolver changed" forever).
pub(crate) fn current_launch_resolver() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    crate::orchestration::model_location::launch_resolver_bin_for_exe(&exe, None)
}

/// The stamp a configure that just finished records: the digest of the
/// inputs as they are NOW (after it wrote), and the environment it ran in.
pub(crate) fn stamp_now(project: &Path, flags: &OutputFlags) -> Result<Stamp> {
    Ok(Stamp {
        digest: input_digest_with(project, &real_env(), flags)?,
        codegen_version: codegen_version(),
        ament_prefix: std::env::var("AMENT_PREFIX_PATH").ok(),
        launch_resolve: current_launch_resolver(),
        skipped: None,
        resolved_dir: None,
    })
}

/// [`is_current`] with the environment and the currently resolved
/// `nros-launch-resolve` injected.
/// An unnamed image, with the real ament check.
#[cfg(test)]
pub(crate) fn is_current_with(
    project: &Path,
    image: &str,
    env: &[(String, Option<String>)],
    resolver: Option<&Path>,
) -> Result<Currency> {
    currency(
        project,
        image,
        false,
        env,
        &OutputFlags::defaults(),
        resolver,
        &real_ament_moved,
        &real_preflight,
    )
}

/// What a skipped image's preflight answers now: `Some(reason)` still
/// refuses, `None` passes.
pub(crate) type PreflightProbe<'a> = &'a dyn Fn(&Path, &str) -> Result<Option<String>>;

/// [`is_current`] with everything it observes injected: whether the image was
/// named, the environment, the resolved `nros-launch-resolve`, which generated
/// crates' ament inputs moved, and (for a SKIP stamp only) the image's
/// toolchain preflight.
// Each argument is one thing the answer observes, injected so a test can hold
// the rest still; bundling them would only rename the list.
#[allow(clippy::too_many_arguments)]
pub(crate) fn currency(
    project: &Path,
    image: &str,
    named: bool,
    env: &[(String, Option<String>)],
    flags: &OutputFlags,
    resolver: Option<&Path>,
    ament_moved: &dyn Fn(&Path) -> Result<Vec<String>>,
    preflight: PreflightProbe<'_>,
) -> Result<Currency> {
    let Some(stamp) = read(project, image) else {
        return Ok(Currency::Stale("no configure stamp recorded".into()));
    };
    if named && let Some(why) = &stamp.skipped {
        return Ok(Currency::Stale(format!("image was skipped: {why}")));
    }
    if let Some(holder) = leaf_selection(project)
        && holder != image
    {
        return Ok(Currency::Stale(format!(
            "leaf output holds image `{holder}`"
        )));
    }
    let now = input_digest_with(project, env, flags)?;
    if stamp.digest != now {
        return Ok(Currency::Stale(format!(
            "inputs changed (recorded {}, now {now})",
            stamp.digest
        )));
    }
    if stamp.launch_resolve.as_deref() != resolver {
        let show = |p: Option<&Path>| p.map_or("none".to_string(), |p| p.display().to_string());
        return Ok(Currency::Stale(format!(
            "launch resolver changed: {} -> {}",
            show(stamp.launch_resolve.as_deref()),
            show(resolver)
        )));
    }
    // A model whose resolve was REFUSED (`model_gate`) is quarantined with a
    // marker beside it. Its inputs can be edited back to exactly what this
    // stamp recorded, so only the marker says the model is gone.
    let model_dir = configured_model_dir(flags, env);
    if let Some(marker) = refused_model_markers(project, model_dir.as_deref())
        .into_iter()
        .next()
    {
        return Ok(Currency::Stale(format!(
            "model resolve was refused (`{}`)",
            marker.display()
        )));
    }
    // An output configure wrote and a build reads is gone (`rm -rf
    // generated`): its inputs are unchanged, so only this sees it.
    if let Some(what) = missing_output(project, image, model_dir.as_deref())? {
        return Ok(Currency::Stale(format!("configure output missing: {what}")));
    }
    // RFC-0120 D7 — the AMENT install's content: `AMENT_PREFIX_PATH`'s value
    // is a digest input, but an `apt` upgrade moves the files under it.
    if let Some(pkg) = ament_moved(project)?.into_iter().next() {
        return Ok(Currency::Stale(format!("ament inputs moved: {pkg}")));
    }
    if let Some(line) = stale_sidecar_components(project).into_iter().next() {
        return Ok(Currency::Stale(line));
    }
    for census in recorded_censuses(project) {
        match census_freshness(&census, project) {
            Freshness::Fresh => {}
            Freshness::Stale(why) | Freshness::Missing(why) => {
                return Ok(Currency::Stale(format!(
                    "entity census `{}` is stale: {}",
                    census.display(),
                    why.lines().next().unwrap_or("")
                )));
            }
        }
    }
    // Ruling R19 — LAST, as the only check that consults the toolchain: a
    // skip stamp is current only while the image's preflight still refuses.
    // Toolchain presence is no digest input, so without this a skipped image
    // stayed skipped after its toolchain was installed.
    if stamp.skipped.is_some() {
        match preflight(project, image) {
            Ok(Some(_still_refused)) => {}
            Ok(None) => {
                return Ok(Currency::Stale(format!(
                    "toolchain now present for {image}"
                )));
            }
            Err(e) => {
                return Ok(Currency::Stale(format!(
                    "preflight of {image} could not be checked: {}",
                    format!("{e:#}").lines().next().unwrap_or("")
                )));
            }
        }
    }
    Ok(Currency::Current)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = crate::test_support::scratch_dir(&format!("configure-stamp-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn stamp_for(project: &Path) -> Stamp {
        Stamp {
            digest: input_digest(project).unwrap(),
            codegen_version: codegen_version(),
            ament_prefix: None,
            launch_resolve: None,
            skipped: None,
            resolved_dir: None,
        }
    }

    #[test]
    fn stamp_dir_for_leaf_and_workspace() {
        let p = Path::new("p");
        assert_eq!(
            stamp_dir(p, "zephyr_zenoh"),
            p.join("build/nros/configured/zephyr_zenoh")
        );
        assert_eq!(
            stamp_dir(p, "demo_bringup:zephyr"),
            p.join("build/nros/configured/demo_bringup__zephyr")
        );
    }

    /// phase-530 W4 (RFC-0120 D2) — a host configure locates an image's
    /// answer by the image's identity: the dir its stamp records. An image
    /// with no stamp, or a skip stamp, is an error naming `nros configure`.
    #[test]
    fn an_image_is_located_by_its_stamp() {
        let p = tmp("locate");
        let err = locate_resolved(&p, "zephyr_zenoh").unwrap_err().to_string();
        assert!(err.contains("has not been configured"), "{err}");
        assert!(err.contains("nros configure --image zephyr_zenoh"), "{err}");

        let resolved = p.join("build/talker__zephyr_zenoh");
        std::fs::create_dir_all(&resolved).unwrap();
        let mut s = stamp_for(&p);
        s.resolved_dir = Some(resolved.clone());
        record(&p, "zephyr_zenoh", &s).unwrap();
        let (dir, got) = locate_resolved(&p, "zephyr_zenoh").unwrap();
        assert_eq!(dir, stamp_dir(&p, "zephyr_zenoh"));
        assert_eq!(got.as_deref(), Some(resolved.as_path()));

        // A recorded dir that is gone is no answer, never a dangling path.
        std::fs::remove_dir_all(&resolved).unwrap();
        assert_eq!(locate_resolved(&p, "zephyr_zenoh").unwrap().1, None);

        // A skipped image was not configured.
        let skip = Stamp {
            skipped: Some("missing toolchain".into()),
            ..stamp_for(&p)
        };
        record(&p, "other", &skip).unwrap();
        let err = locate_resolved(&p, "other").unwrap_err().to_string();
        assert!(
            err.contains("skipped") && err.contains("missing toolchain"),
            "{err}"
        );
    }

    /// A workspace image's stamp key is `<bringup>:<id>`; the hook may name it
    /// by its bare id when exactly one bringup declares it.
    #[test]
    fn a_bare_id_names_its_one_qualified_stamp() {
        let p = tmp("locate-bare");
        let resolved = p.join("build/demo_bringup__zephyr");
        std::fs::create_dir_all(&resolved).unwrap();
        let mut s = stamp_for(&p);
        s.resolved_dir = Some(resolved.clone());
        record(&p, "demo_bringup:zephyr", &s).unwrap();
        let (dir, got) = locate_resolved(&p, "zephyr").unwrap();
        assert_eq!(dir, stamp_dir(&p, "demo_bringup:zephyr"));
        assert_eq!(got.as_deref(), Some(resolved.as_path()));
        // Two bringups declaring it: the bare id names neither.
        record(&p, "other_bringup:zephyr", &s).unwrap();
        assert!(locate_resolved(&p, "zephyr").is_err());
    }

    #[test]
    fn digest_moves_on_system_toml_edit() {
        let p = tmp("digest-moves");
        std::fs::write(p.join("system.toml"), "[system]\nname = \"a\"\n").unwrap();
        let before = input_digest(&p).unwrap();
        assert!(before.starts_with("sha256:"));
        std::fs::write(p.join("system.toml"), "[system]\nname = \"b\"\n").unwrap();
        assert_ne!(before, input_digest(&p).unwrap());
    }

    #[test]
    fn digest_sees_launch_files_and_ignores_other_xml() {
        let p = tmp("digest-launch");
        std::fs::create_dir_all(p.join("launch")).unwrap();
        std::fs::write(p.join("launch/a.launch.xml"), "<launch/>").unwrap();
        std::fs::write(p.join("notes.xml"), "<x/>").unwrap();
        let before = input_digest(&p).unwrap();
        std::fs::write(p.join("notes.xml"), "<y/>").unwrap();
        assert_eq!(before, input_digest(&p).unwrap());
        std::fs::write(p.join("launch/a.launch.xml"), "<launch a='1'/>").unwrap();
        assert_ne!(before, input_digest(&p).unwrap());
    }

    #[test]
    fn digest_ignores_build_and_generated() {
        let p = tmp("digest-ignores");
        std::fs::write(p.join("system.toml"), "[system]\n").unwrap();
        let before = input_digest(&p).unwrap();
        for dir in ["build", "generated", "target", "nested/generated"] {
            std::fs::create_dir_all(p.join(dir)).unwrap();
            std::fs::write(p.join(dir).join("package.xml"), "<package/>").unwrap();
            std::fs::write(p.join(dir).join("a.msg"), "int32 x\n").unwrap();
        }
        // A nested repository is not this project's.
        std::fs::create_dir_all(p.join("sub/.git")).unwrap();
        std::fs::write(p.join("sub/Cargo.toml"), "[package]\n").unwrap();
        assert_eq!(before, input_digest(&p).unwrap());
    }

    /// Ruling R6: what a configure records is what [`is_current`] compares
    /// against, so a stamp written now reads `Current` through the REAL
    /// lookup (not an injected resolver).
    #[test]
    fn stamp_now_reads_current_through_the_real_lookup() {
        let p = tmp("stamp-now");
        std::fs::write(p.join("system.toml"), "[system]\n").unwrap();
        let stamp = stamp_now(&p, &OutputFlags::defaults()).unwrap();
        assert_eq!(stamp.launch_resolve, current_launch_resolver());
        record(&p, "img", &stamp).unwrap();
        assert_eq!(
            is_current(&p, "img", false, &OutputFlags::defaults()).unwrap(),
            Currency::Current
        );
    }

    /// Ruling R9: an image's own stamp is not enough on a leaf whose shared
    /// output another image wrote last.
    #[test]
    fn leaf_output_held_by_another_image_is_stale() {
        let p = tmp("leaf-selection");
        std::fs::write(p.join("system.toml"), "[system]\n").unwrap();
        record(&p, "linux", &stamp_for(&p)).unwrap();
        record(&p, "linux_xrce", &stamp_for(&p)).unwrap();
        record_leaf_selection(&p, "linux_xrce").unwrap();
        match is_current_with(&p, "linux", &real_env(), None).unwrap() {
            Currency::Stale(why) => assert!(why.contains("`linux_xrce`"), "{why}"),
            Currency::Current => panic!("`linux` read current over `linux_xrce`'s output"),
        }
        assert_eq!(
            is_current_with(&p, "linux_xrce", &real_env(), None).unwrap(),
            Currency::Current
        );
    }

    #[test]
    fn no_stamp_is_stale() {
        let p = tmp("no-stamp");
        assert!(matches!(
            is_current(&p, "img", false, &OutputFlags::defaults()).unwrap(),
            Currency::Stale(why) if why.contains("no configure stamp")
        ));
    }

    #[test]
    fn recorded_stamp_is_current() {
        let p = tmp("recorded");
        std::fs::write(p.join("system.toml"), "[system]\n").unwrap();
        let stamp = stamp_for(&p);
        record(&p, "ws:zephyr", &stamp).unwrap();
        assert_eq!(read(&p, "ws:zephyr"), Some(stamp));
        assert_eq!(
            is_current_with(&p, "ws:zephyr", &real_env(), None).unwrap(),
            Currency::Current
        );
        std::fs::write(p.join("system.toml"), "[system]\nname = \"x\"\n").unwrap();
        assert!(matches!(
            is_current_with(&p, "ws:zephyr", &real_env(), None).unwrap(),
            Currency::Stale(why) if why.contains("inputs changed")
        ));
    }

    /// A project with one Rust component and a sidecar stamped by the real
    /// key function. Returns (project, sidecar).
    fn sidecar_fixture(name: &str, stamp_with: Option<&str>) -> (PathBuf, PathBuf) {
        use crate::orchestration::metadata_refresh as mr;
        let p = tmp(name);
        std::fs::create_dir_all(p.join("src")).unwrap();
        std::fs::create_dir_all(p.join("metadata")).unwrap();
        std::fs::write(
            p.join("package.xml"),
            "<package format=\"3\"><name>talker_pkg</name><version>0.1.0</version>\
             <description>t</description><maintainer email=\"x@x\">x</maintainer>\
             <license>Apache-2.0</license></package>",
        )
        .unwrap();
        std::fs::write(
            p.join("nros.toml"),
            "[component]\nversion = 1\npackage = \"talker_pkg\"\ncomponent = \"talker\"\n\
             language = \"rust\"\n[component.metadata]\nsource_metadata = \"metadata/talker.json\"\n",
        )
        .unwrap();
        std::fs::write(p.join("src/lib.rs"), "// the component\n").unwrap();
        let sidecar = p.join("metadata/talker.json");
        std::fs::write(
            &sidecar,
            r#"{"version": 4, "package": "talker_pkg", "component": "talker",
                "language": "rust", "executable": "talker", "exported_symbol": null,
                "nodes": [], "callbacks": [], "parameters": [],
                "trace": {"generator": "t", "package_manifest": "package.xml",
                          "source_artifacts": []}}"#,
        )
        .unwrap();
        // A nano-ros root the probe key can be computed against.
        let root = tmp(&format!("{name}-nros"));
        let list = root.join(mr::PROBE_SOURCE_DIRS);
        std::fs::create_dir_all(list.parent().unwrap()).unwrap();
        std::fs::write(&list, "# generated\npackages/core/probed\n").unwrap();
        let src = root.join("packages/core/probed/src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("lib.rs"), "pub const X: bool = false;\n").unwrap();
        let key = mr::probe_inputs_key(&p, Some(&root)).expect("keyed");
        mr::stamp_provenance(&sidecar, stamp_with.unwrap_or(&key)).unwrap();
        (p, sidecar)
    }

    #[test]
    fn stale_when_metadata_sidecar_inputs_moved() {
        let (p, _) = sidecar_fixture("sidecar", None);
        record(&p, "img", &stamp_for(&p)).unwrap();
        assert_eq!(
            is_current_with(&p, "img", &real_env(), None).unwrap(),
            Currency::Current
        );

        // `lib.rs` is no digest input, so only the sidecar check can answer.
        std::fs::write(p.join("src/lib.rs"), "// the component, edited\n").unwrap();
        match is_current_with(&p, "img", &real_env(), None).unwrap() {
            Currency::Stale(why) => assert!(why.contains("metadata sidecar"), "{why}"),
            Currency::Current => panic!("a moved component source must stale the stamp"),
        }
    }

    /// R5: a sidecar stamped `UNKEYED` has nothing to compare, so it is not
    /// stale (it was flagged on every call before).
    #[test]
    fn unkeyed_sidecar_is_not_stale() {
        let (p, _) = sidecar_fixture(
            "unkeyed",
            Some(crate::orchestration::metadata_refresh::UNKEYED),
        );
        record(&p, "img", &stamp_for(&p)).unwrap();
        std::fs::write(p.join("src/lib.rs"), "// edited\n").unwrap();
        assert_eq!(
            is_current_with(&p, "img", &real_env(), None).unwrap(),
            Currency::Current
        );
    }

    #[test]
    fn digest_moves_on_env_input() {
        let p = tmp("digest-env");
        std::fs::write(p.join("system.toml"), "[system]\n").unwrap();
        let env = |a: Option<&str>, n: Option<&str>| {
            vec![
                ("AMENT_PREFIX_PATH".to_string(), a.map(str::to_string)),
                (
                    "NROS_INTERFACE_SEARCH_PATH".to_string(),
                    n.map(str::to_string),
                ),
            ]
        };
        let base = input_digest_with_env(&p, &env(None, None)).unwrap();
        let a = input_digest_with_env(&p, &env(Some("/opt/ros"), None)).unwrap();
        let n = input_digest_with_env(&p, &env(None, Some("/opt/ros"))).unwrap();
        assert_ne!(base, a);
        assert_ne!(base, n);
        assert_ne!(a, n, "the variable's name is part of the input");
        assert_eq!(base, input_digest_with_env(&p, &env(None, None)).unwrap());
    }

    /// A census beside a model, recording one source tree.
    fn census_fixture(p: &Path) -> PathBuf {
        use crate::orchestration::metadata_refresh as mr;
        std::fs::create_dir_all(p.join("src/comp")).unwrap();
        std::fs::write(p.join("src/comp/node.cpp"), "// SUB: /a\n").unwrap();
        let digest = mr::source_digest(&p.join("src/comp")).unwrap();
        let dir = p.join("build/nros/models/bringup");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("m.census.json");
        let doc = serde_json::json!({
            "schema": "x",
            "provenance": { "inputs": [
                { "role": "source_tree", "path": "src/comp", "digest": digest }
            ]},
            "census": { "version": mr::RECORDER_SCHEMA_VERSION, "nodes": [] },
        });
        std::fs::write(&path, doc.to_string()).unwrap();
        path
    }

    #[test]
    fn stale_when_recorded_census_moved() {
        let p = tmp("census-moved");
        census_fixture(&p);
        record(&p, "img", &stamp_for(&p)).unwrap();
        assert_eq!(
            is_current_with(&p, "img", &real_env(), None).unwrap(),
            Currency::Current
        );
        std::fs::write(p.join("src/comp/node.cpp"), "// SUB: /a\n// SUB: /b\n").unwrap();
        match is_current_with(&p, "img", &real_env(), None).unwrap() {
            Currency::Stale(why) => assert!(why.contains("entity census"), "{why}"),
            Currency::Current => panic!("a moved census input must stale the stamp"),
        }
    }

    /// R2: no census recorded anywhere is not stale on that account.
    #[test]
    fn no_census_record_is_not_stale() {
        let p = tmp("no-census");
        std::fs::write(p.join("system.toml"), "[system]\n").unwrap();
        record(&p, "img", &stamp_for(&p)).unwrap();
        assert!(recorded_censuses(&p).is_empty());
        assert_eq!(
            is_current_with(&p, "img", &real_env(), None).unwrap(),
            Currency::Current
        );
    }

    #[test]
    fn stale_when_launch_resolver_changes() {
        let p = tmp("resolver");
        std::fs::write(p.join("system.toml"), "[system]\n").unwrap();
        let mut stamp = stamp_for(&p);
        stamp.launch_resolve = Some(PathBuf::from("/old/nros-launch-resolve"));
        record(&p, "img", &stamp).unwrap();
        let old = Path::new("/old/nros-launch-resolve");
        assert_eq!(
            is_current_with(&p, "img", &real_env(), Some(old)).unwrap(),
            Currency::Current
        );
        let new = Path::new("/new/nros-launch-resolve");
        match is_current_with(&p, "img", &real_env(), Some(new)).unwrap() {
            Currency::Stale(why) => assert_eq!(
                why,
                "launch resolver changed: /old/nros-launch-resolve -> /new/nros-launch-resolve"
            ),
            Currency::Current => panic!("a moved resolver must stale the stamp"),
        }
    }

    fn nothing_moved(_: &Path) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    fn still_refuses(_: &Path, _: &str) -> Result<Option<String>> {
        Ok(Some("missing Rust target `x`".into()))
    }

    /// RFC-0120 D7 / Ruling R17 — an `apt` upgrade moves the ament files under
    /// an unchanged `AMENT_PREFIX_PATH`, so only the generated crates' content
    /// stamps can see it, and the configure stamp must ask them.
    #[test]
    fn stale_when_ament_content_moved() {
        let p = tmp("ament-moved");
        std::fs::write(p.join("system.toml"), "[system]\n").unwrap();
        record(&p, "img", &stamp_for(&p)).unwrap();
        assert_eq!(
            currency(
                &p,
                "img",
                false,
                &real_env(),
                &OutputFlags::defaults(),
                None,
                &nothing_moved,
                &still_refuses
            )
            .unwrap(),
            Currency::Current
        );
        let moved = |_: &Path| -> Result<Vec<String>> { Ok(vec!["std_msgs".to_string()]) };
        assert_eq!(
            currency(
                &p,
                "img",
                false,
                &real_env(),
                &OutputFlags::defaults(),
                None,
                &moved,
                &still_refuses
            )
            .unwrap(),
            Currency::Stale("ament inputs moved: std_msgs".into())
        );
    }

    /// Ruling R15 — a SKIP stamp is current for a configure that does not
    /// name the image, and stale (naming the reason) for one that does.
    #[test]
    fn named_skipped_image_is_stale() {
        let p = tmp("named-skip");
        std::fs::write(p.join("system.toml"), "[system]\n").unwrap();
        let mut stamp = stamp_for(&p);
        stamp.skipped = Some("missing Rust target `x` (run: rustup target add x)".into());
        record(&p, "ws:img", &stamp).unwrap();
        assert_eq!(
            currency(
                &p,
                "ws:img",
                false,
                &real_env(),
                &OutputFlags::defaults(),
                None,
                &nothing_moved,
                &still_refuses
            )
            .unwrap(),
            Currency::Current,
            "an unnamed skipped image with an unchanged digest is current"
        );
        assert_eq!(
            currency(
                &p,
                "ws:img",
                true,
                &real_env(),
                &OutputFlags::defaults(),
                None,
                &nothing_moved,
                &still_refuses
            )
            .unwrap(),
            Currency::Stale(
                "image was skipped: missing Rust target `x` (run: rustup target add x)".into()
            )
        );
    }

    /// A refused resolve quarantines the model; editing the input back to the
    /// recorded bytes must not make the stamp current again.
    #[test]
    fn stale_when_a_model_resolve_was_refused() {
        let p = tmp("refused-model");
        std::fs::write(p.join("system.toml"), "[system]\n").unwrap();
        record(&p, "img", &stamp_for(&p)).unwrap();
        let models = p.join("build/nros/models/demo_bringup");
        std::fs::create_dir_all(&models).unwrap();
        let marker = crate::model_gate::marker_path(&models.join("system_model.yaml"));
        std::fs::write(&marker, "check: x\ninput: y\n").unwrap();
        match currency(
            &p,
            "img",
            false,
            &real_env(),
            &OutputFlags::defaults(),
            None,
            &nothing_moved,
            &still_refuses,
        )
        .unwrap()
        {
            Currency::Stale(why) => assert!(why.contains("refused"), "{why}"),
            Currency::Current => panic!("a refused model must stale the stamp"),
        }
    }

    /// Ruling R19 — a SKIP stamp stays current only while the image's
    /// preflight still refuses; once the toolchain appears, the image
    /// reconfigures (once: the next configure records a full stamp).
    #[test]
    fn skipped_image_reconfigures_once_its_toolchain_appears() {
        let p = tmp("skip-toolchain");
        std::fs::write(p.join("system.toml"), "[system]\n").unwrap();
        let mut stamp = stamp_for(&p);
        stamp.skipped = Some("missing Rust target `x`".into());
        record(&p, "ws:img", &stamp).unwrap();
        assert_eq!(
            currency(
                &p,
                "ws:img",
                false,
                &real_env(),
                &OutputFlags::defaults(),
                None,
                &nothing_moved,
                &still_refuses
            )
            .unwrap(),
            Currency::Current,
            "still refused: the skip stands"
        );
        let asked = std::cell::RefCell::new(Vec::new());
        let present = |_: &Path, image: &str| -> Result<Option<String>> {
            asked.borrow_mut().push(image.to_string());
            Ok(None)
        };
        assert_eq!(
            currency(
                &p,
                "ws:img",
                false,
                &real_env(),
                &OutputFlags::defaults(),
                None,
                &nothing_moved,
                &present
            )
            .unwrap(),
            Currency::Stale("toolchain now present for ws:img".into())
        );
        assert_eq!(*asked.borrow(), ["ws:img"], "the image's own preflight");

        // A FULL stamp never asks the toolchain.
        record(&p, "ws:img", &stamp_for(&p)).unwrap();
        let never = |_: &Path, _: &str| -> Result<Option<String>> {
            panic!("a full stamp must not run a preflight")
        };
        assert_eq!(
            currency(
                &p,
                "ws:img",
                false,
                &real_env(),
                &OutputFlags::defaults(),
                None,
                &nothing_moved,
                &never
            )
            .unwrap(),
            Currency::Current
        );
    }

    /// A single-image leaf with one `[[component]]`, so a build reads its
    /// model: what `deleted_model_is_stale` deletes.
    fn leaf_with_a_component(name: &str) -> PathBuf {
        let p = tmp(name);
        std::fs::write(
            p.join("system.toml"),
            "[system]\nname = \"t\"\nrmw = \"zenoh\"\n\n[[component]]\npkg = \"t\"\n\
             class = \"t::T\"\nname = \"t\"\n\n[image.linux]\nboard = \"linux\"\n",
        )
        .unwrap();
        std::fs::write(
            p.join("Cargo.toml"),
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        p
    }

    /// Where a leaf's resolved model lives under its own build dir.
    fn leaf_model_file(p: &Path) -> PathBuf {
        let rel = nros_orchestration_ir::model_location::launch_to_model_rel(p, None, &[]).unwrap();
        nros_orchestration_ir::model_location::model_search_paths(p, &rel)
            .into_iter()
            .find(|c| c.starts_with(p.join("build")))
            .expect("a build-dir rung for the leaf's model")
    }

    /// phase-530 W1 (I1) — inputs alone cannot see a deleted OUTPUT: the
    /// digest is unchanged, and the next `nros build` fails on the missing
    /// model with an internal error. The stamp must call that stale.
    #[test]
    fn deleted_model_is_stale() {
        let p = leaf_with_a_component("deleted-model");
        let model = leaf_model_file(&p);
        std::fs::create_dir_all(model.parent().unwrap()).unwrap();
        std::fs::write(&model, "nodes: []\n").unwrap();
        record(&p, "linux", &stamp_for(&p)).unwrap();
        let ask = || {
            currency(
                &p,
                "linux",
                false,
                &real_env(),
                &OutputFlags::defaults(),
                None,
                &nothing_moved,
                &still_refuses,
            )
            .unwrap()
        };
        assert_eq!(ask(), Currency::Current, "the model is there");
        std::fs::remove_file(&model).unwrap();
        match ask() {
            Currency::Stale(why) => assert!(
                why.starts_with("configure output missing: the resolved model"),
                "{why}"
            ),
            Currency::Current => panic!("a deleted model left the stamp current"),
        }
    }

    /// phase-530 W1 (I1) — the same for a `generated/<pkg>` path-dep the
    /// leaf manifest names.
    #[test]
    fn deleted_generated_crate_is_stale() {
        let p = tmp("deleted-generated");
        std::fs::write(p.join("system.toml"), "[system]\n").unwrap();
        std::fs::write(
            p.join("Cargo.toml"),
            "[package]\nname = \"t\"\nversion = \"0.1.0\"\n\n[dependencies]\n\
             std_msgs = { path = \"generated/std_msgs\" }\n",
        )
        .unwrap();
        let crate_dir = p.join("generated/std_msgs");
        std::fs::create_dir_all(&crate_dir).unwrap();
        std::fs::write(crate_dir.join("Cargo.toml"), "[package]\n").unwrap();
        record(&p, "img", &stamp_for(&p)).unwrap();
        let ask = || {
            currency(
                &p,
                "img",
                false,
                &real_env(),
                &OutputFlags::defaults(),
                None,
                &nothing_moved,
                &still_refuses,
            )
            .unwrap()
        };
        assert_eq!(ask(), Currency::Current);
        std::fs::remove_dir_all(p.join("generated")).unwrap();
        assert_eq!(
            ask(),
            Currency::Stale(
                "configure output missing: the generated message crate `std_msgs` \
                 (generated/std_msgs)"
                    .into()
            )
        );
    }

    /// phase-530 W1 (I2) — a flag that changes what configure writes is a
    /// digest input; one that changes only whether it runs is not.
    #[test]
    fn output_flags_are_digest_inputs() {
        use clap::Parser;
        #[derive(Parser)]
        struct A {
            #[command(flatten)]
            sync: crate::cmd::ws::SyncArgs,
        }
        let flags = |argv: &[&str]| {
            let mut full = vec!["configure"];
            full.extend_from_slice(argv);
            OutputFlags::of(&A::try_parse_from(full).unwrap().sync)
        };
        let p = tmp("flag-inputs");
        std::fs::write(p.join("system.toml"), "[system]\n").unwrap();
        let env = real_env();
        let d = |f: &OutputFlags| input_digest_with(&p, &env, f).unwrap();
        let base = d(&flags(&[]));
        assert_eq!(base, d(&OutputFlags::defaults()), "defaults == no flags");
        for moves in [
            &["--ros-edition", "jazzy"][..],
            &["--generated-base", "gen"],
            &["--model-dir", "/m"],
            &["--no-metadata"],
            &["--base-paths", "/u"],
            &["--nano-ros-path", "/n"],
            &["--nano-ros-root", "/r"],
        ] {
            assert_ne!(base, d(&flags(moves)), "{moves:?} is an output flag");
        }
        for stays in [
            &["--check"][..],
            &["--dry-run"],
            &["--verbose"],
            &["--offline"],
            &["--image", "x"],
        ] {
            assert_eq!(base, d(&flags(stays)), "{stays:?} writes nothing");
        }
    }

    /// phase-530 W1 (I2) — a refused model under `NROS_MODEL_DIR` (where C1
    /// wrote it) stales the stamp too, not only one under `build/`.
    #[test]
    fn refused_model_under_the_model_dir_is_stale() {
        let p = tmp("refused-model-dir");
        let md = tmp("refused-model-dir-models");
        std::fs::write(p.join("system.toml"), "[system]\n").unwrap();
        let env: Vec<(String, Option<String>)> = ENV_INPUTS
            .iter()
            .map(|v| {
                let val = (*v == "NROS_MODEL_DIR").then(|| md.display().to_string());
                ((*v).to_string(), val)
            })
            .collect();
        let flags = OutputFlags::defaults();
        let stamp = Stamp {
            digest: input_digest_with(&p, &env, &flags).unwrap(),
            ..stamp_for(&p)
        };
        record(&p, "img", &stamp).unwrap();
        let ask = || {
            currency(
                &p,
                "img",
                false,
                &env,
                &flags,
                None,
                &nothing_moved,
                &still_refuses,
            )
            .unwrap()
        };
        assert_eq!(ask(), Currency::Current);
        std::fs::create_dir_all(md.join("t_bringup")).unwrap();
        let marker =
            crate::model_gate::marker_path(&md.join("t_bringup").join("system_model.yaml"));
        std::fs::write(&marker, "check: x\ninput: y\n").unwrap();
        match ask() {
            Currency::Stale(why) => assert!(why.contains("refused"), "{why}"),
            Currency::Current => panic!("a refused model under NROS_MODEL_DIR was missed"),
        }
    }

    /// phase-530 W1 (I2) — `NROS_MODEL_DIR` moves where C1 writes the
    /// models, so its value is a digest input like `AMENT_PREFIX_PATH`'s.
    #[test]
    fn model_dir_env_is_a_stamp_input() {
        let p = tmp("model-dir-env");
        std::fs::write(p.join("system.toml"), "[system]\n").unwrap();
        let env = |md: Option<&str>| -> Vec<(String, Option<String>)> {
            ENV_INPUTS
                .iter()
                .map(|v| {
                    let val = (*v == "NROS_MODEL_DIR")
                        .then(|| md.map(str::to_string))
                        .flatten();
                    ((*v).to_string(), val)
                })
                .collect()
        };
        let base = input_digest_with_env(&p, &env(None)).unwrap();
        record(
            &p,
            "img",
            &Stamp {
                digest: base.clone(),
                ..stamp_for(&p)
            },
        )
        .unwrap();
        assert_eq!(
            is_current_with(&p, "img", &env(None), None).unwrap(),
            Currency::Current
        );
        assert_ne!(base, input_digest_with_env(&p, &env(Some("/m"))).unwrap());
        match is_current_with(&p, "img", &env(Some("/m")), None).unwrap() {
            Currency::Stale(why) => assert!(why.starts_with("inputs changed"), "{why}"),
            Currency::Current => panic!("a moved NROS_MODEL_DIR left the stamp current"),
        }
    }
}
