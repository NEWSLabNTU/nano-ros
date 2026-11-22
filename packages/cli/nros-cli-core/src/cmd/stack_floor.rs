//! Issue 2400 W5 — the cargo road CHECKs the stack its board states.
//!
//! The cmake and west roads walk an image's deepest path in a post-build
//! command (`cmake/NanoRosStackFloor.cmake`). A cargo image has no build
//! system of ours after cargo, so `nros build` runs the same walker
//! (`scripts/nros-stack-floor.py`) once cargo has linked:
//!
//! 1. [`arm`] gives the image's cargo command what the walk reads: the
//!    target-scoped `-C save-temps` bitcode, an LLD `-Map`, and `-fstack-usage`
//!    on the C that cc-rs compiles, with `RUSTC_WRAPPER` emptied (a sccache
//!    hit restores the rlib, not the bitcode).
//! 2. [`check`] asks cargo again, with `--message-format=json`, which build
//!    scripts ran for THIS image. That rebuilds nothing. The board's script
//!    writes the stack it resolved into its `OUT_DIR`
//!    (`nros_board_common::stack_stated`). The walker reads cargo's messages
//!    through `scripts/lib/cargo_out_dir.py`, the one reader of that stream
//!    (`--stated-by-build`), and checks the floor against the stated stack
//!    under the same rule as every road: a MEASURED image (one with an
//!    `[[allowance]]` row) fails when its floor is above the stack, an
//!    unmeasured one warns.
//!
//! A board whose build states no stack is reported as "not checked: the board
//! states no stack", which is never a pass. `NROS_STACK_FLOOR=OFF` skips it all.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use eyre::{Result, WrapErr};

use crate::{
    builder::{handoff::Handoff, plan::Driver},
    cmd::{build::ResolvedBuild, build_next::BuildOutput},
};

/// The file a board's build script writes; `nros_board_common::stack_stated::FILE`.
pub const STATED_FILE: &str = "nros-stack-stated.json";

/// Platforms whose threads do not run on a stack the build sizes, each with
/// the reason its images are reported as not checked. ThreadX's Linux port
/// creates every thread with `pthread_create(.., NULL, ..)`, so the thread
/// runs on the host's default pthread stack and the stack handed to
/// `tx_thread_create` is never used: a walk checked against it would pass
/// for a stack nothing runs on.
const UNCHECKABLE_PLATFORMS: &[(&str, &str)] = &[(
    "threadx-linux",
    "the Linux port runs each thread on a host pthread stack, which the build does not size",
)];

/// Platforms whose boards state a stack on the cargo road. A platform joins
/// when its board build scripts call `nros_board_common::stack_stated::emit`;
/// until then its images are reported as not checked, and pay nothing.
/// Bare metal states the mps2-an385 main stack (root `main`) since issue 2494.
/// NuttX states its init task's stack (root `nros_nuttx_entry`, the kernel's
/// `CONFIG_INIT_STACKSIZE`), which the image runs on, since phase-557 W5b.
/// ThreadX on riscv64 states its app thread's stack, and walks since the
/// toolchain's C runtime objects copied into its board crate take their
/// frames from their own code (issue 2514).
const STATING_PLATFORMS: &[&str] = &["freertos", "bare-metal", "nuttx", "threadx-riscv64"];

/// What [`arm`] decided for one image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StackFloorStep {
    /// Walk the image after cargo.
    Check(Box<CargoStackFloor>),
    /// Say why the image's stack is not checked, after cargo.
    NotChecked(String),
}

/// Everything the post-cargo walk needs, fixed at planning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoStackFloor {
    /// The image's own cargo command plus `--message-format=json`.
    pub rerun: Handoff,
    /// The linked executable, uplifted to `<target dir>/<triple>/<profile>`.
    /// Its intermediates (`deps/`, `build/`) are wherever cargo's build dir
    /// put them, which only cargo's own messages say (`nros build` moves them
    /// for a caller-named `--target-dir`, RFC-0100 D13), so the walk reads
    /// them from there (`--intermediates-from-build`), never from here.
    pub exe: PathBuf,
    /// Where the map, report and llc cache go.
    pub dir: PathBuf,
    /// The allowance key: `stack_floor_image`, else `<bringup>@<board>`.
    pub key: String,
    pub rmw: String,
    pub triple: String,
    pub script: PathBuf,
    pub table: PathBuf,
    /// The image opted in to a raise (`[image.<id>] stack_floor = "raise"`):
    /// the walk records a measured shortfall instead of failing on it, and
    /// [`crate::cmd::stack_floor_raise::after_build`] rebuilds at the floor.
    pub raise: bool,
    /// The project's own name (the workspace directory's): the walker masks
    /// it out of its callee order, so a project's name never moves its walk.
    pub name_token: Option<String>,
    /// Issue 2784 -- the generated entry's [`SCHED_TASKS_FILE`], when it has
    /// sched tasks: the walk then checks each task's stack as its own role.
    pub sched_tasks: Option<PathBuf>,
}

impl CargoStackFloor {
    pub fn map(&self) -> PathBuf {
        self.dir.join("image.map")
    }
}

/// The cargo environment variable for a profile's `strip`, from the profile's
/// output directory name (`debug` is the `dev` profile).
fn profile_strip_env(profile_dir: &str) -> String {
    let name = if profile_dir == "debug" {
        "dev"
    } else {
        profile_dir
    };
    format!(
        "CARGO_PROFILE_{}_STRIP",
        name.to_ascii_uppercase().replace('-', "_")
    )
}

