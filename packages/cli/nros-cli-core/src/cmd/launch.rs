//! Phase 212.J — `nros launch` host-side launcher.
//!
//! Reads a bringup pkg's `system.toml` (+ optional resolved
//! `nros-plan.json`) and spawns one host process per `[[component]]` with
//! the env the runtime expects (`NROS_LOCATOR`, `ROS_DOMAIN_ID`, +
//! per-component parameters / remaps from the component crate's
//! `[package.metadata.nros.component]`).
//!
//! Lets users `nros launch <bringup>` on a desktop / native_sim host
//! without depending on the ament index — `ros2 launch` requires
//! `colcon build && source install/setup.bash`, this doesn't. This is
//! the **canonical desktop launcher** for development; `ros2 launch`
//! remains available for ament-installed consumers (separate path, no
//! overlap). See `docs/design/multi-node-workspace-layout.md` §11 for
//! the role of bringup pkgs and how `nros launch` reads `launch/` from
//! the source tree, not from an ament install share path.
//!
//! Scope: **hosted/native targets only**. A `[deploy.<target>]` whose
//! `kind` is `"qemu"` / `"flash"` is a clear error — those need their own
//! runner (`nros run`), not host process spawn.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use clap::Args as ClapArgs;
use eyre::{Result, WrapErr, bail, eyre};

use crate::orchestration::cargo_metadata_schema::{
    ComponentMetadata, PackageMetadataNros, SystemToml, WorkspaceMetadataNros,
};

const DEFAULT_LOCATOR: &str = "tcp/127.0.0.1:7447";

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// Bringup package directory (or pkg name, looked up under the workspace
    /// root). Omit to use the workspace's
    /// `[workspace.metadata.nros].default_system`.
    pub bringup: Option<String>,

    /// Workspace root holding the cargo workspace `Cargo.toml`. Defaults to
    /// the current dir.
    #[arg(long)]
    pub workspace_root: Option<PathBuf>,

    /// `[deploy.<target>]` to use. When unset, the launcher picks
    /// `[system].default_target` (Phase 212.J.2) if present, else falls
    /// back to `"native"` if that block exists, else the first deploy
    /// entry in sorted order. Empty `[deploy]` is allowed — the launcher
    /// falls back to baked defaults.
    #[arg(long)]
    pub target: Option<String>,

    /// Cargo profile dir under `<workspace>/target/<profile>/<exec>`.
    #[arg(long, default_value = "debug")]
    pub profile: String,

    /// Foreground mode: block until first child exits or signal arrives, then
    /// propagate SIGTERM to all children. Mutually exclusive with
    /// `--detach`. Default when neither is set.
    #[arg(long, conflicts_with_all = ["detach", "stop"])]
    pub foreground: bool,

    /// Detach mode: write `<ws>/.nros/launch/<bringup>.pids` and return
    /// immediately. The user later runs `nros launch --stop <pidfile>`.
    #[arg(long, conflicts_with_all = ["foreground", "stop"])]
    pub detach: bool,

    /// Stop a previously-detached launch by sending SIGTERM to every PID in
    /// the given pidfile.
    #[arg(long, value_name = "PID-FILE")]
    pub stop: Option<PathBuf>,

    /// Phase 212.L.6 — multi-launch disambiguation. The launcher does not
    /// drive component spawn from XML today (it spawns from `system.toml`'s
    /// `[[component]]` list), but accepting `--file` keeps the verb
    /// surface uniform with `nros plan` / `nros codegen-system`, and
    /// invokes the shared resolver so bad input fails fast.
    #[arg(long = "file")]
    pub file: Option<String>,

    /// Phase 212.L.6 — `<node exec="…">` override for synthesised
    /// launches. Same uniform-surface motivation as `--file`.
    #[arg(long = "exec")]
    pub exec: Option<String>,
}

