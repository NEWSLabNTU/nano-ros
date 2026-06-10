// 194.3c — riscv NuttX FFI build. Calls the arch-generic run_nuttx(); all
// riscv specifics (target triple, linker, NUTTX_LINKER_SCRIPT,
// NUTTX_ARCH_INCLUDES, NUTTX_CROSS, NUTTX_VECTORTAB_OBJ="", libgcc flags) come
// from this crate's .cargo/config.toml [env].
fn main() {
    nros_board_common::nuttx_ffi_build::run_nuttx();
}