/// `NROS_STACK_FLOOR=OFF` (or `0`, `FALSE`, `NO`) turns the walk off, as on
/// the cmake road's `-DNROS_STACK_FLOOR=OFF`.
/// The one vocabulary of `NROS_STACK_FLOOR`, which `cmake/NanoRosStackFloor.cmake`
/// reads with the same two lists (issue 2803).
pub const SWITCH_ON: &[&str] = &["ON", "CHECK", "1", "TRUE", "YES"];
pub const SWITCH_OFF: &[&str] = &["OFF", "0", "FALSE", "NO"];

fn disabled(value: Option<&str>) -> bool {
    let Some(v) = value.map(str::to_ascii_uppercase) else {
        return false;
    };
    if SWITCH_OFF.contains(&v.as_str()) {
        return true;
    }
    if !v.is_empty() && !SWITCH_ON.contains(&v.as_str()) {
        // walked, the safe direction; the cmake road refuses the same value
        eprintln!(
            "nros build: NROS_STACK_FLOOR={v} is neither ON ({}) nor OFF ({}); the stack \
             floor is checked",
            SWITCH_ON.join(", "),
            SWITCH_OFF.join(", ")
        );
    }
    false
}

/// cargo's environment spelling of a target triple (`thumbv7m-none-eabi` ->
/// `THUMBV7M_NONE_EABI`).
fn env_triple(triple: &str) -> String {
    triple.to_ascii_uppercase().replace(['-', '.'], "_")
}

/// The allowance key for an image: the stated `stack_floor_image`, else
/// `<bringup>@<board>`, the cmake road's `_nros_stack_floor_image_key`.
pub fn image_key(stated: Option<&str>, qualified: &str, board: &str) -> String {
    if let Some(k) = stated.filter(|k| !k.is_empty()) {
        return k.to_string();
    }
    let bringup = qualified.split(':').next().unwrap_or(qualified);
    format!("{bringup}@{board}")
}

/// `existing` with `extra` appended, space-separated.
fn joined(existing: Option<OsString>, extra: &str) -> OsString {
    match existing {
        Some(v) if !v.is_empty() => {
            let mut v = v;
            v.push(" ");
            v.push(extra);
            v
        }
        _ => extra.into(),
    }
}

