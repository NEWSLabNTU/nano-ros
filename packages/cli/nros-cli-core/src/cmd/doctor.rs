//! `nros doctor` — Phase 111.A.7. Aggregates per-platform doctors.
//!
//! v1 strategy: shell out to `just doctor` from the detected workspace
//! root. The justfile already orchestrates every per-module doctor
//! recipe (`just nuttx doctor`, `just zephyr doctor`, ...) and is the
//! source of truth for what "healthy" means. We surface the existing
//! mechanism through a single user-facing verb instead of recreating
//! the diagnostic surface from scratch.

use clap::Args as ClapArgs;
use eyre::{Result, WrapErr, bail, eyre};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use crate::{
    cmd::board::find_workspace_root,
    orchestration::{cargo_metadata_schema::SystemToml, sdk_index::SdkIndex},
};

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Restrict the check to one module (e.g. `nuttx`, `zephyr`,
    /// `freertos`). Forwarded as `just <platform> doctor`.
    #[arg(long)]
    pub platform: Option<String>,

    /// Restrict the license-gate check to one board's package set
    /// (Phase 217.B.2). When set, only `[gated.*]` entries listed in
    /// `[board.<name>].packages` are checked — keeps unrelated gated
    /// SDKs out of the report for board-scoped runs. The board's
    /// `board.cmake` `NROS_BOARD_GATED_PKGS` is the SSoT;
    /// `nros-sdk-index.toml` `[board.<name>].packages` mirrors it.
    #[arg(long)]
    pub board: Option<String>,

    /// Path to the nano-ros workspace root (auto-detected if omitted)
    #[arg(long)]
    pub workspace: Option<PathBuf>,

    /// Bringup `system.toml` (or a directory containing one) whose
    /// `[deploy.<target>]` blocks to health-check (RFC-0004 §4). When
    /// omitted, the cwd's `system.toml` is used if present.
    #[arg(long)]
    pub config: Option<PathBuf>,
}

pub fn run(args: Args) -> Result<()> {
    // RFC-0004 §4 — deploy-target health check. Resolve the bringup
    // `system.toml` (explicit `--config`, else the cwd's `system.toml`) and
    // report each `[deploy.<target>]` block. `None` ⇒ no system.toml here
    // (e.g. running in the nano-ros repo) → only the workspace health below
    // runs.
    let config = resolve_config(args.config.as_deref());
    let deploy_problems = match &config {
        Some(path) => check_deploy_targets(path)?,
        None => None,
    };

    // Phase 222.B.3 — flag use of deprecated `nros build` / `run` / `deploy` /
    // `monitor` verbs inside any shell-step arrays. WARN only (gated
    // migration); the verbs still work today and disappear in 0.5.0
    // (Phase 222.C).
    if let Some(path) = &config {
        check_deprecated_verbs(path);
    }

    // Phase 187.7 — license-gated SDK presence (NVIDIA SPE, ARM FVP, …): never
    // fetched, only instructed. Read before `args.workspace` is moved below.
    // Phase 217.B.2 — when `--board <name>` is set, filter to that board's
    // `packages` so unrelated gates don't show up.
    let gate_problems = check_license_gates(args.workspace.as_deref(), args.board.as_deref())?;

    // The nano-ros workspace health (`just doctor`). When a bringup
    // `system.toml` was checked, missing the nano-ros workspace is non-fatal
    // (we're in a user deploy project, not the nano-ros repo); otherwise it
    // stays a hard requirement.
    let root = match args.workspace {
        Some(p) => Some(p),
        None => match find_workspace_root() {
            Ok(r) => Some(r),
            Err(_) if deploy_problems.is_some() => {
                eprintln!(
                    "nros doctor: no nano-ros workspace here — skipped `just doctor` \
                     (checked deploy targets only)"
                );
                None
            }
            Err(e) => {
                return Err(e).wrap_err(
                    "could not auto-detect the nano-ros workspace root; \
                     pass --workspace <path> explicitly",
                );
            }
        },
    };

    if let Some(root) = root {
        run_just_doctor(&root, args.platform.as_deref())?;
    }

    let problems = deploy_problems.unwrap_or(0) + gate_problems;
    if problems > 0 {
        bail!("nros doctor: {problems} problem(s) (deploy targets + license gates)");
    }
    Ok(())
}

