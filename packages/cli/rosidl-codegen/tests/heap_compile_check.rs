//! Manual end-to-end compile check for `mode = "heap"` generated code
//! (RFC-0033 / Phase 229.5). Generates a heap message, drops it into a temp
//! crate that path-depends on the real nros-core/nros-serdes, and runs
//! `cargo check`. Ignored by default (spawns cargo); run with:
//!   cargo test -p rosidl-codegen --test heap_compile_check -- --ignored

use rosidl_codegen::{
    CapacityResolver, RosEdition, generate_nros_message_package, generate_nros_service_package,
};
use rosidl_parser::{parse_message, parse_service};
use std::{collections::HashSet, fs, path::PathBuf, process::Command};

/// Issue 0693 follow-up — `cc` is a PRECONDITION of a compile check, not an
/// optional extra.
///
/// All three syntax checks in this file used to answer a spawn failure with
/// `eprintln!("SKIP: cc not found"); return;`, which reports PASS. A file whose
/// entire purpose is "the generated C compiles" cannot conclude that without a
/// compiler, and saying so quietly is how a suite keeps a green tick while
/// verifying nothing — the class issue 0693 was filed for.
fn cc_output(args: &[&str]) -> std::process::Output {
    std::process::Command::new("cc").args(args).output().expect(
        "`cc` not found — this file compiles generated C to check it is well-formed, \
         so without a compiler it cannot answer its own question. Install a C \
         compiler (build-essential / clang) rather than letting the check pass.",
    )
}

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
        RosEdition::Humble.type_hash(),
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

/// phase-303 W4 — a generated SERVICE (with the DHEADER-wrapped serialize/
/// deserialize) compiles. AddTwoInts is primitive-only (self-contained).
#[test]
#[ignore = "spawns cargo check against a generated crate"]
fn generated_service_with_dheader_wrap_compiles() {
    let srv = parse_service("int64 a\nint64 b\n---\nint64 sum\n").unwrap();
    let ph = RosEdition::Humble.type_hash();
    let pkg = generate_nros_service_package(
        "my_srvs",
        "AddTwoInts",
        &srv,
        &HashSet::new(),
        "0.1.0",
        ph,
        ph,
        ph,
        &CapacityResolver::empty(),
    )
    .expect("generate");

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf();
    let core = repo_root.join("packages/core");
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    fs::create_dir_all(root.join("src/srv")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[package]
name = "srv_check"
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
        "pub mod srv { pub mod add_two_ints; }\n",
    )
    .unwrap();
    fs::write(root.join("src/srv/add_two_ints.rs"), &pkg.service_rs).unwrap();

    let out = Command::new(env!("CARGO"))
        .args(["check", "--quiet"])
        .current_dir(root)
        .output()
        .expect("spawn cargo check");
    assert!(
        out.status.success(),
        "generated service crate failed to compile:\n{}\n--- generated ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        pkg.service_rs
    );
}