/// Add `extra` to `key` in `hand`'s environment: after a value the handoff
/// already sets, else after the one this process has, so neither is replaced.
/// The value of a FORCED `[env]` row for `key` in the settings files the
/// cargo command names with `--config <file>` (the last one that sets it,
/// as cargo merges them). `None` when no file forces it.
fn forced_config_env(hand: &Handoff, key: &str) -> Option<String> {
    let mut found = None;
    for pair in hand.args.windows(2) {
        if pair[0] != "--config" {
            continue;
        }
        let mut path = PathBuf::from(&pair[1]);
        if path.is_relative()
            && let Some(cwd) = &hand.cwd
        {
            path = cwd.join(path);
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(doc) = text.parse::<toml_edit::DocumentMut>() else {
            continue;
        };
        let Some(row) = doc.get("env").and_then(|e| e.get(key)) else {
            continue;
        };
        if row.get("force").and_then(|f| f.as_bool()) == Some(true)
            && let Some(v) = row.get("value").and_then(|v| v.as_str())
        {
            found = Some(v.to_string());
        }
    }
    found
}

/// The linker the image's `--config` files name for `triple`
/// (`[target.<triple>] linker`; the last file that states one wins, as cargo
/// merges them), or `None` when they state none.
fn config_linker(hand: &Handoff, triple: &str) -> Option<String> {
    let mut found = None;
    for pair in hand.args.windows(2) {
        if pair[0] != "--config" {
            continue;
        }
        let mut path = PathBuf::from(&pair[1]);
        if path.is_relative()
            && let Some(cwd) = &hand.cwd
        {
            path = cwd.join(path);
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(doc) = text.parse::<toml_edit::DocumentMut>() else {
            continue;
        };
        if let Some(l) = doc
            .get("target")
            .and_then(|t| t.get(triple))
            .and_then(|t| t.get("linker"))
            .and_then(|l| l.as_str())
        {
            found = Some(l.to_string());
        }
    }
    found
}

/// Phase-557 W5b -- the rustc flag that makes the link write its map. A C
/// compiler driver (NuttX links through `arm-none-eabi-gcc`) passes a linker
/// flag only behind `-Wl,` and rejects a bare `-Map=`, while a direct linker
/// (rust-lld, `ld`) takes `-Map=` and rejects `-Wl,`.
fn map_link_arg(linker: Option<&str>, map: &Path) -> String {
    let driver = linker
        .map(|l| {
            let name = Path::new(l)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            ["gcc", "g++", "cc", "c++", "clang", "clang++"]
                .iter()
                .any(|d| name == *d || name.ends_with(&format!("-{d}")))
        })
        .unwrap_or(false);
    if driver {
        format!("-Clink-arg=-Wl,-Map={}", map.display())
    } else {
        format!("-Clink-arg=-Map={}", map.display())
    }
}

fn merge_env(hand: &mut Handoff, key: &str, extra: &str, inherited: Option<OsString>) {
    if let Some(slot) = hand.env.iter_mut().find(|(k, _)| k == key) {
        slot.1 = joined(Some(std::mem::take(&mut slot.1)), extra);
    } else {
        hand.env.push((key.into(), joined(inherited, extra)));
    }
}

/// Decide, per cargo plan, whether its stack is checked, and arm its cargo
/// command for the walk. `env` reads this process's environment (a test
/// passes its own).
pub fn arm(
    plans: &mut [ResolvedBuild],
    nros_root: Option<&Path>,
    workspace_root: Option<&Path>,
    env: &dyn Fn(&str) -> Option<OsString>,
) {
    if disabled(
        env("NROS_STACK_FLOOR")
            .and_then(|v| v.into_string().ok())
            .as_deref(),
    ) {
        return;
    }
    for p in plans.iter_mut() {
        p.stack_floor = plan_one(p, nros_root, workspace_root, env);
    }
}

fn plan_one(
    p: &mut ResolvedBuild,
    nros_root: Option<&Path>,
    workspace_root: Option<&Path>,
    env: &dyn Fn(&str) -> Option<OsString>,
) -> Option<StackFloorStep> {
    if p.driver != Driver::Cargo {
        return None;
    }
    if let Some(why) = UNCHECKABLE_PLATFORMS
        .iter()
        .find(|(plat, _)| *plat == p.platform)
        .map(|(_, why)| why)
    {
        return Some(StackFloorStep::NotChecked((*why).into()));
    }
    let Some(BuildOutput::Cargo {
        target_dir,
        triple: Some(triple),
        profile_dir,
        bin: Some(bin),
    }) = p.output.clone()
    else {
        // A host image (no triple) runs on a host thread's stack, which no
        // board states; a manifest with no single bin links nothing to walk.
        return None;
    };
    if !STATING_PLATFORMS.contains(&p.platform.as_str()) {
        return Some(StackFloorStep::NotChecked(format!(
            "the {} board `{}` states no stack on this road",
            p.platform, p.board
        )));
    }
    let Some(root) = nros_root else {
        return Some(StackFloorStep::NotChecked(
            "no nano-ros checkout was found to read the walker from".into(),
        ));
    };
    let script = root.join("scripts/nros-stack-floor.py");
    // The json twin, as the cmake road reads: stdlib `json` loads it on every
    // Python the build supports.
    let table = root.join("config/stack-floors.json");
    if !script.is_file() || !table.is_file() {
        return Some(StackFloorStep::NotChecked(format!(
            "{} has no scripts/nros-stack-floor.py or config/stack-floors.json",
            root.display()
        )));
    }
    let hand = p.handoff.as_mut()?;
    let profile = target_dir.join(&triple).join(&profile_dir);
    let dir = target_dir.join("nros-stack-floor");
    let mut step = CargoStackFloor {
        // Set below, once the build is armed: the rerun must be the SAME
        // command, or cargo rebuilds the image without the walk's flags and
        // relinks over the one the walk reads.
        rerun: hand.clone(),
        exe: profile.join(&bin),
        key: image_key(p.stack_floor_image.as_deref(), &p.qualified, &p.board),
        rmw: p.rmw.clone().unwrap_or_else(|| "zenoh".into()),
        triple: triple.clone(),
        script,
        table,
        dir,
        raise: p.stack_floor_raise,
        name_token: workspace_root
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned()),
        // Written by `nros build`'s entry generation, which runs before this.
        sched_tasks: p
            .entry_root
            .as_ref()
            .map(|e| e.manifest_dir.clone())
            .or_else(|| hand.cwd.clone())
            .map(|d| d.join(SCHED_TASKS_FILE))
            .filter(|f| f.is_file()),
    };
    // Target-scoped, so build scripts and proc-macros keep their flags, and
    // JOINED with a config file's `[target.<triple>] rustflags` rather than
    // replacing them, as a bare RUSTFLAGS would.
    let t = env_triple(&triple);
    let rf = format!("CARGO_TARGET_{t}_RUSTFLAGS");
    let linker = env(&format!("CARGO_TARGET_{t}_LINKER"))
        .map(|v| v.to_string_lossy().into_owned())
        .or_else(|| config_linker(hand, &triple));
    let map = format!(
        "-Csave-temps {}",
        map_link_arg(linker.as_deref(), &step.map())
    );
    let inherited = env(&rf);
    merge_env(hand, &rf, &map, inherited);
    // cc-rs reads the FIRST of CFLAGS_<triple>, CFLAGS_<t_>, TARGET_CFLAGS,
    // CFLAGS, so the flag joins whichever this build would have read.
    let cf = format!("CFLAGS_{}", triple.replace(['-', '.'], "_"));
    let inherited = env(&cf)
        .or_else(|| env(&format!("CFLAGS_{triple}")))
        .or_else(|| env("TARGET_CFLAGS"))
        .or_else(|| env("CFLAGS"));
    merge_env(hand, &cf, "-fstack-usage -fcallgraph-info=su", inherited);
    // Issue 2692's carrier writes this same variable as a FORCED `[env]` row
    // in the build's settings file, and a forced row beats the process
    // environment, so the merge above alone never reached cc-rs: the walk
    // found no C frame and refused the role. Restate the row with the walk's
    // flags after it, as a later `--config`, which cargo merges over the
    // file's value (measured). The profile flags stay first.
    if let Some(row) = forced_config_env(hand, &cf) {
        let merged = joined(Some(row.into()), "-fstack-usage -fcallgraph-info=su");
        let value = toml_edit::Value::from(merged.to_string_lossy().into_owned());
        hand.args.push("--config".into());
        hand.args.push(format!("env.{cf}.value={value}").into());
    }
    // A profile with no debuginfo (release, nros-minsizerel) makes cargo pass
    // `-C strip=debuginfo`, which drops the `.debug_frame` the walk reads
    // newlib's frames from (`--cfi-elf`). The profile's own knob keeps it, so
    // cargo passes no strip at all, unless the caller chose one. The sections
    // are not loaded: the image's bytes on the target are unchanged.
    let strip = profile_strip_env(&profile_dir);
    if env(&strip).is_none()
        && !hand
            .env
            .iter()
            .any(|(k, _)| k.as_os_str() == strip.as_str())
    {
        hand.env.push((strip.into(), "none".into()));
    }
    hand.env.retain(|(k, _)| k != "RUSTC_WRAPPER");
    hand.env.push(("RUSTC_WRAPPER".into(), "".into()));
    step.rerun = hand.clone();
    step.rerun.args.push("--message-format=json".into());
    Some(StackFloorStep::Check(Box::new(step)))
}

/// Before cargo: the directory the link writes its map into.
pub fn prepare(step: &StackFloorStep) -> Result<()> {
    if let StackFloorStep::Check(c) = step {
        std::fs::create_dir_all(&c.dir)
            .wrap_err_with(|| format!("creating {}", c.dir.display()))?;
    }
    Ok(())
}

/// Issue 2784 -- the file a generated Rust entry carries for the stack-floor
/// check: its sched tasks and the stack each runs on. The C/C++ road's walk
/// reads the same fields out of `nros_priority_plan.json`.
pub const SCHED_TASKS_FILE: &str = "nros-sched-tasks.json";

/// Write [`SCHED_TASKS_FILE`] into a generated entry's directory: each sched
/// task's name, its authored `stack_bytes`, and the stack `nros::main!`
/// reserves for it (`reserved_stack_bytes`, from the one lowering the macro
/// calls: a number, `0` for the port's default, `null` for the boot task).
/// Nothing is written for an image without sched tasks, and an existing file
/// is replaced only when its text changes (a rewrite would re-stale every
/// fixture built over it). Returns whether the file now exists.
pub fn write_sched_tasks(
    entry_dir: &Path,
    rtos_key: &str,
    board: &str,
    table: Option<&nros_orchestration_ir::ResolvedSchedTaskTable>,
) -> Result<bool> {
    let path = entry_dir.join(SCHED_TASKS_FILE);
    let Some(table) = table.filter(|t| !t.is_single_sched_task()) else {
        if path.exists() {
            std::fs::remove_file(&path).wrap_err_with(|| format!("removing {}", path.display()))?;
        }
        return Ok(false);
    };
    let declared: Vec<Option<u64>> = table
        .sched_tasks
        .iter()
        .map(|t| t.stack_bytes.map(u64::from))
        .collect();
    let family = crate::orchestration::board_key::board_family(board).ok();
    let reserved = family.and_then(|f| f.rust_entry_task_stacks(&declared));
    // The task the runner runs on the CALLER's thread (the boot task): the
    // family's rule, reserved memory or not. The OPEN role is that thread.
    let boot_task = family
        .and_then(|f| f.c_abi_runners())
        .and_then(|r| r.sched_task_memory)
        .map(|m| match m.boot_sched_task {
            nros_entry_lower::BootSchedTask::First => 0,
            nros_entry_lower::BootSchedTask::Last => declared.len().saturating_sub(1),
        });
    let rows: Vec<serde_json::Value> = table
        .sched_tasks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            serde_json::json!({
                "name": t.name,
                "stack_bytes": t.stack_bytes,
                "reserved_stack_bytes": reserved.as_ref().and_then(|r| r[i]),
            })
        })
        .collect();
    let doc = serde_json::json!({
        "schema": 1,
        "sched_task_key": rtos_key,
        "reserves_task_memory": reserved.is_some(),
        "boot_task": boot_task,
        "sched_tasks": rows,
    });
    let text = serde_json::to_string_pretty(&doc)? + "\n";
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(&path, text).wrap_err_with(|| format!("writing {}", path.display()))?;
    }
    Ok(true)
}

