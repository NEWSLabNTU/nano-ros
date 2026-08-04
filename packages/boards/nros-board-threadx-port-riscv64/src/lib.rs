//! # nros-board-threadx-port-riscv64
//!
//! **Layer 2 of RFC-0064: the ThreadX RISC-V64/GNU *arch port*, forked from
//! upstream — not a board.**
//!
//! This crate ships no runtime Rust. It owns six vendored-and-modified files
//! under `port/`, and exposes only the paths a `build.rs` (or a CMake module)
//! needs to compile them:
//!
//! | Path | Upstream original |
//! |---|---|
//! | `port/inc/tx_port.h`                  | `third-party/threadx/kernel/ports/risc-v64/gnu/inc/tx_port.h` |
//! | `port/src/tx_thread_schedule.S`       | `…/ports/risc-v64/gnu/src/tx_thread_schedule.S` |
//! | `port/src/tx_thread_context_save.S`   | `…/ports/risc-v64/gnu/src/tx_thread_context_save.S` |
//! | `port/src/tx_thread_context_restore.S`| `…/ports/risc-v64/gnu/src/tx_thread_context_restore.S` |
//! | `port/src/tx_thread_stack_build.S`    | `…/ports/risc-v64/gnu/src/tx_thread_stack_build.S` |
//! | `port/src/tx_thread_system_return.S`  | `…/ports/risc-v64/gnu/src/tx_thread_system_return.S` |
//!
//! Each file keeps its upstream MIT header and its own `Original:` /
//! `Based on:` line; this table is the index, those headers are the record.
//!
//! # Why the fork exists (the `ULONG` rationale)
//!
//! Upstream's RISC-V64 port types `ULONG` as `unsigned long` — 8 bytes on
//! rv64. NetX Duo's packet code does `ULONG *` pointer arithmetic assuming
//! **4-byte** words, so an 8-byte `ULONG` silently mis-parses every network
//! header. The fix is to type `ULONG` as `unsigned int`, matching the Linux
//! x86_64 and every AArch64 ThreadX port. Retyping it shifts every field
//! offset inside `TX_THREAD`, and the port's context-switch assembly loads
//! those fields at hard-coded offsets with 8-byte `ld`/`sd` — so the header
//! change forces a matching change in five `.S` files, which use explicit
//! `TX_TCB_*_OFF` offsets from `tx_port.h` plus 4-byte `lwu`/`sw` for `ULONG`
//! fields (pointer fields stay 8-byte). Kernel correctness is preserved
//! because pointer-sized operations use `ALIGN_TYPE` (`ULONG64`).
//!
//! # Why this is a unit of its own, and why the two ThreadX boards did NOT merge
//!
//! phase-337 W4. `nros-board-threadx-linux` ships **no** `.S` at all, because
//! upstream's Linux port already types `ULONG` as 4 bytes and needs no fork.
//! Merging the two boards would therefore mean `cfg`-gating RISC-V assembly
//! into a crate that also serves Linux — the wrong cut. The asymmetry is not
//! between the two *boards*; it is that one of them needs a forked *arch
//! port*. So the arch port becomes its own layer-2 unit, and both boards stay
//! thin overlays on top of it.
//!
//! # Precedence — the one way this can go silently wrong
//!
//! `port/inc/tx_port.h` must be searched **before**
//! `<THREADX_DIR>/ports/risc-v64/gnu/inc`. If it is not, the compile still
//! succeeds — against upstream's 8-byte `ULONG` — and the failure is runtime
//! packet corruption, not a diagnostic. Every consumer therefore prepends
//! [`inc_dir`], and `nros-board-common/c/threadx_hooks.c` carries a
//! `_Static_assert(sizeof(ULONG) == 4, …)` so a lost override fails the build
//! instead of the network.

use std::path::PathBuf;

/// Root of the vendored port tree: `port/`, holding `inc/` and `src/`.
///
/// Resolved from this crate's own manifest directory, so there is exactly one
/// spelling of the path in the tree and no repo-root walk to get wrong.
pub fn port_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("port")
}

/// Include dir holding the forked `tx_port.h`. **Must be searched before the
/// upstream port's `inc/`** — see the crate docs.
pub fn inc_dir() -> PathBuf {
    port_dir().join("inc")
}

/// Source dir holding the five forked context-switch `.S` files.
pub fn src_dir() -> PathBuf {
    port_dir().join("src")
}

/// The upstream `ports/risc-v64/gnu/src/*.S` files this port REPLACES.
///
/// Consumers exclude these names when globbing the upstream port's `src/` and
/// compile [`src_dir`]'s copies instead. Kept as data so the exclusion list
/// and the shipped files cannot drift: [`assert_overrides_present`] checks
/// they agree.
pub const ASM_OVERRIDES: &[&str] = &[
    "tx_thread_schedule.S",
    "tx_thread_context_save.S",
    "tx_thread_context_restore.S",
    "tx_thread_stack_build.S",
    "tx_thread_system_return.S",
];

/// Panic unless every name in [`ASM_OVERRIDES`] exists in [`src_dir`] and vice
/// versa.
///
/// A build script calls this before compiling. Without it, deleting or
/// renaming a `.S` here degrades to "upstream's version gets compiled instead"
/// — the same silent-8-byte-`ULONG` failure mode the crate docs describe,
/// reached from the source side rather than the header side.
pub fn assert_overrides_present() {
    let dir = src_dir();
    for name in ASM_OVERRIDES {
        let p = dir.join(name);
        assert!(
            p.is_file(),
            "nros-board-threadx-port-riscv64: {} is listed in ASM_OVERRIDES but missing. \
             Compiling without it silently falls back to upstream's 8-byte-ULONG version.",
            p.display()
        );
    }
    let mut on_disk: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| {
            panic!(
                "nros-board-threadx-port-riscv64: read_dir({}): {e}",
                dir.display()
            )
        })
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "S"))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    on_disk.sort();
    let mut listed: Vec<String> = ASM_OVERRIDES.iter().map(|s| s.to_string()).collect();
    listed.sort();
    assert_eq!(
        on_disk, listed,
        "nros-board-threadx-port-riscv64: port/src/*.S and ASM_OVERRIDES disagree. \
         A file present here but unlisted is never compiled AND never excluded from \
         the upstream glob, so upstream's copy wins silently."
    );
}

/// Emit `cargo:rerun-if-changed` for every file this crate owns.
pub fn emit_rerun_directives() {
    println!(
        "cargo:rerun-if-changed={}",
        inc_dir().join("tx_port.h").display()
    );
    for name in ASM_OVERRIDES {
        println!("cargo:rerun-if-changed={}", src_dir().join(name).display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_asm_matches_the_override_list() {
        assert_overrides_present();
    }

    #[test]
    fn the_forked_tx_port_header_is_present_and_types_ulong_as_four_bytes() {
        let h = std::fs::read_to_string(inc_dir().join("tx_port.h")).expect("tx_port.h");
        // The whole reason this unit exists. If someone re-syncs the header
        // from upstream, this fails rather than the network.
        let types_ulong_as_u32 = h.lines().any(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            f.as_slice() == ["typedef", "unsigned", "int", "ULONG;"]
        });
        assert!(
            types_ulong_as_u32,
            "tx_port.h no longer types ULONG as `unsigned int` — see the crate docs"
        );
    }
}
