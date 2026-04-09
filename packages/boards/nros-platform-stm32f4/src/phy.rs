//! PHY detection and configuration for STM32F4 Ethernet
//!
//! This module provides auto-detection of common Ethernet PHY chips
//! by reading their identification registers via MDIO.
//!
//! # Supported PHYs
//!
//! | PHY         | OUI           | Part Number | Boards                    |
//! |-------------|---------------|-------------|---------------------------|
//! | LAN8742A    | 0x0007C0      | 0x0013      | NUCLEO-F429ZI, Nucleo-F7  |
//! | DP83848     | 0x080017      | 0x09        | Many eval boards          |
//! | KSZ8081     | 0x000885      | 0x60        | Custom designs            |
//! | LAN8720     | 0x0007C0      | 0x000F      | Discovery, custom         |
//!
//! # PHY Identification Registers
//!
//! - Register 2: PHY ID 1 (bits 3-18 of OUI)
//! - Register 3: PHY ID 2 (bits 19-24 of OUI, part number, revision)

/// Standard PHY register addresses
pub mod registers {
    /// Basic Control Register
    pub const BCR: u8 = 0;
    /// Basic Status Register
    pub const BSR: u8 = 1;
    /// PHY Identifier 1 (OUI bits 3-18)
    pub const PHYID1: u8 = 2;
    /// PHY Identifier 2 (OUI bits 19-24, part number, revision)
    pub const PHYID2: u8 = 3;
}

/// Known PHY types
#[derive(Debug, Clone, Copy, PartialEq, Eq, defmt::Format)]
pub enum PhyType {
    /// Microchip LAN8742A (common on STM32 Nucleo boards)
    Lan8742A,
    /// Texas Instruments DP83848
    Dp83848,
    /// Microchip KSZ8081
    Ksz8081,
    /// Microchip LAN8720
    Lan8720,
    /// Unknown PHY (ID stored for diagnostics)
    Unknown(u32),
}

impl PhyType {
    /// Get the default PHY address for this type
    ///
    /// Most boards use address 0, but some may differ.
    pub fn default_address(&self) -> u8 {
        match self {
            PhyType::Lan8742A => 0,
            PhyType::Dp83848 => 1,
            PhyType::Ksz8081 => 0,
            PhyType::Lan8720 => 0,
            PhyType::Unknown(_) => 0,
        }
    }

    /// Get a human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            PhyType::Lan8742A => "LAN8742A",
            PhyType::Dp83848 => "DP83848",
            PhyType::Ksz8081 => "KSZ8081",
            PhyType::Lan8720 => "LAN8720",
            PhyType::Unknown(_) => "Unknown",
        }
    }
}

/// PHY identification constants
mod phy_ids {
    // LAN8742A: OUI = 0x0007C0, Model = 0x13, Rev varies
    // PHYID1 = 0x0007, PHYID2 = 0xC130..0xC13F
    pub const LAN8742A_ID1: u16 = 0x0007;
    pub const LAN8742A_ID2_MASK: u16 = 0xFFF0;
    pub const LAN8742A_ID2_VAL: u16 = 0xC130;

    // LAN8720: OUI = 0x0007C0, Model = 0x0F, Rev varies
    // PHYID1 = 0x0007, PHYID2 = 0xC0F0..0xC0FF
    pub const LAN8720_ID1: u16 = 0x0007;
    pub const LAN8720_ID2_MASK: u16 = 0xFFF0;
    pub const LAN8720_ID2_VAL: u16 = 0xC0F0;

    // DP83848: OUI = 0x080017, Model = 0x09, Rev varies
    // PHYID1 = 0x2000, PHYID2 = 0x5C90..0x5C9F
    pub const DP83848_ID1: u16 = 0x2000;
    pub const DP83848_ID2_MASK: u16 = 0xFFF0;
    pub const DP83848_ID2_VAL: u16 = 0x5C90;

    // KSZ8081: OUI = 0x000885, Model = 0x60 (actually varies)
    // PHYID1 = 0x0022, PHYID2 = 0x1560..0x156F
    pub const KSZ8081_ID1: u16 = 0x0022;
    pub const KSZ8081_ID2_MASK: u16 = 0xFFF0;
    pub const KSZ8081_ID2_VAL: u16 = 0x1560;
}

/// Detect PHY type from ID registers
///
/// # Arguments
///
/// * `id1` - Value from PHY register 2 (PHYID1)
/// * `id2` - Value from PHY register 3 (PHYID2)
///
/// # Returns
///
/// The detected PHY type, or `Unknown` with the combined ID.
pub fn detect_phy_type(id1: u16, id2: u16) -> PhyType {
    use phy_ids::*;

    // Check for LAN8742A
    if id1 == LAN8742A_ID1 && (id2 & LAN8742A_ID2_MASK) == LAN8742A_ID2_VAL {
        return PhyType::Lan8742A;
    }

    // Check for LAN8720
    if id1 == LAN8720_ID1 && (id2 & LAN8720_ID2_MASK) == LAN8720_ID2_VAL {
        return PhyType::Lan8720;
    }

    // Check for DP83848
    if id1 == DP83848_ID1 && (id2 & DP83848_ID2_MASK) == DP83848_ID2_VAL {
        return PhyType::Dp83848;
    }

    // Check for KSZ8081
    if id1 == KSZ8081_ID1 && (id2 & KSZ8081_ID2_MASK) == KSZ8081_ID2_VAL {
        return PhyType::Ksz8081;
    }

    // Unknown PHY - return combined ID for diagnostics
    let combined_id = ((id1 as u32) << 16) | (id2 as u32);
    PhyType::Unknown(combined_id)
}

/// Scan for PHY on common addresses
///
/// This function scans PHY addresses 0-3 looking for a valid PHY ID.
/// Most PHYs use address 0 or 1.
///
/// # Arguments
///
/// * `read_register` - Function to read a PHY register: `fn(phy_addr, reg) -> u16`
///
/// # Returns
///
/// A tuple of (PHY address, PHY type) if found, or None if no PHY detected.
pub fn scan_for_phy<F>(mut read_register: F) -> Option<(u8, PhyType)>
where
    F: FnMut(u8, u8) -> u16,
{
    // Common PHY addresses to scan
    const ADDRESSES: [u8; 4] = [0, 1, 2, 3];

    for &addr in &ADDRESSES {
        let id1 = read_register(addr, registers::PHYID1);
        let id2 = read_register(addr, registers::PHYID2);

        // Skip if we read all 1s or all 0s (no PHY present)
        if id1 == 0xFFFF || id1 == 0x0000 {
            continue;
        }

        let phy_type = detect_phy_type(id1, id2);
        defmt::info!(
            "PHY found at address {}: {} (ID1=0x{:04x}, ID2=0x{:04x})",
            addr,
            phy_type.name(),
            id1,
            id2
        );
        return Some((addr, phy_type));
    }

    defmt::warn!("No PHY found during scan");
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_lan8742a() {
        let phy = detect_phy_type(0x0007, 0xC130);
        assert_eq!(phy, PhyType::Lan8742A);

        // With different revision
        let phy = detect_phy_type(0x0007, 0xC131);
        assert_eq!(phy, PhyType::Lan8742A);
    }

    #[test]
    fn test_detect_dp83848() {
        let phy = detect_phy_type(0x2000, 0x5C90);
        assert_eq!(phy, PhyType::Dp83848);
    }

    #[test]
    fn test_detect_unknown() {
        let phy = detect_phy_type(0x1234, 0x5678);
        assert!(matches!(phy, PhyType::Unknown(0x12345678)));
    }
}
