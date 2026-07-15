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
        "my_msgs/Frame.label" = { cap = 0, mode = "heap" }
        "my_msgs/Frame.tags" = { cap = 0, mode = "heap" }
        "#,
    )
    .unwrap();
    let msg =
        parse_message("uint8[] pixels\nfloat32[] ranges\nstring label\nstring[] tags\nint32 seq\n")
            .unwrap();
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
/// Mirrors `cmake/ffi_lib_rs.in`'s `fixed_str()` (the production FFI-crate
/// lib.rs wrapper provides this helper; the generated per-message file calls
/// it for every non-heap string field — including the fixed-capacity string
/// ELEMENTS of a heap `string[]` sequence).
fn fixed_str(buf: &[u8]) -> &str {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    core::str::from_utf8(&buf[..end]).unwrap_or("")
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

/// Regression test for the phase-277 W4 fixed-string NUL-truncation bug
/// (commit 8e2076d81). The generated C++ FFI `serialize_*_fields` used to run
/// `core::str::from_utf8` over the WHOLE fixed-capacity `char[N]` buffer
/// before `.trim_end_matches('\0')`. `nros::FixedString` only writes bytes up
/// to the NUL terminator, so the tail is uninitialized C++ stack memory —
/// any non-UTF8 garbage there made `from_utf8` fail on the WHOLE buffer and
/// `.unwrap_or("")` silently serialized an EMPTY string. The fix routes
/// through a shared `fixed_str()` helper (`cmake/ffi_lib_rs.in`) that stops at
/// the FIRST NUL before UTF-8 validation.
///
/// This builds the generated `serialize_std_msgs_msg_string_fields` glue into
/// a real crate (against `nros-serdes`), feeds it a buffer whose post-NUL
/// tail is non-UTF8 garbage, and asserts the resulting CDR bytes decode to
/// EXACTLY the pre-NUL content.
///
/// Ignored by default (spawns cargo run); run with:
///   cargo test -p rosidl-codegen --test cpp_heap_compile_check -- --ignored
#[test]
#[ignore = "spawns cargo run"]
fn generated_fixed_string_serialize_truncates_at_nul_garbage() {
    let msg = parse_message("string data\n").unwrap();
    let resolver = CapacityResolver::empty();
    let pkg = generate_cpp_message_package("std_msgs", "String", &msg, "h", &resolver).unwrap();

    // Regression guard: the template must route through the shared fixed_str()
    // helper, not re-introduce the old whole-buffer from_utf8+trim pattern.
    assert!(
        pkg.ffi_rs.contains("fixed_str(&msg.data)"),
        "expected fixed_str(&msg.data) in generated serialize fn:\n{}",
        pkg.ffi_rs
    );
    assert!(
        !pkg.ffi_rs.contains("trim_end_matches"),
        "old buggy whole-buffer from_utf8 + trim_end_matches pattern reappeared:\n{}",
        pkg.ffi_rs
    );

    let root = repo_root();
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::write(
        tmp.path().join("Cargo.toml"),
        format!(
            r#"[package]
name = "fixed_str_nul_check"
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
        tmp.path().join("src/main.rs"),
        r#"#![allow(non_camel_case_types, dead_code)]
use nros_serdes::{CdrReader, CdrWriter, DeserError, SerError};

/// Stub for the C++-side publish hook the generated `ffi_publish_*` wrapper
/// calls. Never exercised here (only `serialize_*_fields` is), but the
/// generated wrapper is `#[no_mangle] pub`, so the symbol must resolve at
/// link time.
#[unsafe(no_mangle)]
extern "C" fn nros_cpp_publish_raw(
    _handle: *mut core::ffi::c_void,
    _data: *const u8,
    _len: usize,
) -> i32 {
    0
}

/// Mirrors `cmake/ffi_lib_rs.in`'s `fixed_str()` exactly (kept in sync
/// manually — see the doc-comment there for the phase-277 W4 rationale).
fn fixed_str(buf: &[u8]) -> &str {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    core::str::from_utf8(&buf[..end]).unwrap_or("")
}

include!("ffi_gen.rs");

fn main() {
    // "hi" + NUL terminator + non-UTF8 garbage (0xFF is never a valid UTF-8
    // byte) filling the rest of the 256-byte fixed-capacity buffer — mirrors
    // uninitialized C++ stack memory past a short nros::FixedString.
    let mut data = [0xFFu8; 256];
    data[0] = b'h';
    data[1] = b'i';
    data[2] = 0u8;
    let cmsg = std_msgs_msg_string_t { data };

    let mut buf = [0u8; 512];
    let mut writer = CdrWriter::new_with_header(&mut buf).unwrap();
    serialize_std_msgs_msg_string_fields(&cmsg, &mut writer).unwrap();
    let len = writer.position();

    let mut reader = CdrReader::new_with_header(&buf[..len]).unwrap();
    let s = reader.read_string().unwrap();
    assert_eq!(
        s, "hi",
        "serialized CDR must contain exactly the pre-NUL content, got {:?}",
        s
    );
    println!("OK: fixed_str truncates at NUL, serialized = {:?}", s);
}
"#,
    )
    .unwrap();

    let out = Command::new(env!("CARGO"))
        .args(["run", "--quiet"])
        .current_dir(tmp.path())
        .output()
        .expect("spawn cargo run");
    assert!(
        out.status.success(),
        "generated fixed-string serialize/deserialize round-trip failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
}
