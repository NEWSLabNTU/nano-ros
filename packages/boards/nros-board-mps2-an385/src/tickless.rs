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
//! phase-521 W5 — the serial link gets the same idle. Its wake source is the
//! CMSDK UART0 RX interrupt (`UARTRX0`, NVIC 0), and the primitive is installed
//! into `zpico_serial::set_wait_idle`, which the zenoh serial read and
//! `zpico_spin_once`'s idle call between executor rounds. Like the LAN9118
//! line, the RX handler only masks the source: QEMU's CMSDK model holds one
//! byte and stops taking more from the host until it is read, so nothing is
//! lost while the source is masked.
//!
//! phase-521 W5 (RFC-0113 D5) — the same sleep is the port's wake-object park
//! (`nros_platform_mps2_an385::wake`), so the executor itself parks on `wfi`
//! between spins and an arrival on either link ends that park.
//!
//! Not built for an RTIC image (`feature = "rtic"`): RTIC owns the vector
//! table, and its 1 ms dual-timer tick already wakes the `wfi` that
//! `enable_wfi_idle` installs on `do_poll`.

use cortex_m::peripheral::NVIC;
use cortex_m_rt::exception;
#[cfg(feature = "ethernet")]
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

/// The one tickless sleep both links use: with interrupts masked, return at
/// once if `pending()` (data already waiting) or the deadline has passed;
/// otherwise `unmask()` the link's RX source, arm the one-shot, `wfi`, disarm.
/// `wfi` still wakes on an interrupt that becomes pending under PRIMASK, and
/// its handler runs when the mask lifts, so data that lands between the check
/// and the sleep ends the sleep instead of being a lost wake. Returns
/// `pending()` as of the end: whether the link has data for the caller.
fn sleep_until(deadline_ns: u64, pending: impl Fn() -> bool, unmask: impl Fn()) -> bool {
    cortex_m::interrupt::free(|_| {
        if pending() {
            return true;
        }
        let now_ns = nros_platform_mps2_an385::clock::clock_ns();
        if deadline_ns <= now_ns {
            return false;
        }
        unmask();
        arm_oneshot((deadline_ns - now_ns).div_ceil(NS_PER_SYST_TICK));
        cortex_m::asm::wfi();
        disarm_oneshot();
        pending()
    })
}

#[cfg(feature = "ethernet")]
mod eth {
    use super::*;

    /// The RX handler masks the source and returns. RSFL is a level, so it
    /// stays masked until the next idle, which unmasks it only after checking
    /// the FIFO is empty. The handler's only other job, waking the `wfi`,
    /// happened when the line rose.
    #[interrupt]
    fn ETHERNET() {
        // SAFETY: the LAN9118 the board initialised, at its fixed address.
        unsafe { irq::mask_rx(MPS2_AN385_BASE) };
    }

    fn pending() -> bool {
        // SAFETY: the board's LAN9118, initialised before the idle is installed.
        (unsafe { irq::rx_frames_pending(MPS2_AN385_BASE) }) || crate::node::eth_rx_buffered()
    }

    fn unmask() {
        // SAFETY: as `pending`.
        unsafe { irq::unmask_rx(MPS2_AN385_BASE) };
    }

    /// The [`nros_smoltcp::WaitIdleFn`] this board installs: sleep to the
    /// deadline or the next frame ([`super::sleep_until`]).
    pub(super) unsafe extern "C" fn wait_idle(deadline_ns: u64) {
        super::sleep_until(deadline_ns, pending, unmask);
    }

    /// The port's [`nros_platform_mps2_an385::wake::WakeIdleFn`]: the same
    /// sleep, reporting whether a frame is waiting.
    pub(super) unsafe extern "C" fn wake_idle(deadline_ns: u64) -> bool {
        super::sleep_until(deadline_ns, pending, unmask)
    }
}

#[cfg(feature = "serial")]
mod uart {
    use super::*;

    /// UART0 receive: mask the source and return. The wake is the point; the
    /// byte stays in the UART until the read takes it.
    #[interrupt]
    fn UARTRX0() {
        cmsdk_uart::mask_rx_irq(cmsdk_uart::UART0_BASE);
        // phase-521 W5 — an XRCE session over this UART learns of it too.
        #[cfg(feature = "xrce-transport")]
        crate::xrce_transport::notify_rx();
    }

    fn pending() -> bool {
        cmsdk_uart::rx_ready_at(cmsdk_uart::UART0_BASE) || zpico_serial::rx_buffered()
    }

    fn unmask() {
        cmsdk_uart::unmask_rx_irq(cmsdk_uart::UART0_BASE);
    }

    /// The [`zpico_serial::WaitIdleFn`] this board installs: the same
    /// [`super::sleep_until`] as the network idle, over the UART's RX buffer
    /// and the bytes `zpico-serial` already holds.
    pub(crate) unsafe extern "C" fn wait_idle(max_us: u64) {
        if max_us == 0 {
            return;
        }
        let deadline_ns = nros_platform_mps2_an385::clock::clock_ns()
            .saturating_add(max_us.saturating_mul(1000));
        super::sleep_until(deadline_ns, pending, unmask);
    }

    /// The port's [`nros_platform_mps2_an385::wake::WakeIdleFn`]: the same
    /// sleep, reporting whether a byte is waiting.
    pub(super) unsafe extern "C" fn wake_idle(deadline_ns: u64) -> bool {
        super::sleep_until(deadline_ns, pending, unmask)
    }
}

/// phase-521 W5 — the serial idle, for the board's XRCE custom transport.
#[cfg(feature = "xrce-transport")]
pub(crate) use uart::wait_idle as serial_wait_idle;

/// The one-shot's handler: stop the counter so a reload cannot fire again.
#[exception]
fn SysTick() {
    disarm_oneshot();
}

/// Arm the transport's RX interrupt line and hand its wait loops the
/// primitive. Called once the transport is up.
pub(crate) fn install() {
    // SAFETY: unmasking a line whose handler is defined above. The source is
    // still masked at the device (the LAN9118's `INT_EN` is cleared by
    // `Lan9118::init`, the UART's RX enable is clear until the first idle), so
    // nothing fires until the first idle unmasks it.
    #[cfg(feature = "ethernet")]
    {
        unsafe { NVIC::unmask(Interrupt::ETHERNET) };
        nros_smoltcp::set_wait_idle(eth::wait_idle);
        nros_platform_mps2_an385::wake::set_wake_idle(eth::wake_idle);
    }
    #[cfg(feature = "serial")]
    {
        cmsdk_uart::mask_rx_irq(cmsdk_uart::UART0_BASE);
        unsafe { NVIC::unmask(Interrupt::UARTRX0) };
        zpico_serial::set_wait_idle(uart::wait_idle);
        nros_platform_mps2_an385::wake::set_wake_idle(uart::wake_idle);
    }
    // phase-521 W5 (RFC-0113 D4/D5) — the port's wake object now parks on the
    // link's RX interrupt, so an arrival ends the executor's park: the zenoh
    // session (CALLER mode) states `Notify` and the executor parks instead of
    // draining the link from `drive_io`.
    #[cfg(feature = "rmw-zenoh")]
    nros_rmw_zenoh::set_caller_rx_wakes_park(true);
}
