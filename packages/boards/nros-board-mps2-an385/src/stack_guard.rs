//! The main stack's guard (phase-553 W7, issue 2494).
//!
//! `memory.x` gives the main stack a stated size, `_stack_size`, at the top of
//! RAM, and reserves a 1 KiB guard below it (`_nros_stack_guard`). This module
//! arms the guard: one MPU region over it with no access. A stack that grows
//! past its size writes into the guard and faults, instead of writing over the
//! statics or the C heap below with no diagnostic.
//!
//! The Cortex-M3 has no stack-limit register (`MSPLIM` is ARMv8-M), and a
//! flip-link layout, with the stack below the statics so an overflow leaves
//! RAM, is no guard on this machine: QEMU's mps2-an385 accepts a write below
//! the SRAM base without a fault (measured, QEMU 6.2 and 11.0). The MPU does
//! fault, on both.
//!
//! The MemManage exception stays disabled, so the guard's fault escalates to
//! HardFault. With `MPU_CTRL.HFNMIENA` clear the MPU is off while HardFault
//! runs, so the exception frame lands in the guard (which holds nothing) and
//! the handler can run on it to report the overflow.

use cortex_m_rt::{ExceptionFrame, exception};
use cortex_m_semihosting::{debug, hio};

unsafe extern "C" {
    /// The guard's base: the lowest address of the reserved 1 KiB.
    static _nros_stack_guard: u8;
    /// The guard's size in bytes, as an absolute symbol (its address).
    static _nros_stack_guard_size: u8;
    /// The stack's lower bound: the guard's top.
    static _stack_end: u8;
}

const MPU_TYPE: *const u32 = 0xE000_ED90 as *const u32;
const MPU_CTRL: *mut u32 = 0xE000_ED94 as *mut u32;
const MPU_RBAR: *mut u32 = 0xE000_ED9C as *mut u32;
const MPU_RASR: *mut u32 = 0xE000_EDA0 as *mut u32;

/// `MPU_CTRL`: ENABLE, and PRIVDEFENA so every address outside a region keeps
/// the default memory map. HFNMIENA stays clear (see the module doc).
const CTRL_ENABLE_PRIVDEFENA: u32 = 0b101;
/// `MPU_RASR`: execute-never, AP = 0b000 (no access), enabled.
const RASR_XN_NO_ACCESS_ENABLED: u32 = (1 << 28) | 1;
/// `MPU_RBAR`: the address is valid, and selects region 0.
const RBAR_VALID_REGION0: u32 = 1 << 4;

fn guard_base() -> u32 {
    core::ptr::addr_of!(_nros_stack_guard) as u32
}

fn guard_size() -> u32 {
    core::ptr::addr_of!(_nros_stack_guard_size) as u32
}

fn stack_end() -> u32 {
    core::ptr::addr_of!(_stack_end) as u32
}

/// Arm the stack guard. Call once, early in boot, before the stack is deep.
/// On a core with no MPU it says so and leaves the stack unguarded.
pub fn install() {
    // SAFETY: MPU_TYPE is a read-only System Control Space register.
    let dregion = (unsafe { MPU_TYPE.read_volatile() } >> 8) & 0xff;
    if dregion == 0 {
        crate::println!("nros: this core has no MPU, so the main stack is not guarded");
        return;
    }
    let base = guard_base();
    let size = guard_size();
    // `memory.x` asserts the alignment; a power of two is what the SIZE
    // field can encode.
    debug_assert!(size.is_power_of_two() && size >= 32 && base.is_multiple_of(size));
    let size_field = size.trailing_zeros() - 1;
    // SAFETY: programming MPU region 0 over the guard `memory.x` reserved
    // below the stack; nothing else lives there, and nothing else in this
    // image programs the MPU. The barriers make the region take effect before
    // the next stack access.
    unsafe {
        MPU_RBAR.write_volatile(base | RBAR_VALID_REGION0);
        MPU_RASR.write_volatile(RASR_XN_NO_ACCESS_ENABLED | (size_field << 1));
        MPU_CTRL.write_volatile(CTRL_ENABLE_PRIVDEFENA);
    }
    cortex_m::asm::dsb();
    cortex_m::asm::isb();
}

/// Write a fixed message on the semihosting console. No formatting: when the
/// stack has overflowed, the handler runs in the 1 KiB guard.
fn say(msg: &[u8]) {
    if let Ok(mut out) = hio::hstdout() {
        let _ = out.write_all(msg);
    }
}

/// The board's HardFault handler. A fault taken with the stack pointer below
/// the stack's bound is the guard catching an overflow; anything else is an
/// ordinary fault. Either way the image stops with a failure status, which
/// QEMU returns as its exit code.
#[exception]
unsafe fn HardFault(frame: &ExceptionFrame) -> ! {
    let sp = core::ptr::from_ref(frame) as u32;
    if sp < stack_end() {
        say(
            b"nros: stack overflow: the main stack grew past its stated size and \
              hit the guard below it. Raise NROS_BAREMETAL_STACK_KB.\n",
        );
    } else {
        crate::println!(
            "nros: hard fault at pc={:#010x} lr={:#010x}",
            frame.pc(),
            frame.lr()
        );
    }
    debug::exit(debug::EXIT_FAILURE);
    #[allow(clippy::empty_loop)]
    loop {}
}
