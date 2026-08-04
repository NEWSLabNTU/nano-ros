// phase-337 W4.b — the board names its arch port explicitly.
//
// `nros-board-threadx-port-riscv64` is layer 2 (RFC-0064): upstream's
// `ports/risc-v64/gnu` forked to type `ULONG` as 4 bytes. This board is the
// layer-3 overlay on top of it, and the edge is a real cargo dependency so
// `cargo tree` shows it rather than a relative path buried in a build script.
fn main() {
    nros_board_threadx_port_riscv64::assert_overrides_present();
    // Emitted BEFORE the riscv64 guard inside `run`, so a host-tooled build
    // still records the dependency and a later firmware build is not served a
    // cached object compiled against an older `tx_port.h`.
    nros_board_threadx_port_riscv64::emit_rerun_directives();
    nros_board_common::threadx_qemu_riscv64_build::run(
        include_bytes!("config/link.lds"),
        &nros_board_threadx_port_riscv64::port_dir(),
    );
}
