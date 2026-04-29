//! LAN9118/SMSC911x Ethernet driver for smoltcp
//!
//! This crate provides a `no_std` compatible driver for the LAN9118 Ethernet
//! controller (and compatible SMSC911x variants), implementing the
//! `smoltcp::phy::Device` trait for integration with the smoltcp TCP/IP stack.
//!
//! # Supported Hardware
//!
//! - SMSC LAN9118
//! - SMSC LAN9220 (QEMU mps2-an385 machine)
//!
//! # Usage
//!
//! ```ignore
//! use lan9118_smoltcp::{Lan9118, Config};
//!
//! // Create driver with static buffers
//! let config = Config {
//!     base_addr: 0x4020_0000,
//!     mac_addr: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
//! };
//!
//! let mut eth = unsafe { Lan9118::new(config) }?;
//! eth.init()?;
//!
//! // Use with smoltcp
//! let mut iface = smoltcp::iface::Interface::new(config, &mut eth, instant);
//! ```

#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod regs;

use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicU32, Ordering};
use smoltcp::phy::{self, Device, DeviceCapabilities, Medium};
use smoltcp::time::Instant;

pub use regs::MPS2_AN385_BASE;

// Phase 97.3.mps2-an385 — RX path diagnostic counters. Incremented
// every time `poll_rx` observes a non-empty FIFO entry. Read by the
// board crate to confirm whether inbound mcast frames reach the chip
// at all (vs. being dropped at higher smoltcp layers).
static RX_PKT_PENDING_TOTAL: AtomicU32 = AtomicU32::new(0);
static RX_PKT_DELIVERED_TOTAL: AtomicU32 = AtomicU32::new(0);
static RX_PKT_ERR_TOTAL: AtomicU32 = AtomicU32::new(0);

/// Snapshot of RX-path counters (pending observed, delivered, err-discarded).
pub fn rx_diag_counters() -> (u32, u32, u32) {
    (
        RX_PKT_PENDING_TOTAL.load(Ordering::Relaxed),
        RX_PKT_DELIVERED_TOTAL.load(Ordering::Relaxed),
        RX_PKT_ERR_TOTAL.load(Ordering::Relaxed),
    )
}

/// Maximum Transmission Unit for Ethernet
pub const MTU: usize = 1500;

/// Maximum Ethernet frame size (MTU + headers + FCS)
pub const MAX_FRAME_SIZE: usize = 1536;

/// Driver error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Hardware not detected at specified address
    DeviceNotFound,
    /// Unknown device ID
    UnknownDevice(u16),
    /// Timeout waiting for operation to complete
    Timeout,
    /// MAC CSR access failed
    MacCsrError,
    /// PHY access failed
    PhyError,
    /// Software reset failed
    ResetFailed,
}

/// Driver configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// Base address of the LAN9118 registers
    pub base_addr: usize,
    /// MAC address (6 bytes)
    pub mac_addr: [u8; 6],
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_addr: MPS2_AN385_BASE,
            // Default locally-administered MAC address
            mac_addr: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
        }
    }
}

/// LAN9118 Ethernet driver
///
/// This driver manages the LAN9118 Ethernet controller and provides
/// the smoltcp `Device` trait for network stack integration.
pub struct Lan9118 {
    base: usize,
    mac_addr: [u8; 6],
    /// Internal RX buffer
    rx_buffer: [u8; MAX_FRAME_SIZE],
    /// Internal TX buffer
    tx_buffer: [u8; MAX_FRAME_SIZE],
    /// Length of data in RX buffer (0 = empty)
    rx_len: usize,
}

impl Lan9118 {
    /// Create a new driver instance.
    ///
    /// # Safety
    ///
    /// - `config.base_addr` must point to valid LAN9118 hardware registers
    /// - Only one driver instance should exist per hardware device
    pub unsafe fn new(config: Config) -> Result<Self, Error> {
        Ok(Self {
            base: config.base_addr,
            mac_addr: config.mac_addr,
            rx_buffer: [0; MAX_FRAME_SIZE],
            tx_buffer: [0; MAX_FRAME_SIZE],
            rx_len: 0,
        })
    }

