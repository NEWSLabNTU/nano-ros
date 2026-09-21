//! phase-460 W1 (issue 1420) -- a refused resolve leaves no model any consumer
//! can trust.
//!
//! The island's E7b: a contract edit the resolver refuses, `nros sync` exits
//! 1, and the model from the PREVIOUS resolve stays on disk, intact by every
//! check it will ever meet -- so `entity-inventory`, `codegen-system` and
//! `codegen entry` all derived from a contract the tree no longer stated.
//!
//! This is the phase's gate: the fixture is resolved once (the control), the
//! contract is edited to `qos: { depht: 4 }`, sync is refused, and every door
//! a model is opened through -- `model-path`, `ws entity-inventory`,
//! `codegen-system`, `codegen entry` -- must exit non-zero NAMING THE MARKER.
//! Editing the contract back does not clear the marker; only the next
//! successful sync does, after which all four pass again.
//!
//! One test function, deliberately: the steps are causally ordered and the
//! resolver override is process-global.
//!
//! Run with:
//! `cargo test --manifest-path packages/cli/Cargo.toml --test refused_resolve_leaves_no_model`

mod common;

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use clap::Parser;
use nros_cli_core::cmd::Cmd;

/// The verbs as the `nros` binary parses them, driven in-process so a
/// refusal is the verb's own `Err`, not a spawned exit code we then parse.
#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

fn nros(argv: &[&str]) -> Result<(), String> {
    let mut full = vec!["nros"];
    full.extend_from_slice(argv);
    let cli = Cli::try_parse_from(&full).map_err(|e| e.to_string())?;
    nros_cli_core::run(cli.cmd).map_err(|e| format!("{e:?}"))
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf()
}

/// A scratch workspace under the repo's gitignored `tmp/` (repo rule), holding
/// a copy of the fixture so the test may edit its contract.
fn scratch_workspace() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let ws = repo_root()
        .join("tmp")
        .join(format!("refused-resolve-{}-{stamp}", std::process::id()));
    let _ = fs::remove_dir_all(&ws);
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/refused_resolve");
    copy_tree(&fixture, &ws);
    ws
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &dest);
        } else {
            fs::copy(entry.path(), &dest).unwrap();
        }
    }
}

struct Doors {
    ws: PathBuf,
    bringup: PathBuf,
    model: PathBuf,
    metadata: PathBuf,
}

impl Doors {
    fn model_dir(&self) -> PathBuf {
        self.ws.join("build").join("nros").join("models")
    }

    fn sync(&self) -> Result<(), String> {
        nros(&[
            "sync",
            self.ws.to_str().unwrap(),
            "--model-dir",
            self.model_dir().to_str().unwrap(),
        ])
    }

    /// The four consumers, each through its own verb. Returned in the order
    /// the phase names them so a failure message reads like the gate table.
    fn consumers(&self, tag: &str) -> Vec<(&'static str, Result<(), String>)> {
        let out = self.ws.join("out").join(tag);
        fs::create_dir_all(&out).unwrap();
        let model = self.model.to_str().unwrap();
        vec![
            (
                "model-path",
                nros(&[
                    "model-path",
                    "--bringup-dir",
                    self.bringup.to_str().unwrap(),
                ]),
            ),
            (
                "entity-inventory",
                nros(&[
                    "ws",
                    "entity-inventory",
                    "--metadata",
                    self.metadata.to_str().unwrap(),
                    "--model",
                    model,
                    "--output-dir",
                    out.join("inventory").to_str().unwrap(),
                ]),
            ),
            (
                "codegen-system",
                nros(&[
                    "codegen-system",
                    "--workspace",
                    self.ws.to_str().unwrap(),
                    "--bringup",
                    "demo_bringup",
                    "--out",
                    out.join("system").to_str().unwrap(),
                ]),
            ),
            (
                "codegen entry",
                nros(&[
                    "codegen",
                    "entry",
                    "--lang",
                    "cpp",
                    "--typed",
                    "--metadata",
                    self.metadata.to_str().unwrap(),
                    "--workspace",
                    self.ws.to_str().unwrap(),
                    "--model",
                    model,
                    "--out",
                    out.join("nros_entry.cpp").to_str().unwrap(),
                ]),
            ),
        ]
    }

