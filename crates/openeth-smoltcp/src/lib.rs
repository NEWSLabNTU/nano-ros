//! OpenCores Ethernet MAC driver for smoltcp
//!
//! This crate provides a `no_std` compatible driver for the OpenCores Ethernet
//! MAC (open_eth), implementing the `smoltcp::phy::Device` trait for integration
//! with the smoltcp TCP/IP stack.
//!
//! # Supported Hardware
//!
//! - QEMU ESP32-C3 machine (`-nic model=open_eth`)
//!
//! # Usage
//!
//! ```ignore
//! use openeth_smoltcp::{OpenEth, Config};
//!
//! let config = Config {
//!     base_addr: 0x600C_D000,
//!     mac_addr: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
//! };
//!
//! let mut eth = unsafe { OpenEth::new(config) };
//! eth.init();
//!
//! // Use with smoltcp
//! let mut iface = smoltcp::iface::Interface::new(config, &mut eth, instant);
//! ```

#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod regs;

use core::ptr::{read_volatile, write_volatile};
use smoltcp::phy::{self, Device, DeviceCapabilities, Medium};
use smoltcp::time::Instant;

pub use regs::ESP32C3_BASE;

/// Maximum Transmission Unit for Ethernet
pub const MTU: usize = 1500;

/// Maximum Ethernet frame size (MTU + headers)
pub const MAX_FRAME_SIZE: usize = 1536;

/// DMA buffer size per descriptor (must hold a full Ethernet frame)
pub const DMA_BUF_SIZE: usize = 1600;

/// Number of TX descriptors (RX descriptors follow immediately after in the BD table)
const TX_BD_COUNT: usize = 1;

/// Driver error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Hardware not detected or reset failed
    InitFailed,
}

/// Driver configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// Base address of the OpenETH registers
    pub base_addr: usize,
    /// MAC address (6 bytes)
    pub mac_addr: [u8; 6],
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_addr: ESP32C3_BASE,
            mac_addr: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
        }
    }
}

/// OpenCores Ethernet MAC driver
///
/// Manages the OpenETH MAC and provides the smoltcp `Device` trait for
/// network stack integration.
pub struct OpenEth {
    base: usize,
    mac_addr: [u8; 6],
    /// Static DMA buffer for TX descriptor
    tx_buf: [u8; DMA_BUF_SIZE],
    /// Static DMA buffer for RX descriptor
    rx_buf: [u8; DMA_BUF_SIZE],
    /// Internal receive buffer (copied from DMA after poll)
    rx_frame: [u8; MAX_FRAME_SIZE],
    /// Length of received frame in rx_frame (0 = empty)
    rx_frame_len: usize,
}

impl OpenEth {
    /// Create a new driver instance.
    ///
    /// # Safety
    ///
    /// - `config.base_addr` must point to valid OpenETH hardware registers
    /// - Only one driver instance should exist per hardware device
    pub unsafe fn new(config: Config) -> Self {
        Self {
            base: config.base_addr,
            mac_addr: config.mac_addr,
            tx_buf: [0; DMA_BUF_SIZE],
            rx_buf: [0; DMA_BUF_SIZE],
            rx_frame: [0; MAX_FRAME_SIZE],
            rx_frame_len: 0,
        }
    }

