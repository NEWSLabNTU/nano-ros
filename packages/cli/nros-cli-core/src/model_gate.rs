//! phase-460 W1 (issue 1420) -- the ONE door a consumer opens a resolved
//! SystemModel through.
//!
//! `nros sync` stages, stamps and renames a resolved model, which is right for
//! the success path. On a REFUSED resolve the resolver writes nothing, so the
//! previous model stayed in place -- and it was intact by every check it would
//! ever meet, because the provenance check hashed the inputs the model RECORDS
//! (those of the resolve that succeeded) and only `run_sync` called it. `nros
//! model-path` handed the path to cmake unchecked; `ws entity-inventory`,
//! `codegen-system` and `codegen entry` loaded whatever the path held. On the
//! island (brief D, E7b) that was an inventory sized from a contract the tree
//! no longer stated.
//!
//! Two rules, one module:
//!
//! 1. QUARANTINE. On refusal the producer moves the previous model to
//!    `<stem>.refused-<utc-stamp>.yaml` beside it (kept for diffing, never
//!    deleted) and writes a `<stem>.refused` marker naming the refusing check
//!    and the input that changed. The path a consumer opens no longer exists,
//!    and the marker refuses every consumer until the next SUCCESSFUL sync
//!    removes it -- even when the inputs are edited back to the last-good
//!    state, because a model whose producer last said no is not current.
//! 2. VERIFY AT EVERY DOOR. [`verify`] is what every consumer that opens a
//!    model calls: `model-path` (so cmake fails at configure, not at boot),
//!    `ws entity-inventory`, `codegen-system`, `codegen entry`, and `run_sync`
//!    itself. It refuses on the marker, then on a resolver pin that
//!    DISAGREES with ours, then on a recorded input whose hash changed, then
//!    on a launch-tree input the model never recorded. One function, so a
//!    further consumer cannot be written without the omission showing in
//!    review. A model recording NO pin is refused by `run_sync`, which can
//!    fix it by re-resolving, and not by a consumer door, which cannot --
//!    see [`PinPolicy`].
//!
//! What this module deliberately does NOT do: re-resolve. A consumer that
//! finds a stale or refused model refuses; resolving at the consumer would put
//! the launch parser back on the cmake path phase-296 deleted, and would let a
//! consumer disagree with the build layer that owns the artifact.
//!
//! The `nros::main!` proc-macro is a fifth door this module cannot reach: it
//! opens the model through `nros_orchestration_ir::model_location::ensure_model`
//! (`packages/core/nros-macros/src/main_macro.rs`), and a proc-macro cannot
//! depend on this crate. That door still re-resolves from the inputs when the
//! artifact is absent -- which is what quarantine leaves -- so it never reads
//! a quarantined model; what it does not see is the marker.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Extension of the marker beside a model: `system_model.yaml` is refused by
/// `system_model.refused` in the same directory.
pub const MARKER_EXT: &str = "refused";

/// `<stem>.refused` beside `<stem>.yaml`.
pub fn marker_path(model: &Path) -> PathBuf {
    model.with_extension(MARKER_EXT)
}

/// `<stem>.refused-<utc-stamp>.yaml` beside `<stem>.yaml` -- where the
/// previous model goes on a refused resolve.
pub fn quarantine_path(model: &Path, stamp: &str) -> PathBuf {
    model.with_extension(format!("{MARKER_EXT}-{stamp}.yaml"))
}

/// The two lines a marker records: the check that refused, and the input the
/// producer found changed against the previous model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    pub check: String,
    pub input: String,
}

impl Marker {
    /// The marker at `path`, or `None` when there is none.
    ///
    /// A marker that exists but cannot be read is still a marker: the file
    /// says "refused" by existing, and the lines only say why.
    pub fn read(path: &Path) -> Option<Marker> {
        if !path.is_file() {
            return None;
        }
        let raw = std::fs::read_to_string(path).unwrap_or_default();
        let mut check = None;
        let mut input = None;
        for line in raw.lines() {
            if let Some(v) = line.strip_prefix("check: ") {
                check = Some(v.trim().to_string());
            } else if let Some(v) = line.strip_prefix("input: ") {
                input = Some(v.trim().to_string());
            }
        }
        Some(Marker {
            check: check.unwrap_or_else(|| "(not recorded)".to_string()),
            input: input.unwrap_or_else(|| "(not recorded)".to_string()),
        })
    }

