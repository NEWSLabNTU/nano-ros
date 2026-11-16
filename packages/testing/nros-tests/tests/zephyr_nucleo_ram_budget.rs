//! The Nucleo-F767ZI serial talker fits its RAM, with headroom, and a failure
//! names the symbols that take it.
//!
//! Zephyr links the board's 384 KiB SRAM as its `RAM` region, and the image
//! (`[image.nucleo_serial]` of `examples/zephyr/c/talker`) is the one in-tree
//! build for it. When the stock
//! node interfaces went on by default, the image gained a 32-slot parameter
//! store (285,696 B of .bss) and a 192 KiB heap default, and the link failed
//! with `region 'RAM' overflowed by 247616 bytes`, naming no symbol. A link
//! error is the last moment the overflow can be seen, and the least useful.
//!
//! So this reads the BUILT image with the operator's own tool
//! (`scripts/nros-mem-report.py --json`) and holds its RAM to a budget below
//! the board's, so growth fails here first, while the image still links, with
//! the largest RAM symbols and owners in the message.
//!
//! A build-artifact check, not a runtime cell: it boots nothing. It runs where
//! the fixture is built, i.e. in a lane that claims the Nucleo platform.
//!
//! Run with: `cargo nextest run -p nros-tests --test zephyr_nucleo_ram_budget`

use nros_tests::{fixtures::build_zephyr_nucleo_c_serial_talker, matrix::PlatformId};
use std::process::Command;

/// The `RAM` region Zephyr's `nucleo_f767zi` links into (the linker's own
/// `Memory region` table: `RAM: ... 384 KB`).
const BOARD_RAM_BYTES: u64 = 384 * 1024;

/// The share of it the image may take. The rest is headroom for an entity
/// added without re-measuring, and it is what turns an overflow into a named
/// failure here rather than an anonymous one at link. MEASURED with the stock
/// node interfaces on (8-slot store, 112 KiB heap): 349,685 B by section,
/// 91 % of the region, against a 373,555 B budget.
const BUDGET_PERCENT: u64 = 95;

#[test]
fn zephyr_nucleo_serial_talker_fits_its_ram_budget() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::ZephyrNucleoF767zi],
        "the Nucleo-F767ZI RAM budget",
    );
    let elf = build_zephyr_nucleo_c_serial_talker()
        .unwrap_or_else(|e| panic!("the Nucleo serial talker image: {e}"));
    let root = nros_tests::project_root();
    let out = Command::new("python3")
        .arg(root.join("scripts/nros-mem-report.py"))
        .arg(&elf)
        .arg("--json")
        .current_dir(&root)
        .output()
        .expect("run scripts/nros-mem-report.py");
    assert!(
        out.status.success(),
        "nros-mem-report.py failed on {}:\n{}",
        elf.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("nros-mem-report.py --json is JSON");
    let ram = report["section_ram_total"]
        .as_u64()
        .expect("the report states section_ram_total");
    let budget = BOARD_RAM_BYTES * BUDGET_PERCENT / 100;
    if ram <= budget {
        return;
    }
    let top: Vec<String> = report["top_ram"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .take(15)
                .map(|r| {
                    format!(
                        "    {:>9} B  {}  [{}]",
                        r["bytes"].as_u64().unwrap_or(0),
                        r["symbol"].as_str().unwrap_or("?"),
                        r["owner"].as_str().unwrap_or("?")
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    panic!(
        "{}: {ram} B of RAM, over the {budget} B budget ({BUDGET_PERCENT} % of the board's \
         {BOARD_RAM_BYTES} B) by {} B. The largest RAM symbols:\n{}\n\
         Re-run `just mem-report {}` for the full table by owner.",
        elf.display(),
        ram - budget,
        top.join("\n"),
        elf.display()
    );
}
