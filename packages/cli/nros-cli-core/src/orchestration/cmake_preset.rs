//! RFC-0048 §6 / phase-287 W5 — CMakePreset emission for the ament consumption
//! shape (shape C′).
//!
//! `nros setup <board>` writes a per-board preset fragment under
//! `~/.nros/presets/<board>.json`; `nros init` writes a project
//! `CMakePresets.json` that `include`s those fragments. `cmake --preset <board>`
//! then cross-configures with the toolchain + `nano_ros_ROOT` set before
//! `project()`, so a leaf's `find_package(nano_ros)` resolves and the ament shape
//! needs no hand-set `-DCMAKE_TOOLCHAIN_FILE` / `-Dnano_ros_ROOT`.
//!
//! No `${…}` templating reaches disk: every path is written as a literal absolute
//! value substituted at emit time (repo root + the store bin dir just
//! provisioned). The one dynamic datum — the cross-compiler's store bin — rides on
//! the preset's `environment.PATH`, because the toolchain file names its compiler
//! by bare name (`arm-none-eabi-gcc`) and CMake finds it on `PATH`.

use std::path::{Path, PathBuf};

use eyre::{Result, WrapErr};
use serde_json::json;

/// Where per-board preset fragments live: `$NROS_HOME/presets`, else
/// `~/.nros/presets`, else `.nros/presets` (last resort).
pub fn presets_dir() -> PathBuf {
    if let Ok(home) = std::env::var("NROS_HOME") {
        if !home.is_empty() {
            return PathBuf::from(home).join("presets");
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return PathBuf::from(home).join(".nros").join("presets");
        }
    }
    PathBuf::from(".nros").join("presets")
}

/// Emit `~/.nros/presets/<board>.json` for one board.
///
/// * `toolchain_file` — absolute path to the CMake toolchain file, or `None` for a
///   host/native board (the preset then carries only `nano_ros_ROOT`).
/// * `bin_dirs` — store bin directories just provisioned; prepended to the
///   preset's `environment.PATH` so the cross-compiler resolves. Non-existent dirs
///   are dropped.
///
/// Returns the written fragment path.
pub fn emit_board_preset(
    board: &str,
    repo_root: &Path,
    toolchain_file: Option<&Path>,
    bin_dirs: &[PathBuf],
) -> Result<PathBuf> {
    emit_board_preset_into(&presets_dir(), board, repo_root, toolchain_file, bin_dirs)
}

/// As [`emit_board_preset`], but writes into an explicit `dir` (testable without
/// touching `NROS_HOME`).
pub fn emit_board_preset_into(
    dir: &Path,
    board: &str,
    repo_root: &Path,
    toolchain_file: Option<&Path>,
    bin_dirs: &[PathBuf],
) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)
        .wrap_err_with(|| format!("create presets dir {}", dir.display()))?;

    let repo = path_str(repo_root);
    let mut cache = serde_json::Map::new();
    cache.insert("nano_ros_ROOT".into(), json!(repo));
    cache.insert("CMAKE_BUILD_TYPE".into(), json!("Release"));

    let mut preset = serde_json::Map::new();
    preset.insert("name".into(), json!(board));
    preset.insert(
        "binaryDir".into(),
        json!(format!("${{sourceDir}}/build/{board}")),
    );
    if let Some(tc) = toolchain_file {
        preset.insert("toolchainFile".into(), json!(path_str(tc)));
    }
    preset.insert("cacheVariables".into(), serde_json::Value::Object(cache));

    // environment.PATH — prepend each existing store bin dir, then inherit the
    // parent PATH via CMake's `$penv{PATH}`.
    let existing: Vec<String> = bin_dirs
        .iter()
        .filter(|d| d.is_dir())
        .map(|d| path_str(d))
        .collect();
    if !existing.is_empty() {
        let joined = format!("{}:$penv{{PATH}}", existing.join(":"));
        let mut env = serde_json::Map::new();
        env.insert("PATH".into(), json!(joined));
        preset.insert("environment".into(), serde_json::Value::Object(env));
    }

    let doc = json!({
        "version": 6,
        "configurePresets": [serde_json::Value::Object(preset)],
    });

    let path = dir.join(format!("{board}.json"));
    let text = serde_json::to_string_pretty(&doc).wrap_err("serialize preset")?;
    std::fs::write(&path, format!("{text}\n"))
        .wrap_err_with(|| format!("write preset {}", path.display()))?;
    Ok(path)
}

