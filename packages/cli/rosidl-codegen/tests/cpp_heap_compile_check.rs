//! Manual end-to-end compile check for `mode = "heap"` generated C++ (RFC-0033 /
//! Phase 229.5, C++ path). Two halves:
//!   * the FFI Rust glue (`*_ffi.rs`) — the unsafe raw-pointer + shared-allocator
//!     code — is `cargo check`'d against the real nros-serdes;
//!   * the `.hpp` header (using `nros::HeapSequence`) is `g++ -fsyntax-only`'d.
//! Ignored by default; run with:
//!   cargo test -p rosidl-codegen --test cpp_heap_compile_check -- --ignored

use rosidl_codegen::{CapacityResolver, generate_cpp_message_package};
use rosidl_parser::parse_message;
use std::{fs, path::PathBuf, process::Command};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .unwrap()
        .to_path_buf()
}

#[test]
#[ignore = "spawns cargo check + g++"]
fn generated_heap_cpp_compiles() {
    let resolver = CapacityResolver::from_toml_str(
        r#"
        [fields]
        "my_msgs/Frame.pixels" = { cap = 0, mode = "heap" }
        "my_msgs/Frame.ranges" = { cap = 0, mode = "heap" }
        "#,
    )
    .unwrap();
    let msg = parse_message("uint8[] pixels\nfloat32[] ranges\nint32 seq\n").unwrap();
    let pkg = generate_cpp_message_package("my_msgs", "Frame", &msg, "h", &resolver).unwrap();
    let root = repo_root();

    // ---- 1. FFI Rust glue: cargo check ----
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        format!(
            r#"[package]
name = "cpp_heap_ffi_check"
version = "0.0.0"
edition = "2024"

[dependencies]
nros-serdes = {{ path = "{}/packages/core/nros-serdes" }}

[workspace]
"#,
            root.display()
        ),
    )
    .unwrap();
    fs::write(tmp.path().join("src/ffi_gen.rs"), &pkg.ffi_rs).unwrap();
    fs::write(
        tmp.path().join("src/lib.rs"),
        r#"#![allow(non_camel_case_types, dead_code)]
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};
unsafe extern "C" {
    fn nros_cpp_publish_raw(h: *mut core::ffi::c_void, p: *const u8, n: usize) -> i32;
}
include!("ffi_gen.rs");
"#,
    )
    .unwrap();
    let out = Command::new(env!("CARGO"))
        .args(["check", "--quiet"])
        .current_dir(tmp.path())
        .output()
        .expect("spawn cargo check");
    assert!(
        out.status.success(),
        "generated heap FFI .rs failed to compile:\n{}\n--- ffi.rs ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        pkg.ffi_rs,
    );

    // ---- 2. .hpp header: g++ -fsyntax-only ----
    let inc = root.join("packages/core/nros-cpp/include");
    let htmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(htmp.path().join("nros")).unwrap();
    // Stub platform.h (the real one needs per-build config).
    fs::write(
        htmp.path().join("nros/platform.h"),
        "#ifndef PSTUB\n#define PSTUB\n#include <cstddef>\nextern \"C\" void* nros_platform_malloc(size_t);\nextern \"C\" void nros_platform_free(void*);\n#endif\n",
    )
    .unwrap();
    fs::write(htmp.path().join("my_msgs_msg_frame.hpp"), &pkg.header).unwrap();
    fs::write(
        htmp.path().join("probe.cpp"),
        "#include \"my_msgs_msg_frame.hpp\"\nint main(){ my_msgs::msg::Frame f; (void)f; return 0; }\n",
    )
    .unwrap();
    let g = Command::new("g++")
        .args([
            "-std=c++14",
            "-fno-exceptions",
            "-fno-rtti",
            "-fsyntax-only",
            "-Wall",
        ])
        .arg("-I")
        .arg(&inc)
        .arg("-I")
        .arg(htmp.path())
        .arg(htmp.path().join("probe.cpp"))
        .output()
        .expect("spawn g++");
    assert!(
        g.status.success(),
        "generated heap .hpp failed to compile:\n{}\n--- header ---\n{}",
        String::from_utf8_lossy(&g.stderr),
        pkg.header,
    );
}
