//! LAN9118/SMSC911x register definitions
//!
//! Based on the LAN9118 datasheet and Zephyr eth_smsc911x driver.

/// Default base address on MPS2-AN385 QEMU machine
pub const MPS2_AN385_BASE: usize = 0x4020_0000;

/// Register offsets from base address
pub mod offset {
    /// RX Data FIFO Port (read-only)
    pub const RX_DATA_PORT: usize = 0x00;
    /// TX Data FIFO Port (write-only)
    pub const TX_DATA_PORT: usize = 0x20;
    /// RX Status FIFO Port (read-only)
    pub const RX_STAT_PORT: usize = 0x40;
    /// RX Status FIFO Peek (read-only, non-consuming)
    pub const RX_STAT_PEEK: usize = 0x44;
    /// TX Status FIFO Port (read-only)
    pub const TX_STAT_PORT: usize = 0x48;
    /// TX Status FIFO Peek (read-only, non-consuming)
    pub const TX_STAT_PEEK: usize = 0x4C;

    /// Chip ID and Revision (read-only)
    pub const ID_REV: usize = 0x50;
    /// Interrupt Configuration
    pub const IRQ_CFG: usize = 0x54;
    /// Interrupt Status (write 1 to clear)
    pub const INT_STS: usize = 0x58;
    /// Interrupt Enable
    pub const INT_EN: usize = 0x5C;
    /// Byte Order Test (read-only, always 0x87654321)
    pub const BYTE_TEST: usize = 0x64;
    /// FIFO Level Interrupts
    pub const FIFO_INT: usize = 0x68;
    /// Receive Configuration
    pub const RX_CFG: usize = 0x6C;
    /// Transmit Configuration
    pub const TX_CFG: usize = 0x70;
    /// Hardware Configuration
    pub const HW_CFG: usize = 0x74;
    /// RX Datapath Control
    pub const RX_DP_CTRL: usize = 0x78;
    /// RX FIFO Information (read-only)
    pub const RX_FIFO_INF: usize = 0x7C;
    /// TX FIFO Information (read-only)
    pub const TX_FIFO_INF: usize = 0x80;

    /// Power Management Control
    pub const PMT_CTRL: usize = 0x84;
    /// GPIO Configuration
    pub const GPIO_CFG: usize = 0x88;
    /// General Purpose Timer Configuration
    pub const GPT_CFG: usize = 0x8C;
    /// General Purpose Timer Count (read-only)
    pub const GPT_CNT: usize = 0x90;
    /// Word Swap
    pub const ENDIAN: usize = 0x98;
    /// Free Running Counter (read-only)
    pub const FREE_RUN: usize = 0x9C;
    /// RX Dropped Frame Counter (read-only)
    pub const RX_DROP: usize = 0xA0;
    /// MAC CSR Synchronizer Command
    pub const MAC_CSR_CMD: usize = 0xA4;
    /// MAC CSR Synchronizer Data
    pub const MAC_CSR_DATA: usize = 0xA8;
    /// Automatic Flow Control Configuration
    pub const AFC_CFG: usize = 0xAC;
    /// EEPROM Command
    pub const E2P_CMD: usize = 0xB0;
    /// EEPROM Data
    pub const E2P_DATA: usize = 0xB4;
}

/// HW_CFG register bits
pub mod hw_cfg {
    /// Software Reset (self-clearing)
    pub const SRST: u32 = 1 << 0;
    /// TX FIFO Size field (bits 19:16), value = KB
    pub const TX_FIF_SZ_SHIFT: u32 = 16;
    pub const TX_FIF_SZ_MASK: u32 = 0xF << TX_FIF_SZ_SHIFT;
    /// Must Be One
    pub const MBO: u32 = 1 << 20;
}

/// TX_CFG register bits
pub mod tx_cfg {
    /// TX Enable
    pub const TX_ON: u32 = 1 << 1;
    /// Stop TX
    pub const STOP_TX: u32 = 1 << 0;
    /// TX Status FIFO Clear
    pub const TXS_DUMP: u32 = 1 << 15;
    /// TX Data FIFO Clear
    pub const TXD_DUMP: u32 = 1 << 14;
}

