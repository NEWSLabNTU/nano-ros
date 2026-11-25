//! Build script for nros-board-mps2-an385
//!
//! Copies the mps2-an385.x linker script to the output directory as memory.x
//! so that cortex-m-rt can find it during linking.
//!
//! phase-471 W4 — one helper, eleven scripts; see
//! `nros_build_paths::link_script`.

use std::path::PathBuf;

/// The main stack an image gets when nothing states one (phase-553 W7). The
/// scaffolded C and C++ talkers measure an 11 KiB high-water mark and the
/// FreeRTOS Rust talker's walked floor is about 26 KiB, so this leaves wide
/// room while giving the stack a bound and a guard. Before W7 the stack had
/// no size at all: it ran from the top of RAM down to the statics.
const DEFAULT_STACK_BYTES: u32 = 256 * 1024;

/// The knob that sizes the main stack, in bytes: `memory.app_stack_bytes`,
/// the application stack on every platform that sizes one. The whole
/// application runs on this board's main stack. Issue 2918 retired
/// `NROS_BAREMETAL_STACK_KB` for it; `knob_env` refuses the old name, naming
/// this one.
const STACK_KNOB: &str = "NROS_MEMORY_APP_STACK_BYTES";

/// The board's RAM, which the stack must fit below.
const RAM_BYTES: u32 = 4 * 1024 * 1024;

fn stack_bytes() -> u32 {
    nros_board_common::platform_config::watch_knob_env(STACK_KNOB);
    let raw = nros_board_common::platform_config::knob_env(&|k| std::env::var(k).ok(), STACK_KNOB);
    match raw.as_deref().map(str::trim) {
        None | Some("") => DEFAULT_STACK_BYTES,
        Some(v) => match v.parse::<u32>() {
            // A whole number of KiB, so the 1 KiB MPU guard below it is
            // aligned (`mps2-an385.x`).
            Ok(b) if b > 0 && b < RAM_BYTES && b % 1024 == 0 => b,
            _ => panic!(
                "{STACK_KNOB}='{v}': the main stack size is a whole number of KiB in bytes \
                 (a multiple of 1024), at least 1024 and below the board's {RAM_BYTES} bytes of RAM"
            ),
        },
    }
}

fn main() {
    // phase-484 W4b-2 (RFC-0103 D10) — the network defaults: this crate's
    // `[board.net]` (`network = "qemu"`) over the platform default, written to
    // `$OUT_DIR/nros_board_net.rs` for `src/config.rs`.
    nros_board_common::board_net::emit_for_crate(
        nros_board_common::board_net::ResolvedNet::platform_default(),
    );
    nros_build_paths::link_script!("mps2-an385.x" => "memory.x");
    // A C-rooted image (CMake owns the link) links with `-Tc-runtime.x`,
    // which includes link.x and keeps and bounds the init arrays (phase-553
    // W2).
    nros_build_paths::link_script!("c-runtime.x");

    // phase-553 W7 (issue 2494) — the main stack's stated size. `memory.x`
    // INCLUDEs this file, which sits beside it in OUT_DIR (the directory
    // `link_script!` put on the link search path). A C-rooted image gets it
    // too: the cmake road copies every top-level `*.x` of this OUT_DIR into
    // its link directory.
    let out_dir =
        PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set for a build script"));
    let bytes = stack_bytes();
    // phase-553 W7.
    let script = format!(
        "/* Written by nros-board-mps2-an385's build script. */\n\
         _stack_size = {bytes};\n"
    );
    let path = out_dir.join("nros-stack.x");
    std::fs::write(&path, script).unwrap_or_else(|e| panic!("writing {}: {e}", path.display()));

    // The stack the image is built with, for `nros build`'s floor check
    // (issue 2400). The whole application runs on the main stack, from
    // cortex-m-rt's `main`.
    nros_board_common::stack_stated::emit(
        &out_dir,
        &[nros_board_common::stack_stated::Stated {
            role: "OPEN",
            root: "main",
            bytes,
            // A template `nros-stack-floor.py` fills: the floor in bytes, rounded
            // up to a whole KiB, as the linker guard needs.
            knob: "NROS_MEMORY_APP_STACK_BYTES={bytes_kib}",
        }],
    );
}
