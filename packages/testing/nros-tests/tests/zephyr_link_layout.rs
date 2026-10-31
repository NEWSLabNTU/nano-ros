//! Link-layout witnesses for Zephyr images on a real 32-bit board
//! (`mps2_an385`): what the LINKER did, read back out of the ELF.
//!
//! **Bucket: compile check.** Each test resolves a west fixture and reads its
//! `zephyr.elf`; nothing boots.
//!
//! * Issue 2077 -- RFC-0045's baked boot config (`NROS_BOOT_CONFIG`, emitted
//!   into `.nros_boot_config` by every typed entry) must sit in `rodata`. No
//!   Zephyr linker script names the section, so until the nano-ros module
//!   placed it (`zephyr/nros_boot_config.ld`) it was an ORPHAN, and on
//!   `mr_canhubk3/s32k344` the linker put it in the 0x100-byte IVT_HEADER
//!   region: "region `IVT_HEADER' overflowed by 268 bytes". On this board the
//!   orphan happens to fit, so the assertion is the placement itself, which
//!   holds on every board: the symbol's section is `rodata`, and no output
//!   section named `.nros_boot_config` exists. The placement is the module's,
//!   so it is the same for every language; the witness is the typed C entry,
//!   because the single-package Rust talker bakes no boot config.
//! * Issue 2078 -- a Rust image with NO IP stack links. The entry
//!   `nros::main!` emits calls `nros_platform_zephyr_wait_network` whatever the
//!   transport, and the symbol was compiled only with `CONFIG_NET_SOCKETS`. The
//!   `build-cortex-m-rust-talker-serial-zenoh` row is that image (zenoh over a
//!   UART, `cmake/zephyr/mps2-an385-serial.conf`); the build linking is the
//!   regression, and the test pins that the image really has no stack.
//! * Issue 2084 -- `CONFIG_NROS_ZEPHYR_STACKS_IN_DTCM` moved only the task
//!   stacks (`nros_thread_stacks`); the tier pool (`nros_tier_stacks`) stayed
//!   in SRAM, and a Rust level image with a 64 KiB slot overflowed the
//!   S32K344's RAM by 30,028 B. The `build-cortex-m-c-tier-stacks-dtcm-zenoh`
//!   row spawns one level on the pool for an mps2_an385 given a "DTCM" region,
//!   and the test requires both arrays inside the DTCM output section.
//!
//! Prerequisite: `just zephyr build-fixtures` (the `build-cortex-m-*` leaves).
//!
//! Run with: `cargo nextest run -p nros-tests --test zephyr_link_layout`

use nros_tests::{
    elf::{Elf, Symbol},
    fixtures::{
        RequireFixture, Rmw, build_zephyr_cortex_m_example,
        build_zephyr_cortex_m_rust_serial_talker, build_zephyr_cortex_m_tier_stacks_dtcm,
    },
};
use std::path::Path;

