//! Issue 1230 — run the emitter under test, at BUILD time.
//!
//! `rosidl-codegen`'s `compilation_test.rs` used to do this at TEST time and
//! then spawn `cargo check` / `cargo clippy` on the result. The generation is
//! the part that had to move first: everything else was already a build-stage
//! problem with a build-stage answer (`[[compile_check_fixture]]`).
//!
//! One `.msg` per file under `msgs/`, so the inputs are data a reader can see
//! and `compile-check-signature.sh` can hash, rather than string literals in a
//! test body. All four land in ONE `$OUT_DIR/messages.rs`, included as
//! `crate::msg`, because that is the path the emitted idiomatic code names
//! ABSOLUTELY (`crate::msg::rmw::<Message>`) — the layout the four tests built
//! by hand around their ONE message.
//!
//! Four messages in one package is where that hand-built layout stops being
//! enough, and the shape it needs is the emitter's own: each half of each
//! message opens with its own `#[cfg(feature = "serde")] use serde::…`, so
//! concatenating four of them into one module is eight imports and eight
//! `E0252`s. The scaffold pack (`packs/scaffold/lib.rs.jinja`) answers it with
//! `pub mod rmw; pub use rmw::*;` — a MODULE per emitted file, flattened by a
//! re-export — and that is what this writes: `msg::rmw::<message>` re-exported
//! into `msg::rmw`, `msg::<message>` re-exported into `msg`. The absolute
//! paths in the emitted code then resolve exactly as they do in a real
//! generated package.
//!
//! A parse or generate failure PANICS. This crate exists to compile the
//! emitter's output; producing no output and building clean would be the
//! vacuous-pass shape the repo files issues about.

use rosidl_codegen::generate_message_package;
use rosidl_parser::parse_message;
use std::{collections::HashSet, env, fs, path::PathBuf};

/// `SimpleMsg` -> `simple_msg`, the module name the emitted code itself uses
/// for a message (the emitter's `to_snake_case` is not public API and these
/// four names are plain CamelCase).
fn snake_case(name: &str) -> String {
    let mut out = String::new();
    for (i, ch) in name.char_indices() {
        if ch.is_ascii_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let msg_dir = manifest_dir.join("msgs");

    // Relative, and therefore safe per-leaf: cargo reads a `rerun-if-changed`
    // path back from the STORED build output rather than re-resolving it
    // (issue 0491 is about env VALUES, not about this).
    println!("cargo:rerun-if-changed=msgs");
    println!("cargo:rerun-if-changed=build.rs");

    let mut names: Vec<String> = fs::read_dir(&msg_dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", msg_dir.display()))
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "msg"))
        .map(|p| {
            p.file_stem()
                .expect("msg file stem")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    assert!(
        !names.is_empty(),
        "no `.msg` inputs under {} — this fixture would compile nothing and \
         report success",
        msg_dir.display()
    );

    let mut rmw = String::new();
    let mut idiomatic = String::new();
    for name in &names {
        let path = msg_dir.join(format!("{name}.msg"));
        let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {name}.msg: {e}"));
        let message = parse_message(&text).unwrap_or_else(|e| panic!("parse {name}.msg: {e:?}"));
        let generated = generate_message_package("test_msgs", name, &message, &HashSet::new())
            .unwrap_or_else(|e| panic!("generate {name}: {e:?}"));
        let module = snake_case(name);
        // No `use rosidl_runtime_rs` is injected around either half: every
        // reference the emitter writes is `crate::rosidl_runtime_rs::…`, so the
        // stub is found from any depth. (The tests' wrapper carried such a
        // `use`, under an `#[allow(unused_imports)]` that says it was never
        // load-bearing.)
        rmw.push_str(&format!(
            "pub mod {module} {{\n{body}\n}}\npub use {module}::*;\n",
            body = generated.message_rmw,
        ));
        // `#[allow(clippy::clone_on_copy)]` is a MEASURED, TRACKED gap, not a
        // convenience: the `packs/rust` idiomatic pack writes
        // `<field>.clone()` for `PrimitiveArray` and `LargeArray` fields, and
        // an array of primitives is `Copy`, so clippy denies four sites in
        // `ArrayMsg`'s two conversions. Issue 1244 owns the emitter fix; the
        // allow names it so this fixture reports the rest of `clippy::all`
        // instead of being red for a defect it did not introduce.
        //
        // The old `test_clippy_no_warnings` saw none of this twice over: it
        // generated `TestMsg` (`int32` + `string`), which has no array field,
        // and it only failed on the substring `"error"` in clippy's stderr —
        // clippy WARNINGS, which is all `-W clippy::all` can produce, read as
        // a pass.
        idiomatic.push_str(&format!(
            "#[allow(clippy::clone_on_copy)] // issue 1244\n\
             pub mod {module} {{\n{body}\n}}\npub use {module}::*;\n",
            body = generated.message_idiomatic,
        ));
    }

    // `#[allow(invalid_value)]` on the rmw layer, and NOWHERE else.
    //
    // Every emitted rmw `Default::default()` is `std::mem::zeroed()` followed by
    // the C `__init` call, which is correct against the REAL
    // `rosidl_runtime_rs::String` — a `#[repr(C)]` pointer + size + capacity,
    // for which an all-zero bit pattern is a valid value. It is NOT correct
    // against the stub in `src/lib.rs`, whose `String` is `std::string::String`
    // (a `NonNull` inside), so rustc's `invalid_value` fires on a property of
    // the STAND-IN rather than of the emitter.
    //
    // This is also why the old `test_check_no_warnings` could pass: it
    // generated `Point` (`int32 x` / `float64 y`), the one message shape with
    // no String field, so the warning its two String-carrying siblings
    // provoked was never in its crate. `deny(warnings)` reaches all four here,
    // which is the strengthening — the narrow allow is what keeps the stub's
    // inaccuracy from being reported as the emitter's.
    let module = format!("#[allow(invalid_value)]\npub mod rmw {{\n{rmw}\n}}\n{idiomatic}\n");
    fs::write(out_dir.join("messages.rs"), module).expect("write messages.rs");
}
