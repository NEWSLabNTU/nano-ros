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
//!   stacks (`nros_thread_stacks`); the sched task pool (`nros_sched_task_stacks`) stayed
//!   in SRAM, and a Rust level image with a 64 KiB slot overflowed the
//!   S32K344's RAM by 30,028 B. The `build-cortex-m-c-sched-task-stacks-dtcm-zenoh`
//!   row spawns one level on the pool for an mps2_an385 given a "DTCM" region,
//!   and the test requires both arrays inside the DTCM output section.
//! * Issue 2793 -- that output section must be the MODULE's
//!   (`zephyr/nros_dtcm_stacks.ld`), never the one Zephyr generates for the
//!   region, which is KEEP: an image with no sched task then still carried the
//!   whole pool. The nucleo-f767zi's serial C talker (a board that turns the
//!   knob on, an image with no sched task) must define no pool at all.
//!
//! Prerequisite: `just zephyr build-fixtures` (the `build-cortex-m-*` leaves).
//!
//! Run with: `cargo nextest run -p nros-tests --test zephyr_link_layout`

use nros_tests::{
    elf::{Elf, Symbol},
    fixtures::{
        RequireFixture, Rmw, build_zephyr_cortex_m_example,
        build_zephyr_cortex_m_rust_serial_talker, build_zephyr_cortex_m_sched_task_stacks_dtcm,
        build_zephyr_nucleo_c_serial_talker,
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

/// The module's DTCM output section (`zephyr/nros_dtcm_stacks.ld`): placed
/// `> DTCM`, so the link itself fails unless the devicetree has the region.
const NROS_DTCM_SECTION: &str = ".nros_dtcm_stacks";

/// `array` is defined and sits inside the module's DTCM output section, not in
/// the KEEP output section Zephyr generates for the region (`DTCM`), which no
/// linker can trim (issue 2793).
fn assert_in_nros_dtcm(elf: &Elf, elf_path: &Path, array: &str, why_absent: &str) {
    let sym = elf.defined(array).unwrap_or_else(|| {
        panic!(
            "{}: `{array}` is not in the image -- {why_absent}",
            elf_path.display()
        )
    });
    let sec = &elf.sections[sym.shndx as usize];
    assert!(
        sec.name == NROS_DTCM_SECTION && (sec.addr..sec.addr + sec.size).contains(&sym.value),
        "{}: `{array}` at {:#x} is in `{}`, not in `{NROS_DTCM_SECTION}` -- \
         CONFIG_NROS_ZEPHYR_STACKS_IN_DTCM (and the sched task knob that follows it) must \
         place it in the module's DTCM section (issues 2084, 2793)",
        elf_path.display(),
        sym.value,
        sec.name,
    );
}

/// Issue 2084 -- with `CONFIG_NROS_ZEPHYR_STACKS_IN_DTCM`, the sched task pool and an
/// entry-declared level stack sit in DTCM beside the task stacks (issue 2793:
/// in the module's own DTCM output section).
#[test]
fn sched_task_stack_pool_follows_stacks_in_dtcm() {
    let elf_path = build_zephyr_cortex_m_sched_task_stacks_dtcm().require(
        "zephyr cortex-m sched-task-stacks-in-DTCM image (build-cortex-m-c-sched-task-stacks-dtcm-zenoh)",
    );
    let elf = read_elf(&elf_path);
    assert!(
        elf.sections.iter().any(|s| s.name == NROS_DTCM_SECTION),
        "{}: no `{NROS_DTCM_SECTION}` output section -- zephyr/CMakeLists.txt no longer adds \
         zephyr/nros_dtcm_stacks.ld, or the fixture no longer turns the knob on, so this \
         image tests nothing",
        elf_path.display()
    );
    // The shim's sched task pool, a level stack declared the way a generated C/C++
    // entry declares one (`NROS_SCHED_TASK_MEMORY_DEFINE`), and the task stacks.
    for array in [
        "nros_sched_task_stacks",
        "__nros_sched_task_stack_1",
        "nros_thread_stacks",
    ] {
        assert_in_nros_dtcm(
            &elf,
            &elf_path,
            array,
            "the fixture no longer spawns both levels and a task on the task pool (an \
             unused stack is dropped by --gc-sections)",
        );
    }
}

/// Issue 2793 -- an image with `CONFIG_NROS_ZEPHYR_STACKS_IN_DTCM` that declares
/// no sched task carries no sched task pool. Zephyr's generated `DTCM` output
/// section is KEEP, so while the pools were placed there the nucleo-f767zi
/// kept 65,792 dead bytes of its 128 KiB DTCM. The witness is the nucleo
/// board's serial C talker: its board turns the knob on, and the talker spawns
/// the transport's tasks and no sched task.
#[test]
fn an_image_without_sched_tasks_carries_no_pool_in_dtcm() {
    let elf_path = build_zephyr_nucleo_c_serial_talker()
        .require("zephyr/c/talker for nucleo_f767zi (image nucleo_serial)");
    let elf = read_elf(&elf_path);
    assert_in_nros_dtcm(
        &elf,
        &elf_path,
        "nros_thread_stacks",
        "the talker opens no transport task, or the nucleo board no longer sets \
         CONFIG_NROS_ZEPHYR_STACKS_IN_DTCM, so this image tests nothing",
    );
    assert!(
        elf.defined("nros_sched_task_stacks").is_none(),
        "{}: defines `nros_sched_task_stacks` although the talker declares no sched task \
         -- the pool is in a section --gc-sections cannot trim (Zephyr's KEEP `DTCM`), \
         or something now references the pool spawn path",
        elf_path.display()
    );
}
