fn main() {
    nros_board_common::threadx_qemu_riscv64_build::run(include_bytes!("config/link.lds"));
}
