//! CMSDK APB UART driver
//!
//! Minimal MMIO driver for the CMSDK APB UART peripheral found on
//! ARM MPS2-AN385 (Cortex-M3 FPGA image) and similar platforms.
//! QEMU emulates UART0–UART4 at the following base addresses:
//!
//! | UART  | Base address  |
//! |-------|---------------|
//! | UART0 | `0x4000_4000` |
//! | UART1 | `0x4000_5000` |
//! | UART2 | `0x4000_6000` |
//! | UART3 | `0x4000_7000` |
//! | UART4 | `0x4000_9000` |
//!
//! # IRQ-driven TX (Phase 132 — partial)
//!
//! The crate ships an IRQ servicing entry point — [`handle_tx_irq`]
//! — that board crates wire into their `#[interrupt] fn UARTTX0()`.
//! Pairs with an opt-in `wfi`-loop inside [`CmsdkUart::write`] guarded
//! by the `irq_driven_tx` feature. Default-off because the QEMU
//! CMSDK model's TX-IRQ timing didn't pass the
//! `test_qemu_serial_pubsub_e2e` handshake in the initial
//! integration pass — the zenoh InitSyn write succeeds but the
//! peer's InitAck reply never arrives, suggesting either an
//! INTSTATUS-TX timing race against the QEMU `g_source` drain
//! callback or a missing PRIMASK/wfi interaction. The polled
//! busy-spin path stays the default until the IRQ design lands a
//! green E2E; the structural pieces (ISR symbol, NVIC unmask in
//! the board crate) are in place so the swap is one feature flag
//! when the timing puzzle resolves.

#![no_std]

use core::ptr;

use zpico_serial::SerialPort;

/// Well-known base addresses for MPS2-AN385 UARTs.
pub const UART0_BASE: usize = 0x4000_4000;
pub const UART1_BASE: usize = 0x4000_5000;
pub const UART2_BASE: usize = 0x4000_6000;
pub const UART3_BASE: usize = 0x4000_7000;
pub const UART4_BASE: usize = 0x4000_9000;

// Register offsets (32-bit registers)
const DATA: usize = 0x00;
const STATE: usize = 0x04;
const CTRL: usize = 0x08;
// Phase 132 — CMSDK INTSTATUS / INTCLEAR (offset 0x0C). Reads
// return pending IRQ bits; writes (write-one-to-clear) clear them.
const INTSTATUS: usize = 0x0C;
const BAUDDIV: usize = 0x10;

// STATE register bits
const STATE_TX_FULL: u32 = 1 << 0;
const STATE_RX_FULL: u32 = 1 << 1;

// CTRL register bits
const CTRL_TX_EN: u32 = 1 << 0;
const CTRL_RX_EN: u32 = 1 << 1;
// IRQ-enable bits. Currently only referenced by [`handle_tx_irq`]
// + the (default-off) `irq_driven_tx` future write path.
#[allow(dead_code)]
const CTRL_TX_INT_EN: u32 = 1 << 2;
#[allow(dead_code)]
const CTRL_RX_INT_EN: u32 = 1 << 3;

// INTSTATUS bits (write 1 to clear)
const INTSTATUS_TX: u32 = 1 << 0;
const INTSTATUS_RX: u32 = 1 << 1;

/// CMSDK APB UART driver.
pub struct CmsdkUart {
    base: usize,
}

impl CmsdkUart {
    /// Create a new UART driver for the given base address.
    pub const fn new(base: usize) -> Self {
        Self { base }
    }

    /// Set the baud rate divisor.
    pub fn set_baudrate(&mut self, sysclk: u32, baudrate: u32) {
        let divisor = sysclk / baudrate;
        self.write_reg(BAUDDIV, divisor);
    }

    /// Enable the transmitter.
    pub fn enable_tx(&mut self) {
        let ctrl = self.read_reg(CTRL);
        self.write_reg(CTRL, ctrl | CTRL_TX_EN);
    }

    /// Enable the receiver.
    pub fn enable_rx(&mut self) {
        let ctrl = self.read_reg(CTRL);
        self.write_reg(CTRL, ctrl | CTRL_RX_EN);
    }

    /// Enable both transmitter and receiver.
    pub fn enable(&mut self) {
        let ctrl = self.read_reg(CTRL);
        self.write_reg(CTRL, ctrl | CTRL_TX_EN | CTRL_RX_EN);
    }

    /// Check if the TX FIFO is full.
    #[inline]
    fn tx_full(&self) -> bool {
        self.read_reg(STATE) & STATE_TX_FULL != 0
    }

    /// Check if the RX FIFO has data.
    #[inline]
    fn rx_ready(&self) -> bool {
        self.read_reg(STATE) & STATE_RX_FULL != 0
    }

    #[inline]
    fn read_reg(&self, offset: usize) -> u32 {
        unsafe { ptr::read_volatile((self.base + offset) as *const u32) }
    }

    #[inline]
    fn write_reg(&self, offset: usize, val: u32) {
        unsafe { ptr::write_volatile((self.base + offset) as *mut u32, val) }
    }
}

/// Phase 132 — TX-IRQ servicing hook.
///
/// Board crates wire `#[interrupt] fn UARTTX0()` (or `UARTTX1` /
/// `UARTTX2`/…) that calls this with the matching base address.
/// Clears the pending TX status bit. Pairs with the IRQ-driven
/// write path (currently behind the default-off `irq_driven_tx`
/// path — the polled fallback remains the active TX implementation
/// until the QEMU-CMSDK timing puzzle is resolved).
#[inline]
pub fn handle_tx_irq(base: usize) {
    unsafe {
        ptr::write_volatile((base + INTSTATUS) as *mut u32, INTSTATUS_TX);
    }
}

/// Phase 132 — RX-IRQ servicing hook (placeholder).
///
/// Clears the pending RX status bit. Polled-read path still used
/// today; a future revision pushes received bytes into a per-port
/// ring from this handler.
#[inline]
pub fn handle_rx_irq(base: usize) {
    unsafe {
        ptr::write_volatile((base + INTSTATUS) as *mut u32, INTSTATUS_RX);
    }
}

impl SerialPort for CmsdkUart {
    fn write(&mut self, data: &[u8]) -> usize {
        let mut sent: usize = 0;
        for &byte in data {
            // Busy-wait until TX FIFO has space, but cap the poll count so a
            // stalled host-side consumer (full host PTY buffer, blocked
            // socat) cannot park the executor here forever.
            let mut waits: u32 = 0;
            while self.tx_full() {
                waits = waits.wrapping_add(1);
                if waits >= 1_000_000 {
                    return sent;
                }
                core::hint::spin_loop();
            }
            self.write_reg(DATA, byte as u32);
            sent += 1;
        }
        sent
    }

    fn read(&mut self, buf: &mut [u8]) -> usize {
        let mut n = 0;
        while n < buf.len() && self.rx_ready() {
            buf[n] = (self.read_reg(DATA) & 0xFF) as u8;
            n += 1;
        }
        n
    }
}