pub fn run(args: Args) -> Result<()> {
    if let Some(pidfile) = args.stop.as_ref() {
        return stop_pidfile(pidfile);
    }

    let workspace_root = args
        .workspace_root
        .clone()
        .map(|p| p.canonicalize().unwrap_or(p))
        .unwrap_or_else(|| std::env::current_dir().expect("current dir readable for nros launch"));

    let bringup_dir = resolve_bringup_dir(&workspace_root, args.bringup.as_deref())?;

    // Phase 212.L.6 — when `--file` or `--exec` are passed, run the
    // shared resolver to validate the launch input early. The host
    // launcher itself spawns from `system.toml::[[component]]`, not the
    // launch XML, so a successful resolve is purely a sanity check;
    // a failure surfaces with the resolver's specific error.
    if args.file.is_some() || args.exec.is_some() {
        let _ = crate::orchestration::launch_synth::resolve_launch(
            &bringup_dir,
            args.file.as_deref(),
            args.exec.as_deref(),
        )?;
    }

    let (system, target_name, target) = load_plan(&bringup_dir, args.target.as_deref())?;

    // Embedded deploy kinds aren't host-spawnable. Surface a clean error.
    if !is_host_kind(&target.kind) {
        bail!(
            "nros launch: [deploy.{target_name}].kind = \"{}\" is not a host target — \
             use `nros run` for embedded deploys (qemu/flash/…)",
            target.kind
        );
    }

    let locator = system
        .system
        .locator
        .clone()
        .unwrap_or_else(|| DEFAULT_LOCATOR.to_string());
    let domain_id = system.system.domain_id;

    let mut commands: Vec<PreparedCommand> = Vec::new();
    for comp in &system.components {
        let prepared =
            prepare_component(&workspace_root, &args.profile, comp, &locator, domain_id)?;
        commands.push(prepared);
    }

    if commands.is_empty() {
        bail!(
            "nros launch: bringup {} declares no [[component]] entries — nothing to spawn",
            bringup_dir.display()
        );
    }

    // Default to foreground when neither flag is set.
    if args.detach {
        spawn_detached(&workspace_root, &bringup_dir, &commands)
    } else {
        spawn_foreground(&commands)
    }
}

/// Resolve the bringup dir from an explicit arg + workspace metadata fallback.
fn resolve_bringup_dir(workspace_root: &Path, arg: Option<&str>) -> Result<PathBuf> {
    if let Some(raw) = arg {
        // Absolute / relative path — accept directly when it's a dir.
        let as_path = Path::new(raw);
        let candidate = if as_path.is_absolute() {
            as_path.to_path_buf()
        } else {
            workspace_root.join(as_path)
        };
        if candidate.is_dir() {
            return Ok(candidate);
        }
        // Otherwise treat as a pkg name relative to workspace root.
        let by_name = workspace_root.join(raw);
        if by_name.is_dir() {
            return Ok(by_name);
        }
        bail!(
            "nros launch: bringup `{raw}` not found under {}",
            workspace_root.display()
        );
    }

    // Fall back to [workspace.metadata.nros].default_system.
    let cargo_toml = workspace_root.join("Cargo.toml");
    if !cargo_toml.is_file() {
        bail!(
            "nros launch: no bringup arg given and no Cargo.toml at {} — \
             pass <bringup> explicitly",
            workspace_root.display()
        );
    }
    let raw = fs::read_to_string(&cargo_toml)
        .wrap_err_with(|| format!("read {}", cargo_toml.display()))?;
    let default_system = read_default_system(&raw).ok_or_else(|| {
        eyre!(
            "nros launch: no `[workspace.metadata.nros] default_system` in {} — \
             pass <bringup> explicitly",
            cargo_toml.display()
        )
    })?;
    let dir = workspace_root.join(&default_system);
    if !dir.is_dir() {
        bail!(
            "nros launch: default_system = \"{default_system}\" but {} is not a directory",
            dir.display()
        );
    }
    Ok(dir)
}

/// Pull the `default_system` field from a workspace `Cargo.toml` (in the
/// `[workspace.metadata.nros]` table). Returns `None` when absent.
fn read_default_system(cargo_toml_raw: &str) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct Outer {
        workspace: Option<WorkspaceTable>,
    }
    #[derive(serde::Deserialize)]
    struct WorkspaceTable {
        metadata: Option<MetadataTable>,
    }
    #[derive(serde::Deserialize)]
    struct MetadataTable {
        nros: Option<WorkspaceMetadataNros>,
    }
    let outer: Outer = toml::from_str(cargo_toml_raw).ok()?;
    outer.workspace?.metadata?.nros?.default_system
}

/// Load the bringup's `system.toml` and pick a deploy target.
fn load_plan(
    bringup_dir: &Path,
    target_arg: Option<&str>,
) -> Result<(SystemToml, String, DeployTargetView)> {
    let system_toml_path = bringup_dir.join("system.toml");
    let raw = fs::read_to_string(&system_toml_path)
        .wrap_err_with(|| format!("read {}", system_toml_path.display()))?;
    let system: SystemToml =
        toml::from_str(&raw).wrap_err_with(|| format!("parse {}", system_toml_path.display()))?;

    // Pick the target. Phase 212.J.2 resolution order:
    //   1. explicit `--target` arg,
    //   2. `[system].default_target`,
    //   3. `[deploy.native]` if present,
    //   4. first deploy entry in sorted order (BTreeMap keys are sorted),
    //   5. synthesise a `native` self-host default when `[deploy]` is empty.
    let (target_name, target) = match target_arg {
        Some(name) => {
            let t = system.deploy.get(name).ok_or_else(|| {
                eyre!(
                    "nros launch: no [deploy.{name}] in {}",
                    system_toml_path.display()
                )
            })?;
            (
                name.to_string(),
                DeployTargetView {
                    kind: t.kind.clone().unwrap_or_default(),
                },
            )
        }
        None => {
            if let Some(default_name) = system.system.default_target.as_deref() {
                let t = system.deploy.get(default_name).ok_or_else(|| {
                    eyre!(
                        "nros launch: [system].default_target = \"{default_name}\" but no \
                         matching [deploy.{default_name}] in {}",
                        system_toml_path.display()
                    )
                })?;
                (
                    default_name.to_string(),
                    DeployTargetView {
                        kind: t.kind.clone().unwrap_or_default(),
                    },
                )
            } else if let Some(t) = system.deploy.get("native") {
                (
                    "native".to_string(),
                    DeployTargetView {
                        kind: t.kind.clone().unwrap_or_default(),
                    },
                )
            } else {
                match system.deploy.iter().next() {
                    Some((n, t)) => (
                        n.clone(),
                        DeployTargetView {
                            kind: t.kind.clone().unwrap_or_default(),
                        },
                    ),
                    None => (
                        "native".to_string(),
                        DeployTargetView {
                            kind: "self".to_string(),
                        },
                    ),
                }
            }
        }
    };
    Ok((system, target_name, target))
}

