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
//! # Usage
//!
//! ```ignore
//! use cmsdk_uart::CmsdkUart;
//!
//! let mut uart = CmsdkUart::new(0x4000_4000);
//! uart.enable_tx();
//! uart.enable_rx();
//! uart.write(b"Hello\n");
//! ```

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
const BAUDDIV: usize = 0x10;

// STATE register bits
const STATE_TX_FULL: u32 = 1 << 0;
const STATE_RX_FULL: u32 = 1 << 1;

// CTRL register bits
const CTRL_TX_EN: u32 = 1 << 0;
const CTRL_RX_EN: u32 = 1 << 1;

/// CMSDK APB UART driver.
///
/// Provides polled (non-interrupt) TX and RX for the CMSDK UART peripheral.
/// The driver busy-waits on TX (acceptable for serial baud rates) and
/// returns immediately for RX if no data is available.
pub struct CmsdkUart {
    base: usize,
}

impl CmsdkUart {
    /// Create a new UART driver for the given base address.
    ///
    /// Does **not** enable TX/RX — call [`enable_tx`](Self::enable_tx) and
    /// [`enable_rx`](Self::enable_rx) (or [`enable`](Self::enable)) after
    /// construction.
    pub const fn new(base: usize) -> Self {
        Self { base }
    }

    /// Set the baud rate divisor.
    ///
    /// `divisor = sysclk / baudrate`. For QEMU the divisor value doesn't
    /// matter (emulated UART is infinitely fast), but real hardware needs
    /// the correct value.
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

impl SerialPort for CmsdkUart {
    fn write(&mut self, data: &[u8]) -> usize {
        for &byte in data {
            // Busy-wait until TX FIFO has space
            while self.tx_full() {
                core::hint::spin_loop();
            }
            self.write_reg(DATA, byte as u32);
        }
        data.len()
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
