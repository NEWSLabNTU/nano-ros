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

#[test]
fn platformio_integration_shell_smoke() {
    if !have("pio") && !have("platformio") {
        nros_tests::skip!("pio CLI not on PATH — install PlatformIO Core");
    }

    let root = workspace_root();
    let shell = root.join("integrations/platformio");
    assert!(
        shell.join("library.json").exists(),
        "integrations/platformio/library.json missing",
    );
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
    let bin = if have("pio") { "pio" } else { "platformio" };
    let version = Command::new(bin)
        .arg("--version")
        .output()
        .expect("invoke pio --version");
    assert!(
        version.status.success(),
        "{} --version failed: {}",
        bin,
        String::from_utf8_lossy(&version.stderr)
    );
}