/// RX_CFG register bits
pub mod rx_cfg {
    /// RX Dump (clear RX FIFO)
    pub const RX_DUMP: u32 = 1 << 15;
    /// DMA Counter End Alignment (bits 9:8)
    pub const RXDOFF_SHIFT: u32 = 8;
}

/// RX_DP_CTRL register bits
pub mod rx_dp_ctrl {
    /// Fast Forward RX packet (discard current packet)
    pub const RX_FFWD: u32 = 1 << 31;
}

/// IRQ_CFG register bits
pub mod irq_cfg {
    /// Master IRQ Enable
    pub const IRQ_EN: u32 = 1 << 8;
    /// IRQ Polarity (1 = active high)
    pub const IRQ_POL: u32 = 1 << 4;
    /// IRQ Type (1 = push-pull, 0 = open-drain)
    pub const IRQ_TYPE: u32 = 1 << 0;
    /// Typical configuration value for QEMU
    pub const DEFAULT: u32 = 0x2200_0111;
}

/// INT_STS and INT_EN register bits
pub mod int {
    /// RX Status FIFO Level
    pub const RSFL: u32 = 1 << 3;
    /// RX Status FIFO Full
    pub const RSFF: u32 = 1 << 4;
    /// RX Dropped Frame
    pub const RXDF: u32 = 1 << 6;
    /// TX Status FIFO Level
    pub const TSFL: u32 = 1 << 7;
    /// TX Status FIFO Full
    pub const TSFF: u32 = 1 << 8;
    /// TX Data Available
    pub const TDFA: u32 = 1 << 9;
    /// TX Data FIFO Overrun
    pub const TDFO: u32 = 1 << 10;
    /// Transmit Error
    pub const TXE: u32 = 1 << 13;
    /// Receive Error
    pub const RXE: u32 = 1 << 14;
    /// PHY Interrupt
    pub const PHY_INT: u32 = 1 << 18;
    /// Software Interrupt
    pub const SW_INT: u32 = 1 << 31;
}

/// RX_FIFO_INF register fields
pub mod rx_fifo_inf {
    /// RX Data FIFO Used Space (bytes) - bits 15:0
    pub const RXDUSED_MASK: u32 = 0xFFFF;
    /// RX Status FIFO Used (packets pending) - bits 23:16
    pub const RXSUSED_SHIFT: u32 = 16;
    pub const RXSUSED_MASK: u32 = 0xFF << RXSUSED_SHIFT;
}

/// TX_FIFO_INF register fields
pub mod tx_fifo_inf {
    /// TX Data FIFO Free Space (bytes) - bits 15:0
    pub const TXDFREE_MASK: u32 = 0xFFFF;
    /// TX Status FIFO Used - bits 23:16
    pub const TXSUSED_SHIFT: u32 = 16;
    pub const TXSUSED_MASK: u32 = 0xFF << TXSUSED_SHIFT;
}

/// RX_STAT_PORT register fields
pub mod rx_stat {
    /// Packet Length (bytes) - bits 29:16
    pub const PKT_LEN_SHIFT: u32 = 16;
    pub const PKT_LEN_MASK: u32 = 0x3FFF << PKT_LEN_SHIFT;
    /// Error Status (bit 15)
    pub const ES: u32 = 1 << 15;
    /// Broadcast Frame (bit 13)
    pub const BROADCAST: u32 = 1 << 13;
    /// Length Error (bit 12)
    pub const LEN_ERR: u32 = 1 << 12;
    /// Runt Frame (bit 11)
    pub const RUNT: u32 = 1 << 11;
    /// Multicast Frame (bit 10)
    pub const MULTICAST: u32 = 1 << 10;
    /// Frame Too Long (bit 7)
    pub const TOO_LONG: u32 = 1 << 7;
    /// Collision Seen (bit 6)
    pub const COLL: u32 = 1 << 6;
    /// Frame Type (1 = Ethernet II) (bit 5)
    pub const FRAME_TYPE: u32 = 1 << 5;
    /// Receive Watchdog Timeout (bit 4)
    pub const WDOG: u32 = 1 << 4;
    /// MII Error (bit 3)
    pub const MII_ERR: u32 = 1 << 3;
    /// Dribbling Bit (bit 2)
    pub const DRIBBLE: u32 = 1 << 2;
    /// CRC Error (bit 1)
    pub const CRC_ERR: u32 = 1 << 1;
}

