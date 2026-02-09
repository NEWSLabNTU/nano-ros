//! Pin configurations for common STM32F4 development boards
//!
//! Ethernet on STM32F4 uses RMII mode by default with the following signals:
//!
//! | Signal   | Description             |
//! |----------|-------------------------|
//! | REF_CLK  | 50 MHz reference clock  |
//! | CRS_DV   | Carrier sense / data valid |
//! | TX_EN    | Transmit enable         |
//! | TXD0     | Transmit data 0         |
//! | TXD1     | Transmit data 1         |
//! | RXD0     | Receive data 0          |
//! | RXD1     | Receive data 1          |
//! | MDC      | Management clock        |
//! | MDIO     | Management data         |

/// Pin configuration preset for STM32F4 boards
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinConfig {
    /// NUCLEO-F429ZI board with integrated Ethernet PHY
    ///
    /// RMII Pins:
    /// - REF_CLK: PA1
    /// - CRS_DV: PA7
    /// - TX_EN: PG11
    /// - TXD0: PG13
    /// - TXD1: PB13
    /// - RXD0: PC4
    /// - RXD1: PC5
    /// - MDC: PC1
    /// - MDIO: PA2
    NucleoF429ZI,

    /// STM32F4-Discovery board with external Ethernet PHY
    ///
    /// Note: Discovery doesn't have built-in Ethernet.
    /// This assumes standard pin mapping for external PHY.
    ///
    /// RMII Pins:
    /// - REF_CLK: PA1
    /// - CRS_DV: PA7
    /// - TX_EN: PB11
    /// - TXD0: PB12
    /// - TXD1: PB13
    /// - RXD0: PC4
    /// - RXD1: PC5
    /// - MDC: PC1
    /// - MDIO: PA2
    DiscoveryF407,

    /// STM32F4-Discovery alternate (using PG pins for TX)
    ///
    /// RMII Pins:
    /// - REF_CLK: PA1
    /// - CRS_DV: PA7
    /// - TX_EN: PG11
    /// - TXD0: PG13
    /// - TXD1: PG14
    /// - RXD0: PC4
    /// - RXD1: PC5
    /// - MDC: PC1
    /// - MDIO: PA2
    DiscoveryF407Alt,
}