    /// Initialize the hardware.
    ///
    /// This performs the full initialization sequence:
    /// 1. Software reset
    /// 2. Set TX buffer descriptor count
    /// 3. Write MAC address
    /// 4. Configure buffer descriptors with DMA buffer pointers
    /// 5. Enable TX and RX
    pub fn init(&mut self) {
        // Step 1: Software reset
        self.write_reg(regs::offset::MODER, regs::moder::RST);

        // Wait for reset to complete (RST bit self-clears)
        for _ in 0..1000 {
            let moder = self.read_reg(regs::offset::MODER);
            if (moder & regs::moder::RST) == 0 {
                break;
            }
            self.delay_us(1);
        }

        // Step 2: Set number of TX descriptors to TX_BD_COUNT
        self.write_reg(regs::offset::TX_BD_NUM, TX_BD_COUNT as u32);

        // Step 3: Write MAC address
        // MAC_ADDR0: bytes [5,4,3,2] (big-endian order)
        let mac = self.mac_addr;
        let addr0 = u32::from(mac[5])
            | (u32::from(mac[4]) << 8)
            | (u32::from(mac[3]) << 16)
            | (u32::from(mac[2]) << 24);
        let addr1 = u32::from(mac[1]) | (u32::from(mac[0]) << 8);
        self.write_reg(regs::offset::MAC_ADDR0, addr0);
        self.write_reg(regs::offset::MAC_ADDR1, addr1);

        // Step 4: Configure TX descriptor
        let tx_bd_addr = self.base + regs::offset::BD_BASE;
        let tx_buf_ptr = self.tx_buf.as_ptr() as u32;
        // word0: not ready yet (RD=0), wrap bit set (last TX descriptor)
        unsafe {
            write_volatile(tx_bd_addr as *mut u32, regs::tx_bd::WR);
            write_volatile((tx_bd_addr + 4) as *mut u32, tx_buf_ptr);
        }

        // Step 5: Configure RX descriptor (starts after TX descriptors)
        let rx_bd_addr = self.base + regs::offset::BD_BASE + (TX_BD_COUNT * 8);
        let rx_buf_ptr = self.rx_buf.as_ptr() as u32;
        // word0: empty (E=1, ready for HW), wrap bit set (last RX descriptor)
        unsafe {
            write_volatile(rx_bd_addr as *mut u32, regs::rx_bd::E | regs::rx_bd::WR);
            write_volatile((rx_bd_addr + 4) as *mut u32, rx_buf_ptr);
        }

        // Step 6: Clear any pending interrupts
        self.write_reg(regs::offset::INT_SOURCE, 0x7F);
        // Disable all interrupts (we poll)
        self.write_reg(regs::offset::INT_MASK, 0);

        // Step 7: Enable TX and RX with full duplex, CRC, and pad
        let moder = regs::moder::TXEN
            | regs::moder::RXEN
            | regs::moder::FULLD
            | regs::moder::CRCEN
            | regs::moder::PAD;
        self.write_reg(regs::offset::MODER, moder);
    }

    /// Get the current MAC address.
    pub fn mac_address(&self) -> [u8; 6] {
        self.mac_addr
    }

    // ========================================================================
    // Register access
    // ========================================================================

    /// Read a memory-mapped register.
    #[inline]
    fn read_reg(&self, offset: usize) -> u32 {
        unsafe { read_volatile((self.base + offset) as *const u32) }
    }

    /// Write a memory-mapped register.
    #[inline]
    fn write_reg(&self, offset: usize, value: u32) {
        unsafe { write_volatile((self.base + offset) as *mut u32, value) }
    }

    // ========================================================================
    // TX/RX operations
    // ========================================================================

    /// Poll for a received frame.
    /// Returns true if a frame was received and is available in rx_frame.
    fn poll_rx(&mut self) -> bool {
        // Already have a frame buffered
        if self.rx_frame_len > 0 {
            return true;
        }

        // Read RX descriptor word0
        let rx_bd_addr = self.base + regs::offset::BD_BASE + (TX_BD_COUNT * 8);
        let word0 = unsafe { read_volatile(rx_bd_addr as *const u32) };

        // Check if frame has been received (E bit cleared by hardware)
        if (word0 & regs::rx_bd::E) != 0 {
            return false;
        }

        // Extract length from descriptor
        let len = ((word0 & regs::rx_bd::LEN_MASK) >> regs::rx_bd::LEN_SHIFT) as usize;

        // Validate length (must include at least Ethernet header, may include CRC)
        if len < 14 || len > DMA_BUF_SIZE {
            // Re-arm the descriptor and skip this frame
            unsafe {
                write_volatile(rx_bd_addr as *mut u32, regs::rx_bd::E | regs::rx_bd::WR);
            }
            return false;
        }

        // Copy frame from DMA buffer to internal buffer (strip 4-byte CRC if present)
        let frame_len = if len > 4 { len - 4 } else { len };
        let frame_len = frame_len.min(MAX_FRAME_SIZE);
        self.rx_frame[..frame_len].copy_from_slice(&self.rx_buf[..frame_len]);
        self.rx_frame_len = frame_len;

        // Re-arm the RX descriptor for next frame
        unsafe {
            write_volatile(rx_bd_addr as *mut u32, regs::rx_bd::E | regs::rx_bd::WR);
        }

        true
    }