/// Resolve the bringup `system.toml` to health-check: the explicit
/// `--config` (a file or a directory carrying one), else the cwd's
/// `system.toml` when present. `None` ⇒ no deploy-target check engages.
fn resolve_config(explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        let path = if p.is_dir() {
            p.join("system.toml")
        } else {
            p.to_path_buf()
        };
        return Some(path);
    }
    let cwd = std::env::current_dir().ok()?;
    let st = cwd.join("system.toml");
    st.is_file().then_some(st)
}

/// Phase 187.7 — license-gate presence check. For each `[gated.*]` SDK in the
/// index (NVIDIA SPE, ARM FVP, …), report whether its env var resolves to an
/// existing directory. These are NEVER fetched or built — only instructed. An
/// unset env is informational (the user simply isn't targeting that board); an
/// env that's set but points nowhere is a misconfiguration (counted). No index
/// nearby ⇒ skip silently.
///
/// Phase 217.B.2 — when `board` is `Some(name)`, only `[gated.*]` entries
/// listed in `[board.<name>].packages` are checked. Also special-cases
/// `arm-fvp`: presence is determined by locating the `FVP_BaseR_AEMv8R`
/// binary (via `ARMFVP_BIN_PATH`, `ARM_FVP_DIR`, PATH, or the
/// `~/.nros/sdks/arm-fvp/current/` symlink the installer drops). A miss is
/// a WARN with a one-liner pointing at `scripts/installers/arm-fvp-installer.sh`
/// + the Arm EULA URL — NOT counted as a problem (gated tool, never
/// hard-fails the doctor run).
fn check_license_gates(workspace: Option<&Path>, board: Option<&str>) -> Result<usize> {
    let Some(index_path) = crate::cmd::setup::locate_index(workspace) else {
        return Ok(0);
    };
    let index = SdkIndex::load(&index_path)?;
    if index.gated.is_empty() {
        return Ok(0);
    }

    // Phase 217.B.2 — board filter: only the gates listed in this board's
    // packages survive. Unknown board ⇒ error (matches `nros setup` policy).
    let board_filter: Option<Vec<String>> = match board {
        None => None,
        Some(b) => {
            let pkgs = crate::cmd::setup::resolve_packages(&index, b)
                .wrap_err_with(|| format!("nros doctor --board {b}"))?;
            Some(pkgs.into_iter().map(str::to_string).collect())
        }
    };

    eprintln!("nros doctor: license-gated SDKs ({})", index_path.display());
    let mut problems = 0usize;
    for (name, g) in &index.gated {
        if let Some(allow) = &board_filter {
            if !allow.iter().any(|p| p == name) {
                continue;
            }
        }
        // Special-case ARM FVP: binary discovery (Zephyr's armfvp.cmake calls
        // `find_program(... PATHS ENV ARMFVP_BIN_PATH)`), and `[gated.arm-fvp]`
        // is the only gate that maps a binary name. All other gates fall
        // through to the env-var check.
        if name == "arm-fvp" {
            check_arm_fvp(g);
            continue;
        }
        let via = g
            .installer
            .as_deref()
            .map(|i| format!(", via {i}"))
            .unwrap_or_default();
        match std::env::var_os(&g.env) {
            None => eprintln!(
                "  [--] {name} {}: not installed — set ${}{via} (never auto-fetched)",
                g.version, g.env
            ),
            Some(v) => {
                let dir = PathBuf::from(&v);
                if dir.is_dir() {
                    eprintln!(
                        "  [OK] {name} {}: ${} = {}",
                        g.version,
                        g.env,
                        dir.display()
                    );
                } else {
                    eprintln!(
                        "  [!!] {name}: ${} set to {} — not a directory",
                        g.env,
                        dir.display()
                    );
                    problems += 1;
                }
            }
        }
    }
    Ok(problems)
}