/// The section headers and static symbol table of an image. Issue 2122 --
/// ONE in-process reader for every test that reads symbols
/// (`nros_tests::elf`); this file carried its own ELF32-only copy.
fn read_elf(path: &Path) -> Elf {
    nros_tests::elf::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The baked boot config. The C entry's is a file-scope `static const`
/// compiled as C++ by the typed entry, so its local symbol is mangled
/// (_ZL16NROS_BOOT_CONFIG); the Rust one is `#[no_mangle]`.
fn boot_config(elf: &Elf) -> Option<&Symbol> {
    elf.symbols.iter().find(|s| {
        s.shndx != 0
            && (s.name == "NROS_BOOT_CONFIG"
                || (s.name.starts_with("_Z") && s.name.ends_with("16NROS_BOOT_CONFIG")))
    })
}

/// `NROS_BOOT_CONFIG` is defined, in `rodata`, inside its bounds, and no
/// orphan `.nros_boot_config` output section exists.
fn assert_boot_config_in_rodata(what: &str, elf_path: &Path) {
    let elf = read_elf(elf_path);
    let sym = boot_config(&elf).unwrap_or_else(|| {
        panic!(
            "{what}: {} defines no NROS_BOOT_CONFIG -- the typed entry baked no \
             RFC-0045 boot config, so this test would check nothing",
            elf_path.display()
        )
    });
    let sec = elf.sections.get(sym.shndx as usize).unwrap_or_else(|| {
        panic!(
            "{what}: NROS_BOOT_CONFIG has section index {} out of range",
            sym.shndx
        )
    });
    assert_eq!(
        sec.name, "rodata",
        "{what}: NROS_BOOT_CONFIG is in output section `{}`, not `rodata` -- \
         `.nros_boot_config` is an orphan again, and the linker places an orphan \
         wherever it likes (on mr_canhubk3/s32k344, the IVT_HEADER region; issue \
         2077). zephyr/CMakeLists.txt must add zephyr/nros_boot_config.ld to RODATA.",
        sec.name
    );
    assert!(
        (sec.addr..sec.addr + sec.size).contains(&sym.value),
        "{what}: NROS_BOOT_CONFIG at {:#x} is outside rodata [{:#x}, {:#x})",
        sym.value,
        sec.addr,
        sec.addr + sec.size
    );
    assert!(
        !elf.sections
            .iter()
            .any(|s| s.name.starts_with(".nros_boot_config")),
        "{what}: an output section named .nros_boot_config exists beside rodata"
    );
}

/// Issue 2077 -- a typed C entry's boot config is placed by the module.
#[test]
fn c_typed_entry_boot_config_is_in_rodata() {
    let elf = build_zephyr_cortex_m_example("c", "talker", Rmw::Zenoh)
        .require("zephyr cortex-m C talker (build-cortex-m-c-talker-zenoh)");
    assert_boot_config_in_rodata("build-cortex-m-c-talker-zenoh", &elf);
}

/// Issue 2078 -- the serial-only Rust image linked (the resolver found it),
/// it defines the wait the entry calls, and it carries no IP stack: a row that
/// silently regained networking would prove nothing about the no-stack link.
#[test]
fn serial_only_rust_image_links_without_ip_stack() {
    let elf_path = build_zephyr_cortex_m_rust_serial_talker()
        .require("zephyr cortex-m serial Rust talker (build-cortex-m-rust-talker-serial-zenoh)");
    let elf = read_elf(&elf_path);
    assert!(
        elf.defined("nros_platform_zephyr_wait_network").is_some(),
        "{}: nros_platform_zephyr_wait_network is not defined, yet the Rust entry \
         calls it (issue 2078)",
        elf_path.display()
    );
    for stack in ["net_if_get_default", "net_context_get", "zsock_socket"] {
        assert!(
            elf.defined(stack).is_none(),
            "{}: defines `{stack}` -- the image has an IP stack, so it no longer \
             witnesses a serial-only link (check the row's conf order: \
             mps2-an385-serial.conf must merge after the board's prj.conf)",
            elf_path.display()
        );
    }
}

/// Issue 2084 -- with `CONFIG_NROS_ZEPHYR_STACKS_IN_DTCM`, the tier pool and an
/// entry-declared level stack sit in the DTCM output section beside the task
/// stacks.
#[test]
fn tier_stack_pool_follows_stacks_in_dtcm() {
    let elf_path = build_zephyr_cortex_m_tier_stacks_dtcm().require(
        "zephyr cortex-m tier-stacks-in-DTCM image (build-cortex-m-c-tier-stacks-dtcm-zenoh)",
    );
    let elf = read_elf(&elf_path);
    let dtcm = elf
        .sections
        .iter()
        .find(|s| s.name == "DTCM")
        .unwrap_or_else(|| {
            panic!(
                "{}: no `DTCM` output section -- the fixture's app.overlay no longer gives \
                 mps2_an385 a \"DTCM\" region, so this image tests nothing",
                elf_path.display()
            )
        });
    // The shim's tier pool, a level stack declared the way a generated C/C++
    // entry declares one (`NROS_TIER_TASK_MEMORY_DEFINE`), and the task stacks.
    for array in [
        "nros_tier_stacks",
        "__nros_tier_stack_1",
        "nros_thread_stacks",
    ] {
        let sym = elf.defined(array).unwrap_or_else(|| {
            panic!(
                "{}: `{array}` is not in the image -- the fixture no longer spawns both levels \
                 (an unused stack is dropped by --gc-sections) or no longer opens a \
                 transport task",
                elf_path.display()
            )
        });
        let sec = &elf.sections[sym.shndx as usize];
        assert!(
            sec.name == "DTCM" && (dtcm.addr..dtcm.addr + dtcm.size).contains(&sym.value),
            "{}: `{array}` at {:#x} is in `{}`, not in DTCM [{:#x}, {:#x}) -- \
             CONFIG_NROS_ZEPHYR_STACKS_IN_DTCM (and the tier knob that follows it) must place \
             it there (issue 2084)",
            elf_path.display(),
            sym.value,
            sec.name,
            dtcm.addr,
            dtcm.addr + dtcm.size
        );
    }
}
