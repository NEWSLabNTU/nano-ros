//! phase-330 W3.b (RFC-0063) — where a consumer looks for the SystemModel.
//!
//! The model is becoming a BUILD ARTIFACT. Until W4 deletes the committed
//! copies, both locations exist, so every consumer needs the same search order
//! — and there are three consumers that each derived the path independently:
//!
//!   * `nros-macros` — `pkg_index.resolve_pkg(bringup).join("config/system_model.yaml")`
//!   * `nros-build`  — `bringup_dir.join("config/system_model.yaml")`
//!   * `cmake/NanoRosEntry.cmake` — a `MODEL <path>` argument with the same default
//!
//! Three copies of a path policy is how the two `TierRtosSpec` mirrors drifted
//! earlier in this phase, so the policy lives here once and the consumers call
//! it.
//!
//! # The order, and why
//!
//! 1. `$NROS_MODEL_DIR` — a SHARED build root. RFC-0065's builder sets this
//!    when several entry crates use one bringup and should share a single
//!    regeneration. Highest priority because it is an explicit instruction.
//! 2. `$OUT_DIR/nros/` — the per-crate cargo build output. This is the W3.a
//!    decision's default for the cargo path: it exists for a standalone
//!    copy-out example exactly as it does inside a workspace, which is what
//!    lets W3.c need no special case.
//! 3. `<bringup>/<model_rel>` — the committed source copy. The legacy location,
//!    still authoritative until W4.
//!
//! The fallback is what makes this landable on its own: with nothing
//! generating into (1) or (2), every consumer resolves exactly as before.

use std::path::{Path, PathBuf};

/// Candidate model paths for `bringup_dir`/`model_rel`, most-preferred first.
///
/// `model_rel` is the bringup-relative path (`config/system_model.yaml`, or a
/// variant like `config/talker_model.yaml`). Build-output candidates use only
/// its FILE NAME: a build directory is already scoped to one build, so
/// re-nesting `config/` there would add a level that means nothing.
pub fn model_search_paths(bringup_dir: &Path, model_rel: &str) -> Vec<PathBuf> {
    let name = Path::new(model_rel)
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("system_model.yaml"));
    // The bringup's own directory name namespaces the build-output copies. A
    // flat layout collides whenever a workspace has two bringups (both emit
    // `system_model.yaml`), and the loser vanishes silently.
    let bringup = bringup_dir.file_name().map(PathBuf::from);
    let mut out = Vec::new();
    if let Some(dir) = std::env::var_os("NROS_MODEL_DIR") {
        let dir = Path::new(&dir);
        if let Some(b) = &bringup {
            out.push(dir.join(b).join(&name));
        }
        out.push(dir.join(&name));
    }
    if let Some(dir) = std::env::var_os("OUT_DIR") {
        let dir = Path::new(&dir).join("nros");
        if let Some(b) = &bringup {
            out.push(dir.join(b).join(&name));
        }
        out.push(dir.join(&name));
    }
    // phase-330 W4/W7 — the WORKSPACE build root, where `nros sync` writes by
    // default once the committed copies are gone: `<ws>/build/nros/models/
    // <bringup>/<name>`. Derived from the conventional `<ws>/src/<bringup>`
    // layout (bringup_dir/../../build) so NO env wiring is needed for the
    // in-tree flows (west entry crates have neither OUT_DIR nor
    // NROS_MODEL_DIR at macro expansion). A bringup outside that layout
    // simply contributes no candidate here.
    if let (Some(b), Some(ws_root)) = (&bringup, bringup_dir.parent().and_then(|src| src.parent()))
    {
        out.push(
            ws_root
                .join("build")
                .join("nros")
                .join("models")
                .join(b)
                .join(&name),
        );
    }
    // Standalone copy-out / single-pkg self-bringups (W3.c): the bringup dir
    // IS the workspace root (`nros sync` run inside it writes
    // `<bringup>/build/nros/models/<bringup>/…`), so the ws-layout rung above
    // misses by two levels. Same namespacing, rooted at the bringup itself.
    if let Some(b) = &bringup {
        out.push(
            bringup_dir
                .join("build")
                .join("nros")
                .join("models")
                .join(b)
                .join(&name),
        );
    }
    out.push(bringup_dir.join(model_rel));
    out
}