    /// Transmit a frame from the internal TX buffer.
    fn tx_send(&mut self, len: usize) {
        if len > MAX_FRAME_SIZE || len == 0 {
            return;
        }

        let tx_bd_addr = self.base + regs::offset::BD_BASE;

        // Set length and flags: RD=1 (ready to send), WR=1 (wrap), PAD, CRC
        let word0 = ((len as u32) << regs::tx_bd::LEN_SHIFT)
            | regs::tx_bd::RD
            | regs::tx_bd::WR
            | regs::tx_bd::PAD
            | regs::tx_bd::CRC;

        unsafe {
            write_volatile(tx_bd_addr as *mut u32, word0);
        }

        // In QEMU, transmission happens instantly - no need to wait for RD to clear
    }

    /// Check if TX is ready (previous transmission complete).
    fn tx_ready(&self) -> bool {
        let tx_bd_addr = self.base + regs::offset::BD_BASE;
        let word0 = unsafe { read_volatile(tx_bd_addr as *const u32) };
        // TX is ready if RD bit is cleared (hardware clears it after transmit)
        (word0 & regs::tx_bd::RD) == 0
    }

    // ========================================================================
    // Timing
    // ========================================================================

    /// Simple busy-wait delay.
    #[inline]
    fn delay_us(&self, us: u32) {
        for _ in 0..(us * 25) {
            core::hint::spin_loop();
        }
    }
}

// ============================================================================
// smoltcp Device trait implementation
// ============================================================================

/// RX token for smoltcp
pub struct OpenEthRxToken<'a> {
    buffer: &'a [u8],
}

impl phy::RxToken for OpenEthRxToken<'_> {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(self.buffer)
    }
}

/// TX token for smoltcp
pub struct OpenEthTxToken<'a> {
    driver: &'a mut OpenEth,
}

impl phy::TxToken for OpenEthTxToken<'_> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let len = len.min(MAX_FRAME_SIZE);
        let result = f(&mut self.driver.tx_buf[..len]);
        self.driver.tx_send(len);
        result
    }
}

impl Device for OpenEth {
    type RxToken<'a>
        = OpenEthRxToken<'a>
    where
        Self: 'a;
    type TxToken<'a>
        = OpenEthTxToken<'a>
    where
        Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        // Poll for received frame
        if !self.poll_rx() {
            return None;
        }

        // Check TX is also ready (smoltcp expects both)
        if !self.tx_ready() {
            return None;
        }

        let len = self.rx_frame_len;
        self.rx_frame_len = 0; // Mark buffer as consumed

        // Create tokens. Similar to LAN9118 - use raw pointer to avoid aliasing issue.
        let rx_ptr = self.rx_frame[..len].as_ptr();
        let rx_token = OpenEthRxToken {
            buffer: unsafe { core::slice::from_raw_parts(rx_ptr, len) },
        };
        let tx_token = OpenEthTxToken { driver: self };

        Some((rx_token, tx_token))
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        if !self.tx_ready() {
            return None;
        }

        Some(OpenEthTxToken { driver: self })
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ethernet;
        caps.max_transmission_unit = MTU;
        caps.max_burst_size = Some(1);
        caps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =====================================================================
    // Constants
    // =====================================================================

    #[test]
    fn test_mtu_is_standard_ethernet() {
        assert_eq!(MTU, 1500);
    }