    fn assert_all_pass(&self, tag: &str) {
        for (door, result) in self.consumers(tag) {
            assert!(
                result.is_ok(),
                "[{tag}] `{door}` must accept a model whose provenance is intact: {}",
                result.unwrap_err()
            );
        }
    }

    fn assert_all_refuse_naming_the_marker(&self, tag: &str) {
        let marker = nros_cli_core::model_gate::marker_path(&self.model);
        let marker_name = marker.file_name().unwrap().to_str().unwrap();
        for (door, result) in self.consumers(tag) {
            let err = match result {
                Ok(()) => panic!("[{tag}] `{door}` accepted a model whose producer refused it"),
                Err(e) => e,
            };
            assert!(
                err.contains("refused by its producer") && err.contains(marker_name),
                "[{tag}] `{door}` refused, but not naming the marker `{marker_name}`:\n{err}"
            );
        }
    }
}

#[test]
fn a_refused_resolve_leaves_no_model_any_consumer_can_trust() {
    common::isolate_model_discovery();
    let resolver = common::pinned_launch_resolver();
    // SAFETY: once, before any verb runs; every reader is this process's own
    // `run_sync`, which resolves the helper through `$NROS_LAUNCH_RESOLVE`
    // first (issue 0285 -- never through `$PATH`). The stale guard compares
    // the running BINARY's checkout with the workspace's; this binary is a
    // test in whatever target dir cargo was given, not the `nros` the guard
    // is for, so its documented bypass is set for the process.
    unsafe {
        std::env::set_var("NROS_LAUNCH_RESOLVE", &resolver);
        std::env::set_var("NROS_SKIP_STALE_CHECK", "1");
    }

    let ws = scratch_workspace();
    let bringup = ws.join("src").join("demo_bringup");
    let doors = Doors {
        model: ws.join("build/nros/models/demo_bringup/system_model.yaml"),
        metadata: ws.join("nros-metadata.json"),
        bringup: bringup.clone(),
        ws: ws.clone(),
    };
    let contract = bringup.join("launch").join("system.contract.yaml");
    let good = fs::read_to_string(&contract).expect("the fixture contract");
    assert!(
        good.contains("depth: 4"),
        "the fixture states the line the test breaks"
    );
    let marker = nros_cli_core::model_gate::marker_path(&doors.model);

    // CONTROL -- a successful resolve, and every door accepts.
    doors.sync().expect("the fixture resolves as authored");
    assert!(
        doors.model.is_file(),
        "sync wrote {}",
        doors.model.display()
    );
    assert!(!marker.exists());
    doors.assert_all_pass("control");

    // THE EDIT the resolver refuses.
    fs::write(&contract, good.replace("depth: 4", "depht: 4")).unwrap();
    let err = doors
        .sync()
        .expect_err("an unknown contract key is refused");
    assert!(
        err.contains(marker.file_name().unwrap().to_str().unwrap()),
        "sync's own error names the marker it left:\n{err}"
    );
    assert!(
        !doors.model.exists(),
        "the path a consumer opens no longer exists after a refused resolve"
    );
    let kept: Vec<PathBuf> = fs::read_dir(doors.model.parent().unwrap())
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.to_string_lossy().contains("system_model.refused-"))
        .collect();
    assert_eq!(
        kept.len(),
        1,
        "the previous model is kept beside the marker: {kept:?}"
    );
    let m = nros_cli_core::model_gate::Marker::read(&marker).expect("the marker exists");
    assert!(
        m.input.contains("system.contract.yaml"),
        "the marker names the input that changed: {m:?}"
    );
    assert!(!m.check.is_empty() && m.check != "(not recorded)", "{m:?}");
    doors.assert_all_refuse_naming_the_marker("refused");

    // EDITED BACK, NOT RESOLVED -- still refused. A model whose producer last
    // said no is not current, whatever the inputs say now.
    fs::write(&contract, &good).unwrap();
    doors.assert_all_refuse_naming_the_marker("edited-back");

    // NEGATIVE CONTROL -- the next successful sync clears the marker, and the
    // four doors accept again. The quarantined copy stays for diffing.
    doors.sync().expect("the restored contract resolves");
    assert!(doors.model.is_file());
    assert!(!marker.exists(), "a successful resolve removes the marker");
    assert!(kept[0].is_file(), "the quarantined copy is kept");
    doors.assert_all_pass("restored");

    let _ = fs::remove_dir_all(&ws);
}
