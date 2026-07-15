// 194.3c — NuttX platform-port compile via the shared parameterized helper.
// The riscv NUTTX_* env (NUTTX_CROSS / NUTTX_PLATFORM_CFLAGS /
// NUTTX_ARCH_INCLUDES) is set by the FFI subcrate's .cargo/config.toml [env]
// and reaches this board build.rs because cargo applies the config [env] to the
// whole build graph. Same one-line body as the arm board crate.
//
// Phase-285 W4 — #127 board-centric image link, mirroring the arm sibling:
// stage the dynamic link pieces (preprocessed rv-virt ld.script, the
// builtins-stub boot archive — riscv has NO vectortab head object, so the
// consuming Entry's env sets `NUTTX_VECTORTAB=` empty — and the `-L` search
// dirs) here in the BOARD build script so a dependent Entry pkg links a
// bootable rv-virt NuttX image with ZERO build.rs of its own. Env-gated on
// `NUTTX_DIR`, so a plain host `cargo check` stays link-directive-free.
use std::path::Path;

fn main() {
    nros_board_common::nuttx_platform_build::run_platform();
    let stub = Path::new(env!("CARGO_MANIFEST_DIR")).join("c/nuttx_builtins_stub.c");
    nros_board_common::nuttx_image_link::run_image_link(&stub);
}
