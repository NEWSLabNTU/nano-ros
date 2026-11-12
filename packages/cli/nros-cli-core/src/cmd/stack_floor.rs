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

/// Platforms whose boards state a stack on the cargo road. A platform joins
/// when its board build scripts call `nros_board_common::stack_stated::emit`;
/// until then its images are reported as not checked, and pay nothing.
const STATING_PLATFORMS: &[&str] = &["freertos"];

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
fn disabled(value: Option<&str>) -> bool {
    value.is_some_and(|v| {
        matches!(
            v.to_ascii_uppercase().as_str(),
            "OFF" | "0" | "FALSE" | "NO"
        )
    })
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
        p.stack_floor = plan_one(p, nros_root, env);
    }
}

fn plan_one(
    p: &mut ResolvedBuild,
    nros_root: Option<&Path>,
    env: &dyn Fn(&str) -> Option<OsString>,
) -> Option<StackFloorStep> {
    if p.driver != Driver::Cargo {
        return None;
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
    };
    // Target-scoped, so build scripts and proc-macros keep their flags, and
    // JOINED with a config file's `[target.<triple>] rustflags` rather than
    // replacing them, as a bare RUSTFLAGS would.
    let t = env_triple(&triple);
    let rf = format!("CARGO_TARGET_{t}_RUSTFLAGS");
    let map = format!("-Csave-temps -Clink-arg=-Map={}", step.map().display());
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
    let st = std::process::Command::new("python3")
        .args(walker_args(c, &cargo_json))
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
        };
        let a: Vec<String> = walker_args(&c, Path::new("/t/m.json"))
            .into_iter()
            .map(|s| s.into_string().unwrap())
            .collect();
        assert!(a.iter().any(|s| s == "--intermediates-from-build"), "{a:?}");
        for flag in ["--link-dir", "--su", "--bitcode-dir"] {
            assert!(!a.iter().any(|s| s == flag), "{flag} in {a:?}");
        }
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
        arm(std::slice::from_mut(&mut p), Some(&root), &|_| None);
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
