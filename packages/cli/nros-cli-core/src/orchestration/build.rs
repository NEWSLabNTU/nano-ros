//! Build orchestration for generated system packages.

use super::{
    NrosPlan,
    generate::{GenerateOptions, GeneratedPackage, generate_package},
    plan::PlanBuildOptions,
};
use eyre::{Context, Result, eyre};
use serde::Serialize;
use std::{
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub package_name: String,
    pub output_dir: PathBuf,
    pub plan_path: PathBuf,
    pub workspace_root: PathBuf,
    pub component_workspace: Option<PathBuf>,
    pub release: bool,
    pub target: Option<String>,
    pub cargo_args: Vec<String>,
    /// Phase 172.D — bypass the staleness gate and always regenerate.
    pub force: bool,
}

/// Schema tag baked into the staleness fingerprint so a future change to
/// what the stamp covers invalidates every old stamp.
const STAMP_SCHEMA: &str = "nano-ros/build-stamp/v1";
/// Stamp file under the generated package root recording the last
/// successful generation's input fingerprint.
const STAMP_FILE: &str = ".nros-build-stamp";

pub fn build_generated_package(options: &BuildOptions) -> Result<GeneratedPackage> {
    // Read the plan once as bytes so the fingerprint sees exactly what the
    // generator consumes (parse separately for the interface cache).
    let plan_bytes = fs::read(&options.plan_path)
        .wrap_err_with(|| format!("failed to read {}", options.plan_path.display()))?;
    let plan: NrosPlan = serde_json::from_slice(&plan_bytes)
        .wrap_err_with(|| format!("failed to parse {}", options.plan_path.display()))?;

    // Phase 172.D — skip regeneration when the generation inputs are
    // unchanged. The generated package's contents are a pure function of
    // the plan, the baked-in paths, and the generator version; component
    // *source* is deliberately NOT in the fingerprint — generation never
    // reads it, and cargo's own incremental fingerprinting (below) owns
    // recompilation staleness for component edits. So the stamp gates
    // generation only; cargo always runs and is itself the recompile gate.
    let fingerprint = generation_fingerprint(&plan_bytes, options);
    let generated = if generation_is_fresh(&options.output_dir, &fingerprint, options.force) {
        eprintln!(
            "nros build: generated package up to date (plan + generator unchanged) \
             — skipping regeneration"
        );
        GeneratedPackage {
            root: options.output_dir.clone(),
            manifest_path: options.output_dir.join("Cargo.toml"),
            plan_path: options.plan_path.clone(),
        }
    } else {
        write_interface_cache(&options.output_dir, &plan)?;
        let generated = generate_package(&GenerateOptions {
            package_name: options.package_name.clone(),
            output_dir: options.output_dir.clone(),
            plan_path: options.plan_path.clone(),
            nros_path: options.workspace_root.join("packages/core/nros"),
            nros_orchestration_path: options
                .workspace_root
                .join("packages/core/nros-orchestration"),
            component_workspace: options.component_workspace.clone(),
        })?;
        // Record the fingerprint only after a clean generation. Written
        // before cargo on purpose: the stamp tracks generation freshness,
        // not build success — a failed cargo run still leaves a valid
        // generated tree, and the next run re-runs cargo (which retries the
        // compile) without needlessly regenerating.
        write_stamp(&options.output_dir, &fingerprint)?;
        generated
    };

    let mut cmd = Command::new("cargo");
    cmd.args(generated_cargo_args(
        &generated.manifest_path,
        &generated_target_dir(&generated.root),
        &plan.build,
        options.release,
        options.target.as_deref(),
        &options.cargo_args,
    ))
    .stdout(Stdio::inherit())
    .stderr(Stdio::inherit())
    .current_dir(&generated.root);
    // Phase 126.M5.nuttx — strip any inherited rustup pin so the
    // generated package's `rust-toolchain.toml` (e.g. NuttX's
    // pinned nightly + rust-src) is actually honoured. Without
    // this, an outer `cargo test` invocation propagates its own
    // RUSTUP_TOOLCHAIN through, and rustup never re-resolves
    // against the generated package's overrides.
    cmd.env_remove("RUSTUP_TOOLCHAIN");

    // Phase 204.15 inc 3 — `[build.cc]` fans out to the C/C++ layer via env that
    // `cc-rs` *appends* to its computed flags (every zenoh-pico/XRCE/net.c/lwIP
    // `cc::Build`), no build.rs edit. `debug=true` adds `-g` without disturbing
    // the opt level → the C-side "debug one layer" case (Rust stays stripped via
    // its profile). `opt_level` → `NROS_CC_OPT` for build scripts that honor it
    // (204.9). Appended to any inherited CFLAGS/CXXFLAGS.
    if let Some(cc) = &plan.build.cc {
        let mut extra: Vec<String> = Vec::new();
        if cc.debug == Some(true) {
            extra.push("-g".to_string());
        }
        extra.extend(cc.cflags.iter().cloned());
        if !extra.is_empty() {
            let suffix = extra.join(" ");
            for var in ["CFLAGS", "CXXFLAGS"] {
                let mut v = std::env::var(var).unwrap_or_default();
                if !v.is_empty() {
                    v.push(' ');
                }
                v.push_str(&suffix);
                cmd.env(var, v);
            }
        }
        if let Some(opt) = &cc.opt_level {
            cmd.env("NROS_CC_OPT", opt);
        }
    }

    let status = cmd
        .status()
        .wrap_err("failed to invoke generated cargo build")?;
    if !status.success() {
        return Err(eyre!(
            "generated package build failed (exit {})",
            status.code().unwrap_or(-1)
        ));
    }

    Ok(generated)
}

