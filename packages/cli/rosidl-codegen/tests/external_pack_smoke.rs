//! phase-335 W4.c — what the `--template-dir` mechanism actually proves.
//!
//! An external pack directory OVERRIDES a bundled template purely at runtime:
//! no recompile, no Rust change. That is override of an EXISTING registry key,
//! not addition of a language — nothing here requests a name the Rust
//! generators do not already request (see `render::OVERRIDE_DIR`). This test
//! lives in its OWN binary so the process-global override + lazily-built render
//! `Environment` are isolated (each integration test file is a separate process).

use std::fs;

#[test]
fn external_pack_dir_overrides_a_bundled_template() {
    let dir = tempfile::tempdir().unwrap();
    // Drop a replacement for the bundled `build.rs` scaffold template. It has no
    // variables, so the rendered output is exactly the file's content (minus the
    // single trailing newline the engine trims).
    fs::write(
        dir.path().join("build.rs.jinja"),
        "// EXTERNAL PACK OVERRIDE MARKER\n",
    )
    .unwrap();

    // Must be set before the first render (the Environment builds lazily).
    rosidl_codegen::render::set_template_dir(dir.path().to_path_buf())
        .expect("set_template_dir once");

    let out = rosidl_codegen::render::render("build.rs", ()).expect("render build.rs");
    assert!(
        out.contains("EXTERNAL PACK OVERRIDE MARKER"),
        "external --template-dir pack must override the bundled template with no rebuild; got: {out:?}"
    );

    // A name NOT present in the override dir still falls back to bundled.
    let bundled = rosidl_codegen::render::render("lib.rs", ()).expect("render lib.rs (bundled)");
    assert!(
        !bundled.contains("EXTERNAL PACK OVERRIDE MARKER"),
        "un-overridden templates must fall back to bundled"
    );
}