/// Phase 217.B.2 — ARM FVP binary discovery. Mirrors
/// `scripts/zephyr/resolve-fvp-bin.sh`: `ARMFVP_BIN_PATH/<bin>` →
/// `ARM_FVP_DIR/models/Linux64_GCC-*/<bin>` → `command -v <bin>` →
/// `~/.nros/sdks/arm-fvp/current/<bin>` (installer landing). Prints PASS /
/// WARN to stderr but never increments the problem counter — gated tool, so
/// a missing FVP must not fail an unrelated `nros doctor` run.
fn check_arm_fvp(g: &crate::orchestration::sdk_index::GatedPackage) {
    const BIN: &str = "FVP_BaseR_AEMv8R";
    let landing = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".nros/sdks/arm-fvp/current"))
        .unwrap_or_default();

    // 1. ARMFVP_BIN_PATH (Zephyr canonical).
    if let Some(v) = std::env::var_os("ARMFVP_BIN_PATH") {
        let dir = PathBuf::from(&v);
        if dir.join(BIN).is_file() {
            eprintln!(
                "  [OK] arm-fvp {}: $ARMFVP_BIN_PATH = {}",
                g.version,
                dir.display()
            );
            return;
        }
    }
    // 2. ARM_FVP_DIR — sdk-index env. Look for `models/Linux64_GCC-*/<BIN>`
    //    OR `<BIN>` directly under the root.
    if let Some(v) = std::env::var_os(&g.env) {
        let root = PathBuf::from(&v);
        if let Some(hit) = find_fvp_under(&root, BIN) {
            eprintln!(
                "  [OK] arm-fvp {}: ${} = {} (binary at {})",
                g.version,
                g.env,
                root.display(),
                hit.display()
            );
            return;
        }
    }
    // 3. PATH fallback.
    if which(BIN).is_ok() {
        eprintln!("  [OK] arm-fvp {}: {BIN} on PATH", g.version);
        return;
    }
    // 4. Installer landing symlink.
    if !landing.as_os_str().is_empty() && landing.join(BIN).is_file() {
        eprintln!(
            "  [OK] arm-fvp {}: {} (installer landing)",
            g.version,
            landing.display()
        );
        return;
    }
    // Miss — WARN only (gated, never a hard fail).
    eprintln!(
        "  [WARN] arm-fvp {}: {BIN} not found — set $ARMFVP_BIN_PATH or ${}, \
         or run scripts/installers/arm-fvp-installer.sh after extracting the \
         Arm FVP tarball (EULA: https://developer.arm.com/downloads/-/arm-ecosystem-fvps). \
         Never auto-fetched.",
        g.version, g.env
    );
}