    fn render(&self) -> String {
        format!(
            "check: {}\ninput: {}\n",
            one_line(&self.check),
            one_line(&self.input)
        )
    }
}

fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// PRODUCER side -- what `run_sync` does when the resolver refuses.
///
/// Moves the previous model (if any) to [`quarantine_path`] and writes the
/// marker. Returns where the previous model went, `None` when there was none
/// (a first resolve that was refused leaves only the marker, so a consumer
/// that would otherwise fall back to resolving on its own still refuses).
pub fn quarantine(model: &Path, check: &str, input: &str) -> std::io::Result<Option<PathBuf>> {
    let kept = if model.is_file() {
        let dest = quarantine_path(model, &utc_stamp());
        std::fs::rename(model, &dest)?;
        Some(dest)
    } else {
        None
    };
    if let Some(dir) = model.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let marker = Marker {
        check: check.to_string(),
        input: input.to_string(),
    };
    std::fs::write(marker_path(model), marker.render())?;
    Ok(kept)
}

/// PRODUCER side -- the next successful resolve removes the marker. The
/// quarantined copies stay; they are in the build tree and named by stamp.
pub fn clear(model: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(marker_path(model)) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Why a consumer may not read the model it named.
#[derive(Debug)]
pub enum Refusal {
    /// The producer last said no; the marker names why.
    Refused {
        model: PathBuf,
        marker: PathBuf,
        check: String,
        input: String,
    },
    /// The model exists but its provenance no longer holds.
    Stale { model: PathBuf, reason: String },
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::Refused {
                model,
                marker,
                check,
                input,
            } => write!(
                f,
                "SystemModel `{}` was refused by its producer and cannot be trusted \
                 (marker `{}`):\n  check: {check}\n  input: {input}\n  \
                 Fix the input and run `nros sync`; the next successful resolve \
                 removes the marker.",
                model.display(),
                marker.display(),
            ),
            Refusal::Stale { model, reason } => write!(
                f,
                "SystemModel `{}` is stale: {reason}\n  Run `nros sync` to re-resolve it.",
                model.display(),
            ),
        }
    }
}

impl std::error::Error for Refusal {}

/// CONSUMER side -- the one gate.
///
/// `model` is the path the consumer is about to open. `bringup_dir` is the
/// package the model was resolved from when the caller knows it; when it does
/// not (`--model <path>` on its own), [`infer_bringup_dir`] recovers it from
/// the two layouts `nros sync` writes, and the input hashes are checked only
/// when that succeeds. The marker and the resolver pin are checked either way,
/// the pin under [`PinPolicy::UnpinnedIsUnverifiable`].
///
/// A model that does not exist and has no marker passes: there is nothing to
/// trust or distrust, and the consumer's own missing-file handling (an error,
/// or `ensure_model`'s resolve from the inputs) stands.
pub fn verify(model: &Path, bringup_dir: Option<&Path>) -> Result<(), Refusal> {
    let marker = marker_path(model);
    if let Some(m) = Marker::read(&marker) {
        return Err(Refusal::Refused {
            model: model.to_path_buf(),
            marker,
            check: m.check,
            input: m.input,
        });
    }
    if !model.is_file() {
        return Ok(());
    }
    let bringup = bringup_dir
        .map(Path::to_path_buf)
        .or_else(|| infer_bringup_dir(model));
    if let Some(reason) =
        provenance_stale_with(model, bringup.as_deref(), PinPolicy::UnpinnedIsUnverifiable)
    {
        return Err(Refusal::Stale {
            model: model.to_path_buf(),
            reason,
        });
    }
    Ok(())
}

