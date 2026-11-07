//! Every gate as a nextest test (phase-543).
//!
//! nextest drives a `harness = false` binary through three calls, which is the
//! whole protocol this implements:
//!
//! * `--list --format terse` prints `<name>: test`, one line per test;
//! * `--list --format terse --ignored` prints the ignored subset: the tests
//!   out of the lane's scope (`NROS_GATE_LANE`, `.config/gate-lanes.toml`), so
//!   nextest shows them as not run, by name, and never counts them as passed;
//! * `<name> --exact --nocapture` runs one test.
//!
//! A test name is `<tier>::<just target>`, e.g. `fast::check::cli-old-spellings`,
//! or `<tier>::<just target>#<unit>` for one part of a `[group('units')]` gate
//! (`build::check::scaffold-builds#rust_project_talker`), which runs with
//! `NROS_GATE_UNIT=<unit>`. The rest after the tier IS the `just` target, so
//! running a test needs no second lookup and cannot disagree with the listing.
//! The listing comes from `scripts/check/check-gate-lists.py --list tests`, the
//! same derivation the old runner read (issue 1072).
//!
//! A gate runs as `just <target>` from the checkout root, with the environment
//! nextest set for THIS process removed: several build gates run their own
//! `cargo nextest`, and an inherited `NEXTEST_*` would steer it.
//!
//! A gate under this harness never passes by skipping (phase-543 W3). It gets
//! its own skip ledger (`NROS_GATE_SKIP_LEDGER`, written by
//! `scripts/build/check-skip.sh`) and `NROS_CHECK_SKIP_STRICT=1`; anything in
//! the ledger afterwards fails the test, even when the gate exited 0.

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

/// Every test with the reason it is out of the lane's scope (`None` = in
/// scope), from the one derivation. Unset `NROS_GATE_LANE` claims everything.
fn gates(root: &Path) -> Result<Vec<(String, Option<String>)>, String> {
    let mut cmd = Command::new("python3");
    cmd.arg("scripts/check/check-gate-lists.py")
        .args(["--list", "tests"])
        .current_dir(root);
    if let Ok(lane) = std::env::var("NROS_GATE_LANE")
        && !lane.is_empty()
    {
        cmd.args(["--lane", &lane]);
    }
    let out = cmd
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
        let (name, why) = match line.split_once('\t') {
            Some((n, w)) => (n, Some(w.to_string())),
            None => (line, None),
        };
        target_of(name).ok_or_else(|| format!("malformed test name `{name}`"))?;
        v.push((name.to_string(), why));
    }
    if v.is_empty() {
        // An empty list is a broken derivation, never "no gates": refuse
        // rather than let nextest report a green run over nothing.
        return Err("check-gate-lists.py listed no gates".into());
    }
    Ok(v)
}

/// `fast::check::x` -> (`check::x`, None); `build::check::x#u` -> (`check::x`,
/// Some(`u`)).
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

fn run(root: &Path, name: &str, target: &str, unit: Option<&str>) -> ExitCode {
    let ledger = std::env::temp_dir().join(format!(
        "nros-gate-skips-{}-{}",
        std::process::id(),
        name.replace([':', '/', '#', '@'], "_")
    ));
    let _ = std::fs::remove_file(&ledger);
    let mut cmd = Command::new("just");
    cmd.arg(target)
        .current_dir(root)
        .env_remove("NROS_GATE_LIST_UNITS")
        .env("NROS_GATE_SKIP_LEDGER", &ledger)
        // The ledger and strict mode belong to THIS checkout's gates; a gate's
        // nested subject in a scratch checkout keeps its own
        // (`scripts/build/check-skip.sh`, `_nros_check_skip_nested`).
        .env("NROS_GATE_SKIP_ROOT", root)
        .env("NROS_CHECK_SKIP_STRICT", "1");
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
    let status = cmd.status();
    let skipped = std::fs::read_to_string(&ledger).unwrap_or_default();
    let _ = std::fs::remove_file(&ledger);
    if !skipped.trim().is_empty() {
        return fail(format_args!(
            "[UNMET PRECONDITION] `{name}` skipped instead of running:\n{}\n  A skip is \
             never a pass. Provision what it needs, or declare it on the recipe with \
             `[group('needs:<cap>')]` so a lane that lacks it leaves the gate out of \
             scope (.config/gate-lanes.toml).",
            skipped.trim_end()
        ));
    }
    match status {
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
        let ignored = has("--ignored");
        return match gates(&root) {
            Ok(v) => {
                for (name, why) in v {
                    if !ignored || why.is_some() {
                        println!("{name}: test");
                    }
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
            Some((target, unit)) => run(&root, name, target, unit),
            None => fail(format_args!("`{name}` is not <tier>::<just target>")),
        },
        _ => {
            eprintln!(
                "nros-gates: run under nextest (`just check nextest-gates`), or one gate \
                 with `<tier>::<target> --exact`"
            );
            ExitCode::from(2)
        }
    }
}
