// 194.3c.2 — the NuttX C platform-port compile (platform.c/net.c) is a
// shared, parameterized helper in `nros-board-common`. This board sets no
// `NUTTX_*` env, so the helper's arm defaults (compiler `arm-none-eabi-gcc`,
// `-mcpu=cortex-a7 -mfloat-abi=hard -mfpu=neon-vfpv4`, `arch/arm/src/*`
// includes) reproduce the previous hand-written build byte-for-byte. A
// new-arch NuttX board reuses the same helper with its own env.
//
// #127 — board-centric image link (RFC-0032 "third leg"). The shared
// `nuttx_image_link` helper stages the dynamic link pieces (processed
// `dramboot.ld`, the vectortab + builtins-stub boot archive, the `-L`
// search dirs) here in the BOARD build script, exploiting that
// `cargo:rustc-link-search` / `cargo:rustc-link-lib` PROPAGATE from a
// dependency's build script to the final `[[bin]]` link (unlike
// `cargo:rustc-link-arg`). The static link args (`-Tdramboot.ld`,
// `--entry=__start`, the kernel-lib `--start-group` list, `-lgcc`) live in
// each Entry pkg's `.cargo/config.toml` rustflags, rendered from this
// board's `nros-board.toml` `cargo_config` — so a dependent Entry links a
// bootable NuttX image with ZERO build.rs of its own.

use std::path::Path;

fn main() {
    nros_board_common::nuttx_platform_build::run_platform();
    // Empty-builtins stub lives in this board crate's `c/` (see its header
    // for why libapps' `builtin_list.o` must be preempted).
    let stub = Path::new(env!("CARGO_MANIFEST_DIR")).join("c/nuttx_builtins_stub.c");
    nros_board_common::nuttx_image_link::run_image_link(&stub);
}
