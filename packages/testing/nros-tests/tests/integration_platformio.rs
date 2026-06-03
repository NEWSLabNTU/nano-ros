//! Phase 139.6 — PlatformIO integration shell smoke test.
//!
//! Drives `pio run` against the bundled example consumer under
//! `integrations/platformio/examples/talker/`. Skips via
//! `nros_tests::skip!` when `pio` is absent.

use std::{path::PathBuf, process::Command};

fn workspace_root() -> PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .nth(3)
        .expect("workspace root above CARGO_MANIFEST_DIR")
        .to_path_buf()
}

fn have(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn platformio_bin() -> Option<PathBuf> {
    if have("pio") {
        return Some(PathBuf::from("pio"));
    }
    if have("platformio") {
        return Some(PathBuf::from("platformio"));
    }
    None
}

#[test]
fn platformio_integration_shell_smoke() {
    let Some(bin) = platformio_bin() else {
        nros_tests::skip!("pio CLI not available — run `just platformio setup`");
    };

    let root = workspace_root();
    let shell = root.join("integrations/platformio");

    // Phase 214.L.2 — the original Phase 139 PlatformIO library
    // shell (`library.json` + `library.properties` +
    // `examples/talker/platformio.ini`) was removed in Phase 208.D.8
    // (commit 6382cd655, 2026-05-30). Phase 212.H.6 later
    // reintroduced PlatformIO support but as an ahead-of-vendor
    // `extra_script` adapter (`nros_codegen.py`), NOT a PIO Library
    // Manager shell, so the original assertions no longer match the
    // tree's intended shape. Skip cleanly when the legacy library
    // manifest is absent rather than re-introducing the deprecated
    // shape.
    if !shell.join("library.json").exists() {
        nros_tests::skip!(
            "integrations/platformio/library.json absent — Phase 208.D.8 \
             retired the PIO Library Manager shell; Phase 212.H.6 \
             reintroduced PlatformIO as an extra_script adapter \
             (integrations/platformio/nros_codegen.py), not a library \
             manifest"
        );
    }

    assert!(
        shell.join("library.properties").exists(),
        "integrations/platformio/library.properties missing",
    );
    let example_ini = shell.join("examples/talker/platformio.ini");
    assert!(
        example_ini.exists(),
        "integrations/platformio/examples/talker/platformio.ini missing",
    );

    // Validate the library manifest parses.
    let lib_json_raw =
        std::fs::read_to_string(shell.join("library.json")).expect("read library.json");
    assert!(
        lib_json_raw.contains("\"name\": \"nano-ros\""),
        "library.json must declare name = nano-ros",
    );
    assert!(
        lib_json_raw.contains("\"frameworks\""),
        "library.json must declare frameworks for PIO discovery",
    );

    // `pio run` against the example needs an internet-fetched
    // platform package (espidf / native) — that's a heavy step gated
    // to per-RTOS CI. Here we run `pio --version` as the cheap
    // smoke that the CLI is functional when present.
    let version = Command::new(&bin)
        .arg("--version")
        .output()
        .expect("invoke pio --version");
    assert!(
        version.status.success(),
        "{} --version failed: {}",
        bin.display(),
        String::from_utf8_lossy(&version.stderr)
    );
}
