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

#[cfg(test)]
mod tests {
    use super::*;

    // =====================================================================
    // Base address
    // =====================================================================

    #[test]
    fn test_esp32c3_base_address() {
        // ESP32-C3 QEMU OpenETH mapped at peripheral address
        assert_eq!(ESP32C3_BASE, 0x600C_D000);
    }

    // =====================================================================
    // Register offsets — verified against ESP-IDF openeth.h
    // =====================================================================

    #[test]
    fn test_register_offsets_match_esp_idf() {
        assert_eq!(offset::MODER, 0x00);
        assert_eq!(offset::INT_SOURCE, 0x04);
        assert_eq!(offset::INT_MASK, 0x08);
        assert_eq!(offset::IPGT, 0x0C);
        assert_eq!(offset::IPGR1, 0x10);
        assert_eq!(offset::IPGR2, 0x14);
        assert_eq!(offset::PACKETLEN, 0x18);
        assert_eq!(offset::COLLCONF, 0x1C);
        assert_eq!(offset::TX_BD_NUM, 0x20);
        assert_eq!(offset::CTRLMODER, 0x24);
        assert_eq!(offset::MIIMODER, 0x28);
        assert_eq!(offset::MIICOMMAND, 0x2C);
        assert_eq!(offset::MIIADDRESS, 0x30);
        assert_eq!(offset::MIITX_DATA, 0x34);
        assert_eq!(offset::MIIRX_DATA, 0x38);
        assert_eq!(offset::MIISTATUS, 0x3C);
        assert_eq!(offset::MAC_ADDR0, 0x40);
        assert_eq!(offset::MAC_ADDR1, 0x44);
        assert_eq!(offset::ETH_HASH0, 0x48);
        assert_eq!(offset::ETH_HASH1, 0x4C);
        assert_eq!(offset::TXCTRL, 0x50);
    }

    #[test]
    fn test_register_offsets_are_4_byte_aligned() {
        let offsets = [
            offset::MODER,
            offset::INT_SOURCE,
            offset::INT_MASK,
            offset::IPGT,
            offset::IPGR1,
            offset::IPGR2,
            offset::PACKETLEN,
            offset::COLLCONF,
            offset::TX_BD_NUM,
            offset::CTRLMODER,
            offset::MIIMODER,
            offset::MIICOMMAND,
            offset::MIIADDRESS,
            offset::MIITX_DATA,
            offset::MIIRX_DATA,
            offset::MIISTATUS,
            offset::MAC_ADDR0,
            offset::MAC_ADDR1,
            offset::ETH_HASH0,
            offset::ETH_HASH1,
            offset::TXCTRL,
        ];
        for off in offsets {
            assert_eq!(off % 4, 0, "Register offset {:#x} not 4-byte aligned", off);
        }
    }

    #[test]
    fn test_bd_base_offset() {
        // Buffer descriptors start at 0x400 from EMAC base
        assert_eq!(offset::BD_BASE, 0x400);
        // BD area is separate from register space (which ends at 0x50)
        assert!(offset::BD_BASE > offset::TXCTRL);
    }

    // =====================================================================
    // MODER register bits
    // =====================================================================

    #[test]
    fn test_moder_bit_positions() {
        assert_eq!(moder::RXEN, 0x0001);
        assert_eq!(moder::TXEN, 0x0002);
        assert_eq!(moder::NOPRE, 0x0004);
        assert_eq!(moder::BRO, 0x0008);
        assert_eq!(moder::IAM, 0x0010);
        assert_eq!(moder::PRO, 0x0020);
        assert_eq!(moder::FULLD, 0x0400);
        assert_eq!(moder::RST, 0x0800);
        assert_eq!(moder::CRCEN, 0x2000);
        assert_eq!(moder::PAD, 0x8000);
    }

    #[test]
    fn test_moder_bits_no_overlap() {
        let bits = [
            moder::RXEN,
            moder::TXEN,
            moder::NOPRE,
            moder::BRO,
            moder::IAM,
            moder::PRO,
            moder::FULLD,
            moder::RST,
            moder::CRCEN,
            moder::PAD,
        ];
        for i in 0..bits.len() {
            for j in (i + 1)..bits.len() {
                assert_eq!(
                    bits[i] & bits[j],
                    0,
                    "MODER bits {:#x} and {:#x} overlap",
                    bits[i],
                    bits[j]
                );
            }
        }
    }

    #[test]
    fn test_moder_default_value() {
        // DEFAULT = 0xA000 = PAD (bit 15) | CRCEN (bit 13)
        assert_eq!(moder::DEFAULT, 0xA000);
        assert_ne!(moder::DEFAULT & moder::PAD, 0);
        assert_ne!(moder::DEFAULT & moder::CRCEN, 0);
        // Other bits should be clear
        assert_eq!(moder::DEFAULT & moder::RXEN, 0);
        assert_eq!(moder::DEFAULT & moder::TXEN, 0);
        assert_eq!(moder::DEFAULT & moder::RST, 0);
    }

    // =====================================================================
    // Interrupt register bits
    // =====================================================================

    #[test]
    fn test_interrupt_bit_positions() {
        assert_eq!(int::TXB, 0x01);
        assert_eq!(int::TXE, 0x02);
        assert_eq!(int::RXF, 0x04);
        assert_eq!(int::RXE, 0x08);
        assert_eq!(int::BUSY, 0x10);
        assert_eq!(int::TXC, 0x20);
        assert_eq!(int::RXC, 0x40);
    }

