//! Phase 212.F end-to-end CLI tests — `nros new system` + `nros check
//! --bringup` + `cargo nros plan <dir>` discovery walk.

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use nros_cli_core::cmd::{
    bringup::{discover_bringups, lint_bringup},
    check,
    new::{self, Args as NewArgs},
};

fn temp_root(tag: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("phase-212-f-{tag}-{}-{stamp}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_workspace(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = [\"talker_pkg\", \"listener_pkg\"]\n",
    )
    .unwrap();
}

fn default_new_args(
    name: PathBuf,
    system_name: Option<PathBuf>,
    components: Vec<String>,
) -> NewArgs {
    NewArgs {
        name: Some(name),
        for_platform: None,
        system_name,
        components,
        component_name: Vec::new(),
        workspace_root: None,
        into: None,
        no_config: false,
        no_readme: false,
        platform: None,
        rmw: "zenoh".to_string(),
        lang: "rust".to_string(),
        use_case: "talker".to_string(),
        component: false,
        deploy: None,
        kind: "self".to_string(),
        target: None,
        board: None,
        bringup: None,
        from_launch: None,
        from_profile: None,
        force: false,
    }
}

/// Serialize the two `set_current_dir` tests — cwd is PROCESS-GLOBAL, so the
/// default parallel libtest runner races them (one test's `set_current_dir`
/// yanks the floor out from under the other's relative scaffold writes —
/// surfaced when the #202 lane started running this suite routinely).
static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn cli_nros_new_system_dispatches_and_scaffolds_bringup_pkg() {
    let _cwd = CWD_LOCK.lock().unwrap();
    let root = temp_root("cli_new_system");
    write_workspace(&root);
    // Simulate user `cd`ing to the workspace root.
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(&root).unwrap();

    let result = new::run(default_new_args(
        PathBuf::from("system"),
        Some(PathBuf::from("demo_bringup")),
        vec!["talker_pkg".to_string(), "listener_pkg".to_string()],
    ));

    std::env::set_current_dir(&original).unwrap();
    result.expect("nros new system dispatches and succeeds");

    let bringup = root.join("demo_bringup");
    assert!(bringup.join("package.xml").is_file());
    assert!(bringup.join("system.toml").is_file());
    assert!(bringup.join("launch/system.launch.xml").is_file());
    assert!(bringup.join(".gitignore").is_file());
    assert!(!bringup.join("Cargo.toml").exists());
    assert!(!bringup.join("CMakeLists.txt").exists());
    assert!(!bringup.join("src").exists());
}

#[test]
fn cli_nros_new_system_without_components_fails_clean() {
    let _cwd = CWD_LOCK.lock().unwrap();
    let root = temp_root("cli_no_components");
    write_workspace(&root);
    let original = std::env::current_dir().unwrap();
    std::env::set_current_dir(&root).unwrap();

    let result = new::run(default_new_args(
        PathBuf::from("system"),
        Some(PathBuf::from("demo_bringup")),
        Vec::new(),
    ));

    std::env::set_current_dir(&original).unwrap();
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("--components"),
        "diagnostic must point at --components: {msg}"
    );
}

#[test]
fn cli_nros_check_rejects_cargo_toml_in_bringup() {
    let root = temp_root("cli_check_reject_cargo");
    let bringup = root.join("demo_bringup");
    fs::create_dir_all(bringup.join("launch")).unwrap();
    fs::write(bringup.join("package.xml"), "<package/>").unwrap();
    fs::write(
        bringup.join("system.toml"),
        "[system]\nname=\"demo\"\nrmw=\"zenoh\"\ndomain_id=0\n",
    )
    .unwrap();
    fs::write(bringup.join("launch/system.launch.xml"), "<launch/>").unwrap();
    fs::write(bringup.join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();

    let err = check::run(check::Args {
        plan: bringup.clone(),
        package_xml_drift: vec![],
        bringup: true,
        workspace: None,
    })
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("pure declarative"), "diagnostic: {msg}");
    assert!(msg.contains("Cargo.toml"), "diagnostic: {msg}");

    // direct API surface also rejects
    let err = lint_bringup(&bringup).unwrap_err();
    assert!(err.to_string().contains("Cargo.toml"));
}

#[test]
fn cli_cargo_nros_plan_discovers_bringup_via_dirwalk() {
    let root = temp_root("cli_discover");
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\n\
         members = [\"talker_pkg\"]\n\
         exclude = [\"demo_bringup\"]\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("talker_pkg")).unwrap();
    fs::write(
        root.join("talker_pkg/Cargo.toml"),
        "[package]\nname=\"talker_pkg\"\nversion=\"0.1.0\"\n",
    )
    .unwrap();
    let bringup = root.join("demo_bringup");
    fs::create_dir_all(bringup.join("launch")).unwrap();
    fs::write(bringup.join("package.xml"), "<package/>").unwrap();
    fs::write(
        bringup.join("system.toml"),
        "[system]\nname=\"demo\"\nrmw=\"zenoh\"\ndomain_id=0\n",
    )
    .unwrap();

    let found = discover_bringups(&root).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].pkg_name, "demo_bringup");
}
