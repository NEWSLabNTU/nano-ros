//! Manual end-to-end compile check for `mode = "heap"` generated code
//! (RFC-0033 / Phase 229.5). Generates a heap message, drops it into a temp
//! crate that path-depends on the real nros-core/nros-serdes, and runs
//! `cargo check`. Ignored by default (spawns cargo); run with:
//!   cargo test -p rosidl-codegen --test heap_compile_check -- --ignored

use rosidl_codegen::{CapacityResolver, RosEdition, generate_nros_message_package};
use rosidl_parser::parse_message;
use std::{collections::HashSet, fs, path::PathBuf, process::Command};

#[test]
#[ignore = "spawns cargo check against a generated crate"]
fn generated_heap_message_compiles() {
    let resolver = CapacityResolver::from_toml_str(
        r#"
        [fields]
        "my_msgs/Frame.pixels" = { cap = 0, mode = "heap" }
        "my_msgs/Frame.label"  = { cap = 0, mode = "heap" }
        "my_msgs/Frame.tags"   = { cap = 0, mode = "heap" }
        "#,
    )
    .unwrap();
    let msg = parse_message("uint8[] pixels\nstring label\nstring[] tags\nint32 seq\n").unwrap();
    let pkg = generate_nros_message_package(
        "my_msgs",
        "Frame",
        &msg,
        &HashSet::new(),
        "0.1.0",
        RosEdition::Humble,
        &resolver,
    )
    .expect("generate");

    // Resolve the in-tree core crates (…/packages/cli/rosidl-codegen → repo root).
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf();
    let core = repo_root.join("packages/core");
    assert!(core.join("nros-core").is_dir(), "core path: {core:?}");

    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join("src/msg")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[package]
name = "heap_check"
version = "0.0.0"
edition = "2021"

[dependencies]
nros-core = {{ path = "{core}/nros-core" }}
nros-serdes = {{ path = "{core}/nros-serdes" }}
heapless = "0.8"

[workspace]
"#,
            core = core.display()
        ),
    )
    .unwrap();
    fs::write(
        root.join("src/lib.rs"),
        "pub mod msg { pub mod frame; pub use frame::Frame; }\n",
    )
    .unwrap();
    fs::write(root.join("src/msg/frame.rs"), &pkg.message_rs).unwrap();

    let out = Command::new(env!("CARGO"))
        .args(["check", "--quiet"])
        .current_dir(root)
        .output()
        .expect("spawn cargo check");
    assert!(
        out.status.success(),
        "generated heap crate failed to compile:\n{}\n--- generated ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        pkg.message_rs
    );
}