    #[test]
    fn test_interrupt_bits_no_overlap() {
        let bits = [
            int::TXB,
            int::TXE,
            int::RXF,
            int::RXE,
            int::BUSY,
            int::TXC,
            int::RXC,
        ];
        for i in 0..bits.len() {
            for j in (i + 1)..bits.len() {
                assert_eq!(
                    bits[i] & bits[j],
                    0,
                    "INT bits {:#x} and {:#x} overlap",
                    bits[i],
                    bits[j]
                );
            }
        }
    }

    #[test]
    fn test_interrupt_all_bits_fit_in_7_bits() {
        let all = int::TXB | int::TXE | int::RXF | int::RXE | int::BUSY | int::TXC | int::RXC;
        assert_eq!(all, 0x7F);
    }

    // =====================================================================
    // TX buffer descriptor bits
    // =====================================================================

    #[test]
    fn test_tx_bd_bit_positions() {
        assert_eq!(tx_bd::LEN_SHIFT, 16);
        assert_eq!(tx_bd::LEN_MASK, 0xFFFF_0000);
        assert_eq!(tx_bd::RD, 0x8000);
        assert_eq!(tx_bd::IRQ, 0x4000);
        assert_eq!(tx_bd::WR, 0x2000);
        assert_eq!(tx_bd::PAD, 0x1000);
        assert_eq!(tx_bd::CRC, 0x0800);
    }

    #[test]
    fn test_tx_bd_control_bits_no_overlap() {
        let bits = [tx_bd::RD, tx_bd::IRQ, tx_bd::WR, tx_bd::PAD, tx_bd::CRC];
        for i in 0..bits.len() {
            for j in (i + 1)..bits.len() {
                assert_eq!(
                    bits[i] & bits[j],
                    0,
                    "TX BD bits {:#x} and {:#x} overlap",
                    bits[i],
                    bits[j]
                );
            }
        }
    }

    #[test]
    fn test_tx_bd_len_field() {
        // Encoding: length in bits 31:16
        let len_100 = (100u32) << tx_bd::LEN_SHIFT;
        assert_eq!(len_100 & tx_bd::LEN_MASK, len_100);
        assert_eq!((len_100 & tx_bd::LEN_MASK) >> tx_bd::LEN_SHIFT, 100);

        // Length field doesn't overlap with control bits
        assert_eq!(tx_bd::LEN_MASK & tx_bd::RD, 0);
        assert_eq!(tx_bd::LEN_MASK & tx_bd::WR, 0);
    }

    // =====================================================================
    // RX buffer descriptor bits
    // =====================================================================

    #[test]
    fn test_rx_bd_bit_positions() {
        assert_eq!(rx_bd::LEN_SHIFT, 16);
        assert_eq!(rx_bd::LEN_MASK, 0xFFFF_0000);
        assert_eq!(rx_bd::E, 0x8000);
        assert_eq!(rx_bd::IRQ, 0x4000);
        assert_eq!(rx_bd::WR, 0x2000);
    }

    #[test]
    fn test_rx_bd_control_bits_no_overlap() {
        let bits = [rx_bd::E, rx_bd::IRQ, rx_bd::WR];
        for i in 0..bits.len() {
            for j in (i + 1)..bits.len() {
                assert_eq!(
                    bits[i] & bits[j],
                    0,
                    "RX BD bits {:#x} and {:#x} overlap",
                    bits[i],
                    bits[j]
                );
            }
        }
    }

    #[test]
    fn test_rx_bd_len_field() {
        let len_1500 = (1500u32) << rx_bd::LEN_SHIFT;
        assert_eq!((len_1500 & rx_bd::LEN_MASK) >> rx_bd::LEN_SHIFT, 1500);
        // Length field doesn't overlap with control bits
        assert_eq!(rx_bd::LEN_MASK & rx_bd::E, 0);
        assert_eq!(rx_bd::LEN_MASK & rx_bd::WR, 0);
    }

    // =====================================================================
    // TX/RX descriptor layout consistency
    // =====================================================================

    #[test]
    fn test_tx_rx_wrap_bits_same_position() {
        // WR bit is at the same position for both TX and RX descriptors
        assert_eq!(tx_bd::WR, rx_bd::WR);
    }

    #[test]
    fn test_tx_rx_irq_bits_same_position() {
        assert_eq!(tx_bd::IRQ, rx_bd::IRQ);
    }

    #[test]
    fn test_tx_rx_len_fields_same_layout() {
        assert_eq!(tx_bd::LEN_SHIFT, rx_bd::LEN_SHIFT);
        assert_eq!(tx_bd::LEN_MASK, rx_bd::LEN_MASK);
    }

    #[test]
    fn test_descriptor_size_is_8_bytes() {
        // Each buffer descriptor is 2 × 32-bit words = 8 bytes
        // word0: control/status + length
        // word1: DMA buffer pointer
        // This is verified by the BD_BASE offset arithmetic in the driver:
        // RX BD at BD_BASE + TX_BD_COUNT * 8
        let descriptor_size = 8usize;
        let tx_bd_count = 1usize;
        let rx_bd_offset = offset::BD_BASE + tx_bd_count * descriptor_size;
        assert_eq!(rx_bd_offset, 0x408);
    }
}