#[derive(Serialize)]
struct InterfaceCacheManifest<'a> {
    schema: &'static str,
    system: &'a str,
    generated_by: &'a str,
    interfaces: &'a [super::plan::PlanInterface],
}

fn write_interface_cache(generated_dir: &Path, plan: &NrosPlan) -> Result<()> {
    let Some(system_dir) = generated_dir.parent() else {
        return Ok(());
    };
    let interfaces_dir = system_dir.join("interfaces");
    let manifest = serde_json::to_string_pretty(&InterfaceCacheManifest {
        schema: "nano-ros/interface-cache/v1",
        system: &plan.system,
        generated_by: &plan.trace.generated_by,
        interfaces: &plan.interfaces,
    })?;

    for lang in ["rust", "c", "cpp"] {
        let lang_dir = interfaces_dir.join(lang);
        fs::create_dir_all(&lang_dir).wrap_err_with(|| {
            format!(
                "failed to create generated interface cache dir {}",
                lang_dir.display()
            )
        })?;
        write_if_changed(&lang_dir.join("manifest.json"), &manifest)?;
    }
    Ok(())
}

fn write_if_changed(path: &Path, contents: &str) -> Result<()> {
    if fs::read_to_string(path).ok().as_deref() == Some(contents) {
        return Ok(());
    }
    fs::write(path, contents).wrap_err_with(|| format!("failed to write {}", path.display()))
}

/// Fingerprint of every input that determines the *generated* package's
/// contents: the generator version, the plan bytes, and the paths baked into
/// the generated manifest/build script. Component source is intentionally
/// excluded — generation never reads it, and cargo owns recompilation
/// staleness for those edits (see `build_generated_package`).
fn generation_fingerprint(plan_bytes: &[u8], options: &BuildOptions) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    STAMP_SCHEMA.hash(&mut h);
    // A generator upgrade can change the templates even when the plan is
    // byte-identical, so the CLI version is part of the key.
    env!("CARGO_PKG_VERSION").hash(&mut h);
    plan_bytes.hash(&mut h);
    options.package_name.hash(&mut h);
    options.workspace_root.hash(&mut h);
    options.component_workspace.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// A generation is fresh when not forced, the generated crate is present, and
/// the recorded stamp matches the current input fingerprint.
fn generation_is_fresh(output_dir: &Path, fingerprint: &str, force: bool) -> bool {
    !force
        && output_dir.join("Cargo.toml").is_file()
        && read_stamp(output_dir).as_deref() == Some(fingerprint)
}

fn read_stamp(output_dir: &Path) -> Option<String> {
    fs::read_to_string(output_dir.join(STAMP_FILE))
        .ok()
        .map(|s| s.trim().to_string())
}

fn write_stamp(output_dir: &Path, fingerprint: &str) -> Result<()> {
    let path = output_dir.join(STAMP_FILE);
    fs::write(&path, fingerprint)
        .wrap_err_with(|| format!("failed to write build stamp {}", path.display()))
}

fn generated_cargo_args(
    manifest_path: &Path,
    target_dir: &Path,
    build: &PlanBuildOptions,
    release_override: bool,
    target_override: Option<&str>,
    passthrough: &[String],
) -> Vec<String> {
    let mut args = vec![
        "build".to_string(),
        "--manifest-path".to_string(),
        manifest_path.display().to_string(),
        "--target-dir".to_string(),
        target_dir.display().to_string(),
    ];

    match target_override {
        Some(target) if !target.is_empty() => {
            args.push("--target".to_string());
            args.push(target.to_string());
        }
        _ if !build.target.is_empty() => {
            args.push("--target".to_string());
            args.push(build.target.clone());
        }
        _ => {}
    }

    if release_override || build.profile == "release" {
        args.push("--release".to_string());
    } else if !matches!(build.profile.as_str(), "" | "debug" | "dev") {
        args.push("--profile".to_string());
        args.push(build.profile.clone());
    }

    args.extend(passthrough.iter().cloned());
    args
}