/// TX Command A format (first word written to TX_DATA_PORT)
pub mod tx_cmd_a {
    /// First Segment flag
    pub const FIRST_SEG: u32 = 1 << 13;
    /// Last Segment flag
    pub const LAST_SEG: u32 = 1 << 12;
    /// Buffer Size (bytes) - bits 10:0
    pub const BUF_SIZE_MASK: u32 = 0x7FF;
}

/// TX Command B format (second word written to TX_DATA_PORT)
pub mod tx_cmd_b {
    /// Packet Tag (bits 31:16) - for software use
    pub const PKT_TAG_SHIFT: u32 = 16;
    /// Packet Length (bits 10:0)
    pub const PKT_LEN_MASK: u32 = 0x7FF;
}

/// MAC CSR Command register bits
pub mod mac_csr_cmd {
    /// CSR Busy (self-clearing when done)
    pub const BUSY: u32 = 1 << 31;
    /// Read/Write (1 = read, 0 = write)
    pub const READ: u32 = 1 << 30;
    /// CSR Address - bits 7:0
    pub const ADDR_MASK: u32 = 0xFF;
}

/// MAC CSR register indices (accessed via MAC_CSR_CMD/MAC_CSR_DATA)
pub mod mac_csr {
    /// MAC Control Register
    pub const MAC_CR: u8 = 1;
    /// MAC Address High
    pub const ADDRH: u8 = 2;
    /// MAC Address Low
    pub const ADDRL: u8 = 3;
    /// Hash Table High
    pub const HASHH: u8 = 4;
    /// Hash Table Low
    pub const HASHL: u8 = 5;
    /// MII Access
    pub const MII_ACC: u8 = 6;
    /// MII Data
    pub const MII_DATA: u8 = 7;
    /// Flow Control
    pub const FLOW: u8 = 8;
    /// VLAN1 Tag
    pub const VLAN1: u8 = 9;
    /// VLAN2 Tag
    pub const VLAN2: u8 = 10;
    /// Wake-up Frame Filter
    pub const WUFF: u8 = 11;
    /// Wake-up Control and Status
    pub const WUCSR: u8 = 12;
}

/// MAC Control Register (MAC_CR) bits
pub mod mac_cr {
    /// Receive All
    pub const RXALL: u32 = 1 << 31;
    /// Hash/Perfect Filtering
    pub const HPFILT: u32 = 1 << 13;
    /// Receive Own Transmissions
    pub const RCVOWN: u32 = 1 << 23;
    /// Loopback Mode
    pub const LOOPBK: u32 = 1 << 21;
    /// Full Duplex
    pub const FDPX: u32 = 1 << 20;
    /// Pass All Multicast
    pub const MCPAS: u32 = 1 << 19;
    /// Promiscuous Mode
    pub const PRMS: u32 = 1 << 18;
    /// Inverse Filtering
    pub const INVFILT: u32 = 1 << 17;
    /// Pass Bad Frames
    pub const PASSBAD: u32 = 1 << 16;
    /// Hash Only Filtering
    pub const HO: u32 = 1 << 15;
    /// Disable Broadcast Frames
    pub const BCAST: u32 = 1 << 11;
    /// Disable Retry
    pub const DISRTY: u32 = 1 << 10;
    /// Automatic Pad Stripping
    pub const PADSTR: u32 = 1 << 8;
    /// Deferral Check
    pub const DFCHK: u32 = 1 << 5;
    /// TX Enable
    pub const TXEN: u32 = 1 << 3;
    /// RX Enable
    pub const RXEN: u32 = 1 << 2;
}

