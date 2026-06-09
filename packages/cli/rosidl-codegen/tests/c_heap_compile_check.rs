//! Manual syntax check for `mode = "heap"` generated C (RFC-0033 / Phase 229.5,
//! C path). Generates a heap C message and runs `gcc -fsyntax-only` against
//! minimal stub `nros/{types,cdr,platform}.h` headers (the real headers need
//! per-build config + opaque-size probes, orthogonal to this check). Ignored by
//! default; run with:
//!   cargo test -p rosidl-codegen --test c_heap_compile_check -- --ignored

use rosidl_codegen::{CapacityResolver, generate_c_message_package};
use rosidl_parser::parse_message;
use std::{fs, process::Command};

const TYPES_H: &str = r#"
#ifndef NROS_TYPES_STUB_H
#define NROS_TYPES_STUB_H
#include <stdint.h>
#include <stddef.h>
typedef struct { const char* type_name; const char* type_hash; size_t serialized_size_max; } nros_message_type_t;
typedef int32_t nros_ret_t;
struct nros_publisher_t;
nros_ret_t nros_publish_raw(struct nros_publisher_t* p, const uint8_t* buf, size_t n);
#endif
"#;

const PLATFORM_H: &str = r#"
#ifndef NROS_PLATFORM_STUB_H
#define NROS_PLATFORM_STUB_H
#include <stddef.h>
void* nros_platform_malloc(size_t size);
void nros_platform_free(void* ptr);
#endif
"#;

const CDR_H: &str = r#"
#ifndef NROS_CDR_STUB_H
#define NROS_CDR_STUB_H
#include <stdint.h>
#include <stddef.h>
#define W(name, ty) int nros_cdr_write_##name(uint8_t** p, const uint8_t* e, const uint8_t* o, ty v);
#define R(name, ty) int nros_cdr_read_##name(const uint8_t** p, const uint8_t* e, const uint8_t* o, ty* v);
W(u8, uint8_t) R(u8, uint8_t)
W(i8, int8_t) R(i8, int8_t)
W(u16, uint16_t) R(u16, uint16_t)
W(i16, int16_t) R(i16, int16_t)
W(u32, uint32_t) R(u32, uint32_t)
W(i32, int32_t) R(i32, int32_t)
W(u64, uint64_t) R(u64, uint64_t)
W(i64, int64_t) R(i64, int64_t)
W(f32, float) R(f32, float)
W(f64, double) R(f64, double)
W(bool, uint8_t) R(bool, uint8_t)
int nros_cdr_write_string(uint8_t** p, const uint8_t* e, const uint8_t* o, const char* s);
int nros_cdr_read_string(const uint8_t** p, const uint8_t* e, const uint8_t* o, char* d, size_t n);
#undef W
#undef R
#endif
"#;

#[test]
#[ignore = "spawns gcc -fsyntax-only"]
fn generated_heap_c_message_compiles() {
    let resolver = CapacityResolver::from_toml_str(
        r#"
        [fields]
        "my_msgs/Blob.data" = { cap = 0, mode = "heap" }
        "my_msgs/Blob.vals" = { cap = 0, mode = "heap" }
        "my_msgs/Blob.label" = { cap = 0, mode = "heap" }
        "#,
    )
    .unwrap();
    // Heap primitive sequences + heap string + a scalar + an owned bounded seq.
    let msg =
        parse_message("uint8[] data\nfloat32[] vals\nstring label\nint32 seq\nint8[<=4] small\n")
            .unwrap();
    let pkg = generate_c_message_package("my_msgs", "Blob", &msg, "h", &resolver).unwrap();

    let tmp = tempfile::tempdir().unwrap();
    let nros = tmp.path().join("nros");
    fs::create_dir_all(&nros).unwrap();
    fs::write(nros.join("types.h"), TYPES_H).unwrap();
    fs::write(nros.join("cdr.h"), CDR_H).unwrap();
    fs::write(nros.join("platform.h"), PLATFORM_H).unwrap();
    fs::write(tmp.path().join("my_msgs_msg_blob.h"), &pkg.header).unwrap();
    let c_path = tmp.path().join("my_msgs_msg_blob.c");
    fs::write(&c_path, &pkg.source).unwrap();

    let out = Command::new("gcc")
        .args(["-fsyntax-only", "-std=c11", "-Wall", "-Wextra", "-Werror"])
        .arg("-I")
        .arg(tmp.path())
        .arg(&c_path)
        .output()
        .expect("spawn gcc");
    assert!(
        out.status.success(),
        "generated heap C failed to compile:\n{}\n--- header ---\n{}\n--- source ---\n{}",
        String::from_utf8_lossy(&out.stderr),
        pkg.header,
        pkg.source,
    );
}
