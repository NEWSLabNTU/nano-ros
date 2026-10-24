//! Receive interrupt — issue 2022.
//!
//! The driver is polled: [`Lan9118`][crate::Lan9118] reads the RX FIFO only
//! when smoltcp asks. That is all a busy-polling board needs, but a board that
//! wants to SLEEP between polls needs something to wake it when a frame lands,
//! and this is that something. The FreeRTOS lwIP driver's
//! `lan9118_lwip_rx_irq_enable` / `_mask` / `_pending` are the model (issue
//! 0917); the rules below are the same ones, for the same reasons.
//!
//! # RSFL is a LEVEL, so the handler masks it
//!
//! The controller drives its IRQ pin from `INT_STS & INT_EN`. RSFL ("RX status
//! FIFO level") asserts while the RX status FIFO is non-empty and stays
//! asserted until the frames are drained. Clearing it in a handler does
//! nothing, because the line re-asserts the instant the handler returns and
//! the core livelocks. So the handler calls [`mask_rx`], and whoever drains the
//! FIFO calls [`unmask_rx`] again once it has nothing left to read.
//!
//! # IRQ_CFG
//!
//! [`Lan9118::init`][crate::Lan9118::init] programs `IRQ_CFG` with
//! `IRQ_EN | IRQ_POL | IRQ_TYPE` (active-high, push-pull). QEMU's model needs
//! all three to assert the line at all: with either polarity bit clear it
//! treats the output as active-low and holds the line asserted while idle.
//!
//! These are free functions over the register base, not methods, because the
//! handler that calls [`mask_rx`] runs while the polling side may hold the
//! driver mutably. They touch only `INT_EN`, `INT_STS` and `RX_FIFO_INF`,
//! which the polled RX path does not write.

use core::ptr::{read_volatile, write_volatile};

use crate::regs::{int, offset, rx_fifo_inf};

#[inline]
fn read(base: usize, off: usize) -> u32 {
    // SAFETY: `base` is a LAN9118 register block, per every caller's contract.
    unsafe { read_volatile((base + off) as *const u32) }
}

#[inline]
fn write(base: usize, off: usize, value: u32) {
    // SAFETY: as `read`.
    unsafe { write_volatile((base + off) as *mut u32, value) }
}

/// Let the RX status FIFO raise the IRQ line again.
///
/// Clears the latched status first, so a frame that arrived while the source
/// was masked raises the line now rather than being mistaken for an old one.
///
/// # Safety
///
/// `base` must be the register block of an initialised LAN9118.
pub unsafe fn unmask_rx(base: usize) {
    write(base, offset::INT_STS, int::RSFL);
    write(base, offset::INT_EN, read(base, offset::INT_EN) | int::RSFL);
}

/// Silence the RX source. The interrupt handler's whole job.
///
/// # Safety
///
/// As [`unmask_rx`].
pub unsafe fn mask_rx(base: usize) {
    write(
        base,
        offset::INT_EN,
        read(base, offset::INT_EN) & !int::RSFL,
    );
    // Write-1-to-clear the latched status so the next frame re-raises it.
    write(base, offset::INT_STS, int::RSFL);
}

/// Whether the RX FIFO holds a frame nobody has read.
///
/// A board must ask this with interrupts disabled before it sleeps: a frame
/// that arrived after the last poll and before the sleep has already raised
/// (and been masked by) the interrupt, so nothing else would wake the core.
///
/// # Safety
///
/// As [`unmask_rx`].
pub unsafe fn rx_frames_pending(base: usize) -> bool {
    (read(base, offset::RX_FIFO_INF) & rx_fifo_inf::RXSUSED_MASK) != 0
}
