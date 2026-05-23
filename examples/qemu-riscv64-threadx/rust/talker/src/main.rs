//! ThreadX QEMU RISC-V Talker
//!
//! Publishes `std_msgs/Int32` messages on `/chatter`.

#![no_std]
#![no_main]

#[unsafe(no_mangle)]
extern "C" fn main() -> ! {
    qemu_riscv64_threadx_talker::start_from_reset()
}
