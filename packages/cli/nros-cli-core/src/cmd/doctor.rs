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
    /// `[image.<id>]` blocks to health-check — and any `[deploy.*]` still
    /// declared. When omitted, the cwd's `system.toml` is used if present.
    #[arg(long)]
    pub config: Option<PathBuf>,
}

/// phase-365 W5 — report a legacy UNVERSIONED install as removable.
///
/// `just workspace install-corrosion` used to install into
/// `<store>/corrosion/` with no version component, so the store carried two
/// layouts from two producers. That flat prefix is what caused issue 0625:
/// `cmake-prefix.sh` globbed `<store>/corrosion/*/`, which matched the flat
/// install's `lib/` and `share/` SUBDIRECTORIES — not versions — and under
/// `sort -Vr` a pure-alpha name sorts before the numeric ones, so it led the
/// prefix path and won 155 of 183 resolutions in one configure.
///
/// Both producers now write `<store>/<tool>/<version>`. An existing flat prefix
/// is inert once nothing enumerates the store, but it is confusing to find and
/// costs disk, so say it is there rather than leaving it to be rediscovered.
fn report_legacy_unversioned_installs() {
    let store = crate::orchestration::sdk_store::store_root();
    let Ok(entries) = std::fs::read_dir(&store) else {
        return;
    };
    for tool in entries.flatten() {
        if !tool.path().is_dir() {
            continue;
        }
        // A version dir contains the install; the flat layout puts `lib/` or
        // `share/` DIRECTLY under the tool, where a version should be.
        // Name ONLY the flat subdirectories. An earlier draft printed
        // `rm -rf <store>/<tool>`, which deletes the VERSIONED installs
        // alongside the legacy one — a doctor that tells you to delete your
        // pinned toolchain is worse than one that says nothing.
        let legacy: Vec<std::path::PathBuf> = ["lib", "share"]
            .iter()
            .map(|d| tool.path().join(d))
            .filter(|p| p.is_dir())
            .collect();
        if !legacy.is_empty() {
            let paths: Vec<String> = legacy.iter().map(|p| p.display().to_string()).collect();
            eprintln!(
                "nros doctor: [REMOVABLE] legacy unversioned install under {}\n\
                 \x20   {}\n\
                 \x20   the store is keyed by version now; this prefix belongs to no \
                 project and no longer participates in resolution (issue 0625).\n\
                 \x20   remove it:  rm -rf {}",
                tool.path().display(),
                paths.join("\n\x20   "),
                paths.join(" ")
            );
        }
    }
}

