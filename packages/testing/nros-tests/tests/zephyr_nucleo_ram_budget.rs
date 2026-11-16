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

use nros_tests::{
    fixtures::{RequireFixture, build_zephyr_nucleo_c_serial_talker},
    matrix::PlatformId,
};
use std::{path::Path, process::Command};

/// The share of the image's linked `RAM` region it may take. The rest is
/// headroom for an entity added without re-measuring, and it is what turns an overflow into a named
/// failure here rather than an anonymous one at link. MEASURED with the stock
/// node interfaces on (8-slot store, 112 KiB heap): 349,685 B by section,
/// 91 % of the 384 KiB region, against a 373,555 B budget.
const BUDGET_PERCENT: u64 = 95;

/// The length of the `RAM` memory region the image was LINKED against, from
/// the `Memory Configuration` table of the linker map beside it
/// (`zephyr/zephyr.map`). Read, never restated: a board or Zephyr line that
/// moves the region moves the ceiling with it, and a constant would hold the
/// image to the wrong one.
fn linked_ram_region_bytes(elf: &Path) -> u64 {
    let map = elf.with_file_name("zephyr.map");
    let text = std::fs::read_to_string(&map)
        .unwrap_or_else(|e| panic!("{}: {e} (the linker map beside the image)", map.display()));
    let mut rows = text
        .lines()
        .skip_while(|l| !l.starts_with("Memory Configuration"))
        .skip(1);
    rows.find_map(|l| {
        let mut cols = l.split_whitespace();
        (cols.next() == Some("RAM")).then(|| {
            let _origin = cols.next();
            cols.next()
                .and_then(|len| u64::from_str_radix(len.trim_start_matches("0x"), 16).ok())
        })?
    })
    .unwrap_or_else(|| {
        panic!(
            "{}: no `RAM` row with a hex length in its Memory Configuration table",
            map.display()
        )
    })
}

#[test]
fn zephyr_nucleo_serial_talker_fits_its_ram_budget() {
    nros_tests::fixtures::lane::require_platform_in_lane(
        &[PlatformId::ZephyrNucleoF767zi],
        "the Nucleo-F767ZI RAM budget",
    );
    let elf = build_zephyr_nucleo_c_serial_talker().require("the Nucleo serial talker image");
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
    let region = linked_ram_region_bytes(&elf);
    let budget = region * BUDGET_PERCENT / 100;
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
        "{}: {ram} B of RAM, over the {budget} B budget ({BUDGET_PERCENT} % of the linked \
         {region} B RAM region) by {} B. The largest RAM symbols:\n{}\n\
         Re-run `just mem-report {}` for the full table by owner.",
        elf.display(),
        ram - budget,
        top.join("\n"),
        elf.display()
    );
}

/// The map reader against the table GNU ld writes (the nucleo image's own
/// rows, `SRAM0` sharing RAM's origin): it takes the `RAM` row's length, not
/// the first row's or an alias's.
#[test]
fn the_ram_region_is_read_from_the_linker_map() {
    let dir = std::env::temp_dir().join(format!("nros-ram-map-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let elf = dir.join("zephyr.elf");
    std::fs::write(
        dir.join("zephyr.map"),
        "Archive member included\n\nMemory Configuration\n\n\
         Name             Origin             Length             Attributes\n\
         FLASH            0x0000000008000000 0x0000000000200000 xr\n\
         RAM              0x0000000020020000 0x0000000000060000 xw\n\
         SRAM0            0x0000000020020000 0x0000000000060000\n\
         DTCM             0x0000000020000000 0x0000000000020000\n",
    )
    .unwrap();
    assert_eq!(linked_ram_region_bytes(&elf), 384 * 1024);
    std::fs::remove_dir_all(&dir).unwrap();
}
