//! phase-290 (RFC-0049) W3.b — Kconfig/platform-toml default drift gate.
//!
//! Zephyr is a Kconfig-native host, so its lane front-end is
//! `zephyr/Kconfig` (hand-wired, per RFC-0049 — no generator). The
//! fragment's `default` lines MUST mirror the zephyr platform package's
//! `[knobs.zenoh.tx]` values or the cmake-TU side and the cargo side
//! drift apart (the issue-0135 ABI class). This test asserts the mirror.

use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("workspace root")
        .to_path_buf()
}

/// Extract the `default <v>` line of one Kconfig `config <name>` block.
fn kconfig_default(kconfig: &str, name: &str) -> String {
    let mut in_block = false;
    for line in kconfig.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("config ") {
            in_block = rest.trim() == name;
            continue;
        }
        if in_block && let Some(v) = t.strip_prefix("default ") {
            return v.trim().to_string();
        }
    }
    panic!("no `default` line for `config {name}` in zephyr/Kconfig");
}

#[test]
fn zephyr_kconfig_mirrors_platform_toml_tx_defaults() {
    let root = workspace_root();
    let kconfig = std::fs::read_to_string(root.join("zephyr/Kconfig")).expect("read Kconfig");
    let toml_text =
        std::fs::read_to_string(root.join("packages/platforms/zephyr/nros-platform.toml"))
            .expect("read zephyr nros-platform.toml");
    let parsed: toml::Value = toml::from_str(&toml_text).expect("parse platform toml");
    // Absent [knobs.zenoh.tx] = builtin defaults (off/off/50). The zephyr
    // flip is currently ABSENT pending issue 0203; the mirror contract
    // still holds either way.
    let tx = parsed
        .get("knobs")
        .and_then(|k| k.get("zenoh"))
        .and_then(|z| z.get("tx"));

    let toml_batch = tx
        .and_then(|t| t.get("batch"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let toml_split = tx
        .and_then(|t| t.get("split_lock"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let toml_flush = tx
        .and_then(|t| t.get("flush_ms"))
        .and_then(|v| v.as_integer())
        .unwrap_or(50);

    let k_batch = kconfig_default(&kconfig, "NROS_ZENOH_TX_BATCH") == "y";
    let k_split = kconfig_default(&kconfig, "NROS_ZENOH_TX_SPLIT_LOCK") == "y";
    let k_flush: i64 = kconfig_default(&kconfig, "NROS_ZENOH_TX_BATCH_FLUSH_MS")
        .parse()
        .expect("FLUSH_MS default must be an int");

    assert_eq!(
        k_batch, toml_batch,
        "NROS_ZENOH_TX_BATCH Kconfig default drifted from the zephyr platform toml"
    );
    assert_eq!(
        k_split, toml_split,
        "NROS_ZENOH_TX_SPLIT_LOCK Kconfig default drifted from the zephyr platform toml"
    );
    assert_eq!(
        k_flush, toml_flush,
        "NROS_ZENOH_TX_BATCH_FLUSH_MS Kconfig default drifted from the zephyr platform toml"
    );
}