    #[test]
    fn test_max_frame_size_covers_mtu_plus_headers() {
        // Ethernet header = 14 bytes (6 dst + 6 src + 2 ethertype)
        // MTU = 1500
        // Total = 1514, rounded to 1536
        assert!(MAX_FRAME_SIZE >= MTU + 14);
        assert_eq!(MAX_FRAME_SIZE, 1536);
    }

    #[test]
    fn test_dma_buf_size_covers_max_frame() {
        // DMA buffer must hold a full frame + any padding/CRC
        assert!(DMA_BUF_SIZE >= MAX_FRAME_SIZE);
        assert_eq!(DMA_BUF_SIZE, 1600);
    }

    #[test]
    fn test_esp32c3_base_reexported() {
        assert_eq!(ESP32C3_BASE, regs::ESP32C3_BASE);
    }

    // =====================================================================
    // Config
    // =====================================================================

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.base_addr, 0x600C_D000);
        // Default MAC is locally-administered unicast
        assert_eq!(config.mac_addr, [0x02, 0x00, 0x00, 0x00, 0x00, 0x01]);
        // Bit 1 of first byte = locally administered
        assert_ne!(config.mac_addr[0] & 0x02, 0);
        // Bit 0 of first byte = 0 means unicast
        assert_eq!(config.mac_addr[0] & 0x01, 0);
    }

    #[test]
    fn test_config_custom() {
        let config = Config {
            base_addr: 0x1234_5000,
            mac_addr: [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF],
        };
        assert_eq!(config.base_addr, 0x1234_5000);
        assert_eq!(config.mac_addr[0], 0xAA);
    }

    #[test]
    fn test_config_clone() {
        let config = Config::default();
        let cloned = config.clone();
        assert_eq!(config.base_addr, cloned.base_addr);
        assert_eq!(config.mac_addr, cloned.mac_addr);
    }

    // =====================================================================
    // OpenEth struct memory layout
    // =====================================================================

    #[test]
    fn test_openeth_struct_size() {
        // Verify the struct fits in reasonable stack space
        // base(8) + mac(6) + tx_buf(1600) + rx_buf(1600) + rx_frame(1536) + rx_frame_len(8)
        let size = core::mem::size_of::<OpenEth>();
        // Should be under 8KB (reasonable for embedded stack)
        assert!(
            size < 8192,
            "OpenEth is {} bytes, expected < 8192",
            size
        );
        // Should be at least the sum of buffer sizes
        assert!(
            size >= DMA_BUF_SIZE * 2 + MAX_FRAME_SIZE,
            "OpenEth is {} bytes, expected >= {}",
            size,
            DMA_BUF_SIZE * 2 + MAX_FRAME_SIZE
        );
    }

    // =====================================================================
    // Device capabilities
    // =====================================================================

    #[test]
    fn test_capabilities() {
        let config = Config {
            base_addr: 0, // won't access hardware in capabilities()
            mac_addr: [0; 6],
        };
        // Safety: we won't call init() or any hardware-accessing methods
        let eth = unsafe { OpenEth::new(config) };
        let caps = eth.capabilities();

        assert_eq!(caps.medium, Medium::Ethernet);
        assert_eq!(caps.max_transmission_unit, 1500);
        assert_eq!(caps.max_burst_size, Some(1));
    }

    // =====================================================================
    // MAC address
    // =====================================================================

    #[test]
    fn test_mac_address_preserved() {
        let mac = [0x02, 0x00, 0x00, 0xDE, 0xAD, 0x42];
        let config = Config {
            base_addr: 0,
            mac_addr: mac,
        };
        let eth = unsafe { OpenEth::new(config) };
        assert_eq!(eth.mac_address(), mac);
    }

    // =====================================================================
    // Error type
    // =====================================================================

    #[test]
    fn test_error_equality() {
        assert_eq!(Error::InitFailed, Error::InitFailed);
    }

    #[test]
    fn test_error_copy() {
        let err = Error::InitFailed;
        let copied = err;
        assert_eq!(err, copied);
    }
}
