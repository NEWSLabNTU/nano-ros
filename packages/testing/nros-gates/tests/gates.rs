//! Every gate as a nextest test (phase-543 W1).
//!
//! nextest drives a `harness = false` binary through three calls, which is the
//! whole protocol this implements:
//!
//! * `--list --format terse` prints `<name>: test`, one line per gate;
//! * `--list --format terse --ignored` prints the ignored subset (none yet: W3
//!   makes a lane's out-of-scope gates that subset);
//! * `<name> --exact --nocapture` runs one gate.
//!
//! A test name is `<tier>::<just target>`, e.g. `fast::check::cli-old-spellings`,
//! or `<tier>::<just target>#<unit>` for one part of a `[group('units')]` gate
//! (`build::check::scaffold-builds#rust_project_talker`), which runs with
//! `NROS_GATE_UNIT=<unit>`.
//! The tier is the lane the gate belongs to today; the rest IS the `just`
//! target, so running a test needs no second lookup and cannot disagree with
//! the listing. The listing comes from `scripts/check/check-gate-lists.py
//! --list tests`, the same derivation the old runner read (issue 1072), so the
//! two cannot list different sets.
//!
//! The gate runs as `just <target>` from the checkout root, with the
//! environment nextest set for THIS process removed: several build gates run
//! their own `cargo nextest`, and an inherited `NEXTEST_*` would steer it.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

/// The checkout this harness was built from: `packages/testing/nros-gates`
/// is three levels below its root.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("nros-gates sits three levels below the checkout root")
        .to_path_buf()
}

/// `(test name, just target)` for every gate, from the one derivation.
fn gates(root: &Path) -> Result<Vec<(String, String)>, String> {
    let out = Command::new("python3")
        .arg("scripts/check/check-gate-lists.py")
        .args(["--list", "tests"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("cannot run python3: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "check-gate-lists.py --list tests failed ({}):\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let text = String::from_utf8(out.stdout).map_err(|e| e.to_string())?;
    let mut v = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let (target, _) = target_of(line).ok_or_else(|| format!("malformed test name `{line}`"))?;
        v.push((line.to_string(), target.to_string()));
    }
    if v.is_empty() {
        // An empty list is a broken derivation, never "no gates": refuse
        // rather than let nextest report a green run over nothing.
        return Err("check-gate-lists.py listed no gates".into());
    }
    Ok(v)
}

/// `fast::check::x` -> (`check::x`, None); `build::check::x#u` -> (`check::x`,
/// Some(`u`)). A unit (phase-543 W2) is one part of a `[group('units')]` gate,
/// run alone through `NROS_GATE_UNIT`.
fn target_of(name: &str) -> Option<(&str, Option<&str>)> {
    let (tier, rest) = name.split_once("::")?;
    let (target, unit) = match rest.split_once('#') {
        Some((t, u)) if !u.is_empty() => (t, Some(u)),
        Some(_) => return None,
        None => (rest, None),
    };
    (!tier.is_empty() && !target.is_empty()).then_some((target, unit))
}

/// A red verdict, with its reason on stderr.
fn fail(msg: impl std::fmt::Display) -> ExitCode {
    eprintln!("nros-gates: {msg}");
    ExitCode::FAILURE
}

fn run(root: &Path, target: &str, unit: Option<&str>) -> ExitCode {
    let mut cmd = Command::new("just");
    cmd.arg(target)
        .current_dir(root)
        .env_remove("NROS_GATE_LIST_UNITS");
    match unit {
        Some(u) => cmd.env("NROS_GATE_UNIT", u),
        None => cmd.env_remove("NROS_GATE_UNIT"),
    };
    let inherited: Vec<OsString> = std::env::vars_os()
        .map(|(k, _)| k)
        .filter(|k| k.to_string_lossy().starts_with("NEXTEST"))
        .collect();
    for k in inherited {
        cmd.env_remove(k);
    }
    match cmd.status() {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(s) => fail(format_args!("gate `{target}` failed: {s}")),
        Err(e) => fail(format_args!("cannot run `just {target}`: {e}")),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a == f);
    let root = repo_root();

    if has("--list") {
        if has("--ignored") {
            return ExitCode::SUCCESS; // W3: a lane's out-of-scope gates
        }
        return match gates(&root) {
            Ok(v) => {
                for (name, _) in v {
                    println!("{name}: test");
                }
                ExitCode::SUCCESS
            }
            Err(e) => fail(e),
        };
    }

    // nextest runs one test per process: `<name> --exact --nocapture`.
    let names: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    match (has("--exact"), names.as_slice()) {
        (true, [name]) => match target_of(name) {
            Some((target, unit)) => run(&root, target, unit),
            None => fail(format_args!("`{name}` is not <tier>::<just target>")),
        },
        _ => {
            eprintln!(
                "nros-gates: run under nextest (`cargo nextest run --manifest-path \
                 packages/testing/nros-gates/Cargo.toml`), or one gate with \
                 `<tier>::<target> --exact`"
            );
            ExitCode::from(2)
        }
    }
}