    /// Initialize the hardware.
    ///
    /// This performs the full initialization sequence:
    /// 1. Verify hardware presence
    /// 2. Software reset
    /// 3. Configure TX FIFO
    /// 4. Configure PHY and establish link
    /// 5. Enable TX/RX
    pub fn init(&mut self) -> Result<(), Error> {
        // Step 1: Verify hardware presence
        self.check_device()?;

        // Step 2: Software reset
        self.soft_reset()?;

        // Step 3: Set TX FIFO size (5KB)
        self.set_tx_fifo_size(5);

        // Step 4: Configure auto flow control
        self.write_reg(regs::offset::AFC_CFG, 0x006E_3740);

        // Step 5: Configure GPIO for LEDs
        self.write_reg(regs::offset::GPIO_CFG, 0x7007_0000);

        // Step 6: Clear and configure interrupts
        self.init_interrupts();

        // Step 7-9: Reset and configure PHY
        self.init_phy()?;

        // Step 10: Configure FIFO interrupts
        self.write_reg(regs::offset::FIFO_INT, 0xFF00_0000);

        // Step 11-12: Enable MAC TX/RX
        self.enable_mac_tx()?;
        self.write_reg(regs::offset::TX_CFG, regs::tx_cfg::TX_ON);
        self.write_reg(regs::offset::RX_CFG, 0);
        self.enable_mac_rx()?;

        // Step 13: Clear RX threshold in FIFO_INT
        let fifo_int = self.read_reg(regs::offset::FIFO_INT);
        self.write_reg(regs::offset::FIFO_INT, fifo_int & !0xFF);

        // Set MAC address
        self.write_mac_address()?;

        Ok(())
    }

    /// Check if the device is present and recognized.
    fn check_device(&self) -> Result<u16, Error> {
        let id_rev = self.read_reg(regs::offset::ID_REV);

        // If top and bottom halves match, device likely not present
        let upper = (id_rev >> 16) as u16;
        let lower = id_rev as u16;
        if upper == lower {
            return Err(Error::DeviceNotFound);
        }

        // Check for known device IDs
        match upper {
            regs::device_id::LAN9220 | regs::device_id::LAN9118 => Ok(upper),
            _ => Err(Error::UnknownDevice(upper)),
        }
    }

    /// Perform a software reset.
    fn soft_reset(&mut self) -> Result<(), Error> {
        // Set SRST bit
        let hw_cfg = self.read_reg(regs::offset::HW_CFG);
        self.write_reg(regs::offset::HW_CFG, hw_cfg | regs::hw_cfg::SRST);

        // Wait for reset to complete (SRST bit self-clears)
        for _ in 0..1000 {
            let hw_cfg = self.read_reg(regs::offset::HW_CFG);
            if (hw_cfg & regs::hw_cfg::SRST) == 0 {
                return Ok(());
            }
            self.delay_us(10);
        }

        Err(Error::ResetFailed)
    }

    /// Set TX FIFO size in KB (valid range: 2-14).
    fn set_tx_fifo_size(&mut self, kb: u32) {
        let kb = kb.clamp(2, 14);
        let hw_cfg = self.read_reg(regs::offset::HW_CFG);
        let hw_cfg = (hw_cfg & !regs::hw_cfg::TX_FIF_SZ_MASK)
            | (kb << regs::hw_cfg::TX_FIF_SZ_SHIFT)
            | regs::hw_cfg::MBO;
        self.write_reg(regs::offset::HW_CFG, hw_cfg);
    }