/// Subset of `DeployTarget` the launcher cares about. Keeps the launch
/// module decoupled from any per-target schema churn.
#[derive(Clone, Debug)]
struct DeployTargetView {
    kind: String,
}

/// Host-spawnable deploy kinds. The launcher is desktop / native_sim only.
fn is_host_kind(kind: &str) -> bool {
    matches!(kind, "self" | "native" | "host" | "")
}

/// One prepared `Command` for a component, with the env baked in.
struct PreparedCommand {
    pkg: String,
    binary: PathBuf,
    env: Vec<(String, String)>,
}

fn prepare_component(
    workspace_root: &Path,
    profile: &str,
    comp: &crate::orchestration::cargo_metadata_schema::SystemComponentEntry,
    locator: &str,
    domain_id: u32,
) -> Result<PreparedCommand> {
    let binary = resolve_binary(workspace_root, profile, &comp.pkg);
    let (params, remaps, namespace) = load_component_meta(workspace_root, &comp.pkg, &comp.class)?;

    let mut env: Vec<(String, String)> = Vec::new();
    env.push(("NROS_LOCATOR".to_string(), locator.to_string()));
    env.push(("ROS_DOMAIN_ID".to_string(), domain_id.to_string()));
    if let Some(ns) = namespace {
        env.push(("NROS_NAMESPACE".to_string(), ns));
    }
    if !params.is_empty() {
        env.push(("NROS_PARAMETERS".to_string(), encode_params(&params)));
    }
    if !remaps.is_empty() {
        env.push(("NROS_REMAPS".to_string(), encode_remaps(&remaps)));
    }
    Ok(PreparedCommand {
        pkg: comp.pkg.clone(),
        binary,
        env,
    })
}

fn resolve_binary(workspace_root: &Path, profile: &str, pkg: &str) -> PathBuf {
    workspace_root.join("target").join(profile).join(pkg)
}

/// Read `<pkg>/Cargo.toml` and surface the resolved component's parameters /
/// remaps / default_namespace. Single-shape vs multi-shape:
/// for multi-shape we match by `class` (the system.toml component's class
/// field; for the multi case the user encodes the variant name in `class`,
/// e.g. `talker_pkg::Talker` → look up `Talker`).
fn load_component_meta(
    workspace_root: &Path,
    pkg: &str,
    class: &str,
) -> Result<(
    std::collections::BTreeMap<String, toml::Value>,
    Vec<crate::orchestration::schema::RemapRule>,
    Option<String>,
)> {
    let cargo_toml = workspace_root.join(pkg).join("Cargo.toml");
    if !cargo_toml.is_file() {
        // Missing component manifest is not fatal — the user may have a
        // standalone binary that doesn't carry the metadata table. We still
        // spawn it with just NROS_LOCATOR/ROS_DOMAIN_ID.
        return Ok((Default::default(), Vec::new(), None));
    }
    let raw = fs::read_to_string(&cargo_toml)
        .wrap_err_with(|| format!("read {}", cargo_toml.display()))?;

    #[derive(serde::Deserialize)]
    struct Outer {
        package: Option<PackageTable>,
    }
    #[derive(serde::Deserialize)]
    struct PackageTable {
        metadata: Option<MetadataTable>,
    }
    #[derive(serde::Deserialize)]
    struct MetadataTable {
        nros: Option<PackageMetadataNros>,
    }
    let outer: Outer =
        toml::from_str(&raw).wrap_err_with(|| format!("parse {}", cargo_toml.display()))?;
    let Some(meta) = outer.package.and_then(|p| p.metadata).and_then(|m| m.nros) else {
        return Ok((Default::default(), Vec::new(), None));
    };
    if let Err(msg) = meta.validate() {
        bail!(
            "nros launch: invalid [package.metadata.nros] in {}: {msg}",
            cargo_toml.display()
        );
    }
    let resolved: Option<ComponentMetadata> = if let Some(c) = meta.component {
        Some(c)
    } else if !meta.components.is_empty() {
        // Match by short class name (the part after `::`).
        let short = class.rsplit("::").next().unwrap_or(class);
        meta.components.get(short).cloned()
    } else {
        None
    };
    Ok(match resolved {
        Some(c) => (c.parameters, c.remaps, c.default_namespace),
        None => (Default::default(), Vec::new(), None),
    })
}