/// CONSUMER side, for a caller that LOCATES the model rather than naming it:
/// the same search ladder as `model_location::resolve_model_path`, with the
/// marker checked on every rung. A refused model is refused even when no rung
/// holds a file any more -- which is exactly what quarantine leaves behind.
///
/// Returns the first existing candidate, verified, or `None` when no rung
/// holds a model and none is marked refused.
pub fn verify_search(bringup_dir: &Path, model_rel: &str) -> Result<Option<PathBuf>, Refusal> {
    let candidates =
        nros_orchestration_ir::model_location::model_search_paths(bringup_dir, model_rel);
    for c in &candidates {
        let marker = marker_path(c);
        if let Some(m) = Marker::read(&marker) {
            return Err(Refusal::Refused {
                model: c.clone(),
                marker,
                check: m.check,
                input: m.input,
            });
        }
    }
    for c in &candidates {
        if c.is_file() {
            verify(c, Some(bringup_dir))?;
            return Ok(Some(c.clone()));
        }
    }
    Ok(None)
}

/// Issue 0320 -- content-addressed staleness, moved here from `cmd/ws.rs`
/// where only `run_sync` could reach it.
///
/// Returns `Some(reason)` when the recorded provenance no longer holds:
///
/// * a recorded input with a non-portable absolute path (the machine-specific
///   legacy models), one that no longer exists, or one whose sha256 changed
///   (an input the mtime gate does not watch -- a sibling include, the
///   `--sched` platform file);
/// * issue 0427 -- a resolver pin that is not ours: a resolver fix changes the
///   OUTPUT for byte-identical inputs. nano-ros stamps `meta.resolver.version`
///   with `NROS_PLAY_LAUNCH_SHA` at resolve time (`stamp_resolver_pin`)
///   because the resolver's self-version is unreliable. Skipped when our own
///   pin is unverifiable, matching `verify_resolver_pin`;
/// * phase-460 W1 -- the SECOND hash input: a file the bringup's launch tree
///   names today (the launch file, its includes, their contract sidecars,
///   `system.toml`) that the model never recorded. The recorded set is the
///   input set of the resolve that succeeded; a sidecar added since is an
///   input the resolver has not seen (issue 1121 made visible, not fixed).
///
/// `None` means the provenance is intact. A model that cannot be parsed
/// returns `None`, so `run_sync` falls back to its mtime gate rather than
/// force-churning, and a consumer fails on its own parse with its own message.
///
/// Input checks need `bringup_dir` (recorded paths resolve against the
/// package root, matching how the resolver strips the launch file's
/// grandparent and how `main_macro` re-joins them); the pin check does not.
///
/// This spelling asks `run_sync`'s question; a consumer door asks
/// [`PinPolicy::UnpinnedIsUnverifiable`] instead, via [`provenance_stale_with`].
pub fn provenance_stale(model_path: &Path, bringup_dir: Option<&Path>) -> Option<String> {
    provenance_stale_with(model_path, bringup_dir, PinPolicy::Required)
}

/// What an ABSENT `meta.resolver` means. A pin that DISAGREES with ours is
/// stale under both policies; this decides only the missing case.
///
/// The two callers ask different questions of the same model.
///
/// * `run_sync` asks "should I re-resolve?". A pinless model is worth one
///   cheap resolve — that is issue 0427's rule, and it is how a model written
///   before the pin existed acquires one.
/// * A CONSUMER door (phase-460 W1) asks "may I trust what is here?". There,
///   absence of a pin is absence of evidence rather than evidence of
///   staleness, and the check is already asymmetric if it says otherwise: it
///   skips itself ENTIRELY when OUR OWN pin is `unknown` (an uninitialised
///   `play_launch`, which is every worktree that has not run
///   `git submodule update --init`). Refusing the model for the mirror-image
///   gap would make `nros model-path` reject a hand-authored or committed
///   model on the machines where the same binary cannot check the pin at all.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PinPolicy {
    /// No `meta.resolver` is staleness — `nros sync` re-resolves and stamps one.
    Required,
    /// No `meta.resolver` is unverifiable, exactly like our own `unknown` pin.
    UnpinnedIsUnverifiable,
}