    /// Initialize interrupt configuration.
    fn init_interrupts(&mut self) {
        // Disable all interrupts
        self.write_reg(regs::offset::INT_EN, 0);
        // Clear all pending interrupts
        self.write_reg(regs::offset::INT_STS, 0xFFFF_FFFF);
        // Configure IRQ polarity and type
        self.write_reg(regs::offset::IRQ_CFG, regs::irq_cfg::DEFAULT);
    }

    /// Initialize PHY and establish link.
    fn init_phy(&mut self) -> Result<(), Error> {
        // Check PHY is present
        let phy_id1 = self.phy_read(regs::phy::PHYID1)?;
        if phy_id1 == 0xFFFF || phy_id1 == 0 {
            return Err(Error::PhyError);
        }

        // Reset PHY
        self.phy_write(regs::phy::BMCR, regs::bmcr::RESET as u16)?;

        // Wait for reset to complete
        for _ in 0..100 {
            self.delay_us(1000);
            let bmcr = self.phy_read(regs::phy::BMCR)?;
            if (bmcr & regs::bmcr::RESET as u16) == 0 {
                break;
            }
        }

        // Advertise all capabilities
        let anar = self.phy_read(regs::phy::ANAR)?;
        self.phy_write(regs::phy::ANAR, anar | regs::anar::ALL_CAPS as u16)?;

        // Enable auto-negotiation and restart
        let bmcr = regs::bmcr::ANENABLE | regs::bmcr::ANRESTART;
        self.phy_write(regs::phy::BMCR, bmcr as u16)?;

        Ok(())
    }

    /// Enable MAC transmit.
    fn enable_mac_tx(&mut self) -> Result<(), Error> {
        let mac_cr = self.mac_read(regs::mac_csr::MAC_CR)?;
        self.mac_write(regs::mac_csr::MAC_CR, mac_cr | regs::mac_cr::TXEN)?;
        Ok(())
    }

    /// Enable MAC receive.
    ///
    /// Phase 97.3.mps2-an385 — also flips `MCPAS` (Pass All Multicast)
    /// + `PRMS` (Promiscuous Mode) so DDS / RTPS multicast frames
    /// (`239.255.0.1` → `01:00:5e:7f:00:01`) and any other-MAC traffic
    /// the smoltcp stack later filters reach the driver. QEMU socket
    /// netdev forwards every frame on the segment regardless of MAC,
    /// so without promiscuous the chip drops sibling-instance
    /// SPDP/SEDP multicasts and discovery never closes.
    fn enable_mac_rx(&mut self) -> Result<(), Error> {
        let mac_cr = self.mac_read(regs::mac_csr::MAC_CR)?;
        self.mac_write(
            regs::mac_csr::MAC_CR,
            mac_cr | regs::mac_cr::RXEN | regs::mac_cr::MCPAS | regs::mac_cr::PRMS,
        )?;
        Ok(())
    }

    /// Write the MAC address to hardware.
    fn write_mac_address(&mut self) -> Result<(), Error> {
        let addr = self.mac_addr;

        // MAC_ADDRL: bytes 0-3
        let addrl = u32::from(addr[0])
            | (u32::from(addr[1]) << 8)
            | (u32::from(addr[2]) << 16)
            | (u32::from(addr[3]) << 24);

        // MAC_ADDRH: bytes 4-5
        let addrh = u32::from(addr[4]) | (u32::from(addr[5]) << 8);

        self.mac_write(regs::mac_csr::ADDRL, addrl)?;
        self.mac_write(regs::mac_csr::ADDRH, addrh)?;

        Ok(())
    }

    /// Set the MAC address.
    pub fn set_mac_address(&mut self, addr: [u8; 6]) -> Result<(), Error> {
        self.mac_addr = addr;
        self.write_mac_address()
    }

    /// Get the current MAC address.
    pub fn mac_address(&self) -> [u8; 6] {
        self.mac_addr
    }

