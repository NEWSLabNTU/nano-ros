//! issue 0964 — what the C++ pack's `SERIALIZED_SIZE_MAX` does, checked against
//! a real compiler rather than against the header text.
//!
//! The C pack poisons an unbounded type's size constant with a `#define` whose
//! body is an undeclared identifier: costless to include, a compile error to
//! NAME. C++ cannot express that shape — a `static constexpr size_t X = TOKEN;`
//! is an error at the point of DEFINITION, so every consumer of the header would
//! break rather than only the consumers that ask for a size.
//!
//! The C++ form is a static data member of an INCOMPLETE type. `[class.static.
//! data]` allows a non-defining static-member declaration to have incomplete
//! type, so:
//!
//!   * including the header, declaring the message, and reading its fields all
//!     compile clean, even under `-Wall -Wextra -Werror`;
//!   * `M::SERIALIZED_SIZE_MAX` in an array bound or a `size_t` context is an
//!     error naming the incomplete type — which carries the message AND the
//!     member that costs it the bound.
//!
//! That claim is only worth anything if a compiler agrees, hence this file. Both
//! halves are asserted, because either one alone passes for the wrong reason: a
//! header that never compiles "poisons" everything, and a header that compiles
//! but poisons nothing states a size again.
//!
//! Ignored by default (spawns g++); run with:
//!
//!     cargo test -p rosidl-codegen --test cpp_bound_compile_check -- --ignored
//!
//! and it is on the `just test-ignored` lane.

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

/// Write the generated header plus a stub `nros/platform.h` (the real one needs
/// per-build config) into a temp dir and hand back the include roots.
fn stage(header: &str, header_name: &str, probe: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("nros")).unwrap();
    fs::write(
        dir.path().join("nros/platform.h"),
        "#ifndef PSTUB\n#define PSTUB\n#include <cstddef>\n\
         extern \"C\" void* nros_platform_malloc(size_t);\n\
         extern \"C\" void nros_platform_free(void*);\n#endif\n",
    )
    .unwrap();
    fs::write(dir.path().join(header_name), header).unwrap();
    let probe_path = dir.path().join("probe.cpp");
    fs::write(&probe_path, probe).unwrap();
    (dir, probe_path)
}

fn compile(dir: &tempfile::TempDir, probe: &PathBuf, strict: bool) -> std::process::Output {
    let inc = repo_root().join("packages/api/nros-cpp/include");
    let mut cmd = Command::new("g++");
    cmd.args([
        "-std=c++14",
        "-fno-exceptions",
        "-fno-rtti",
        "-fsyntax-only",
    ]);
    if strict {
        cmd.args(["-Wall", "-Wextra", "-Werror"]);
    }
    cmd.arg("-I")
        .arg(&inc)
        .arg("-I")
        .arg(dir.path())
        .arg(probe)
        .output()
        .expect("spawn g++")
}

/// A BOUNDED type states its derived bound, and the constant is usable where a
/// size is required — an array bound, which is the shape every consumer of it
/// uses (`uint8_t buf[M::SERIALIZED_SIZE_MAX]`).
#[test]
#[ignore = "spawns g++"]
fn a_bounded_type_states_a_usable_constant() {
    let msg = parse_message("int32 data\n").unwrap();
    let pkg =
        generate_cpp_message_package("std_msgs", "Int32", &msg, "h", &CapacityResolver::empty())
            .unwrap();
    assert!(
        pkg.header.contains("SERIALIZED_SIZE_MAX = 12;"),
        "expected the derived bound (12 = max of the two encodings):\n{}",
        pkg.header
    );
    let (dir, probe) = stage(
        &pkg.header,
        "std_msgs_msg_int32.hpp",
        "#include \"std_msgs_msg_int32.hpp\"\n\
         static_assert(std_msgs::msg::Int32::SERIALIZED_SIZE_MAX == 12, \"\");\n\
         int f() { unsigned char b[std_msgs::msg::Int32::SERIALIZED_SIZE_MAX]; return (int)sizeof(b); }\n",
    );
    let out = compile(&dir, &probe, true);
    assert!(
        out.status.success(),
        "a bounded type's header must compile and its constant must be usable:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// An UNBOUNDED type states NO size. Two halves, both required:
/// the header is clean to include and use, and NAMING the constant is an error
/// that prints the poison identifier.
#[test]
#[ignore = "spawns g++"]
fn an_unbounded_type_costs_nothing_to_include_and_poisons_only_on_use() {
    let msg = parse_message("string data\n").unwrap();
    let pkg =
        generate_cpp_message_package("std_msgs", "String", &msg, "h", &CapacityResolver::empty())
            .unwrap();
    assert!(
        !pkg.header.contains("SERIALIZED_SIZE_MAX = "),
        "an unbounded type must state no size:\n{}",
        pkg.header
    );

    // Half 1 — include, declare, read a field. Clean, under -Werror.
    let (dir, probe) = stage(
        &pkg.header,
        "std_msgs_msg_string.hpp",
        "#include \"std_msgs_msg_string.hpp\"\n\
         static std_msgs::msg::String g;\n\
         int f() { return (int)g.data.length(); }\n",
    );
    let out = compile(&dir, &probe, true);
    assert!(
        out.status.success(),
        "an unbounded type's header must still be free to include and use:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Half 2 — naming the constant is an error, and the error names the type
    // AND the member. Not `-Werror`: this must fail as an ERROR, not a warning.
    let (dir, probe) = stage(
        &pkg.header,
        "std_msgs_msg_string.hpp",
        "#include \"std_msgs_msg_string.hpp\"\n\
         int f() { unsigned char b[std_msgs::msg::String::SERIALIZED_SIZE_MAX]; return (int)sizeof(b); }\n",
    );
    let out = compile(&dir, &probe, false);
    assert!(
        !out.status.success(),
        "naming an unbounded type's size constant must be a compile error"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("NROS_UNBOUNDED__std_msgs_msg_string__field_data"),
        "the diagnostic must name the type and the member that costs the bound, \
         not just report a missing member:\n{stderr}"
    );
}
