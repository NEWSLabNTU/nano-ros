//! OpenCores Ethernet MAC register definitions
//!
//! Based on ESP-IDF `components/esp_eth/src/openeth/openeth.h` and the
//! OpenCores ethmac specification. Used by QEMU's `open_eth` NIC model.

/// Default base address on ESP32-C3 QEMU (mapped via `-nic model=open_eth`)
pub const ESP32C3_BASE: usize = 0x600C_D000;

/// Register offsets from base address
pub mod offset {
    /// Mode Register
    pub const MODER: usize = 0x00;
    /// Interrupt Source Register
    pub const INT_SOURCE: usize = 0x04;
    /// Interrupt Mask Register
    pub const INT_MASK: usize = 0x08;
    /// Inter-Packet Gap Register
    pub const IPGT: usize = 0x0C;
    /// Inter-Packet Gap Register (non back-to-back)
    pub const IPGR1: usize = 0x10;
    /// Inter-Packet Gap Register 2 (non back-to-back)
    pub const IPGR2: usize = 0x14;
    /// Packet Length Register (min/max)
    pub const PACKETLEN: usize = 0x18;
    /// Collision and Retry Configuration
    pub const COLLCONF: usize = 0x1C;
    /// Number of TX Buffer Descriptors
    pub const TX_BD_NUM: usize = 0x20;
    /// Control Module Mode Register
    pub const CTRLMODER: usize = 0x24;
    /// MII Mode Register
    pub const MIIMODER: usize = 0x28;
    /// MII Command Register
    pub const MIICOMMAND: usize = 0x2C;
    /// MII Address Register
    pub const MIIADDRESS: usize = 0x30;
    /// MII Transmit Data Register
    pub const MIITX_DATA: usize = 0x34;
    /// MII Receive Data Register
    pub const MIIRX_DATA: usize = 0x38;
    /// MII Status Register
    pub const MIISTATUS: usize = 0x3C;
    /// MAC Address 0 (bytes [5,4,3,2])
    pub const MAC_ADDR0: usize = 0x40;
    /// MAC Address 1 (bytes [1,0] in lower 16 bits)
    pub const MAC_ADDR1: usize = 0x44;
    /// Ethernet Hash Register 0
    pub const ETH_HASH0: usize = 0x48;
    /// Ethernet Hash Register 1
    pub const ETH_HASH1: usize = 0x4C;
    /// TX/RX Control Register
    pub const TXCTRL: usize = 0x50;

    /// Base address of buffer descriptors (relative to EMAC base)
    pub const BD_BASE: usize = 0x400;
}

/// MODER register bits
pub mod moder {
    /// Receive Enable
    pub const RXEN: u32 = 1 << 0;
    /// Transmit Enable
    pub const TXEN: u32 = 1 << 1;
    /// No Preamble
    pub const NOPRE: u32 = 1 << 2;
    /// Back-to-Back
    pub const BRO: u32 = 1 << 3;
    /// Interframe Gap
    pub const IAM: u32 = 1 << 4;
    /// Promiscuous Mode
    pub const PRO: u32 = 1 << 5;
    /// Full Duplex
    pub const FULLD: u32 = 1 << 10;
    /// Reset (self-clearing)
    pub const RST: u32 = 1 << 11;
    /// CRC Enable
    pub const CRCEN: u32 = 1 << 13;
    /// Pad Enable
    pub const PAD: u32 = 1 << 15;
    /// Default value after reset
    pub const DEFAULT: u32 = 0xA000;
}

/// INT_SOURCE / INT_MASK register bits
pub mod int {
    /// Transmit Buffer
    pub const TXB: u32 = 1 << 0;
    /// Transmit Error
    pub const TXE: u32 = 1 << 1;
    /// Receive Frame
    pub const RXF: u32 = 1 << 2;
    /// Receive Error
    pub const RXE: u32 = 1 << 3;
    /// Busy (RX buffer not available)
    pub const BUSY: u32 = 1 << 4;
    /// Transmit Control Frame
    pub const TXC: u32 = 1 << 5;
    /// Receive Control Frame
    pub const RXC: u32 = 1 << 6;
}

/// Buffer descriptor word0 bits (TX)
pub mod tx_bd {
    /// Length field shift (bits 31:16)
    pub const LEN_SHIFT: u32 = 16;
    /// Length field mask
    pub const LEN_MASK: u32 = 0xFFFF << LEN_SHIFT;
    /// Ready bit - set to 1 to transmit, cleared by hardware when done
    pub const RD: u32 = 1 << 15;
    /// IRQ - generate interrupt after transmit
    pub const IRQ: u32 = 1 << 14;
    /// Wrap - last descriptor in ring
    pub const WR: u32 = 1 << 13;
    /// Pad short frames
    pub const PAD: u32 = 1 << 12;
    /// Append CRC
    pub const CRC: u32 = 1 << 11;
}

/// Buffer descriptor word0 bits (RX)
pub mod rx_bd {
    /// Length field shift (bits 31:16)
    pub const LEN_SHIFT: u32 = 16;
    /// Length field mask
    pub const LEN_MASK: u32 = 0xFFFF << LEN_SHIFT;
    /// Empty bit - set to 1 to mark available for HW, cleared when frame received
    pub const E: u32 = 1 << 15;
    /// IRQ - generate interrupt on receive
    pub const IRQ: u32 = 1 << 14;
    /// Wrap - last descriptor in ring
    pub const WR: u32 = 1 << 13;
}