/// Scan a small set of common Arm-ships layouts for `bin` under `root`.
/// Mirrors `scripts/zephyr/resolve-fvp-bin.sh` step 2. Returns the absolute
/// path on first hit. Cheap glob — `read_dir` only, no recursive `find`.
fn find_fvp_under(root: &Path, bin: &str) -> Option<PathBuf> {
    let direct = root.join(bin);
    if direct.is_file() {
        return Some(direct);
    }
    for sub in ["models", "Base_RevC_AEMv8R_pkg/models"] {
        let models = root.join(sub);
        if let Ok(rd) = std::fs::read_dir(&models) {
            for ent in rd.flatten() {
                let p = ent.path().join(bin);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    for sub in ["bin", "Base_RevC_AEMv8R_pkg/bin"] {
        let cand = root.join(sub).join(bin);
        if cand.is_file() {
            return Some(cand);
        }
    }
    None
}

/// Report each `[deploy.<target>]` block in the bringup `system.toml`
/// (RFC-0004 §4). Returns the problem count, or `None` when `config` is not a
/// loadable `system.toml`.
///
/// The live `DeployTarget` carries `kind` / `target` / `board` / `launch` —
/// no vendor-pin/build/package machinery (that backed the retired Phase-172
/// model). The one checkable health condition is a `launch` that points at a
/// file which does not exist relative to the bringup dir.
fn check_deploy_targets(config: &Path) -> Result<Option<usize>> {
    if !config.is_file() {
        return Ok(None);
    }
    let raw = match std::fs::read_to_string(config) {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };
    // Not a parseable system.toml (e.g. a plain component manifest) — skip.
    let Ok(system) = toml::from_str::<SystemToml>(&raw) else {
        return Ok(None);
    };
    let bringup_dir = config
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    eprintln!("nros doctor: deploy targets ({})", config.display());
    let mut problems = 0usize;
    for (name, deploy) in &system.deploy {
        let kind = deploy.kind.as_deref().unwrap_or("(derived)");
        let target = deploy.target.as_deref().unwrap_or("(derived)");
        let board = deploy.board.as_deref().unwrap_or("-");
        if let Some(launch) = &deploy.launch {
            let launch_path = bringup_dir.join(launch);
            if !launch_path.exists() {
                eprintln!(
                    "  [!!] {name}: kind={kind} launch={launch} — file {} not found",
                    launch_path.display()
                );
                problems += 1;
                continue;
            }
        }
        eprintln!("  [OK] {name}: kind={kind} target={target} board={board}");
    }
    Ok(Some(problems))
}

/// Phase 222.B.3 — surface `nros build` / `run` / `deploy` / `monitor` / `launch` usage
/// inside any `[deploy.<name>]` `build` / `package` shell-step arrays as WARN
/// (not FAIL — gated migration). The wrapper verbs still work today; deletion
/// lands in Phase 222.C with the 0.5.0 bump.
///
/// Best-effort raw TOML scan: parses the file as a generic `toml::Value` so
/// the lint surfaces drift in any TOML config carrying such arrays. Silent
/// no-op when the file is absent or unparseable — this is a hint, not an
/// authority. (The live `system.toml` `DeployTarget` no longer carries
/// `build`/`package` arrays, so this is dormant on the RFC-0004 path.)
fn check_deprecated_verbs(config: &Path) {
    if !config.is_file() {
        return;
    }
    let raw = match std::fs::read_to_string(config) {
        Ok(r) => r,
        Err(_) => return,
    };
    let doc: toml::Value = match toml::from_str(&raw) {
        Ok(d) => d,
        Err(_) => return,
    };
    let Some(deploy) = doc.get("deploy").and_then(|v| v.as_table()) else {
        return;
    };

    let mut printed_header = false;
    for (name, target) in deploy {
        let Some(table) = target.as_table() else {
            continue;
        };
        for field in ["build", "package"] {
            let Some(arr) = table.get(field).and_then(|v| v.as_array()) else {
                continue;
            };
            for step in arr {
                let Some(cmd) = step.as_str() else { continue };
                let trimmed = cmd.trim_start();
                let Some((verb, replacement)) = match_deprecated_verb(trimmed) else {
                    continue;
                };
                if !printed_header {
                    eprintln!(
                        "nros doctor: deprecated-verb usage in {} ({}-list)",
                        config.display(),
                        field
                    );
                    printed_header = true;
                }
                eprintln!(
                    "  [WARN] {}: [deploy.{name}].{field} = \"{cmd}\" — `nros {verb}` deprecated \
                     in 222.B; will fail in 0.5.0 (222.C). Replace with: {replacement}.",
                    config.display(),
                );
            }
        }
    }
}

/// Return `Some((verb, replacement))` if `cmd` starts with one of the
/// deprecated `nros` verbs (after an optional leading `nros` token).
/// Matches `nros build …`, `nros run …`, `nros deploy …`,
/// `nros monitor …`, `nros launch …` (Phase 222.D).
fn match_deprecated_verb(cmd: &str) -> Option<(&'static str, &'static str)> {
    let rest = cmd.strip_prefix("nros ")?.trim_start();
    // Peel off the first token. `split_whitespace` collapses runs of WS;
    // we only need the head word.
    let head = rest.split_whitespace().next()?;
    match head {
        "build" => Some((
            "build",
            "cargo build / cmake --build / west build / idf.py build",
        )),
        "run" => Some((
            "run",
            "cargo run / west <runner> run / probe-rs run / idf.py monitor",
        )),
        "deploy" => Some((
            "deploy",
            "the platform's native flash+run combo (west flash, idf.py flash, probe-rs run)",
        )),
        "monitor" => Some(("monitor", "probe-rs attach / idf.py monitor / picocom")),
        "launch" => Some((
            "launch",
            "cargo run -p <entry_pkg> (composed Entry pkg IS the launch product per Phase 212.N + 222.D)",
        )),
        _ => None,
    }
}

fn run_just_doctor(root: &Path, platform: Option<&str>) -> Result<()> {
    if which("just").is_err() {
        return Err(eyre!(
            "`just` is not on PATH. Install it (https://just.systems) \
             or run individual checks manually."
        ));
    }

    let mut cmd = Command::new("just");
    cmd.current_dir(root)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    match platform {
        Some(p) => {
            cmd.arg(p).arg("doctor");
        }
        None => {
            cmd.arg("doctor");
        }
    }

    let status = cmd
        .status()
        .wrap_err_with(|| format!("failed to invoke `just` in {}", root.display()))?;
    if !status.success() {
        return Err(eyre!(
            "doctor reported failures (exit {})",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

fn which(bin: &str) -> Result<PathBuf> {
    let path = std::env::var_os("PATH").ok_or_else(|| eyre!("PATH unset"))?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(bin);
        if is_executable(&candidate) {
            return Ok(candidate);
        }
    }
    Err(eyre!("{bin} not found on PATH"))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && std::fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC-0004 §4 — `check_deploy_targets` parses a bringup `system.toml`,
    /// reports each `[deploy.<target>]`, and flags a deploy whose `launch`
    /// file is missing relative to the bringup dir.
    #[test]
    fn deploy_targets_flag_missing_launch_file() {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("nros_doctor_deploy_{n}"));
        std::fs::create_dir_all(&dir).unwrap();
        let system_toml = dir.join("system.toml");

        // Two healthy targets → 0 problems.
        std::fs::write(
            &system_toml,
            "[system]\nname=\"d\"\nrmw=\"zenoh\"\ndomain_id=0\n\
             [deploy.native]\nkind=\"self\"\ntarget=\"x86_64-unknown-linux-gnu\"\n\
             [deploy.qemu]\nkind=\"qemu\"\nboard=\"mps2_an385\"\n",
        )
        .unwrap();
        assert_eq!(check_deploy_targets(&system_toml).unwrap(), Some(0));

        // A deploy referencing a non-existent launch file → 1 problem.
        std::fs::write(
            &system_toml,
            "[system]\nname=\"d\"\nrmw=\"zenoh\"\ndomain_id=0\n\
             [deploy.native]\nkind=\"self\"\nlaunch=\"missing.launch.xml\"\n",
        )
        .unwrap();
        assert_eq!(check_deploy_targets(&system_toml).unwrap(), Some(1));

        // Present launch file → 0 problems.
        std::fs::write(dir.join("present.launch.xml"), "<launch/>").unwrap();
        std::fs::write(
            &system_toml,
            "[system]\nname=\"d\"\nrmw=\"zenoh\"\ndomain_id=0\n\
             [deploy.native]\nkind=\"self\"\nlaunch=\"present.launch.xml\"\n",
        )
        .unwrap();
        assert_eq!(check_deploy_targets(&system_toml).unwrap(), Some(0));

        // A non-system.toml (no [system]) → None (skipped, not a problem).
        std::fs::write(&system_toml, "[component]\nname=\"c\"\n").unwrap();
        assert_eq!(check_deploy_targets(&system_toml).unwrap(), None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn license_gate_flags_misconfigured_env_only() {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let ws = std::env::temp_dir().join(format!("nros_gate_{n}"));
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(
            ws.join("nros-sdk-index.toml"),
            "[gated.nv-spe-fsp]\nversion=\"36.3\"\nenv=\"NROS_TEST_GATE_ENV\"\ninstaller=\"x\"\n",
        )
        .unwrap();
        let env = "NROS_TEST_GATE_ENV";

        // Unset ⇒ informational, not a problem.
        unsafe { std::env::remove_var(env) };
        assert_eq!(check_license_gates(Some(&ws), None).unwrap(), 0);
        // Set to a non-existent dir ⇒ misconfigured ⇒ 1 problem.
        unsafe { std::env::set_var(env, ws.join("nope")) };
        assert_eq!(check_license_gates(Some(&ws), None).unwrap(), 1);
        // Set to an existing dir ⇒ OK.
        unsafe { std::env::set_var(env, &ws) };
        assert_eq!(check_license_gates(Some(&ws), None).unwrap(), 0);

        unsafe { std::env::remove_var(env) };
        std::fs::remove_dir_all(&ws).ok();
    }

    /// Phase 217.B.2 — `--board <name>` restricts the gated check to that
    /// board's package set. A board listing `arm-fvp` triggers the FVP path
    /// (binary discovery); a missing FVP is WARN-only (problems == 0).
    #[test]
    fn license_gate_board_filter_arm_fvp_warns_only() {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let ws = std::env::temp_dir().join(format!("nros_gate_fvp_{n}"));
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(
            ws.join("nros-sdk-index.toml"),
            "[gated.arm-fvp]\n\
             version=\"11.24\"\n\
             env=\"NROS_TEST_ARMFVP_DIR\"\n\
             installer=\"arm-fvp-installer\"\n\
             [gated.nv-spe-fsp]\n\
             version=\"36.3\"\n\
             env=\"NROS_TEST_NVSPE_DIR\"\n\
             installer=\"x\"\n\
             [board.fvp-test]\n\
             packages=[\"arm-fvp\"]\n",
        )
        .unwrap();

        let envs = [
            "NROS_TEST_ARMFVP_DIR",
            "NROS_TEST_NVSPE_DIR",
            "ARMFVP_BIN_PATH",
            "HOME",
        ];
        let saved: Vec<_> = envs.iter().map(|e| (*e, std::env::var_os(e))).collect();
        for e in &envs {
            unsafe { std::env::remove_var(e) };
        }
        // Point HOME at a temp dir with no installer landing.
        let home = ws.join("home");
        std::fs::create_dir_all(&home).unwrap();
        unsafe { std::env::set_var("HOME", &home) };

        // Misconfigured NVSPE env ⇒ would be 1 problem WITHOUT the filter —
        // with `--board fvp-test`, only arm-fvp is checked, so it must be 0.
        unsafe { std::env::set_var("NROS_TEST_NVSPE_DIR", ws.join("nope")) };
        let problems = check_license_gates(Some(&ws), Some("fvp-test")).unwrap();
        assert_eq!(
            problems, 0,
            "board filter must skip non-arm-fvp gates and WARN (not FAIL) on missing FVP"
        );

        for (e, v) in saved {
            match v {
                Some(v) => unsafe { std::env::set_var(e, v) },
                None => unsafe { std::env::remove_var(e) },
            }
        }
        std::fs::remove_dir_all(&ws).ok();
    }

    /// Phase 222.B.3 — `match_deprecated_verb` recognises the four
    /// deprecated `nros` verbs (with leading whitespace and trailing args)
    /// and skips everything else.
    #[test]
    fn matches_deprecated_verbs() {
        assert!(match_deprecated_verb("nros build").is_some());
        assert!(match_deprecated_verb("nros build --release").is_some());
        assert!(match_deprecated_verb("nros run").is_some());
        assert!(match_deprecated_verb("nros deploy native").is_some());
        assert!(match_deprecated_verb("nros monitor --env foo").is_some());
        // Phase 222.D — launch joins the set.
        assert!(match_deprecated_verb("nros launch demo_bringup").is_some());
        // Non-deprecated verbs / non-nros commands should not match.
        assert!(match_deprecated_verb("nros setup native").is_none());
        assert!(match_deprecated_verb("cargo build").is_none());
        assert!(match_deprecated_verb("west build -b mps2_an385").is_none());
        // No leading `nros ` ⇒ not our concern.
        assert!(match_deprecated_verb("build").is_none());
    }

    /// Phase 217.B.2 — unknown board name on `--board` is a hard error
    /// (matches `nros setup`'s policy — no silent wrong package set).
    #[test]
    fn license_gate_board_filter_rejects_unknown() {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let ws = std::env::temp_dir().join(format!("nros_gate_unk_{n}"));
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(
            ws.join("nros-sdk-index.toml"),
            "[gated.arm-fvp]\nversion=\"1\"\nenv=\"E\"\ninstaller=\"i\"\n\
             [board.known]\npackages=[]\n",
        )
        .unwrap();
        let err = check_license_gates(Some(&ws), Some("nope")).unwrap_err();
        let s = format!("{err:#}");
        assert!(
            s.contains("nope") || s.contains("unknown board"),
            "expected unknown-board error, got: {s}"
        );
        std::fs::remove_dir_all(&ws).ok();
    }
}