/// The model path a consumer should read.
///
/// Returns the first candidate that exists. When none does, returns the
/// COMMITTED location rather than a build-output one, so the "not found" error
/// a user sees names the file they can actually create.
pub fn resolve_model_path(bringup_dir: &Path, model_rel: &str) -> PathBuf {
    let candidates = model_search_paths(bringup_dir, model_rel);
    for c in &candidates {
        if c.is_file() {
            return c.clone();
        }
    }
    candidates
        .last()
        .cloned()
        .unwrap_or_else(|| bringup_dir.join(model_rel))
}

/// phase-330 W7 — map the INPUT coordinates of a system to the model's
/// bringup-relative path.
///
/// The user-facing input is `(bringup, launch file, launch args)`; the model
/// is a build artifact nobody names. This is the ONE mapping from the former
/// to the latter, shared by `nros::main!(launch = …)`, `nros-build` and (via
/// the CLI) `nano_ros_entry(LAUNCH …)` — the same one-home rule as
/// [`model_search_paths`] above.
///
/// Rules (phase-330 W4.0 derive-plus-declare):
/// 1. `args` non-empty → a `[[model]]` declaration in `system.toml` matching
///    `(launch, args)` MUST exist (declarations are the SSoT for which
///    bindings exist); its `out` names the model.
/// 2. A declaration matching `(launch, no args)` also wins when present.
/// 3. No declaration: the DEFAULT launch (explicit `[system] default_launch`,
///    or the conventional `system.launch.xml`) maps to
///    `config/system_model.yaml`; any other launch maps to
///    `config/<stem>_model.yaml` (stem = file name minus `.launch.xml`).
///
/// Returns the bringup-relative path (`config/<name>`), which
/// [`resolve_model_path`] then locates (build dir first, committed fallback
/// until W4.a deletes it). Errors are strings naming what the USER must fix.
pub fn launch_to_model_rel(
    bringup_dir: &Path,
    launch_file: Option<&str>,
    args: &[(String, String)],
) -> Result<String, String> {
    let system_toml = bringup_dir.join("system.toml");
    let doc: Option<toml::Table> = std::fs::read_to_string(&system_toml)
        .ok()
        .and_then(|raw| raw.parse::<toml::Table>().ok());

    let default_launch = doc
        .as_ref()
        .and_then(|d| d.get("system"))
        .and_then(|s| s.get("default_launch"))
        .and_then(|v| v.as_str())
        .unwrap_or("system.launch.xml")
        .to_string();
    let launch = launch_file.unwrap_or(&default_launch);

    // [[model]] declarations for this launch file.
    let decls: Vec<(Vec<(String, String)>, String)> = doc
        .as_ref()
        .and_then(|d| d.get("model"))
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    let decl_launch = e.get("launch")?.as_str()?;
                    if decl_launch != launch {
                        return None;
                    }
                    let out = e.get("out")?.as_str()?.to_string();
                    let mut decl_args: Vec<(String, String)> = e
                        .get("args")
                        .and_then(|a| a.as_table())
                        .map(|t| {
                            t.iter()
                                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                                .collect()
                        })
                        .unwrap_or_default();
                    decl_args.sort();
                    Some((decl_args, out))
                })
                .collect()
        })
        .unwrap_or_default();

    let mut want: Vec<(String, String)> = args.to_vec();
    want.sort();
    if let Some((_, out)) = decls.iter().find(|(a, _)| *a == want) {
        let out_rel = if out.contains('/') {
            out.clone()
        } else {
            format!("config/{out}")
        };
        return Ok(out_rel);
    }
    if !want.is_empty() {
        let known: Vec<String> = decls
            .iter()
            .map(|(a, out)| format!("{out} (args {a:?})"))
            .collect();
        return Err(format!(
            "no `[[model]]` declaration in `{}` matches launch `{launch}` with args {want:?} — \
             binding variants must be DECLARED (phase-330 W4.0). Declared for this launch: [{}]",
            system_toml.display(),
            known.join(", "),
        ));
    }

    // Derive rule.
    if launch == default_launch {
        return Ok("config/system_model.yaml".to_string());
    }
    let stem = launch
        .strip_suffix(".launch.xml")
        .or_else(|| launch.strip_suffix(".xml"))
        .unwrap_or(launch);
    let stem = stem.rsplit('/').next().unwrap_or(stem);
    Ok(format!("config/{stem}_model.yaml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Env is process-global, so these run under one lock and restore what they
    // set — a leaked NROS_MODEL_DIR would silently reorder every later test.
    fn with_env<T>(pairs: &[(&str, Option<&str>)], f: impl FnOnce() -> T) -> T {
        use std::sync::{Mutex, OnceLock};
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _g = LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let saved: Vec<(String, Option<std::ffi::OsString>)> = pairs
            .iter()
            .map(|(k, _)| ((*k).to_string(), std::env::var_os(k)))
            .collect();
        for (k, v) in pairs {
            match v {
                Some(v) => unsafe { std::env::set_var(k, v) },
                None => unsafe { std::env::remove_var(k) },
            }
        }
        let out = f();
        for (k, v) in saved {
            match v {
                Some(v) => unsafe { std::env::set_var(&k, v) },
                None => unsafe { std::env::remove_var(&k) },
            }
        }
        out
    }

    #[test]
    fn committed_location_is_the_fallback() {
        with_env(&[("NROS_MODEL_DIR", None), ("OUT_DIR", None)], || {
            let p = resolve_model_path(
                Path::new("/ws/src/demo_bringup"),
                "config/system_model.yaml",
            );
            assert_eq!(
                p,
                PathBuf::from("/ws/src/demo_bringup/config/system_model.yaml")
            );
        });
    }

    #[test]
    fn build_output_outranks_committed_and_drops_the_config_level() {
        with_env(
            &[("NROS_MODEL_DIR", None), ("OUT_DIR", Some("/build/out"))],
            || {
                let c = model_search_paths(
                    Path::new("/ws/src/demo_bringup"),
                    "config/system_model.yaml",
                );
                assert_eq!(
                    c[0],
                    PathBuf::from("/build/out/nros/demo_bringup/system_model.yaml")
                );
                assert_eq!(c[1], PathBuf::from("/build/out/nros/system_model.yaml"));
                assert_eq!(
                    c[2],
                    PathBuf::from("/ws/src/demo_bringup/config/system_model.yaml")
                );
            },
        );
    }

    #[test]
    fn shared_model_dir_outranks_out_dir() {
        with_env(
            &[
                ("NROS_MODEL_DIR", Some("/build/nros")),
                ("OUT_DIR", Some("/build/out")),
            ],
            || {
                let c = model_search_paths(Path::new("/ws/b"), "config/system_model.yaml");
                assert_eq!(c[0], PathBuf::from("/build/nros/b/system_model.yaml"));
                assert_eq!(c[1], PathBuf::from("/build/nros/system_model.yaml"));
            },
        );
    }

    #[test]
    fn variant_models_keep_their_own_name() {
        with_env(&[("NROS_MODEL_DIR", Some("/b")), ("OUT_DIR", None)], || {
            let c = model_search_paths(Path::new("/ws/b"), "config/talker_model.yaml");
            assert_eq!(c[0], PathBuf::from("/b/b/talker_model.yaml"));
        });
    }

    #[test]
    fn missing_everywhere_reports_the_committed_path() {
        with_env(
            &[
                ("NROS_MODEL_DIR", Some("/nope")),
                ("OUT_DIR", Some("/nope2")),
            ],
            || {
                let p = resolve_model_path(Path::new("/ws/b"), "config/system_model.yaml");
                assert_eq!(p, PathBuf::from("/ws/b/config/system_model.yaml"));
            },
        );
    }
}