/// [`provenance_stale`] with the pin policy named. See [`PinPolicy`].
pub fn provenance_stale_with(
    model_path: &Path,
    bringup_dir: Option<&Path>,
    pin_policy: PinPolicy,
) -> Option<String> {
    let raw = std::fs::read_to_string(model_path).ok()?;
    let model = ros_launch_manifest_model::SystemModel::from_yaml_str(&raw).ok()?;
    if let Some(bringup_dir) = bringup_dir {
        for input in &model.meta.inputs {
            let recorded = Path::new(&input.path);
            if recorded.is_absolute() {
                return Some(format!("non-portable absolute input path `{}`", input.path));
            }
            let resolved = bringup_dir.join(recorded);
            let Ok(bytes) = std::fs::read(&resolved) else {
                return Some(format!("recorded input missing `{}`", input.path));
            };
            let digest = format!("{:x}", Sha256::digest(&bytes));
            if digest != input.sha256 {
                return Some(format!("input hash changed `{}`", input.path));
            }
        }
    }
    let ours = env!("NROS_PLAY_LAUNCH_SHA");
    if ours != "unknown" {
        match model.meta.resolver.as_ref().map(|r| r.version.as_str()) {
            Some(v) if v == ours => {}
            Some(v) => {
                return Some(format!(
                    "resolver pin changed (model `{}` != ours `{}`)",
                    &v[..v.len().min(12)],
                    &ours[..ours.len().min(12)]
                ));
            }
            None => match pin_policy {
                PinPolicy::Required => return Some("no resolver pin recorded".into()),
                PinPolicy::UnpinnedIsUnverifiable => {}
            },
        }
    }
    if let Some(bringup_dir) = bringup_dir
        && let Some(reason) = launch_tree_unrecorded(&model, model_path, bringup_dir)
    {
        return Some(reason);
    }
    None
}

/// The second hash input: a file the launch tree names that `meta.inputs`
/// does not record. See [`provenance_stale`].
fn launch_tree_unrecorded(
    model: &ros_launch_manifest_model::SystemModel,
    model_path: &Path,
    bringup_dir: &Path,
) -> Option<String> {
    let name = model_path.file_name()?.to_str()?;
    let (launch_file, _args) = nros_orchestration_ir::model_location::model_rel_to_inputs(
        bringup_dir,
        &format!("config/{name}"),
    )?;
    let launch_dir = bringup_dir.join("launch");
    let launch = launch_dir.join(&launch_file);
    if !launch.is_file() {
        // A cargo leaf's launch file is SYNTHESISED under `build/`; its
        // inputs are the recorded ones and nothing else names them.
        return None;
    }
    if model.meta.inputs.is_empty() {
        // An EMPTY recorded set is not an incomplete one. This rule compares
        // what the launch tree names today against what the resolve recorded,
        // and a model that recorded nothing -- a hand-authored or committed
        // one, which R-code.1 asks a toml-declaring bringup to carry -- offers
        // nothing to compare: every file the tree names is "not recorded", so
        // the reason would be the first name in an arbitrary order rather than
        // a fact about the tree. Issue 1121's case (a sidecar added since a
        // real resolve) always has a non-empty recorded set. Same reading as
        // `PinPolicy::UnpinnedIsUnverifiable`: absence of evidence is not
        // evidence of staleness.
        return None;
    }
    let recorded: Vec<PathBuf> = model
        .meta
        .inputs
        .iter()
        .filter_map(|i| std::fs::canonicalize(bringup_dir.join(&i.path)).ok())
        .collect();
    let mut named: Vec<PathBuf> = Vec::new();
    let mut xmls = vec![launch.clone()];
    for inc in launch_include_names(&launch) {
        let p = launch_dir.join(inc);
        if p.is_file() {
            xmls.push(p);
        }
    }
    for xml in xmls {
        if let Some(stem) = xml
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix(".launch.xml"))
        {
            let sidecar = launch_dir.join(format!("{stem}.contract.yaml"));
            if sidecar.is_file() {
                named.push(sidecar);
            }
        }
        named.push(xml);
    }
    let system_toml = bringup_dir.join("system.toml");
    if system_toml.is_file() {
        named.push(system_toml);
    }
    for file in named {
        let Ok(canon) = std::fs::canonicalize(&file) else {
            continue;
        };
        if !recorded.contains(&canon) {
            let rel = file
                .strip_prefix(bringup_dir)
                .unwrap_or(&file)
                .display()
                .to_string();
            return Some(format!(
                "launch-tree input not recorded `{rel}` (the model predates it)"
            ));
        }
    }
    None
}

