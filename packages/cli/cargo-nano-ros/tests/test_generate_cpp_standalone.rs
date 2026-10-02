//! `nros generate cpp` from `package.xml` — the standalone C++ front door
//! (issue 1512, for the cargo-rooted bare-metal C++ leaf).
//!
//! The property that matters is that it is a FRONT DOOR and not a second
//! emitter: for one package, its output must be byte-identical to what the CMake
//! args-file path writes. A divergence there would mean the two roads ship
//! different headers or different serializers for the same message.

use cargo_nano_ros::{
    GenerateCStandaloneConfig, GenerateCppConfig, collect_interface_files,
    generate_cpp_from_args_file, generate_cpp_from_package_xml, load_index_with_fallback,
};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

fn write_manifest(dir: &Path, deps: &[&str]) -> PathBuf {
    let depends: String = deps
        .iter()
        .map(|d| format!("  <depend>{d}</depend>\n"))
        .collect();
    let xml = format!(
        "<?xml version=\"1.0\"?>\n<package format=\"3\">\n  <name>probe_pkg</name>\n  \
         <version>0.1.0</version>\n  <description>probe</description>\n  \
         <maintainer email=\"dev@example.com\">dev</maintainer>\n  \
         <license>MIT</license>\n{depends}</package>\n"
    );
    let path = dir.join("package.xml");
    fs::write(&path, xml).unwrap();
    path
}

/// Every file under `root`, keyed by its path relative to `root`.
fn tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path.strip_prefix(root).unwrap().to_path_buf();
                out.insert(rel, fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn standalone(manifest_dir: &TempDir, out: &Path) {
    generate_cpp_from_package_xml(GenerateCStandaloneConfig {
        manifest_path: manifest_dir.path().join("package.xml"),
        output_dir: out.to_path_buf(),
        force: true,
        verbose: false,
        ros_edition: "humble".to_string(),
        codegen_config: None,
    })
    .expect("standalone C++ generation");
}

#[test]
fn standalone_cpp_resolves_the_transitive_interface_packages() {
    let tmp = TempDir::new().unwrap();
    write_manifest(tmp.path(), &["std_msgs"]);
    let out = tmp.path().join("generated");
    standalone(&tmp, &out);

    // `std_msgs` reaches `builtin_interfaces` (Header.stamp), and both get the
    // full CMake layout: umbrella header, per-message header, the split Rust
    // FFI glue, and `mod.rs`.
    for pkg in ["std_msgs", "builtin_interfaces"] {
        assert!(
            out.join(pkg).join(format!("{pkg}.hpp")).is_file(),
            "{pkg}.hpp"
        );
        assert!(out.join(pkg).join("mod.rs").is_file(), "{pkg}/mod.rs");
    }
    let msg = out.join("std_msgs/msg");
    assert!(msg.join("std_msgs_msg_string.hpp").is_file());
    assert!(msg.join("std_msgs_msg_string_types.rs").is_file());
    assert!(msg.join("std_msgs_msg_string_exports.rs").is_file());
}

#[test]
fn standalone_cpp_is_byte_identical_to_the_args_file_road() {
    // `builtin_interfaces` has no interface dependencies, so the args file the
    // CMake road would write for it is fully determined by its share dir.
    let pkg = "builtin_interfaces";

    let tmp = TempDir::new().unwrap();
    write_manifest(tmp.path(), &[pkg]);
    let standalone_out = tmp.path().join("standalone");
    standalone(&tmp, &standalone_out);

    let index = load_index_with_fallback(false).unwrap();
    let share = &index
        .find_package(pkg)
        .expect("builtin_interfaces resolves")
        .share_dir;
    let args_out = tmp.path().join("args").join(pkg);
    let args = serde_json::json!({
        "package_name": pkg,
        "output_dir": args_out,
        "interface_files": collect_interface_files(share).unwrap(),
        "dependencies": [],
        "ros_edition": "humble",
    });
    let args_file = tmp.path().join("args.json");
    fs::write(&args_file, serde_json::to_string(&args).unwrap()).unwrap();
    generate_cpp_from_args_file(GenerateCppConfig {
        args_file,
        verbose: false,
    })
    .expect("args-file C++ generation");

    let a = tree(&standalone_out.join(pkg));
    let b = tree(&args_out);
    assert!(!a.is_empty(), "the standalone road wrote nothing for {pkg}");
    assert_eq!(
        a.keys().collect::<Vec<_>>(),
        b.keys().collect::<Vec<_>>(),
        "the two roads wrote different FILE SETS for {pkg}"
    );
    for (rel, bytes) in &a {
        assert!(
            &b[rel] == bytes,
            "{} differs between the standalone and args-file roads",
            rel.display()
        );
    }
}