fn generated_target_dir(generated_root: &Path) -> PathBuf {
    generated_root
        .parent()
        .map(|parent| parent.join("target"))
        .unwrap_or_else(|| generated_root.join("target"))
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    use crate::orchestration::NrosPlan;

    use super::{
        BuildOptions, generated_cargo_args, generated_target_dir, generation_fingerprint,
        generation_is_fresh, write_stamp,
    };

    fn fixture_plan(name: &str) -> NrosPlan {
        let raw = std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("fixtures")
                .join("orchestration")
                .join(name),
        )
        .expect("read plan fixture");
        serde_json::from_str(&raw).expect("parse plan fixture")
    }

    #[test]
    fn generated_cargo_args_use_plan_target_and_profile() {
        let plan = fixture_plan("plan_pub_sub.json");

        assert_eq!(
            generated_cargo_args(
                PathBuf::from("/tmp/generated/Cargo.toml").as_path(),
                PathBuf::from("/tmp/target").as_path(),
                &plan.build,
                false,
                None,
                &[],
            ),
            [
                "build",
                "--manifest-path",
                "/tmp/generated/Cargo.toml",
                "--target-dir",
                "/tmp/target",
                "--target",
                "x86_64-unknown-linux-gnu",
                "--release",
            ]
        );
    }

    #[test]
    fn generated_cargo_args_allow_cli_target_and_passthrough_overrides() {
        let plan = fixture_plan("plan_pub_sub.json");

        assert_eq!(
            generated_cargo_args(
                PathBuf::from("/tmp/generated/Cargo.toml").as_path(),
                PathBuf::from("/tmp/target").as_path(),
                &plan.build,
                false,
                Some("thumbv7em-none-eabihf"),
                &["--offline".to_string(), "--quiet".to_string()],
            ),
            [
                "build",
                "--manifest-path",
                "/tmp/generated/Cargo.toml",
                "--target-dir",
                "/tmp/target",
                "--target",
                "thumbv7em-none-eabihf",
                "--release",
                "--offline",
                "--quiet",
            ]
        );
    }

    #[test]
    fn generated_cargo_args_emit_custom_plan_profile() {
        let mut plan = fixture_plan("plan_pub_sub.json");
        plan.build.profile = "size".to_string();

        assert_eq!(
            generated_cargo_args(
                PathBuf::from("/tmp/generated/Cargo.toml").as_path(),
                PathBuf::from("/tmp/target").as_path(),
                &plan.build,
                false,
                None,
                &[],
            ),
            [
                "build",
                "--manifest-path",
                "/tmp/generated/Cargo.toml",
                "--target-dir",
                "/tmp/target",
                "--target",
                "x86_64-unknown-linux-gnu",
                "--profile",
                "size",
            ]
        );
    }

    #[test]
    fn generated_target_dir_matches_system_layout() {
        assert_eq!(
            generated_target_dir(PathBuf::from("/tmp/system/nros/generated").as_path()),
            PathBuf::from("/tmp/system/nros/target")
        );
    }

    fn opts(output_dir: PathBuf) -> BuildOptions {
        BuildOptions {
            package_name: "demo_system".to_string(),
            output_dir,
            plan_path: PathBuf::from("/tmp/nros-plan.json"),
            workspace_root: PathBuf::from("/ws/nano-ros"),
            component_workspace: Some(PathBuf::from("/ws/components")),
            release: false,
            target: None,
            cargo_args: Vec::new(),
            force: false,
        }
    }

    #[test]
    fn fingerprint_is_stable_and_input_sensitive() {
        let o = opts(PathBuf::from("/out"));
        let base = generation_fingerprint(b"plan-A", &o);

        // Identical inputs → identical fingerprint.
        assert_eq!(base, generation_fingerprint(b"plan-A", &o));
        // Different plan bytes → different fingerprint.
        assert_ne!(base, generation_fingerprint(b"plan-B", &o));

        // Each baked-in path / name participates in the key.
        let mut o2 = o.clone();
        o2.package_name = "other".to_string();
        assert_ne!(base, generation_fingerprint(b"plan-A", &o2));
        let mut o3 = o.clone();
        o3.workspace_root = PathBuf::from("/ws/other");
        assert_ne!(base, generation_fingerprint(b"plan-A", &o3));
        let mut o4 = o.clone();
        o4.component_workspace = None;
        assert_ne!(base, generation_fingerprint(b"plan-A", &o4));
    }

    fn temp_dir(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{name}-{}-{stamp}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn freshness_requires_crate_present_matching_stamp_and_not_forced() {
        let dir = temp_dir("nros-staleness");
        let fp = "deadbeefdeadbeef";

        // No generated crate yet → not fresh (must generate).
        assert!(!generation_is_fresh(&dir, fp, false));

        // Crate present but no stamp → not fresh.
        std::fs::write(dir.join("Cargo.toml"), "[package]\n").unwrap();
        assert!(!generation_is_fresh(&dir, fp, false));

        // Matching stamp → fresh (skip regeneration).
        write_stamp(&dir, fp).unwrap();
        assert!(generation_is_fresh(&dir, fp, false));

        // `--force` defeats a matching stamp.
        assert!(!generation_is_fresh(&dir, fp, true));

        // A changed fingerprint (new plan / generator) → stale.
        assert!(!generation_is_fresh(&dir, "feedfacefeedface", false));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