/// phase-330 W4.0 -- file names referenced by `<include file="...">` in a
/// launch file.
///
/// A targeted scan, not a full parse: `parse_launch_file` resolves
/// substitutions and needs a `PkgIndex`, and all this decision needs is "is
/// this launch file pulled in by another one". Only the file NAME is compared,
/// so a `$(find-pkg-share ...)` prefix does not defeat it. Lives here because
/// the gate's launch-tree walk and `run_sync`'s include bookkeeping are the
/// same question.
pub fn launch_include_names(path: &Path) -> Vec<String> {
    let Ok(raw) = std::fs::read(path) else {
        return Vec::new();
    };
    let mut reader = quick_xml::Reader::from_reader(raw.as_slice());
    let mut buf = Vec::new();
    let mut out = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(quick_xml::events::Event::Eof) | Err(_) => break,
            Ok(quick_xml::events::Event::Start(e) | quick_xml::events::Event::Empty(e))
                if e.name().as_ref() == b"include" =>
            {
                for attr in e.attributes().flatten() {
                    if attr.key.as_ref() == b"file"
                        && let Ok(v) = attr.unescape_value()
                        && let Some(n) = Path::new(v.as_ref()).file_name().and_then(|s| s.to_str())
                    {
                        out.push(n.to_string());
                    }
                }
            }
            _ => {}
        }
        buf.clear();
    }
    out
}

/// The bringup package a model was resolved from, recovered from where
/// `nros sync` put the model -- for a consumer handed only `--model <path>`.
///
/// Two layouts are known: the retired committed location
/// `<bringup>/config/<model>`, and the build location
/// `<root>/build/nros/models/<bringup-dir-name>/<model>` where `<root>` is
/// the workspace (bringup under `src/<name>` or `<name>`) or the bringup
/// itself (a standalone self-bringup, where `nros sync` runs inside it).
/// `$OUT_DIR/nros/<hash>-<name>/` carries no path back and yields `None`;
/// the caller then checks the marker and the pin but not the hashes.
pub fn infer_bringup_dir(model: &Path) -> Option<PathBuf> {
    let is_bringup = |p: &Path| p.join("system.toml").is_file() || p.join("launch").is_dir();
    let dir = model.parent()?;
    if dir.file_name().and_then(|n| n.to_str()) == Some("config") {
        let b = dir.parent()?;
        return is_bringup(b).then(|| b.to_path_buf());
    }
    let name = dir.file_name()?;
    let models = dir.parent()?;
    let nros = models.parent()?;
    let build = nros.parent()?;
    if models.file_name().and_then(|n| n.to_str()) != Some("models")
        || nros.file_name().and_then(|n| n.to_str()) != Some("nros")
        || build.file_name().and_then(|n| n.to_str()) != Some("build")
    {
        return None;
    }
    let root = build.parent()?;
    let mut candidates = vec![root.join("src").join(name), root.join(name)];
    if root.file_name() == Some(name) {
        candidates.push(root.to_path_buf());
    }
    candidates.into_iter().find(|c| is_bringup(c))
}