/// MII Access Register bits
pub mod mii_acc {
    /// MII Busy
    pub const BUSY: u32 = 1 << 0;
    /// MII Write
    pub const WRITE: u32 = 1 << 1;
    /// PHY Address shift (bits 15:11)
    pub const PHY_ADDR_SHIFT: u32 = 11;
    /// MII Register Index shift (bits 10:6)
    pub const REG_ADDR_SHIFT: u32 = 6;
}

/// Standard PHY register indices
pub mod phy {
    /// PHY Address (usually 1 for internal PHY)
    pub const ADDR: u8 = 1;

    /// Basic Control Register
    pub const BMCR: u8 = 0;
    /// Basic Status Register
    pub const BMSR: u8 = 1;
    /// PHY Identifier 1
    pub const PHYID1: u8 = 2;
    /// PHY Identifier 2
    pub const PHYID2: u8 = 3;
    /// Auto-Negotiation Advertisement
    pub const ANAR: u8 = 4;
    /// Auto-Negotiation Link Partner Ability
    pub const ANLPAR: u8 = 5;
    /// Auto-Negotiation Expansion
    pub const ANER: u8 = 6;
}

/// PHY Basic Control Register (BMCR) bits
pub mod bmcr {
    /// PHY Reset (self-clearing)
    pub const RESET: u32 = 1 << 15;
    /// Loopback
    pub const LOOPBACK: u32 = 1 << 14;
    /// Speed Select (1 = 100Mbps, 0 = 10Mbps)
    pub const SPEED100: u32 = 1 << 13;
    /// Auto-Negotiation Enable
    pub const ANENABLE: u32 = 1 << 12;
    /// Power Down
    pub const PDOWN: u32 = 1 << 11;
    /// Isolate
    pub const ISOLATE: u32 = 1 << 10;
    /// Restart Auto-Negotiation
    pub const ANRESTART: u32 = 1 << 9;
    /// Duplex Mode (1 = full, 0 = half)
    pub const FULLDPLX: u32 = 1 << 8;
}

/// PHY Basic Status Register (BMSR) bits
pub mod bmsr {
    /// 100BASE-TX Full Duplex
    pub const CAP_100FULL: u32 = 1 << 14;
    /// 100BASE-TX Half Duplex
    pub const CAP_100HALF: u32 = 1 << 13;
    /// 10BASE-T Full Duplex
    pub const CAP_10FULL: u32 = 1 << 12;
    /// 10BASE-T Half Duplex
    pub const CAP_10HALF: u32 = 1 << 11;
    /// Auto-Negotiation Complete
    pub const ANEGCOMPLETE: u32 = 1 << 5;
    /// Remote Fault
    pub const RFAULT: u32 = 1 << 4;
    /// Auto-Negotiation Ability
    pub const ANEGCAPABLE: u32 = 1 << 3;
    /// Link Status (1 = link up)
    pub const LSTATUS: u32 = 1 << 2;
    /// Jabber Detect
    pub const JABBER: u32 = 1 << 1;
    /// Extended Capability
    pub const ERCAP: u32 = 1 << 0;
}

/// Auto-Negotiation Advertisement Register bits
pub mod anar {
    /// Asymmetric Pause
    pub const PAUSE_ASYM: u32 = 1 << 11;
    /// Pause
    pub const PAUSE: u32 = 1 << 10;
    /// 100BASE-TX Full Duplex
    pub const ADV_100FULL: u32 = 1 << 8;
    /// 100BASE-TX Half Duplex
    pub const ADV_100HALF: u32 = 1 << 7;
    /// 10BASE-T Full Duplex
    pub const ADV_10FULL: u32 = 1 << 6;
    /// 10BASE-T Half Duplex
    pub const ADV_10HALF: u32 = 1 << 5;
    /// Selector Field (IEEE 802.3)
    pub const SELECTOR: u32 = 0x01;
    /// All capabilities (10/100 half/full + pause)
    pub const ALL_CAPS: u32 = 0xDE0 | SELECTOR;
}

/// Known device IDs (upper 16 bits of ID_REV)
pub mod device_id {
    /// SMSC LAN9220
    pub const LAN9220: u16 = 0x9220;
    /// SMSC LAN9118 (as emulated by QEMU)
    pub const LAN9118: u16 = 0x0118;
}
