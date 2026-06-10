// 194.3c.2 — the NuttX C platform-port compile (platform.c/net.c) is now a
// shared, parameterized helper in `nros-board-common`. This board sets no
// `NUTTX_*` env, so the helper's arm defaults (compiler `arm-none-eabi-gcc`,
// `-mcpu=cortex-a7 -mfloat-abi=hard -mfpu=neon-vfpv4`, `arch/arm/src/*`
// includes) reproduce the previous hand-written build byte-for-byte. A
// new-arch NuttX board reuses the same helper with its own env.
fn main() {
    nros_board_common::nuttx_platform_build::run_platform();
}
