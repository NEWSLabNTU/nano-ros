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
