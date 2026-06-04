//! Phase 222.B.4 — integration tests for the `nros build` / `run` /
//! `deploy` / `monitor` deprecation surface.
//!
//! Three assertions per verb (matrix-driven across all four):
//! 1. `nros <verb> --help` stdout includes the `(deprecated` suffix
//!    inserted by 222.B.1.
//! 2. A no-op invocation (--help is fine here too — the warning fires
//!    regardless of whether the underlying tool actually runs, since
//!    222.B.2 emits before any dispatch) prints the
//!    `nros <verb> is deprecated` + `nros 0.5.0` warning on stderr,
//!    AND points users at `NROS_SUPPRESS_DEPRECATION=1`.
//!    NB: we drive `nros <verb>` against a synthetic tempdir so the
//!    verb's body reaches the warning point WITHOUT us needing to
//!    succeed on the wrapper itself. The wrapper's exit code is
//!    irrelevant — the deprecation signal is on stderr regardless.
//! 3. With `NROS_SUPPRESS_DEPRECATION=1` set, the warning text is
//!    absent from stderr.
//!
//! The test crate already builds the `nros` binary (`nros-cli` ships
//! `[[bin]] name = "nros"`), so `env!("CARGO_BIN_EXE_nros")` resolves.

use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn nros_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_nros"))
}

fn temp_root(tag: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("phase-222-b-{tag}-{}-{stamp}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// 222.B.1 — every deprecated verb's `--help` carries the `(deprecated`
/// suffix announced in the about/doc line.
#[test]
fn help_shows_deprecation_suffix() {
    for verb in ["build", "run", "deploy", "monitor"] {
        let output = Command::new(nros_bin())
            .arg(verb)
            .arg("--help")
            .env_remove("NROS_SUPPRESS_DEPRECATION")
            .output()
            .expect("spawn nros");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let combined = format!("{stdout}\n{stderr}");
        assert!(
            output.status.success(),
            "`nros {verb} --help` should exit 0\nstdout: {stdout}\nstderr: {stderr}",
        );
        assert!(
            combined.contains("(deprecated"),
            "expected `(deprecated` in --help for `nros {verb}`.\n\
             stdout: {stdout}\nstderr: {stderr}",
        );
        assert!(
            combined.contains("0.5.0"),
            "expected `0.5.0` removal-target in --help for `nros {verb}`.\n\
             stdout: {stdout}\nstderr: {stderr}",
        );
    }
}

/// 222.B.2 — verb invocation (with whatever minimum args drive it past
/// clap into the `run()` body) emits the deprecation warning on stderr.
///
/// `nros build` and `nros run` walk the project root, so we point them
/// at an empty tempdir. They'll fail (no Cargo/CMake/Zephyr manifest)
/// — but the warning has fired by then. The test asserts stderr, not
/// exit code.
///
/// `nros deploy` requires a `--config nros.toml`; we feed it a missing
/// path so it errors fast — again, the warning has already printed.
///
/// `nros monitor` shells out to `espflash`; we don't have it installed
/// in CI, so it errors out at the spawn. Warning prints first.
#[test]
fn warning_fires_on_invocation() {
    let root = temp_root("warn_fires");

    for (verb, extra_args) in [
        (
            "build",
            vec!["--project".to_string(), root.display().to_string()],
        ),
        (
            "run",
            vec!["--project".to_string(), root.display().to_string()],
        ),
        (
            "deploy",
            vec![
                "--config".to_string(),
                root.join("nros.toml").display().to_string(),
            ],
        ),
        ("monitor", vec![]),
    ] {
        let output = Command::new(nros_bin())
            .arg(verb)
            .args(&extra_args)
            .env_remove("NROS_SUPPRESS_DEPRECATION")
            .output()
            .unwrap_or_else(|e| panic!("spawn nros {verb}: {e}"));
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(
            stderr.contains(&format!("`nros {verb}` is deprecated")),
            "expected deprecation warning on stderr for `nros {verb}`.\n\
             stderr: {stderr}",
        );
        assert!(
            stderr.contains("nros 0.5.0"),
            "expected `nros 0.5.0` removal target on stderr for `nros {verb}`.\n\
             stderr: {stderr}",
        );
        assert!(
            stderr.contains("NROS_SUPPRESS_DEPRECATION"),
            "expected `NROS_SUPPRESS_DEPRECATION` opt-out hint on stderr for `nros {verb}`.\n\
             stderr: {stderr}",
        );
    }

    let _ = fs::remove_dir_all(&root);
}

/// 222.B.2 — `NROS_SUPPRESS_DEPRECATION=1` silences the warning on every
/// deprecated verb. The verb body still runs (and still errors on the
/// synthetic fixture); the warning text must be absent from stderr.
#[test]
fn warning_silenced_with_env_opt_out() {
    let root = temp_root("warn_silenced");

    for (verb, extra_args) in [
        (
            "build",
            vec!["--project".to_string(), root.display().to_string()],
        ),
        (
            "run",
            vec!["--project".to_string(), root.display().to_string()],
        ),
        (
            "deploy",
            vec![
                "--config".to_string(),
                root.join("nros.toml").display().to_string(),
            ],
        ),
        ("monitor", vec![]),
    ] {
        let output = Command::new(nros_bin())
            .arg(verb)
            .args(&extra_args)
            .env("NROS_SUPPRESS_DEPRECATION", "1")
            .output()
            .unwrap_or_else(|e| panic!("spawn nros {verb}: {e}"));
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(
            !stderr.contains(&format!("`nros {verb}` is deprecated")),
            "deprecation warning fired despite NROS_SUPPRESS_DEPRECATION=1 \
             on `nros {verb}`.\nstderr: {stderr}",
        );
    }

    let _ = fs::remove_dir_all(&root);
}