/// The walker's command line. The roles, the stated stacks and the knob come
/// from the board's [`STATED_FILE`], which the walker finds through cargo's
/// JSON messages in `cargo_json` (`--stated-by-build`): one reader of that
/// stream, `scripts/lib/cargo_out_dir.py`.
pub fn walker_args(c: &CargoStackFloor, cargo_json: &Path) -> Vec<OsString> {
    let mut a: Vec<OsString> = vec![c.script.clone().into()];
    let mut put = |k: &str, v: OsString| {
        a.push(k.into());
        a.push(v);
    };
    put("--table", c.table.clone().into());
    put("--rmw", c.rmw.clone().into());
    put("--target", c.triple.clone().into());
    put("--image", c.key.clone().into());
    put("--stated-by-build", cargo_json.into());
    put("--link-map", c.map().into());
    put("--cfi-elf", c.exe.clone().into());
    put("--report", c.dir.join("report.json").into());
    put("--llc-cache", c.dir.join("llc").into());
    a.push("--sysroot".into());
    a.push("--intermediates-from-build".into());
    if c.raise {
        a.push("--raise".into());
    }
    if let Some(n) = &c.name_token {
        a.push("--name-token".into());
        a.push(n.into());
    }
    if let Some(f) = &c.sched_tasks {
        a.push("--sched-tasks".into());
        a.push(f.into());
    }
    a
}