    /// Check if link is up.
    pub fn link_is_up(&mut self) -> bool {
        if let Ok(bmsr) = self.phy_read(regs::phy::BMSR) {
            (bmsr & regs::bmsr::LSTATUS as u16) != 0
        } else {
            false
        }
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

    /// Read a MAC CSR register.
    fn mac_read(&self, reg: u8) -> Result<u32, Error> {
        // Wait for not busy
        self.wait_mac_not_busy()?;

        // Issue read command
        let cmd = regs::mac_csr_cmd::BUSY | regs::mac_csr_cmd::READ | u32::from(reg);
        self.write_reg(regs::offset::MAC_CSR_CMD, cmd);

        // Wait for completion
        self.wait_mac_not_busy()?;

        // Read result
        Ok(self.read_reg(regs::offset::MAC_CSR_DATA))
    }

    /// Write a MAC CSR register.
    fn mac_write(&self, reg: u8, value: u32) -> Result<(), Error> {
        // Wait for not busy
        self.wait_mac_not_busy()?;

        // Write data first
        self.write_reg(regs::offset::MAC_CSR_DATA, value);

        // Issue write command (BUSY without READ flag)
        let cmd = regs::mac_csr_cmd::BUSY | u32::from(reg);
        self.write_reg(regs::offset::MAC_CSR_CMD, cmd);

        // Wait for completion
        self.wait_mac_not_busy()?;

        Ok(())
    }

    /// Wait for MAC CSR to become not busy.
    fn wait_mac_not_busy(&self) -> Result<(), Error> {
        for _ in 0..1000 {
            let cmd = self.read_reg(regs::offset::MAC_CSR_CMD);
            if (cmd & regs::mac_csr_cmd::BUSY) == 0 {
                return Ok(());
            }
            self.delay_us(1);
        }
        Err(Error::MacCsrError)
    }

    /// Read a PHY register via MII.
    fn phy_read(&self, reg: u8) -> Result<u16, Error> {
        // Check MII not busy
        let mii_acc = self.mac_read(regs::mac_csr::MII_ACC)?;
        if (mii_acc & regs::mii_acc::BUSY) != 0 {
            return Err(Error::PhyError);
        }

        // Issue read command
        let cmd = (u32::from(regs::phy::ADDR) << regs::mii_acc::PHY_ADDR_SHIFT)
            | (u32::from(reg) << regs::mii_acc::REG_ADDR_SHIFT)
            | regs::mii_acc::BUSY;
        self.mac_write(regs::mac_csr::MII_ACC, cmd)?;

        // Wait for completion
        for _ in 0..1000 {
            self.delay_us(10);
            let mii_acc = self.mac_read(regs::mac_csr::MII_ACC)?;
            if (mii_acc & regs::mii_acc::BUSY) == 0 {
                // Read data
                let data = self.mac_read(regs::mac_csr::MII_DATA)?;
                return Ok(data as u16);
            }
        }

        Err(Error::Timeout)
    }

    /// Write a PHY register via MII.
    fn phy_write(&self, reg: u8, value: u16) -> Result<(), Error> {
        // Check MII not busy
        let mii_acc = self.mac_read(regs::mac_csr::MII_ACC)?;
        if (mii_acc & regs::mii_acc::BUSY) != 0 {
            return Err(Error::PhyError);
        }

        // Write data
        self.mac_write(regs::mac_csr::MII_DATA, u32::from(value))?;

        // Issue write command
        let cmd = (u32::from(regs::phy::ADDR) << regs::mii_acc::PHY_ADDR_SHIFT)
            | (u32::from(reg) << regs::mii_acc::REG_ADDR_SHIFT)
            | regs::mii_acc::WRITE
            | regs::mii_acc::BUSY;
        self.mac_write(regs::mac_csr::MII_ACC, cmd)?;

        // Wait for completion
        for _ in 0..1000 {
            self.delay_us(10);
            let mii_acc = self.mac_read(regs::mac_csr::MII_ACC)?;
            if (mii_acc & regs::mii_acc::BUSY) == 0 {
                return Ok(());
            }
        }

        Err(Error::Timeout)
    }

    // ========================================================================
    // TX/RX FIFO operations
    // ========================================================================

    /// Check if there are packets waiting in the RX FIFO.
    fn rx_packets_pending(&self) -> u32 {
        let rx_fifo_inf = self.read_reg(regs::offset::RX_FIFO_INF);
        (rx_fifo_inf & regs::rx_fifo_inf::RXSUSED_MASK) >> regs::rx_fifo_inf::RXSUSED_SHIFT
    }

    /// Poll for received packet and store in internal buffer.
    /// Returns true if a packet was received.
    fn poll_rx(&mut self) -> bool {
        // Already have a packet buffered
        if self.rx_len > 0 {
            return true;
        }

        // Check for pending packets
        if self.rx_packets_pending() == 0 {
            return false;
        }
        RX_PKT_PENDING_TOTAL.fetch_add(1, Ordering::Relaxed);
        // Read RX status
        let rx_stat = self.read_reg(regs::offset::RX_STAT_PORT);
        let pkt_len =
            ((rx_stat & regs::rx_stat::PKT_LEN_MASK) >> regs::rx_stat::PKT_LEN_SHIFT) as usize;

        // Check for errors
        if (rx_stat & regs::rx_stat::ES) != 0 {
            self.rx_discard();
            RX_PKT_ERR_TOTAL.fetch_add(1, Ordering::Relaxed);
            return false;
        }

        // Packet length includes 4-byte FCS
        if !(4..=MAX_FRAME_SIZE).contains(&pkt_len) {
            self.rx_discard();
            RX_PKT_ERR_TOTAL.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        let data_len = pkt_len - 4;

        // Round up to DWORD boundary for reading
        let read_words = pkt_len.div_ceil(4);

        // Read packet data
        let mut offset = 0;
        for i in 0..read_words {
            let word = self.read_reg(regs::offset::RX_DATA_PORT);

            // Copy bytes to buffer (skip FCS at end)
            let word_offset = i * 4;
            let bytes_to_copy = if word_offset + 4 <= data_len {
                4
            } else {
                data_len.saturating_sub(word_offset)
            };

            if bytes_to_copy > 0 {
                let word_bytes = word.to_le_bytes();
                self.rx_buffer[offset..offset + bytes_to_copy]
                    .copy_from_slice(&word_bytes[..bytes_to_copy]);
                offset += bytes_to_copy;
            }
        }

        self.rx_len = data_len;
        RX_PKT_DELIVERED_TOTAL.fetch_add(1, Ordering::Relaxed);
        true
    }

    /// Discard current RX packet (fast-forward).
    fn rx_discard(&self) {
        self.write_reg(regs::offset::RX_DP_CTRL, regs::rx_dp_ctrl::RX_FFWD);
        // Wait for fast-forward to complete
        for _ in 0..100 {
            if (self.read_reg(regs::offset::RX_DP_CTRL) & regs::rx_dp_ctrl::RX_FFWD) == 0 {
                break;
            }
            self.delay_us(1);
        }
    }

    /// Get available space in TX FIFO.
    fn tx_fifo_free(&self) -> usize {
        let tx_fifo_inf = self.read_reg(regs::offset::TX_FIFO_INF);
        (tx_fifo_inf & regs::tx_fifo_inf::TXDFREE_MASK) as usize
    }

    /// Check if TX is ready.
    fn tx_ready(&self) -> bool {
        // Need: 8 bytes for commands + max frame rounded to DWORD
        self.tx_fifo_free() >= MAX_FRAME_SIZE + 8
    }

    /// Transmit a packet from the internal TX buffer.
    fn tx_send(&mut self, len: usize) {
        if len > MAX_FRAME_SIZE {
            return;
        }

        // Write TX Command A
        let cmd_a = regs::tx_cmd_a::FIRST_SEG | regs::tx_cmd_a::LAST_SEG | (len as u32);
        self.write_reg(regs::offset::TX_DATA_PORT, cmd_a);

        // Write TX Command B
        let cmd_b = ((len as u32) << regs::tx_cmd_b::PKT_TAG_SHIFT) | (len as u32);
        self.write_reg(regs::offset::TX_DATA_PORT, cmd_b);

        // Write packet data (DWORD aligned)
        let words = len.div_ceil(4);
        for i in 0..words {
            let offset = i * 4;
            let word = if offset + 4 <= len {
                u32::from_le_bytes([
                    self.tx_buffer[offset],
                    self.tx_buffer[offset + 1],
                    self.tx_buffer[offset + 2],
                    self.tx_buffer[offset + 3],
                ])
            } else {
                // Partial last word
                let mut bytes = [0u8; 4];
                for (j, b) in bytes.iter_mut().enumerate() {
                    if offset + j < len {
                        *b = self.tx_buffer[offset + j];
                    }
                }
                u32::from_le_bytes(bytes)
            };
            self.write_reg(regs::offset::TX_DATA_PORT, word);
        }
    }

    // ========================================================================
    // Timing
    // ========================================================================

    /// Simple busy-wait delay.
    #[inline]
    fn delay_us(&self, us: u32) {
        // Busy-wait loop. ~25 iterations ≈ 1µs at 25MHz
        for _ in 0..(us * 25) {
            core::hint::spin_loop();
        }
    }
}

// ============================================================================
// smoltcp Device trait implementation
// ============================================================================

/// RX token for smoltcp
pub struct Lan9118RxToken<'a> {
    buffer: &'a [u8],
}

impl phy::RxToken for Lan9118RxToken<'_> {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(self.buffer)
    }
}