/// Encode parameters as `key=value;…` (TOML-encoded value). Stable env shape
/// the launched binary can parse; the binary owns its own param schema.
fn encode_params(params: &std::collections::BTreeMap<String, toml::Value>) -> String {
    params
        .iter()
        .map(|(k, v)| format!("{k}={}", encode_toml_value(v)))
        .collect::<Vec<_>>()
        .join(";")
}

fn encode_toml_value(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn encode_remaps(remaps: &[crate::orchestration::schema::RemapRule]) -> String {
    remaps
        .iter()
        .map(|r| format!("{}:={}", r.from, r.to))
        .collect::<Vec<_>>()
        .join(";")
}

// ---------------------------------------------------------------------------
// Foreground spawn — blocks until first child exits or SIGINT/SIGTERM.
// ---------------------------------------------------------------------------

fn spawn_foreground(commands: &[PreparedCommand]) -> Result<()> {
    let children = Arc::new(Mutex::new(Vec::<Child>::new()));
    for cmd in commands {
        let child = spawn_one(cmd, false)?;
        eprintln!("nros launch: spawned {} (pid {})", cmd.pkg, child.id());
        children.lock().unwrap().push(child);
    }

    let stop = install_signal_handler()?;

    // Poll loop: stop on first child exit OR signal.
    loop {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let mut guard = children.lock().unwrap();
        let mut any_exited = false;
        for child in guard.iter_mut() {
            match child.try_wait() {
                Ok(Some(_)) => {
                    any_exited = true;
                    break;
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("nros launch: try_wait failed: {e}");
                    any_exited = true;
                    break;
                }
            }
        }
        drop(guard);
        if any_exited {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    // Phase 212.J.3 — Propagate SIGTERM, wait up to GRACE_PERIOD for clean
    // shutdown, then escalate to SIGKILL for stragglers.
    const GRACE_PERIOD: Duration = Duration::from_secs(5);
    let mut guard = children.lock().unwrap();
    for child in guard.iter_mut() {
        send_sigterm(child.id() as i32);
    }
    let deadline = std::time::Instant::now() + GRACE_PERIOD;
    loop {
        let mut all_done = true;
        for child in guard.iter_mut() {
            match child.try_wait() {
                Ok(Some(_)) | Err(_) => {}
                Ok(None) => all_done = false,
            }
        }
        if all_done || std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    // Escalate: SIGKILL anything still alive, then reap.
    for child in guard.iter_mut() {
        if matches!(child.try_wait(), Ok(None)) {
            send_sigkill(child.id() as i32);
        }
    }
    for child in guard.iter_mut() {
        let _ = child.wait();
    }
    Ok(())
}

/// Install a SIGINT/SIGTERM handler that flips a shared flag. The flag is a
/// process-global `OnceLock<Arc<AtomicBool>>` — the libc signal handler is
/// extern "C" + has no closure state, so the flag must live in static storage.
/// First call installs the handler; subsequent calls just hand back the same
/// flag (so `run()` can be re-invoked from tests).
#[cfg(unix)]
fn install_signal_handler() -> Result<Arc<AtomicBool>> {
    use std::sync::OnceLock;
    static SIGNAL_FLAG: OnceLock<Arc<AtomicBool>> = OnceLock::new();

    extern "C" fn handler(_sig: libc::c_int) {
        // Async-signal-safe: only an atomic store on an already-init OnceLock.
        if let Some(f) = STATIC_FLAG_FOR_HANDLER.get() {
            f.store(true, Ordering::SeqCst);
        }
    }
    // Mirror of SIGNAL_FLAG, accessible to the handler. They point at the
    // same Arc once the OnceLock is initialised.
    static STATIC_FLAG_FOR_HANDLER: OnceLock<Arc<AtomicBool>> = OnceLock::new();

    let flag = SIGNAL_FLAG
        .get_or_init(|| {
            let f = Arc::new(AtomicBool::new(false));
            let _ = STATIC_FLAG_FOR_HANDLER.set(f.clone());
            unsafe {
                libc::signal(libc::SIGINT, handler as *const () as libc::sighandler_t);
                libc::signal(libc::SIGTERM, handler as *const () as libc::sighandler_t);
            }
            f
        })
        .clone();
    // Each invocation resets the flag so a prior signal doesn't poison the
    // next run (relevant in tests).
    flag.store(false, Ordering::SeqCst);
    Ok(flag)
}

#[cfg(not(unix))]
fn install_signal_handler() -> Result<Arc<AtomicBool>> {
    Ok(Arc::new(AtomicBool::new(false)))
}

#[cfg(unix)]
fn send_sigterm(pid: i32) {
    unsafe {
        let _ = libc::kill(pid, libc::SIGTERM);
    }
}

#[cfg(not(unix))]
fn send_sigterm(_pid: i32) {}

#[cfg(unix)]
fn send_sigkill(pid: i32) {
    unsafe {
        let _ = libc::kill(pid, libc::SIGKILL);
    }
}

#[cfg(not(unix))]
fn send_sigkill(_pid: i32) {}

// ---------------------------------------------------------------------------
// Detach spawn — write PID file, return immediately.
// ---------------------------------------------------------------------------

fn spawn_detached(
    workspace_root: &Path,
    bringup_dir: &Path,
    commands: &[PreparedCommand],
) -> Result<()> {
    let mut pids: Vec<u32> = Vec::with_capacity(commands.len());
    let mut spawned: Vec<Child> = Vec::with_capacity(commands.len());
    for cmd in commands {
        let child = spawn_one(cmd, true)?;
        pids.push(child.id());
        eprintln!("nros launch: spawned {} (pid {})", cmd.pkg, child.id());
        spawned.push(child);
    }

    let pidfile = pidfile_path(workspace_root, bringup_dir);
    if let Some(parent) = pidfile.parent() {
        fs::create_dir_all(parent).wrap_err_with(|| format!("create {}", parent.display()))?;
    }
    let body = render_pidfile(std::process::id(), &pids);
    fs::write(&pidfile, body).wrap_err_with(|| format!("write {}", pidfile.display()))?;
    eprintln!("nros launch: detached; pidfile {}", pidfile.display());

    // We intentionally drop `spawned` without waiting — the children are
    // now daemonized from this process's perspective.
    std::mem::forget(spawned);
    Ok(())
}

fn pidfile_path(workspace_root: &Path, bringup_dir: &Path) -> PathBuf {
    let stem = bringup_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("bringup");
    // Phase 212.J.3 — `.nros/launch/<bringup>.pids` (NOT under `target/`,
    // which `cargo clean` blasts away mid-run).
    workspace_root
        .join(".nros")
        .join("launch")
        .join(format!("{stem}.pids"))
}

/// Pidfile shape: one PID per line. First line is the launcher's own PID
/// (so `--stop` can be filtered to ignore it if needed); subsequent lines
/// are children, one per `[[component]]` in declaration order.
fn render_pidfile(parent: u32, children: &[u32]) -> String {
    let mut s = String::new();
    s.push_str(&format!("# nros launch — Phase 212.J\nparent={parent}\n"));
    for pid in children {
        s.push_str(&format!("{pid}\n"));
    }
    s
}

fn parse_pidfile(body: &str) -> Vec<i32> {
    body.lines()
        .filter_map(|line| {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') || t.starts_with("parent=") {
                None
            } else {
                t.parse::<i32>().ok()
            }
        })
        .collect()
}

fn stop_pidfile(pidfile: &Path) -> Result<()> {
    let body =
        fs::read_to_string(pidfile).wrap_err_with(|| format!("read {}", pidfile.display()))?;
    let pids = parse_pidfile(&body);
    if pids.is_empty() {
        bail!("nros launch --stop: {} has no PIDs", pidfile.display());
    }
    for pid in pids {
        send_sigterm(pid);
        eprintln!("nros launch: sent SIGTERM to {pid}");
    }
    // Phase 212.J.3.b — drop the pidfile so a second `--stop` doesn't
    // try to re-signal stale PIDs (the OS may have recycled them).
    if let Err(e) = fs::remove_file(pidfile) {
        eprintln!(
            "nros launch: warning: failed to remove {}: {e}",
            pidfile.display()
        );
    }
    Ok(())
}

/// Spawn one prepared command. `detached` suppresses inherited stdio so the
/// child survives a parent exit cleanly.
fn spawn_one(cmd: &PreparedCommand, detached: bool) -> Result<Child> {
    let mut c = Command::new(&cmd.binary);
    // Clear inherited env then re-add explicitly — the launcher is the SSOT
    // for runtime env (NROS_LOCATOR / ROS_DOMAIN_ID / params / remaps). The
    // caller's PATH is preserved because the binary is invoked by absolute
    // path (no PATH search).
    c.env_clear();
    // Preserve a minimal env so children can find shared libs / use sh
    // (`LD_LIBRARY_PATH`, `PATH`, `HOME`, locale).
    for key in ["PATH", "HOME", "USER", "LANG", "LC_ALL", "LD_LIBRARY_PATH"] {
        if let Ok(v) = std::env::var(key) {
            c.env(key, v);
        }
    }
    for (k, v) in &cmd.env {
        c.env(k, v);
    }
    if detached {
        c.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    } else {
        c.stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
    }
    c.spawn().wrap_err_with(|| {
        format!(
            "nros launch: spawn {} ({}) failed — is the binary built? \
             try `cargo build -p {}`",
            cmd.pkg,
            cmd.binary.display(),
            cmd.pkg
        )
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch(tag: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("nros-launch-{tag}-{}-{stamp}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Write a `<dir>/<exec>` shell script that prints + sleeps + writes a
    /// marker file when run. Used as a stand-in component binary in tests.
    fn write_stub_exec(dir: &Path, exec: &str, marker: &Path) {
        let script = format!(
            "#!/bin/sh\necho started >\"{marker}\"\nwhile true; do sleep 1; done\n",
            marker = marker.display()
        );
        let p = dir.join(exec);
        fs::write(&p, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Stub that writes its env to a file then exits (for env-propagation test).
    fn write_env_dump_exec(dir: &Path, exec: &str, dump: &Path) {
        let script = format!("#!/bin/sh\nenv >\"{}\"\n", dump.display(),);
        let p = dir.join(exec);
        fs::write(&p, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// Build a fixture workspace with a bringup pkg + two components.
    fn write_fixture(root: &Path, bringup_name: &str, comps: &[(&str, &str)], deploy_kind: &str) {
        // Workspace Cargo.toml carrying `default_system`.
        fs::write(
            root.join("Cargo.toml"),
            format!(
                "[workspace]\nresolver = \"2\"\nmembers = []\n\
                 [workspace.metadata.nros]\ndefault_system = \"{bringup_name}\"\n"
            ),
        )
        .unwrap();

        // Bringup pkg.
        let bringup = root.join(bringup_name);
        fs::create_dir_all(bringup.join("launch")).unwrap();
        fs::write(bringup.join("package.xml"), "<?xml version=\"1.0\"?><package format=\"3\"><name>bringup</name><version>0.1.0</version></package>").unwrap();
        let mut sys = String::new();
        sys.push_str("[system]\nname=\"demo\"\nrmw=\"zenoh\"\ndomain_id=7\n");
        sys.push_str("locator=\"tcp/127.0.0.1:7450\"\n");
        for (pkg, class) in comps {
            sys.push_str(&format!(
                "[[component]]\npkg = \"{pkg}\"\nclass = \"{class}\"\nname = \"{pkg}\"\n"
            ));
        }
        sys.push_str(&format!(
            "[deploy.native]\nkind = \"{deploy_kind}\"\ntarget = \"x86_64-unknown-linux-gnu\"\n"
        ));
        fs::write(bringup.join("system.toml"), sys).unwrap();
        fs::write(bringup.join("launch/system.launch.xml"), "<launch/>").unwrap();
    }

    #[test]
    fn nros_launch_spawns_components() {
        let root = scratch("spawn");
        write_fixture(
            &root,
            "demo_bringup",
            &[
                ("talker_pkg", "talker_pkg::Talker"),
                ("listener_pkg", "listener_pkg::Listener"),
            ],
            "self",
        );
        // Pretend-built binaries: write executables to target/debug/<pkg>.
        let bin_dir = root.join("target/debug");
        fs::create_dir_all(&bin_dir).unwrap();
        let marker_a = root.join("talker.marker");
        let marker_b = root.join("listener.marker");
        write_stub_exec(&bin_dir, "talker_pkg", &marker_a);
        write_stub_exec(&bin_dir, "listener_pkg", &marker_b);

        // Spawn in foreground from a background thread; signal it to stop.
        let root_clone = root.clone();
        let handle = std::thread::spawn(move || {
            let args = Args {
                bringup: Some("demo_bringup".to_string()),
                workspace_root: Some(root_clone),
                target: None,
                profile: "debug".to_string(),
                foreground: true,
                detach: false,
                stop: None,
                file: None,
                exec: None,
            };
            run(args).unwrap();
        });

        // Wait until both markers appear (children started).
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if marker_a.exists() && marker_b.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(marker_a.exists(), "talker did not start");
        assert!(marker_b.exists(), "listener did not start");

        // Send SIGTERM to ourselves — handler flips the stop flag → launcher
        // propagates SIGTERM to children → join.
        unsafe {
            libc::kill(std::process::id() as i32, libc::SIGTERM);
        }
        handle.join().expect("foreground launcher joined cleanly");
    }

    #[test]
    fn nros_launch_detach_writes_pid_file() {
        let root = scratch("detach");
        write_fixture(
            &root,
            "demo_bringup",
            &[("talker_pkg", "talker_pkg::Talker")],
            "self",
        );
        let bin_dir = root.join("target/debug");
        fs::create_dir_all(&bin_dir).unwrap();
        let marker = root.join("talker.marker");
        write_stub_exec(&bin_dir, "talker_pkg", &marker);

        let args = Args {
            bringup: Some("demo_bringup".to_string()),
            workspace_root: Some(root.clone()),
            target: None,
            profile: "debug".to_string(),
            foreground: false,
            detach: true,
            stop: None,
            file: None,
            exec: None,
        };
        run(args).expect("detach run");

        let pidfile = root.join(".nros/launch/demo_bringup.pids");
        assert!(
            pidfile.is_file(),
            "pidfile not written: {}",
            pidfile.display()
        );
        let body = fs::read_to_string(&pidfile).unwrap();
        let pids = parse_pidfile(&body);
        assert_eq!(pids.len(), 1, "expected 1 child PID, got {pids:?}");
        assert!(
            body.contains("parent="),
            "pidfile lacks parent= header: {body}"
        );

        // --stop sends SIGTERM. The detached child is a `while true; do sleep`
        // loop; SIGTERM kills it.
        let stop = Args {
            bringup: None,
            workspace_root: Some(root.clone()),
            target: None,
            profile: "debug".to_string(),
            foreground: false,
            detach: false,
            stop: Some(pidfile.clone()),
            file: None,
            exec: None,
        };
        run(stop).expect("stop run");
        // Give the OS a moment to reap.
        std::thread::sleep(Duration::from_millis(200));
        // No assertion on process state — the test verifies --stop runs cleanly
        // and parse_pidfile finds the PID. Reap any orphan to keep the test
        // host clean.
        for pid in pids {
            unsafe {
                let _ = libc::kill(pid, libc::SIGKILL);
            }
        }
    }

    #[test]
    fn nros_launch_uses_default_system_when_no_arg() {
        let root = scratch("default_system");
        write_fixture(
            &root,
            "demo_bringup",
            &[("talker_pkg", "talker_pkg::Talker")],
            "self",
        );
        // No need to build the binary — we just exercise resolution.
        let dir = resolve_bringup_dir(&root, None).expect("default_system resolves");
        assert_eq!(dir, root.join("demo_bringup"));
    }

    #[test]
    fn nros_launch_resolve_bringup_explicit_path_wins() {
        let root = scratch("explicit");
        write_fixture(
            &root,
            "alt_bringup",
            &[("talker_pkg", "talker_pkg::Talker")],
            "self",
        );
        let dir = resolve_bringup_dir(&root, Some("alt_bringup")).expect("by name resolves");
        assert_eq!(dir, root.join("alt_bringup"));
    }

    #[test]
    fn nros_launch_propagates_env_vars() {
        let root = scratch("env_vars");
        write_fixture(
            &root,
            "demo_bringup",
            &[("talker_pkg", "talker_pkg::Talker")],
            "self",
        );
        let bin_dir = root.join("target/debug");
        fs::create_dir_all(&bin_dir).unwrap();
        let dump = root.join("env.dump");
        write_env_dump_exec(&bin_dir, "talker_pkg", &dump);

        // Run foreground — env-dump stub exits immediately, which trips the
        // "first child exited" path and returns.
        let args = Args {
            bringup: Some("demo_bringup".to_string()),
            workspace_root: Some(root.clone()),
            target: None,
            profile: "debug".to_string(),
            foreground: true,
            detach: false,
            stop: None,
            file: None,
            exec: None,
        };
        run(args).expect("foreground env-dump");

        // Wait for the dump file to materialize.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if dump.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(dump.exists(), "env dump never written");
        let body = fs::read_to_string(&dump).unwrap();
        assert!(
            body.contains("NROS_LOCATOR=tcp/127.0.0.1:7450"),
            "NROS_LOCATOR missing: {body}"
        );
        assert!(
            body.contains("ROS_DOMAIN_ID=7"),
            "ROS_DOMAIN_ID missing: {body}"
        );
    }

    #[test]
    fn nros_launch_rejects_qemu_kind() {
        let root = scratch("qemu_reject");
        write_fixture(
            &root,
            "demo_bringup",
            &[("talker_pkg", "talker_pkg::Talker")],
            "qemu",
        );
        let args = Args {
            bringup: Some("demo_bringup".to_string()),
            workspace_root: Some(root.clone()),
            target: None,
            profile: "debug".to_string(),
            foreground: true,
            detach: false,
            stop: None,
            file: None,
            exec: None,
        };
        let err = run(args).unwrap_err().to_string();
        assert!(err.contains("not a host target"), "diagnostic: {err}");
        assert!(err.contains("nros run"), "diagnostic: {err}");
    }

    #[test]
    fn read_default_system_walks_workspace_metadata_table() {
        let raw = "[workspace]\nresolver=\"2\"\n[workspace.metadata.nros]\ndefault_system=\"demo_bringup\"\n";
        assert_eq!(read_default_system(raw).as_deref(), Some("demo_bringup"));
        assert!(read_default_system("[workspace]\nmembers=[]\n").is_none());
    }

    #[test]
    fn pidfile_round_trip_keeps_child_pids_drops_parent_header() {
        let body = render_pidfile(1234, &[42, 99]);
        assert!(body.contains("parent=1234"));
        let pids = parse_pidfile(&body);
        assert_eq!(pids, vec![42, 99]);
    }

    #[test]
    fn encode_remaps_emits_ros_remap_syntax() {
        use crate::orchestration::schema::RemapRule;
        let remaps = vec![
            RemapRule {
                from: "chatter".into(),
                to: "topic/chatter".into(),
            },
            RemapRule {
                from: "tf".into(),
                to: "tf_static".into(),
            },
        ];
        assert_eq!(
            encode_remaps(&remaps),
            "chatter:=topic/chatter;tf:=tf_static"
        );
    }

    #[test]
    fn encode_params_emits_key_value_pairs() {
        let mut p = std::collections::BTreeMap::new();
        p.insert("rate_hz".to_string(), toml::Value::Integer(10));
        p.insert("greeting".to_string(), toml::Value::String("hi".into()));
        let enc = encode_params(&p);
        // BTreeMap iteration is sorted.
        assert_eq!(enc, "greeting=hi;rate_hz=10");
    }

    /// Phase 212.J.2 — write a bringup with multiple deploy blocks (and an
    /// optional `[system].default_target`). Verifies:
    ///   * explicit `--target` wins,
    ///   * absent CLI arg + `default_target` set → that block wins,
    ///   * absent CLI arg + no `default_target` + `[deploy.native]` present
    ///     → `native` wins (over alphabetically-earlier siblings).
    fn write_multi_target_fixture(root: &Path, bringup: &str, default_target: Option<&str>) {
        fs::write(
            root.join("Cargo.toml"),
            format!(
                "[workspace]\nresolver = \"2\"\nmembers = []\n\
                 [workspace.metadata.nros]\ndefault_system = \"{bringup}\"\n"
            ),
        )
        .unwrap();
        let dir = root.join(bringup);
        fs::create_dir_all(dir.join("launch")).unwrap();
        fs::write(dir.join("package.xml"),
            "<?xml version=\"1.0\"?><package format=\"3\"><name>bringup</name><version>0.1.0</version></package>").unwrap();
        let mut sys = String::new();
        sys.push_str("[system]\nname=\"demo\"\nrmw=\"zenoh\"\ndomain_id=7\n");
        sys.push_str("locator=\"tcp/127.0.0.1:7450\"\n");
        if let Some(t) = default_target {
            sys.push_str(&format!("default_target = \"{t}\"\n"));
        }
        sys.push_str("[[component]]\npkg = \"talker_pkg\"\nclass = \"talker_pkg::Talker\"\nname = \"talker_pkg\"\n");
        // Deliberately list `native` LAST so we know any selection of `native`
        // is NOT just `BTreeMap.iter().next()` falling out — must be explicit.
        sys.push_str("[deploy.aaa_first_alpha]\nkind = \"self\"\n");
        sys.push_str("[deploy.qemu-mps2-an385]\nkind = \"qemu\"\n");
        sys.push_str("[deploy.native]\nkind = \"self\"\n");
        fs::write(dir.join("system.toml"), sys).unwrap();
        fs::write(dir.join("launch/system.launch.xml"), "<launch/>").unwrap();
    }

    #[test]
    fn nros_launch_target_picks_deploy_slice() {
        let root = scratch("target_slice_default");
        // No [system].default_target → expect `native` chosen over the
        // alphabetically-earlier `aaa_first_alpha`.
        write_multi_target_fixture(&root, "demo_bringup", None);
        let bringup = root.join("demo_bringup");
        let (_sys, name, _t) = load_plan(&bringup, None).expect("load with no target");
        assert_eq!(
            name, "native",
            "no --target and no default_target should fall back to [deploy.native]"
        );

        // Explicit --target overrides everything.
        let (_sys, name, view) =
            load_plan(&bringup, Some("qemu-mps2-an385")).expect("explicit target");
        assert_eq!(name, "qemu-mps2-an385");
        assert_eq!(view.kind, "qemu");
    }

    #[test]
    fn nros_launch_target_honours_default_target_field() {
        let root = scratch("target_slice_default_field");
        write_multi_target_fixture(&root, "demo_bringup", Some("aaa_first_alpha"));
        let bringup = root.join("demo_bringup");
        let (_sys, name, _t) = load_plan(&bringup, None).expect("load");
        assert_eq!(
            name, "aaa_first_alpha",
            "default_target = \"aaa_first_alpha\" should beat the [deploy.native] fallback"
        );
    }

    #[test]
    fn nros_launch_default_target_pointing_at_missing_block_errors() {
        let root = scratch("target_slice_default_bad");
        write_multi_target_fixture(&root, "demo_bringup", Some("no_such_block"));
        let bringup = root.join("demo_bringup");
        let err = load_plan(&bringup, None).unwrap_err().to_string();
        assert!(err.contains("no_such_block"), "diagnostic: {err}");
    }
}