/// After cargo: walk the image and check the stack its board stated.
pub fn check(qualified: &str, step: &StackFloorStep) -> Result<()> {
    let c = match step {
        StackFloorStep::NotChecked(why) => {
            eprintln!("nros build: stack floor of `{qualified}` not checked: {why}");
            return Ok(());
        }
        StackFloorStep::Check(c) => c,
    };
    let out = c
        .rerun
        .command()
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .wrap_err_with(|| format!("running `{}`", c.rerun.display()))?;
    if !out.status.success() {
        eyre::bail!(
            "`{}` failed while asking cargo which build scripts ran:\n{}",
            c.rerun.display(),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let cargo_json = c.dir.join("cargo-messages.json");
    std::fs::write(&cargo_json, &out.stdout)
        .wrap_err_with(|| format!("writing {}", cargo_json.display()))?;
    let mut walker = std::process::Command::new("python3");
    walker.args(walker_args(c, &cargo_json));
    // The build's jobserver (`share_jobserver`): the walker runs its callee
    // orders in parallel on its tokens, never beside them (issue 2414).
    for (k, v) in &c.rerun.jobserver {
        match v {
            Some(v) => walker.env(k, v),
            None => walker.env_remove(k),
        };
    }
    let st = walker
        .status()
        .wrap_err("running python3 for the stack-floor walk")?;
    if !st.success() {
        eyre::bail!(
            "`{qualified}`: the stack floor check failed (see above); \
             NROS_STACK_FLOOR=OFF skips the check"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_is_the_stated_one_else_bringup_at_board() {
        assert_eq!(
            image_key(Some("talker/rust/zenoh@b"), "x:y", "b"),
            "talker/rust/zenoh@b"
        );
        assert_eq!(
            image_key(None, "freertos_rs_talker_bringup:mps2", "freertos"),
            "freertos_rs_talker_bringup@freertos"
        );
        assert_eq!(image_key(Some(""), "a:b", "c"), "a@c");
    }

    /// The link-check lane's road (`nros build -- --target-dir <d>`): `nros
    /// build` gives cargo a `build.build-dir` under `<d>/nros-class/<key>/`, so
    /// `<d>/<triple>/<profile>/deps` holds nothing the link map names. The walk
    /// therefore takes the intermediates from cargo's own messages and is never
    /// handed a dir derived from the target dir.
    #[test]
    fn the_walk_reads_the_intermediates_from_cargo_not_from_the_target_dir() {
        let c = CargoStackFloor {
            rerun: Handoff::new("cargo", ["build"]),
            exe: "/t/thumbv7m-none-eabi/nros-minsizerel/e".into(),
            dir: "/t/nros-stack-floor".into(),
            key: "k".into(),
            rmw: "zenoh".into(),
            triple: "thumbv7m-none-eabi".into(),
            script: "/r/scripts/nros-stack-floor.py".into(),
            table: "/r/config/stack-floors.json".into(),
            raise: false,
            name_token: None,
            sched_tasks: None,
        };
        let a: Vec<String> = walker_args(&c, Path::new("/t/m.json"))
            .into_iter()
            .map(|s| s.into_string().unwrap())
            .collect();
        assert!(a.iter().any(|s| s == "--intermediates-from-build"), "{a:?}");
        for flag in ["--link-dir", "--su", "--bitcode-dir", "--sched-tasks"] {
            assert!(!a.iter().any(|s| s == flag), "{flag} in {a:?}");
        }
        // Issue 2784 -- an entry with sched tasks hands the walk its file.
        let c = CargoStackFloor {
            sched_tasks: Some("/e/nros-sched-tasks.json".into()),
            ..c
        };
        let a: Vec<String> = walker_args(&c, Path::new("/t/m.json"))
            .into_iter()
            .map(|s| s.into_string().unwrap())
            .collect();
        let i = a
            .iter()
            .position(|s| s == "--sched-tasks")
            .expect("--sched-tasks");
        assert_eq!(a[i + 1], "/e/nros-sched-tasks.json");
    }

    /// Issue 2784 -- a generated Rust entry states each sched task's stack as
    /// `nros::main!` reserves it; one without sched tasks gets no file.
    #[test]
    fn the_sched_tasks_file_states_what_the_macro_reserves() {
        use nros_orchestration_ir::{ResolvedSchedTask, ResolvedSchedTaskTable};
        let dir = tempfile::tempdir().unwrap();
        let task = |name: &str, stack: Option<u32>| ResolvedSchedTask {
            name: name.into(),
            priority: 1,
            stack_bytes: stack,
            spin_period_us: None,
            preempt_threshold: None,
            time_slice_us: None,
            sched_class: None,
            class: None,
            period_us: None,
            budget_us: None,
            deadline_us: None,
            deadline_policy: None,
            core: None,
            members: Vec::new(),
        };
        let table = ResolvedSchedTaskTable {
            sched_tasks: vec![task("high", Some(65_536)), task("low", None)],
        };
        assert!(
            write_sched_tasks(dir.path(), "freertos", "mps2-an385-freertos", Some(&table)).unwrap()
        );
        let doc: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join(SCHED_TASKS_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(doc["reserves_task_memory"], true);
        assert_eq!(doc["boot_task"], 1, "FreeRTOS boots the last task");
        // FreeRTOS boots the LAST task, which reserves nothing.
        assert_eq!(doc["sched_tasks"][0]["reserved_stack_bytes"], 65_536);
        assert!(doc["sched_tasks"][1]["reserved_stack_bytes"].is_null());
        let single = ResolvedSchedTaskTable {
            sched_tasks: vec![task("default", None)],
        };
        assert!(
            !write_sched_tasks(dir.path(), "freertos", "mps2-an385-freertos", Some(&single))
                .unwrap()
        );
        assert!(!dir.path().join(SCHED_TASKS_FILE).exists());
    }

    /// The json rerun is the build's own command: cloned before arming, it
    /// lacked the walk's flags, so cargo rebuilt the whole image without them
    /// and relinked over the ELF the walk reads (stripped, on a profile with
    /// no debuginfo: the link-check lane's `nros-minsizerel`).
    #[test]
    fn the_rerun_is_the_armed_build_plus_json() {
        let root = std::env::temp_dir().join(format!("nros-sf-rerun-{}", std::process::id()));
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::write(root.join("scripts/nros-stack-floor.py"), "").unwrap();
        std::fs::write(root.join("config/stack-floors.json"), "{}").unwrap();
        let mut p = ResolvedBuild {
            launch_exec: None,
            qualified: "demo:b".into(),
            board: "b".into(),
            platform: "freertos".into(),
            driver: Driver::Cargo,
            handoff: Some(Handoff::new(
                "cargo",
                ["build", "--profile", "nros-minsizerel"],
            )),
            rmw: None,
            entry_package: None,
            target: None,
            profile: None,
            cmake_build_dir: None,
            capabilities: Vec::new(),
            stack_floor_image: None,
            stack_floor: None,
            stack_floor_raise: false,
            kernel: None,
            entry_root: None,
            configure: None,
            record: Default::default(),
            output: Some(BuildOutput::Cargo {
                target_dir: "/t".into(),
                triple: Some("thumbv7m-none-eabi".into()),
                profile_dir: "nros-minsizerel".into(),
                bin: Some("e".into()),
            }),
            descriptor: None,
            run_tables: Default::default(),
            package_depends: None,
        };
        arm(std::slice::from_mut(&mut p), Some(&root), None, &|_| None);
        let _ = std::fs::remove_dir_all(&root);
        let Some(StackFloorStep::Check(c)) = &p.stack_floor else {
            panic!("{:?}", p.stack_floor);
        };
        let hand = p.handoff.as_ref().unwrap();
        assert_eq!(c.rerun.env, hand.env);
        let mut args = hand.args.clone();
        args.push("--message-format=json".into());
        assert_eq!(c.rerun.args, args);
        assert!(
            hand.env
                .iter()
                .any(|(k, v)| k == "CARGO_PROFILE_NROS_MINSIZEREL_STRIP" && v == "none"),
            "{:?}",
            hand.env
        );
    }

    /// Bare metal states its main stack (issue 2494), so its image is walked;
    /// a platform whose board states nothing is reported, not walked.
    #[test]
    fn a_stating_platform_is_walked_and_another_is_reported() {
        let root = std::env::temp_dir().join(format!("nros-sf-plat-{}", std::process::id()));
        std::fs::create_dir_all(root.join("scripts")).unwrap();
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::write(root.join("scripts/nros-stack-floor.py"), "").unwrap();
        std::fs::write(root.join("config/stack-floors.json"), "{}").unwrap();
        let image = |platform: &str| ResolvedBuild {
            launch_exec: None,
            qualified: "demo:b".into(),
            board: "b".into(),
            platform: platform.into(),
            driver: Driver::Cargo,
            handoff: Some(Handoff::new("cargo", ["build", "--release"])),
            rmw: None,
            entry_package: None,
            target: None,
            profile: None,
            cmake_build_dir: None,
            capabilities: Vec::new(),
            stack_floor_image: None,
            stack_floor: None,
            stack_floor_raise: false,
            kernel: None,
            entry_root: None,
            configure: None,
            record: Default::default(),
            output: Some(BuildOutput::Cargo {
                target_dir: "/t".into(),
                triple: Some("thumbv7m-none-eabi".into()),
                profile_dir: "release".into(),
                bin: Some("e".into()),
            }),
            descriptor: None,
            run_tables: Default::default(),
            package_depends: None,
        };
        let mut v = vec![
            image("bare-metal"),
            image("threadx"),
            image("nuttx"),
            image("threadx-riscv64"),
            image("threadx-linux"),
        ];
        arm(&mut v, Some(&root), None, &|_| None);
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            matches!(v[0].stack_floor, Some(StackFloorStep::Check(_))),
            "{:?}",
            v[0].stack_floor
        );
        assert!(
            matches!(&v[1].stack_floor, Some(StackFloorStep::NotChecked(w)) if w.contains("states no stack")),
            "{:?}",
            v[1].stack_floor
        );
        // phase-557 W5b -- NuttX states its init task's stack.
        assert!(
            matches!(v[2].stack_floor, Some(StackFloorStep::Check(_))),
            "{:?}",
            v[2].stack_floor
        );
        // issue 2514 -- the riscv64 ThreadX board's app thread.
        assert!(
            matches!(v[3].stack_floor, Some(StackFloorStep::Check(_))),
            "{:?}",
            v[3].stack_floor
        );
        // ThreadX on Linux runs its threads on host pthread stacks.
        assert!(
            matches!(&v[4].stack_floor, Some(StackFloorStep::NotChecked(w))
                if w.contains("host pthread stack")),
            "{:?}",
            v[4].stack_floor
        );
    }

    /// Phase-557 W5b -- NuttX links through a gcc driver, which rejects a
    /// bare `-Map=`; a direct linker rejects `-Wl,`.
    #[test]
    fn the_map_flag_follows_the_linker_flavor() {
        let m = Path::new("/b/image.map");
        assert_eq!(
            map_link_arg(Some("arm-none-eabi-gcc"), m),
            "-Clink-arg=-Wl,-Map=/b/image.map"
        );
        assert_eq!(
            map_link_arg(Some("/opt/x/bin/riscv64-unknown-elf-gcc"), m),
            "-Clink-arg=-Wl,-Map=/b/image.map"
        );
        assert_eq!(
            map_link_arg(Some("clang"), m),
            "-Clink-arg=-Wl,-Map=/b/image.map"
        );
        assert_eq!(
            map_link_arg(Some("rust-lld"), m),
            "-Clink-arg=-Map=/b/image.map"
        );
        assert_eq!(
            map_link_arg(Some("arm-none-eabi-ld"), m),
            "-Clink-arg=-Map=/b/image.map"
        );
        assert_eq!(map_link_arg(None, m), "-Clink-arg=-Map=/b/image.map");
    }

    #[test]
    fn the_strip_knob_is_the_profile_of_the_output_dir() {
        assert_eq!(profile_strip_env("debug"), "CARGO_PROFILE_DEV_STRIP");
        assert_eq!(
            profile_strip_env("nros-minsizerel"),
            "CARGO_PROFILE_NROS_MINSIZEREL_STRIP"
        );
    }

    #[test]
    fn off_spellings_disable_the_walk() {
        for v in ["OFF", "off", "0", "false", "No"] {
            assert!(disabled(Some(v)), "{v}");
        }
        assert!(!disabled(None));
        assert!(!disabled(Some("ON")));
        // issue 2803 -- the cmake road's spelling is an on spelling here too
        assert!(!disabled(Some("CHECK")));
    }

    fn nros_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    /// Issue 2803 -- the cmake road reads the SAME two lists: its matcher's
    /// alternations are exactly [`SWITCH_ON`] and [`SWITCH_OFF`].
    #[test]
    fn the_cmake_road_reads_the_same_vocabulary() {
        let text =
            std::fs::read_to_string(nros_root().join("cmake/NanoRosStackFloor.cmake")).unwrap();
        let alt = |head: &str| -> Vec<String> {
            let i = text
                .find(head)
                .unwrap_or_else(|| panic!("no `{head}` in the module"));
            let rest = &text[i + head.len()..];
            rest[..rest.find(')').unwrap()]
                .split('|')
                .map(str::to_string)
                .collect()
        };
        assert_eq!(alt(r#"_mode MATCHES "^("#), SWITCH_OFF);
        assert_eq!(alt(r#"elseif(_mode MATCHES "^("#), SWITCH_ON);
    }

    /// Issue 2693 -- the frame flags for cc-rs C are the cmake road's rule:
    /// `-fcallgraph-info=su` for GCC only. TI Arm Clang refuses it, and the
    /// cargo road passing it to every compiler failed each TI C build.
    #[test]
    fn the_cargo_roads_c_frame_flags_ask_callgraph_info_of_gcc_only() {
        let dir = tempfile::tempdir().unwrap();
        let module = nros_root().join("cmake/NanoRosStackFloor.cmake");
        let mut text = format!(
            "cmake_minimum_required(VERSION 3.20)\nproject(t NONE)\ninclude(\"{}\")\n",
            module.display()
        );
        for id in ["GNU", "TIClang", "Clang"] {
            text += &format!(
                "set(CMAKE_C_COMPILER_ID {id})\n_nros_stack_floor_c_frame_flags(f)\n\
                 message(STATUS \"FL {id}=${{f}}\")\n"
            );
        }
        std::fs::write(dir.path().join("CMakeLists.txt"), text).unwrap();
        let out = std::process::Command::new("cmake")
            .arg("-S")
            .arg(dir.path())
            .arg("-B")
            .arg(dir.path().join("b"))
            .output()
            .expect("cmake is a precondition of the cli-tests lane");
        let log = String::from_utf8_lossy(&out.stdout).to_string()
            + &String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{log}");
        let flags = |id: &str| {
            let tag = format!("-- FL {id}=");
            log.lines()
                .find_map(|l| l.strip_prefix(tag.as_str()))
                .unwrap_or_else(|| panic!("no {tag} line in {log}"))
                .to_string()
        };
        assert_eq!(flags("GNU"), "-fstack-usage -fcallgraph-info=su");
        assert_eq!(flags("TIClang"), "-fstack-usage");
        assert_eq!(flags("Clang"), "-fstack-usage");
    }

    /// Issue 2818 -- the `-fstack-usage` closure reaches a library linked
    /// through a raw flag (the RMW backend's `-Wl,--whole-archive,
    /// $<TARGET_FILE:x>`) and through issue 0475's `LINK_DEPENDS`, not only by
    /// target name. Missing them left the Cyclone backend frameless.
    #[test]
    fn the_stack_floor_closure_follows_a_library_named_inside_a_flag() {
        let dir = tempfile::tempdir().unwrap();
        let module = nros_root().join("cmake/NanoRosStackFloor.cmake");
        let mut text = format!(
            "cmake_minimum_required(VERSION 3.20)\nproject(t NONE)\ninclude(\"{}\")\n",
            module.display()
        );
        for lib in ["direct", "backend", "core"] {
            text += &format!(
                "add_library({lib} STATIC IMPORTED)\n\
                 set_target_properties({lib} PROPERTIES IMPORTED_LOCATION /x/lib{lib}.a)\n"
            );
        }
        text += "add_custom_target(img)\n\
                 set_property(TARGET img PROPERTY LINK_LIBRARIES direct \
                 \"-Wl,--whole-archive,$<TARGET_FILE:backend>,--no-whole-archive\")\n\
                 set_property(TARGET img PROPERTY LINK_DEPENDS \"$<TARGET_FILE:core>\")\n\
                 _nros_stack_floor_closure(img c)\nmessage(STATUS \"CL=${c}\")\n";
        std::fs::write(dir.path().join("CMakeLists.txt"), text).unwrap();
        let out = std::process::Command::new("cmake")
            .arg("-S")
            .arg(dir.path())
            .arg("-B")
            .arg(dir.path().join("b"))
            .output()
            .expect("cmake is a precondition of the cli-tests lane");
        let log = String::from_utf8_lossy(&out.stdout).to_string()
            + &String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{log}");
        let closure: Vec<&str> = log
            .lines()
            .find_map(|l| l.strip_prefix("-- CL="))
            .unwrap_or_else(|| panic!("no CL line in {log}"))
            .split(';')
            .collect();
        for want in ["img", "direct", "backend", "core"] {
            assert!(closure.contains(&want), "{want} not in {closure:?}");
        }
    }

    /// Issue 2803 -- configured for real: the environment turns the cmake
    /// road off, `-D` beats it, and a reconfigure sees the environment move.
    #[test]
    fn the_cmake_road_reads_the_environment_and_a_d_wins() {
        let dir = tempfile::tempdir().unwrap();
        let module = nros_root().join("cmake/NanoRosStackFloor.cmake");
        std::fs::write(
            dir.path().join("CMakeLists.txt"),
            format!(
                "cmake_minimum_required(VERSION 3.20)\nproject(t NONE)\n\
                 include(\"{}\")\n_nros_stack_floor_enabled(on)\n\
                 message(STATUS \"SF=${{on}}\")\n",
                module.display()
            ),
        )
        .unwrap();
        let build = dir.path().join("b");
        let configure = |env: Option<&str>, d: Option<&str>| -> String {
            let mut c = std::process::Command::new("cmake");
            c.arg("-S").arg(dir.path()).arg("-B").arg(&build);
            if let Some(d) = d {
                c.arg(format!("-DNROS_STACK_FLOOR={d}"));
            }
            match env {
                Some(v) => c.env("NROS_STACK_FLOOR", v),
                None => c.env_remove("NROS_STACK_FLOOR"),
            };
            let out = c
                .output()
                .expect("cmake is a precondition of the cli-tests lane");
            let log = String::from_utf8_lossy(&out.stdout).to_string()
                + &String::from_utf8_lossy(&out.stderr);
            assert!(out.status.success(), "{log}");
            log.lines()
                .find_map(|l| l.strip_prefix("-- SF="))
                .unwrap_or_else(|| panic!("no SF line in {log}"))
                .to_string()
        };
        assert_eq!(configure(None, None), "TRUE", "the default checks");
        assert_eq!(
            configure(Some("OFF"), None),
            "FALSE",
            "the environment's OFF"
        );
        assert_eq!(
            configure(Some("ON"), None),
            "TRUE",
            "the cargo road's ON, seen on reconfigure"
        );
        assert_eq!(configure(Some("off"), None), "FALSE");
        assert_eq!(configure(None, None), "TRUE", "the environment unset again");
        assert_eq!(
            configure(Some("ON"), Some("OFF")),
            "FALSE",
            "a -D beats the environment"
        );
        assert_eq!(configure(Some("ON"), None), "FALSE", "and keeps beating it");
    }

    #[test]
    fn a_forced_settings_row_is_restated_with_the_walks_flags() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("forced.toml"),
            "[env]\nCFLAGS_x = { value = \"-mcpu=\\\"r5\\\" -O2\", force = true }\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("plain.toml"),
            "[env]\nCFLAGS_x = { value = \"-O1\" }\n",
        )
        .unwrap();
        let mut h = Handoff::new("cargo", ["build", "--config", "forced.toml"]);
        h.cwd = Some(dir.path().to_path_buf());
        // A forced row is found (relative to the command's directory)...
        assert_eq!(
            forced_config_env(&h, "CFLAGS_x").as_deref(),
            Some("-mcpu=\"r5\" -O2")
        );
        assert_eq!(forced_config_env(&h, "CFLAGS_y"), None);
        // ...an unforced one is not: the process environment already wins
        // over it, so the merge into `hand.env` reaches cc-rs.
        let mut p = Handoff::new("cargo", ["build", "--config", "plain.toml"]);
        p.cwd = Some(dir.path().to_path_buf());
        assert_eq!(forced_config_env(&p, "CFLAGS_x"), None);
        // The restatement is a `--config` value cargo parses back to the
        // profile flags followed by the walk's.
        let merged = joined(Some("-mcpu=\"r5\" -O2".into()), "-fstack-usage");
        let value = toml_edit::Value::from(merged.to_string_lossy().into_owned());
        let doc: toml_edit::DocumentMut = format!("v = {value}").parse().unwrap();
        assert_eq!(doc["v"].as_str(), Some("-mcpu=\"r5\" -O2 -fstack-usage"));
    }

    #[test]
    fn flags_join_what_the_build_already_had() {
        let mut h = Handoff::new("cargo", ["build"]).with_env("CARGO_TARGET_X_RUSTFLAGS", "-Ca");
        merge_env(
            &mut h,
            "CARGO_TARGET_X_RUSTFLAGS",
            "-Csave-temps",
            Some("-Cignored".into()),
        );
        merge_env(&mut h, "CFLAGS_x", "-fstack-usage", Some("-O2".into()));
        merge_env(&mut h, "CFLAGS_y", "-fstack-usage", None);
        let get = |k: &str| h.env.iter().find(|(a, _)| a == k).map(|(_, v)| v.clone());
        assert_eq!(
            get("CARGO_TARGET_X_RUSTFLAGS"),
            Some("-Ca -Csave-temps".into())
        );
        assert_eq!(get("CFLAGS_x"), Some("-O2 -fstack-usage".into()));
        assert_eq!(get("CFLAGS_y"), Some("-fstack-usage".into()));
        assert_eq!(env_triple("thumbv7m-none-eabi"), "THUMBV7M_NONE_EABI");
    }
}