/// phase-303 W4 — the generated C (with the DHEADER FFI wrap +
/// `nros_cdr_write_encaps_header`) is syntactically valid against `nros/cdr.h`.
/// `cc -fsyntax-only`, no link. Skips if `cc` is absent.
#[test]
#[ignore = "spawns cc -fsyntax-only against generated C"]
fn generated_c_with_dheader_wrap_syntax_checks() {
    use rosidl_codegen::generate_c_message_package;

    let msg = parse_message("int32 seq\nstring frame_id\nfloat64 value\n").unwrap();
    let pkg = generate_c_message_package(
        "my_msgs",
        "Framed",
        &msg,
        RosEdition::Humble.type_hash(),
        &CapacityResolver::empty(),
    )
    .expect("generate C");

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf();
    let inc = repo_root.join("packages/api/nros-c/include");
    let plat = repo_root.join("packages/platform/nros-platform-api/include");
    let gen_dir = repo_root.join("target/nros-c-generated");
    let tmp = tempfile::tempdir().unwrap();
    // Write the generated header next to the source so its #include resolves.
    let hdr = tmp.path().join("my_msgs__msg__framed.h");
    fs::write(&hdr, &pkg.header).unwrap();
    let src = tmp.path().join("framed.c");
    // Point the source's #include at our temp header name.
    let source = pkg.source.replacen(
        "#include \"",
        &format!("#include \"{}\"\n// ", hdr.display()),
        1,
    );
    fs::write(&src, source).unwrap();

    let out = cc_output(&[
        "-fsyntax-only",
        "-I",
        gen_dir.to_str().unwrap(),
        "-I",
        inc.to_str().unwrap(),
        "-I",
        plat.to_str().unwrap(),
        src.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "generated C failed -fsyntax-only:\n{}\n--- source ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        pkg.source
    );
}

/// phase-303 W4 — the generated C SERVICE (DHEADER-wrapped) syntax-checks.
#[test]
#[ignore = "spawns cc -fsyntax-only against generated C"]
fn generated_c_service_with_dheader_wrap_syntax_checks() {
    use rosidl_codegen::generate_c_service_package;

    let srv = parse_service("int64 a\nint64 b\nstring note\n---\nint64 sum\nbool ok\n").unwrap();
    let pkg = generate_c_service_package(
        "my_srvs",
        "AddTwoInts",
        &srv,
        RosEdition::Humble.type_hash(),
        &CapacityResolver::empty(),
    )
    .expect("generate C service");

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf();
    let inc = repo_root.join("packages/api/nros-c/include");
    let plat = repo_root.join("packages/platform/nros-platform-api/include");
    let gen_dir = repo_root.join("target/nros-c-generated");
    let tmp = tempfile::tempdir().unwrap();
    let hdr = tmp.path().join(&pkg.header_name);
    fs::create_dir_all(hdr.parent().unwrap()).ok();
    fs::write(&hdr, &pkg.header).unwrap();
    let src = tmp.path().join("srv.c");
    let source = pkg.source.replacen(
        "#include \"",
        &format!("#include \"{}\"\n// ", hdr.display()),
        1,
    );
    fs::write(&src, source).unwrap();

    let out = cc_output(&[
        "-fsyntax-only",
        "-I",
        gen_dir.to_str().unwrap(),
        "-I",
        inc.to_str().unwrap(),
        "-I",
        plat.to_str().unwrap(),
        src.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "generated C service failed -fsyntax-only:\n{}\n--- source ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        pkg.source
    );
}

/// phase-303 W4 — the generated C ACTION (DHEADER-wrapped) syntax-checks.
#[test]
#[ignore = "spawns cc -fsyntax-only against generated C"]
fn generated_c_action_with_dheader_wrap_syntax_checks() {
    use rosidl_codegen::generate_c_action_package;
    use rosidl_parser::parse_action;

    let action =
        parse_action("int32 order\n---\nint32[] sequence\n---\nint32[] partial\n").unwrap();
    let pkg = generate_c_action_package(
        "my_acts",
        "Fibonacci",
        &action,
        RosEdition::Humble.type_hash(),
        &CapacityResolver::empty(),
    )
    .expect("generate C action");

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf();
    let inc = repo_root.join("packages/api/nros-c/include");
    let plat = repo_root.join("packages/platform/nros-platform-api/include");
    let gen_dir = repo_root.join("target/nros-c-generated");
    let tmp = tempfile::tempdir().unwrap();
    let hdr = tmp.path().join(&pkg.header_name);
    fs::create_dir_all(hdr.parent().unwrap()).ok();
    fs::write(&hdr, &pkg.header).unwrap();
    let src = tmp.path().join("act.c");
    let source = pkg.source.replacen(
        "#include \"",
        &format!("#include \"{}\"\n// ", hdr.display()),
        1,
    );
    fs::write(&src, source).unwrap();

    let out = cc_output(&[
        "-fsyntax-only",
        "-I",
        gen_dir.to_str().unwrap(),
        "-I",
        inc.to_str().unwrap(),
        "-I",
        plat.to_str().unwrap(),
        src.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "generated C action failed -fsyntax-only:\n{}\n--- source ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        pkg.source
    );
}
