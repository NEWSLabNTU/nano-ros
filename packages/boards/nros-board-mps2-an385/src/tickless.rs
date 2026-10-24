//! issue 2022 — the tickless idle for the transport's waits.
//!
//! A bare-metal image's transport waits are poll loops (`zpico_spin_once`,
//! TCP connect, `read_exact`), and without this every one of them busy-polled
//! smoltcp to its deadline. `nros_smoltcp::idle_until_ms` is the seam those
//! loops call once a poll found nothing; this module is the primitive behind
//! it: sleep on `wfi` until the deadline or until a frame arrives.
//!
//! Two wake sources, both needed:
//!
//! * **The LAN9118 RX interrupt** (`ETHERNET`, NVIC 13 in QEMU's mps2-an385),
//!   through `lan9118_smoltcp::irq`. Without it a `wfi` armed only on the
//!   deadline is the blind sleep RFC-0106 D5 forbids, moved down a layer: a
//!   reply landing mid-sleep would wait for the timer.
//! * **A SysTick one-shot** at the deadline. SysTick is the one counter on
//!   this board nobody else owns: TIMER0 is the monotonic clock, TIMER1 the
//!   sporadic-server timer, the dual timer the RTIC tick (issue 1804). It is
//!   24 bits at 25 MHz, so one shot reaches ~671 ms; a longer deadline wakes
//!   early, and the loop polls and idles again.
//!
//! Not built for an RTIC image (`feature = "rtic"`): RTIC owns the vector
//! table, and its 1 ms dual-timer tick already wakes the `wfi` that
//! `enable_wfi_idle` installs on `do_poll`.

use cortex_m::peripheral::NVIC;
use cortex_m_rt::exception;
use lan9118_smoltcp::{MPS2_AN385_BASE, irq};
use mps2_an385_pac::{Interrupt, interrupt};

// SysTick, by address — the PAC exposes interrupt names only, and the dual
// timer beside it in `rtic.rs` is driven the same way.
const SYST_CSR: usize = 0xE000_E010;
const SYST_RVR: usize = 0xE000_E014;
const SYST_CVR: usize = 0xE000_E018;
const SYST_CSR_ENABLE: u32 = 1 << 0;
const SYST_CSR_TICKINT: u32 = 1 << 1;
/// Count the processor clock (25 MHz), not the external reference.
const SYST_CSR_CLKSOURCE: u32 = 1 << 2;
/// The counter is 24 bits.
const SYST_MAX_RELOAD: u64 = 0x00FF_FFFF;
/// `ICSR.PENDSTCLR` — drop a SysTick that fired while we were already awake.
const SCB_ICSR: usize = 0xE000_ED04;
const ICSR_PENDSTCLR: u32 = 1 << 25;

/// SysTick ticks per nanosecond is 25 MHz / 1 GHz = 1/40.
const NS_PER_SYST_TICK: u64 = 40;

#[inline]
fn write(addr: usize, value: u32) {
    // SAFETY: a System Control Space register, always mapped on Cortex-M3.
    unsafe { core::ptr::write_volatile(addr as *mut u32, value) }
}

fn arm_oneshot(ticks: u64) {
    write(SYST_CSR, 0);
    write(SYST_RVR, ticks.clamp(1, SYST_MAX_RELOAD) as u32);
    write(SYST_CVR, 0);
    write(
        SYST_CSR,
        SYST_CSR_ENABLE | SYST_CSR_TICKINT | SYST_CSR_CLKSOURCE,
    );
}

fn disarm_oneshot() {
    write(SYST_CSR, 0);
    write(SCB_ICSR, ICSR_PENDSTCLR);
}

/// The RX handler masks the source and returns. RSFL is a level, so it stays
/// masked until the next idle, which unmasks it only after checking the FIFO
/// is empty. The handler's only other job, waking the `wfi`, happened when the
/// line rose.
#[interrupt]
fn ETHERNET() {
    // SAFETY: the LAN9118 the board initialised, at its fixed address.
    unsafe { irq::mask_rx(MPS2_AN385_BASE) };
}

/// The one-shot's handler: stop the counter so a reload cannot fire again.
#[exception]
fn SysTick() {
    disarm_oneshot();
}

/// The [`nros_smoltcp::WaitIdleFn`] this board installs.
///
/// The FIFO check, the unmask, the timer and the `wfi` all run with interrupts
/// masked. `wfi` still wakes on an interrupt that becomes pending under
/// PRIMASK, and the handler then runs when the mask lifts, so a frame that
/// lands between the check and the sleep ends the sleep instead of being a
/// lost wake.
unsafe extern "C" fn wait_idle(deadline_ns: u64) {
    cortex_m::interrupt::free(|_| {
        // SAFETY: the board's LAN9118, initialised before this was installed.
        if unsafe { irq::rx_frames_pending(MPS2_AN385_BASE) } || crate::node::eth_rx_buffered() {
            return;
        }
        let now_ns = nros_platform_mps2_an385::clock::clock_ns();
        if deadline_ns <= now_ns {
            return;
        }
        // SAFETY: as above.
        unsafe { irq::unmask_rx(MPS2_AN385_BASE) };
        arm_oneshot((deadline_ns - now_ns).div_ceil(NS_PER_SYST_TICK));
        cortex_m::asm::wfi();
        disarm_oneshot();
    });
}

/// Arm the RX interrupt line and hand smoltcp's wait loops the primitive.
/// Called once the network is up.
pub(crate) fn install() {
    // SAFETY: unmasking a line whose handler is defined above. The source is
    // still masked at the NIC (`INT_EN` is cleared by `Lan9118::init`), so
    // nothing fires until the first idle unmasks it.
    unsafe { NVIC::unmask(Interrupt::ETHERNET) };
    nros_smoltcp::set_wait_idle(wait_idle);
}