/// RFC-0097 D12 — with no workspace to scope it to, `nros doctor` verifies the
/// INSTALL: **launcher, store, pin resolution**.
///
/// Before this, a `doctor` that could not auto-detect a nano-ros workspace
/// simply failed — so the one user who most needs a health check, the one who
/// has just run `install.sh` and has nothing to point it at, was the one user
/// it refused to answer. "Where is my store, did the launcher land, and does
/// the pin in this directory resolve" is a question that needs no workspace at
/// all.
///
/// Every input is a PARAMETER, so this is testable against a constructed store
/// without a test mutating `$NROS_HOME`/`$NROS_STORE` (which in this crate is a
/// cross-thread race — `tests/store_reclaim.rs`).
///
/// Exactly ONE thing here counts as a problem: **a pin naming a nano-ros this
/// store cannot produce**, which is a project that cannot be built. An absent
/// store, an unfronted launcher and a binary that is not a store toolchain are
/// all legitimate states — a contributor running the tree's own build has all
/// three — and a doctor that fails on a working setup is a doctor people learn
/// to ignore.
fn install_report(store: &Path, exe: &Path, cwd: &Path) -> (Vec<String>, usize) {
    use crate::orchestration::{dispatch, pin, store as store_mod};

    let mut lines = Vec::new();
    let mut problems = 0usize;

    // --- the store ---------------------------------------------------------
    if store.is_dir() {
        let entries = store_mod::scan(store);
        let bytes: u64 = entries.iter().map(|e| e.size_bytes).sum();
        lines.push(format!(
            "  [OK] store      {} ({}, {} entr{})",
            store.display(),
            store_mod::format_size(bytes),
            entries.len(),
            if entries.len() == 1 { "y" } else { "ies" }
        ));
    } else {
        // Not a problem: a contributor building in the checkout has no store,
        // and telling them to make one would be wrong.
        lines.push(format!(
            "  [--] store      {} does not exist yet — nothing has been provisioned here",
            store.display()
        ));
    }

    // --- the launcher ------------------------------------------------------
    //
    // `<store>/bin/nros` is what `install.sh` fronts and what a user's PATH
    // points at. Reported separately from "am I a store toolchain", because
    // those come apart in both directions: a contributor's in-tree build is not
    // a store toolchain and needs no front, while a front pointing at nothing
    // is an install that half happened.
    let front = store.join("bin").join("nros");
    if front.exists() {
        lines.push(format!("  [OK] launcher   {}", front.display()));
    } else {
        lines.push(format!(
            "  [--] launcher   no {} — install with `curl -fsSL <install.sh> | sh`, \
             or use a checkout's own build",
            front.display()
        ));
    }
    match pin::running_version(exe) {
        Some((v, origin)) => lines.push(format!(
            "  [OK] running    nano-ros {v} (from the {})",
            origin.label()
        )),
        None => lines.push(format!(
            "  [--] running    {} is not a store toolchain (a checkout build, or an \
             unpacked prefix) — it will never dispatch",
            exe.display()
        )),
    }

    // --- pin resolution ----------------------------------------------------
    //
    // Asked DIRECTLY, not via `dispatch::decide`. An earlier draft read the
    // launcher's verdict and counted `Decision::Missing` as the problem, and
    // an end-to-end run showed that arm is unreachable: if this binary IS a
    // store toolchain, `redispatch()` has already met the missing version in
    // `main` and failed before `doctor` parsed anything; and if it is NOT one,
    // `decide` returns `Proceed("not a launcher")` at rung 4 without ever
    // looking at the pin. So the verdict a doctor could reuse is exactly the
    // one it can never see — and the CHECKOUT BUILD case, which is most of
    // them, learned nothing about whether its project's pin is satisfiable.
    match pin::find_and_load(cwd) {
        Err(e) => {
            problems += 1;
            lines.push(format!(
                "  [!!] pin        present but unreadable: {e:#}\n\
                 \x20              a pin that cannot be parsed is not a floating project, \
                 it is an unreadable one"
            ));
        }
        Ok(None) => lines.push(
            "  [--] pin        this project names no toolchain — the build floats \
             (`nros build` writes one on its first run)"
                .to_string(),
        ),
        Ok(Some(p)) => {
            let running = pin::running_version(exe).map(|(v, _)| v);
            if let Some(bin) = pin::installed_bin(store, &p.version) {
                lines.push(format!(
                    "  [OK] pin        {} names nano-ros {} → {}",
                    p.path.display(),
                    p.version,
                    bin.display()
                ));
            } else if running.as_deref() == Some(p.version.as_str()) {
                lines.push(format!(
                    "  [OK] pin        {} names nano-ros {}, which is what is running here",
                    p.path.display(),
                    p.version
                ));
            } else {
                problems += 1;
                lines.push(format!(
                    "  [!!] pin        {} names nano-ros {}, which is not in this store\n\
                     \x20              looked in: {}\n\
                     \x20              install it:  curl -fsSL <install.sh> | sh -s -- --version {}",
                    p.path.display(),
                    p.version,
                    pin::candidate_bins(store, &p.version)
                        .iter()
                        .map(|b| b.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                    p.version
                ));
            }
        }
    }

    // --- what the launcher would do ----------------------------------------
    //
    // Informational, never a problem: it is `dispatch::decide` itself, so this
    // line cannot disagree with the launcher about what will happen next — the
    // rung above says whether the pin CAN be satisfied, this one says whether
    // this process will hand over.
    match dispatch::decide(&dispatch::Context {
        exe: exe.to_path_buf(),
        cwd: cwd.to_path_buf(),
        store: store.to_path_buf(),
        dispatched: std::env::var(dispatch::DISPATCHED_ENV).ok(),
        skip: std::env::var_os(dispatch::SKIP_ENV).is_some(),
    }) {
        Ok(dispatch::Decision::Proceed(why)) => {
            lines.push(format!("  [--] dispatch   stays in this process — {why}"));
        }
        Ok(dispatch::Decision::Exec { version, bin }) => lines.push(format!(
            "  [OK] dispatch   would exec nano-ros {version} at {}",
            bin.display()
        )),
        Ok(dispatch::Decision::Missing { version, .. }) => lines.push(format!(
            "  [--] dispatch   would install nano-ros {version} first"
        )),
        Err(e) => lines.push(format!("  [--] dispatch   undecidable: {e:#}")),
    }

    (lines, problems)
}

pub fn run(args: Args) -> Result<()> {
    report_legacy_unversioned_installs();

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
                     (checked the bringup's images only)"
                );
                None
            }
            // RFC-0097 D12 — no workspace is not a failure, it is a different
            // QUESTION. A user who has just run `install.sh` has no workspace
            // to point this at, and refusing them was refusing the only person
            // who needed the answer.
            Err(_) => None,
        },
    };

    let install_problems = match &root {
        Some(root) => {
            run_just_doctor(root, args.platform.as_deref())?;
            0
        }
        None => {
            eprintln!("nros doctor: no nano-ros workspace here — checking the INSTALL instead");
            let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("nros"));
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            let store = crate::orchestration::store::root();
            eprintln!(
                "nros doctor: store root {} (from {})",
                store.display(),
                crate::orchestration::store::root_origin()
            );
            let (lines, problems) = install_report(&store, &exe, &cwd);
            for line in lines {
                eprintln!("{line}");
            }
            eprintln!(
                "  (point it at a workspace with `--workspace <path>` for the \
                 per-platform checks)"
            );
            problems
        }
    };

    let problems = deploy_problems.unwrap_or(0) + gate_problems + install_problems;
    if problems > 0 {
        bail!("nros doctor: {problems} problem(s) (images + license gates + install)");
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
///   hard-fails the doctor run).
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

    // The Arm FVP is NOT license-gated and stopped being `[gated.arm-fvp]` on
    // 2026-09-06 (a public CDN permalink with a pinned digest — measured, not
    // assumed). It is still reported HERE, because this is where a user looks
    // for "do I have the simulator", and because its discovery is special
    // whatever section it lives in: Zephyr's `armfvp.cmake` does
    // `find_program(... PATHS ENV ARMFVP_BIN_PATH)`, so the question is about a
    // BINARY on a path, not an env var pointing at an install root.
    if index.tool.contains_key("arm-fvp")
        && board_filter
            .as_ref()
            .is_none_or(|allow| allow.iter().any(|p| p == "arm-fvp"))
    {
        check_arm_fvp(&index.tool["arm-fvp"].version);
    }
    for (name, g) in &index.gated {
        if let Some(allow) = &board_filter
            && !allow.iter().any(|p| p == name)
        {
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
fn check_arm_fvp(version: &str) {
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
                version,
                dir.display()
            );
            return;
        }
    }
    // 2. ARM_FVP_DIR — sdk-index env. Look for `models/Linux64_GCC-*/<BIN>`
    //    OR `<BIN>` directly under the root.
    if let Some(v) = std::env::var_os("ARM_FVP_DIR") {
        let root = PathBuf::from(&v);
        if let Some(hit) = find_fvp_under(&root, BIN) {
            eprintln!(
                "  [OK] arm-fvp {version}: $ARM_FVP_DIR = {} (binary at {})",
                root.display(),
                hit.display()
            );
            return;
        }
    }
    // 3. PATH fallback.
    if which(BIN).is_ok() {
        eprintln!("  [OK] arm-fvp {}: {BIN} on PATH", version);
        return;
    }
    // 4. Installer landing symlink.
    if !landing.as_os_str().is_empty() && landing.join(BIN).is_file() {
        eprintln!(
            "  [OK] arm-fvp {}: {} (installer landing)",
            version,
            landing.display()
        );
        return;
    }
    // Miss — WARN only (gated, never a hard fail).
    eprintln!(
        "  [WARN] arm-fvp {version}: {BIN} not found — run `nros setup --tool \
         arm-fvp` (x86_64 Linux only), or point $ARMFVP_BIN_PATH / $ARM_FVP_DIR \
         at an install you already have.\n\
         \x20        This used to say \"never auto-fetched\"; it is fetched now — \
         the model is a public Arm CDN permalink with a pinned digest, not a \
         licence wall."
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

    // Issue 0951 — IMAGES are the buildable unit (RFC-0065 D6), so they get
    // their own section rather than being folded into the deploy display: the
    // two tables answer different questions, and a workspace that has finished
    // migrating has an empty deploy table and everything to say here.
    //
    // The launch check delegates to `validate_image_launch` rather than
    // re-deriving the path. That function already knows an image's `launch` is
    // relative to the bringup's `launch/` directory — a second implementation
    // of that join is how doctor would come to disagree with the builder about
    // whether a workspace is healthy — and its message lists the launch files
    // that DO exist, which is the thing a reader needs at that moment.
    let mut problems = 0usize;
    if !system.image.is_empty() {
        eprintln!("nros doctor: images ({})", config.display());
        for id in system.image.keys() {
            let img = system.image_for(id).expect("key just enumerated");
            let board = img.board.as_deref().unwrap_or("(derived)");
            match crate::orchestration::image::validate_image_launch(id, &img, &bringup_dir) {
                Ok(()) => eprintln!("  [OK] {id}: board={board}"),
                Err(e) => {
                    eprintln!("  [!!] {id}: board={board} — {e}");
                    problems += 1;
                }
            }
        }
    }

    if system.deploy.is_empty() {
        return Ok(Some(problems));
    }
    eprintln!("nros doctor: deploy targets ({})", config.display());
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

fn run_just_doctor(root: &Path, platform: Option<&str>) -> Result<()> {
    if which("just").is_err() {
        // phase-368 — absent `just` is the NORMAL user condition, not an
        // error: the deploy-target and license-gate checks above already ran
        // natively, and the recipes this would forward to are contributor
        // lanes (fixture builds, in-tree sweeps). Erroring here made a
        // user's `nros doctor` exit red AFTER its own checks passed, for a
        // tool the user track deliberately does not require.
        match platform {
            Some(p) => eprintln!(
                "nros doctor: skipped the contributor platform checks for `{p}` — \
                 they run via `just {p} doctor`, and `just` is not on PATH \
                 (contributor tooling; the native checks above already ran)."
            ),
            None => eprintln!(
                "nros doctor: skipped the contributor workspace checks — they run \
                 via `just doctor`, and `just` is not on PATH (contributor \
                 tooling; the native checks above already ran)."
            ),
        }
        return Ok(());
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
mod install_report_tests {
    use super::*;
    use crate::orchestration::{dispatch, pin};

    /// A store entry shaped like one `scripts/install.sh` writes.
    fn install_toolchain(store: &Path, version: &str) -> PathBuf {
        let bin = store.join("sdk").join("nros").join(version).join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let exe = bin.join("nros");
        std::fs::write(&exe, b"#!/bin/sh\nexit 0\n").unwrap();
        exe
    }

    /// A user's project directory — deliberately NOT inside a checkout, which
    /// a tempdir root guarantees (`dispatch::decide` proceeds unconditionally
    /// inside one, so a checkout would make every pin assertion vacuous).
    fn project(dir: &Path, rel: &str, pin_version: Option<&str>) -> PathBuf {
        let p = dir.join(rel);
        std::fs::create_dir_all(&p).unwrap();
        if let Some(v) = pin_version {
            pin::write(&p, v).unwrap();
        }
        p
    }

    /// `install_report` asks `dispatch::decide`, which reads these two names
    /// off the process env. Neither is ever set in a test run; assert it rather
    /// than let a stray export make the pin assertions vacuous.
    fn assert_dispatch_env_clean() {
        for v in [dispatch::DISPATCHED_ENV, dispatch::SKIP_ENV] {
            assert!(
                std::env::var_os(v).is_none(),
                "${v} is set — it would short-circuit the launcher decision \
                 this test is about"
            );
        }
    }

    /// The D12 acceptance: `nros doctor` with NO workspace answers about the
    /// INSTALL — launcher, store, pin resolution.
    ///
    /// Measured before this landed, from an empty directory:
    /// `Error: could not auto-detect the nano-ros workspace root; pass
    /// --workspace <path> explicitly`. A user who has just run `install.sh` has
    /// no workspace to pass, so the one person who needed the answer was the
    /// one person refused it.
    #[test]
    fn with_no_workspace_the_report_covers_launcher_store_and_pin() {
        assert_dispatch_env_clean();
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("store");
        let exe = install_toolchain(&store, "0.7.9");
        std::fs::create_dir_all(store.join("bin")).unwrap();
        std::fs::write(store.join("bin").join("nros"), b"#!/bin/sh\n").unwrap();
        let cwd = project(tmp.path(), "proj", None);

        let (lines, problems) = install_report(&store, &exe, &cwd);
        assert_eq!(problems, 0, "a healthy install has no problems:\n{lines:#?}");
        let text = lines.join("\n");
        for needle in ["store", "launcher", "running", "pin"] {
            assert!(
                text.contains(needle),
                "the install report must cover `{needle}`:\n{text}"
            );
        }
        assert!(
            text.contains(&store.display().to_string()),
            "the report must name the store it looked at:\n{text}"
        );
        assert!(
            text.contains("0.7.9"),
            "the report must name the version that is running:\n{text}"
        );
    }

    /// The one PROBLEM this report counts: a project pins a nano-ros the store
    /// cannot produce. That is a project which cannot be built, and it is
    /// exactly what a fresh user hits after cloning someone else's repo.
    #[test]
    fn a_pin_naming_an_uninstalled_toolchain_is_the_problem_it_counts() {
        assert_dispatch_env_clean();
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("store");
        let exe = install_toolchain(&store, "0.7.9");
        let cwd = project(tmp.path(), "proj", Some("0.9.9"));

        let (lines, problems) = install_report(&store, &exe, &cwd);
        assert_eq!(problems, 1, "the unresolvable pin must count:\n{lines:#?}");
        let text = lines.join("\n");
        assert!(text.contains("0.9.9"), "name the version wanted:\n{text}");
        assert!(
            text.contains(&cwd.join(pin::FILE_NAME).display().to_string()),
            "name the pin file that asked for it:\n{text}"
        );
        assert!(
            text.contains("install"),
            "a problem must carry a remedy:\n{text}"
        );
    }

    /// A pin the store CAN satisfy is not a problem, and the report says which
    /// binary would run. The verdict comes from `dispatch::decide`, so this
    /// also pins that the doctor and the launcher cannot disagree.
    #[test]
    fn a_resolvable_pin_names_the_binary_the_launcher_would_exec() {
        assert_dispatch_env_clean();
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("store");
        let exe = install_toolchain(&store, "0.7.9");
        let other = install_toolchain(&store, "0.9.9");
        let cwd = project(tmp.path(), "proj", Some("0.9.9"));

        let (lines, problems) = install_report(&store, &exe, &cwd);
        assert_eq!(problems, 0, "{lines:#?}");
        assert!(
            lines.join("\n").contains(&other.display().to_string()),
            "the report must name the binary the pin resolves to:\n{}",
            lines.join("\n")
        );
    }

    /// An empty store is a legitimate state — a contributor building the
    /// tree's own CLI has neither store nor front — so it is REPORTED and not
    /// counted. A doctor that fails on a working setup gets ignored.
    #[test]
    fn an_absent_store_is_reported_and_is_not_a_problem() {
        assert_dispatch_env_clean();
        let tmp = tempfile::tempdir().unwrap();
        let store = tmp.path().join("no-store-here");
        let exe = tmp.path().join("checkout").join("target/release/nros");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"#!/bin/sh\n").unwrap();
        let cwd = project(tmp.path(), "proj", None);

        let (lines, problems) = install_report(&store, &exe, &cwd);
        assert_eq!(problems, 0, "{lines:#?}");
        let text = lines.join("\n");
        assert!(
            text.contains("does not exist yet"),
            "an absent store must be SAID, not silently skipped:\n{text}"
        );
        assert!(
            text.contains("not a store toolchain"),
            "a checkout build must be named as such:\n{text}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC-0004 §4 — `check_deploy_targets` parses a bringup `system.toml`,
    /// reports each `[image.<id>]`, and flags one whose `launch`
    /// file is missing relative to the bringup dir.
    #[test]
    fn deploy_targets_flag_missing_launch_file() {
        let dir = crate::test_support::scratch_dir("doctor_deploy");
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

    /// Issue 0951 — images get their own section, and their launch check is
    /// `validate_image_launch`, so doctor and the builder cannot disagree
    /// about whether a workspace is healthy. Note the path base differs from
    /// the deploy side: an image's `launch` is relative to `launch/`.
    #[test]
    fn images_are_checked_against_their_launch_directory() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("system.toml");
        let head = "[system]\nname=\"d\"\nrmw=\"zenoh\"\ndomain_id=0\n";

        // No launch key at all — nothing to verify, no problem.
        std::fs::write(&cfg, format!("{head}[image.a]\nboard=\"native\"\n")).unwrap();
        assert_eq!(check_deploy_targets(&cfg).unwrap(), Some(0));

        // Names a file that does not exist.
        std::fs::write(
            &cfg,
            format!("{head}[image.a]\nboard=\"native\"\nlaunch=\"gone.launch.xml\"\n"),
        )
        .unwrap();
        assert_eq!(check_deploy_targets(&cfg).unwrap(), Some(1));

        // The same name, present — under `launch/`, which is the base an
        // image's value is relative to. A copy of the deploy side's join
        // (bringup_dir/<value>) would report this one missing.
        std::fs::create_dir_all(dir.path().join("launch")).unwrap();
        std::fs::write(dir.path().join("launch/gone.launch.xml"), "<launch/>").unwrap();
        assert_eq!(check_deploy_targets(&cfg).unwrap(), Some(0));
    }

    #[test]
    fn license_gate_flags_misconfigured_env_only() {
        let ws = crate::test_support::scratch_dir("gate");
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
        let ws = crate::test_support::scratch_dir("gate_fvp");
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
}
