//! A project scaffolded by the nros-0.6.0-nros11 release configures on this
//! release after `nros configure --migrate`.
//!
//! That release's `nros new` wrote a talker contract with `rate_hz:`, which the
//! contract checker refuses since rlm v0.3.27. A plain configure must refuse it
//! naming a remedy nano-ros ships (`nros configure --migrate`, never
//! `play_launch migrate`) and the SOURCE contract, and `--migrate` must rewrite
//! the contract with rlm's own migration so the next configure is clean.
//!
//! Drives the real binary: a C or C++ image's configure runs a CMake probe
//! that calls back into the `nros` that started it, so an in-process verb
//! (whose executable is a test harness) cannot configure one.
//!
//! Run with:
//! `cargo nextest run --manifest-path packages/cli/Cargo.toml -p nros-cli --test nros11_scaffold_migrate`

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf()
}

/// The in-tree resolver, by absolute path (issue 0285); asserted, not skipped.
fn launch_resolver() -> PathBuf {
    let r = nros_cli_core::orchestration::model_location::launch_resolver_in_checkout(&repo_root());
    assert!(
        r.is_file(),
        "nros-launch-resolve not built at {} -- run `just setup-launch-resolve`",
        r.display()
    );
    r
}

fn nros(cwd: &Path, argv: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_nros"))
        .args(argv)
        .current_dir(cwd)
        .env("NROS_LAUNCH_RESOLVE", launch_resolver())
        .env("NROS_SKIP_STALE_CHECK", "1")
        .output()
        .expect("spawn nros")
}

fn text(o: &Output) -> String {
    format!(
        "status: {}\n--- stdout\n{}\n--- stderr\n{}",
        o.status,
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// The talker contract `nros new` wrote in the nros-0.6.0-nros11 release
/// (`new_workspace.rs`'s `bringup_contract` and `contract_node_rows` at that
/// tag; the talker row is the same in every language), for the node `node`.
fn nros11_talker_contract(node: &str) -> String {
    format!(
        "# What this image's node DECLARES, beside system.toml. The build sizes its\n\
         # buffers and its callback table from this, so it must list everything the\n\
         # node creates: when you add a timer, a publisher, a subscription or a\n\
         # parameter in the code, state it here too.\n\
         # `nros build --explain-sizing` lists anything still left to a default.\n\
         \nversion: 1\n\nnodes:\n\
         \x20 {node}:\n\
         \x20   params: {{}}\n\
         \x20   paths:\n\
         \x20     on_tick:\n\
         \x20       trigger: {{ timer: {{ rate_hz: 1 }} }}\n\
         \x20       output: [chatter]\n\
         \x20   pub:\n\
         \x20     chatter:\n\
         \x20       qos:\n\
         \x20         durability: volatile\n\
         \ntopics:\n\
         \x20 /chatter:\n\
         \x20   type: std_msgs/msg/String\n\
         \x20   pub: [{node}/chatter]\n\
         \x20   # Read outside this workspace, by `ros2 topic echo` or a ROS 2 node.\n\
         \x20   external: sub\n"
    )
}

fn nros11_scaffold_configures_after_migrate(lang: &str) {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = repo_root().join("tmp").join(format!(
        "nros11-migrate-{lang}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    let new = nros(
        &dir,
        &[
            "new",
            "app",
            "--template",
            "talker",
            "--lang",
            lang,
            "--board",
            "linux",
        ],
    );
    assert!(new.status.success(), "nros new: {}", text(&new));
    let ws = dir.join("app");
    let contract = ws.join("src/app_node_bringup/system.contract.yaml");
    assert!(contract.is_file(), "{} is scaffolded", contract.display());
    // The project as the nros11 release left it.
    fs::write(&contract, nros11_talker_contract("app_node")).unwrap();

    let refused = nros(&ws, &["configure"]);
    let all = text(&refused);
    assert!(
        !refused.status.success(),
        "the nros11 contract is refused: {all}"
    );
    assert!(
        all.contains("To fix: run `nros configure --migrate`"),
        "the refusal leads with the remedy nano-ros ships: {all}"
    );
    assert!(
        all.contains(contract.to_str().unwrap()),
        "the remedy names the source contract: {all}"
    );
    assert!(
        !all.contains("play_launch migrate"),
        "no remedy names a tool nano-ros does not ship: {all}"
    );

    let migrated = nros(&ws, &["configure", "--migrate"]);
    assert!(
        migrated.status.success(),
        "configure --migrate: {}",
        text(&migrated)
    );
    assert!(
        String::from_utf8_lossy(&migrated.stderr)
            .contains("nros configure --migrate: src/app_node_bringup/system.contract.yaml:14:"),
        "--migrate prints each key it rewrote: {}",
        text(&migrated)
    );
    let body = fs::read_to_string(&contract).unwrap();
    assert!(body.contains("trigger: { timer: { rate: 1Hz } }"), "{body}");
    assert_eq!(
        body.replace("rate: 1Hz", "rate_hz: 1"),
        nros11_talker_contract("app_node"),
        "only the renamed key moved"
    );

    // And a plain configure is clean from here.
    let again = nros(&ws, &["configure"]);
    assert!(again.status.success(), "configure: {}", text(&again));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_nros11_rust_scaffold_configures_after_migrate() {
    nros11_scaffold_configures_after_migrate("rust");
}

#[test]
fn an_nros11_c_scaffold_configures_after_migrate() {
    nros11_scaffold_configures_after_migrate("c");
}

#[test]
fn an_nros11_cpp_scaffold_configures_after_migrate() {
    nros11_scaffold_configures_after_migrate("cpp");
}
