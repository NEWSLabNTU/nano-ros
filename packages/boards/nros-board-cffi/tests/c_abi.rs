//! Phase 173.4 — compile `tests/board_consumer.c` against
//! `<nros/board.h>` to prove the header is valid C and its
//! `nros_board_*` signatures match how a standalone C app consumes
//! them. Compile-only (to an object); linking the symbols needs a
//! board that invoked `nros_board_export!` (see export_compiles.rs).

#[test]
fn c_consumer_compiles_against_board_header() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let include = format!("{manifest}/include");
    let src = format!("{manifest}/tests/board_consumer.c");
    let out =
        std::env::var("OUT_DIR").unwrap_or_else(|_| std::env::temp_dir().display().to_string());
    let obj = format!("{out}/board_consumer.o");

    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let status = std::process::Command::new(&cc)
        .args(["-c", "-Wall", "-Wextra", "-Werror", "-std=c11"])
        .arg("-I")
        .arg(&include)
        .arg(&src)
        .arg("-o")
        .arg(&obj)
        .status()
        .expect("failed to invoke C compiler — install a cc toolchain");

    assert!(
        status.success(),
        "C consumer failed to compile against <nros/board.h> — ABI drift or header error"
    );
}