/// `YYYYMMDDTHHMMSSZ` from the system clock; no date crate for one stamp.
fn utc_stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}{mo:02}{d:02}T{h:02}{m:02}{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(bytes: &[u8]) -> String {
        format!("{:x}", Sha256::digest(bytes))
    }

    fn write_model(dir: &Path, inputs: Vec<(String, String)>) -> PathBuf {
        write_model_with_pin(dir, inputs, env!("NROS_PLAY_LAUNCH_SHA"))
    }

    fn write_model_with_pin(dir: &Path, inputs: Vec<(String, String)>, pin: &str) -> PathBuf {
        let mut m = ros_launch_manifest_model::SystemModel::default();
        m.meta.version = ros_launch_manifest_model::SCHEMA_VERSION;
        m.meta.inputs = inputs
            .into_iter()
            .map(|(path, sha256)| ros_launch_manifest_model::InputHash { path, sha256 })
            .collect();
        // Issue 0427 -- stamp the resolver pin the same way `stamp_resolver_pin` does.
        m.meta.resolver = Some(ros_launch_manifest_model::ResolverInfo {
            tool: "nros-launch-resolve".into(),
            version: pin.into(),
        });
        let p = dir.join("system_model.yaml");
        std::fs::write(&p, serde_yaml_ng::to_string(&m).unwrap()).unwrap();
        p
    }

    // Issue 0320 -- content-addressed staleness. These moved with the function
    // from `cmd/ws.rs`; the bringup has no `launch/`, so only the recorded set
    // is checked.

    #[test]
    fn intact_provenance_is_not_stale() {
        let tmp = tempfile::tempdir().unwrap();
        let bringup = tmp.path();
        let content = b"[system]\n";
        std::fs::write(bringup.join("system.toml"), content).unwrap();
        let model = write_model(bringup, vec![("system.toml".into(), sha(content))]);
        assert_eq!(provenance_stale(&model, Some(bringup)), None);
        assert!(verify(&model, Some(bringup)).is_ok());
    }

    #[test]
    fn changed_hash_is_stale() {
        let tmp = tempfile::tempdir().unwrap();
        let bringup = tmp.path();
        std::fs::write(bringup.join("system.toml"), b"new\n").unwrap();
        let model = write_model(bringup, vec![("system.toml".into(), sha(b"old\n"))]);
        assert!(
            provenance_stale(&model, Some(bringup))
                .unwrap()
                .contains("hash changed")
        );
        let err = verify(&model, Some(bringup)).unwrap_err().to_string();
        assert!(
            err.contains("is stale") && err.contains("system.toml"),
            "{err}"
        );
    }

    /// The 43 legacy models: an absolute path is non-portable and must
    /// regenerate even when the file it points at still exists and matches.
    #[test]
    fn absolute_path_is_stale_even_when_file_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let bringup = tmp.path();
        let abs = bringup.join("system.toml");
        std::fs::write(&abs, b"x\n").unwrap();
        let model = write_model(bringup, vec![(abs.display().to_string(), sha(b"x\n"))]);
        assert!(
            provenance_stale(&model, Some(bringup))
                .unwrap()
                .contains("absolute")
        );
    }

    #[test]
    fn missing_input_is_stale() {
        let tmp = tempfile::tempdir().unwrap();
        let bringup = tmp.path();
        let model = write_model(bringup, vec![("gone.toml".into(), sha(b"x"))]);
        assert!(
            provenance_stale(&model, Some(bringup))
                .unwrap()
                .contains("missing")
        );
    }

    /// Issue 0427 -- a model whose inputs are byte-identical but was produced by a
    /// DIFFERENT resolver pin is stale, so a resolver fix reaches existing models.
    /// The pin is checked with NO bringup dir too: it is the one input every
    /// consumer can verify from the model alone.
    #[test]
    fn resolver_pin_change_is_stale() {
        // Skip when our own pin is unverifiable -- the check itself is disabled then.
        if env!("NROS_PLAY_LAUNCH_SHA") == "unknown" {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let bringup = tmp.path();
        let content = b"[system]\n";
        std::fs::write(bringup.join("system.toml"), content).unwrap();
        let model = write_model_with_pin(
            bringup,
            vec![("system.toml".into(), sha(content))],
            "deadbeefdeadbeef",
        );
        for dir in [Some(bringup), None] {
            assert!(
                provenance_stale(&model, dir)
                    .unwrap()
                    .contains("resolver pin changed"),
                "a model with a foreign resolver pin must be stale (bringup {dir:?})"
            );
        }
    }

    /// Issue 0427 -- a model with NO recorded resolver pin (pre-fix / legacy) is
    /// stale, so it re-resolves and gains the pin.
    #[test]
    fn missing_resolver_pin_is_stale() {
        if env!("NROS_PLAY_LAUNCH_SHA") == "unknown" {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let bringup = tmp.path();
        let content = b"[system]\n";
        std::fs::write(bringup.join("system.toml"), content).unwrap();
        let mut m = ros_launch_manifest_model::SystemModel::default();
        m.meta.version = ros_launch_manifest_model::SCHEMA_VERSION;
        m.meta.inputs = vec![ros_launch_manifest_model::InputHash {
            path: "system.toml".into(),
            sha256: sha(content),
        }];
        let model = bringup.join("system_model.yaml");
        std::fs::write(&model, serde_yaml_ng::to_string(&m).unwrap()).unwrap();
        assert!(
            provenance_stale(&model, Some(bringup))
                .unwrap()
                .contains("no resolver pin"),
            "a model with no resolver pin must be stale"
        );
    }

    /// phase-460 W1 -- the launch-tree rule says nothing about a model that
    /// recorded NO inputs: every file the tree names is then "not recorded",
    /// so the reason would name whichever came first rather than a fact about
    /// the tree. The sidecar case above is the rule doing its job, and it
    /// keeps a non-empty recorded set.
    #[test]
    fn an_empty_recorded_input_set_is_not_an_incomplete_one() {
        let tmp = tempfile::tempdir().unwrap();
        let bringup = tmp.path();
        std::fs::create_dir_all(bringup.join("launch")).unwrap();
        std::fs::write(bringup.join("system.toml"), b"[system]\n").unwrap();
        std::fs::write(bringup.join("launch/system.launch.xml"), b"<launch/>\n").unwrap();
        std::fs::write(bringup.join("launch/system.contract.yaml"), b"version: 1\n").unwrap();
        let model = write_model_with_pin(bringup, vec![], env!("NROS_PLAY_LAUNCH_SHA"));
        assert_eq!(provenance_stale(&model, Some(bringup)), None);
    }

    /// phase-460 W1 -- the same pinless model a CONSUMER door meets passes,
    /// because that door cannot fix what it refuses. `nros sync` re-resolves
    /// and stamps the pin (the test above); `nros model-path` would only leave
    /// cmake with no model at all. The asymmetry this removes is the check's
    /// own: it skips entirely when OUR pin is `unknown`, so the model-side gap
    /// cannot be the fatal one.
    #[test]
    fn a_consumer_door_accepts_a_model_with_no_resolver_pin() {
        if env!("NROS_PLAY_LAUNCH_SHA") == "unknown" {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let bringup = tmp.path();
        let content = b"[system]\n";
        std::fs::write(bringup.join("system.toml"), content).unwrap();
        let mut m = ros_launch_manifest_model::SystemModel::default();
        m.meta.version = ros_launch_manifest_model::SCHEMA_VERSION;
        m.meta.inputs = vec![ros_launch_manifest_model::InputHash {
            path: "system.toml".into(),
            sha256: sha(content),
        }];
        let model = bringup.join("system_model.yaml");
        std::fs::write(&model, serde_yaml_ng::to_string(&m).unwrap()).unwrap();
        assert!(
            provenance_stale(&model, Some(bringup)).is_some(),
            "run_sync must still re-resolve a pinless model"
        );
        assert!(
            verify(&model, Some(bringup)).is_ok(),
            "a consumer door must accept a pinless model: {:?}",
            verify(&model, Some(bringup))
        );
    }

    /// ...and a pin that DISAGREES is still refused at the consumer door --
    /// the policy decides the missing case only.
    #[test]
    fn a_consumer_door_still_refuses_a_foreign_resolver_pin() {
        if env!("NROS_PLAY_LAUNCH_SHA") == "unknown" {
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        let bringup = tmp.path();
        let content = b"[system]\n";
        std::fs::write(bringup.join("system.toml"), content).unwrap();
        let model = write_model_with_pin(
            bringup,
            vec![("system.toml".into(), sha(content))],
            "deadbeefdeadbeef",
        );
        assert!(matches!(
            verify(&model, Some(bringup)),
            Err(Refusal::Stale { .. })
        ));
    }

    // phase-460 W1 -- the marker and the quarantine.

    #[test]
    fn marker_refuses_even_when_the_model_is_absent_and_inputs_are_intact() {
        let tmp = tempfile::tempdir().unwrap();
        let bringup = tmp.path();
        let content = b"[system]\n";
        std::fs::write(bringup.join("system.toml"), content).unwrap();
        let model = write_model(bringup, vec![("system.toml".into(), sha(content))]);
        let kept = quarantine(
            &model,
            "rate-hierarchy: 2 contract error(s)",
            "launch/x.contract.yaml",
        )
        .unwrap()
        .expect("the previous model is kept");
        assert!(
            !model.exists(),
            "the path a consumer opens no longer exists"
        );
        assert!(kept.is_file() && kept.to_string_lossy().contains(".refused-"));
        let err = verify(&model, Some(bringup)).unwrap_err();
        let text = err.to_string();
        assert!(text.contains("refused by its producer"), "{text}");
        assert!(text.contains("system_model.refused"), "{text}");
        assert!(text.contains("rate-hierarchy"), "{text}");
        assert!(text.contains("launch/x.contract.yaml"), "{text}");
        // A caller that never names the file finds the marker on the ladder.
        let err = verify_search(bringup, "system_model.yaml").unwrap_err();
        assert!(matches!(err, Refusal::Refused { .. }));
        // The next successful sync clears it; the copy stays for diffing.
        clear(&model).unwrap();
        assert!(verify(&model, Some(bringup)).is_ok());
        assert!(kept.is_file());
    }

    #[test]
    fn a_first_refusal_with_no_previous_model_still_marks() {
        let tmp = tempfile::tempdir().unwrap();
        let model = tmp
            .path()
            .join("models")
            .join("b")
            .join("system_model.yaml");
        assert_eq!(quarantine(&model, "c", "i").unwrap(), None);
        assert!(marker_path(&model).is_file());
        assert!(verify(&model, None).is_err());
    }

    #[test]
    fn an_absent_model_with_no_marker_is_not_the_gates_business() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(verify(&tmp.path().join("system_model.yaml"), None).is_ok());
        assert_eq!(
            verify_search(tmp.path(), "config/system_model.yaml").unwrap(),
            None
        );
    }

    #[test]
    fn a_sidecar_the_model_never_recorded_is_stale() {
        let tmp = tempfile::tempdir().unwrap();
        let bringup = tmp.path();
        std::fs::create_dir_all(bringup.join("launch")).unwrap();
        let toml = b"[system]\n";
        let xml = b"<launch/>\n";
        std::fs::write(bringup.join("system.toml"), toml).unwrap();
        std::fs::write(bringup.join("launch/system.launch.xml"), xml).unwrap();
        let model = write_model(
            bringup,
            vec![
                ("system.toml".into(), sha(toml)),
                ("launch/system.launch.xml".into(), sha(xml)),
            ],
        );
        assert_eq!(provenance_stale(&model, Some(bringup)), None);
        std::fs::write(bringup.join("launch/system.contract.yaml"), b"version: 1\n").unwrap();
        let why = provenance_stale(&model, Some(bringup)).unwrap();
        assert!(
            why.contains("not recorded") && why.contains("system.contract.yaml"),
            "{why}"
        );
    }

    #[test]
    fn bringup_dir_is_recovered_from_both_layouts() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path();
        let bringup = ws.join("src").join("demo_bringup");
        std::fs::create_dir_all(bringup.join("launch")).unwrap();
        let built = ws.join("build/nros/models/demo_bringup/system_model.yaml");
        assert_eq!(infer_bringup_dir(&built), Some(bringup.clone()));
        assert_eq!(
            infer_bringup_dir(&bringup.join("config/system_model.yaml")),
            Some(bringup.clone())
        );
        // A standalone self-bringup: the root IS the bringup.
        let solo = ws.join("solo");
        std::fs::create_dir_all(solo.join("launch")).unwrap();
        assert_eq!(
            infer_bringup_dir(&solo.join("build/nros/models/solo/system_model.yaml")),
            Some(solo)
        );
        assert_eq!(
            infer_bringup_dir(&ws.join("elsewhere/system_model.yaml")),
            None
        );
    }

    #[test]
    fn utc_stamp_is_a_sortable_civil_date() {
        let s = utc_stamp();
        assert_eq!(s.len(), 16, "{s}");
        assert!(
            s.starts_with("20") && s.ends_with('Z') && s.as_bytes()[8] == b'T',
            "{s}"
        );
    }
}
