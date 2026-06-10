// 194.3c — NuttX platform-port compile via the shared parameterized helper.
// The riscv NUTTX_* env (NUTTX_CROSS / NUTTX_PLATFORM_CFLAGS /
// NUTTX_ARCH_INCLUDES) is set by the FFI subcrate's .cargo/config.toml [env]
// and reaches this board build.rs because cargo applies the config [env] to the
// whole build graph. Same one-line body as the arm board crate.
fn main() {
    nros_board_common::nuttx_platform_build::run_platform();
}
