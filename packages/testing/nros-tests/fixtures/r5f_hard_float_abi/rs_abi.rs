// phase-575 W1 -- the Rust half, built for `armv7r-none-eabihf`: a float in
// s0/s1 and back in s0 under AAPCS-VFP.
#![no_std]

#[unsafe(no_mangle)]
pub extern "C" fn rs_scale(x: f32, k: f32) -> f32 {
    x * k
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