/// Generate `<project_dir>/CMakePresets.json` that `include`s every per-board
/// fragment currently in the presets dir. Idempotent — re-run after a new
/// `nros setup <board>` to pick the new fragment up. Returns the written path and
/// the count of included fragments.
pub fn write_project_presets(project_dir: &Path) -> Result<(PathBuf, usize)> {
    write_project_presets_from(&presets_dir(), project_dir)
}

/// As [`write_project_presets`], but reads fragments from an explicit
/// `presets_dir` (testable without touching `NROS_HOME`).
pub fn write_project_presets_from(dir: &Path, project_dir: &Path) -> Result<(PathBuf, usize)> {
    let mut fragments: Vec<String> = Vec::new();
    if dir.is_dir() {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
            .wrap_err_with(|| format!("read presets dir {}", dir.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
            .collect();
        entries.sort();
        fragments = entries.iter().map(|p| path_str(p)).collect();
    }

    let doc = json!({
        "version": 6,
        "include": fragments,
    });
    let path = project_dir.join("CMakePresets.json");
    let text = serde_json::to_string_pretty(&doc).wrap_err("serialize CMakePresets.json")?;
    std::fs::write(&path, format!("{text}\n"))
        .wrap_err_with(|| format!("write {}", path.display()))?;
    Ok((path, fragments.len()))
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn embedded_preset_carries_toolchain_and_path() {
        let tmp = tempfile::tempdir().unwrap();
        let presets = tmp.path().join("presets");
        let repo = tmp.path().join("nano-ros");
        let toolchain = repo.join("cmake/toolchain/armv7a-nuttx-eabi.cmake");
        let bin = tmp.path().join("sdk/gcc/bin");
        std::fs::create_dir_all(&bin).unwrap();

        let path = emit_board_preset_into(
            &presets,
            "nuttx-qemu-arm",
            &repo,
            Some(&toolchain),
            &[bin.clone()],
        )
        .unwrap();

        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let preset = &doc["configurePresets"][0];
        assert_eq!(preset["name"], "nuttx-qemu-arm");
        // toolchainFile is the absolute path we passed.
        assert_eq!(preset["toolchainFile"], path_str(&toolchain));
        // nano_ros_ROOT is absolute (repo root), Release build.
        assert_eq!(preset["cacheVariables"]["nano_ros_ROOT"], path_str(&repo));
        assert_eq!(preset["cacheVariables"]["CMAKE_BUILD_TYPE"], "Release");
        // The existing store bin dir is on environment.PATH, before $penv{PATH}.
        let env_path = preset["environment"]["PATH"].as_str().unwrap();
        assert!(env_path.starts_with(&path_str(&bin)));
        assert!(env_path.ends_with("$penv{PATH}"));
    }

    #[test]
    fn native_preset_has_no_toolchain_and_drops_missing_bins() {
        let tmp = tempfile::tempdir().unwrap();
        let presets = tmp.path().join("presets");
        let repo = tmp.path().join("nano-ros");
        let missing = tmp.path().join("does/not/exist/bin");

        let path =
            emit_board_preset_into(&presets, "posix", &repo, None, &[missing]).unwrap();
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let preset = &doc["configurePresets"][0];
        assert!(preset.get("toolchainFile").is_none());
        // A non-existent bin dir is dropped ⇒ no environment block emitted.
        assert!(preset.get("environment").is_none());
    }

    #[test]
    fn init_includes_every_fragment_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        let presets = tmp.path().join("presets");
        let repo = tmp.path().join("nano-ros");
        emit_board_preset_into(&presets, "posix", &repo, None, &[]).unwrap();
        emit_board_preset_into(&presets, "nuttx-qemu-arm", &repo, None, &[]).unwrap();

        let proj = tmp.path().join("proj");
        std::fs::create_dir_all(&proj).unwrap();
        let (path, n) = write_project_presets_from(&presets, &proj).unwrap();
        assert_eq!(n, 2);
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let includes = doc["include"].as_array().unwrap();
        assert_eq!(includes.len(), 2);
        // sorted: nuttx-qemu-arm.json before posix.json
        assert!(includes[0].as_str().unwrap().ends_with("nuttx-qemu-arm.json"));
        assert!(includes[1].as_str().unwrap().ends_with("posix.json"));
    }
}