/// TX token for smoltcp
pub struct Lan9118TxToken<'a> {
    driver: &'a mut Lan9118,
}

impl phy::TxToken for Lan9118TxToken<'_> {
    fn consume<R, F>(self, len: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let len = len.min(MAX_FRAME_SIZE);
        let result = f(&mut self.driver.tx_buffer[..len]);
        self.driver.tx_send(len);
        result
    }
}

impl Device for Lan9118 {
    type RxToken<'a>
        = Lan9118RxToken<'a>
    where
        Self: 'a;
    type TxToken<'a>
        = Lan9118TxToken<'a>
    where
        Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        // Poll for received packet
        if !self.poll_rx() {
            return None;
        }

        // Check TX is also ready (smoltcp expects both)
        if !self.tx_ready() {
            return None;
        }

        let len = self.rx_len;
        self.rx_len = 0; // Mark buffer as consumed

        // Create tokens
        // Safety: rx_buffer and tx_buffer are separate arrays in the struct,
        // so we can have an immutable reference to rx_buffer and a mutable
        // reference to the rest of self (including tx_buffer) simultaneously.
        let rx_slice = &self.rx_buffer[..len];

        // We need to create both tokens. The RxToken holds a slice of rx_buffer,
        // while TxToken needs &mut self. This is problematic...
        //
        // The standard workaround is to use raw pointers:
        let rx_ptr = rx_slice.as_ptr();
        let rx_token = Lan9118RxToken {
            buffer: unsafe { core::slice::from_raw_parts(rx_ptr, len) },
        };

        // Now we can create the tx token with &mut self
        // This is safe because:
        // 1. rx_token only reads from rx_buffer[..len]
        // 2. tx_token writes to tx_buffer and hardware registers
        // 3. These do not overlap
        let tx_token = Lan9118TxToken { driver: self };

        Some((rx_token, tx_token))
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        if !self.tx_ready() {
            return None;
        }

        Some(Lan9118TxToken { driver: self })
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.medium = Medium::Ethernet;
        caps.max_transmission_unit = MTU;
        caps.max_burst_size = Some(1);
        caps
    }
}
