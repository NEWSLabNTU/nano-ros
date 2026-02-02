//! QEMU MPS2-AN385 platform support
//!
//! This module provides the LAN9118 Ethernet driver initialization for
//! QEMU's MPS2-AN385 machine emulation.
//!
//! # Example
//!
//! ```ignore
//! use nano_ros_baremetal::platform::qemu_mps2;
//!
//! let eth = qemu_mps2::create_ethernet([0x02, 0x00, 0x00, 0x00, 0x00, 0x00])
//!     .expect("Failed to create Ethernet driver");
//! ```

use crate::error::{Error, Result};
use crate::node::EthernetDevice;

pub use lan9118_smoltcp::{Lan9118, MPS2_AN385_BASE};

/// Create and initialize the LAN9118 Ethernet driver
///
/// # Arguments
///
/// * `mac` - MAC address as 6 octets (should be locally administered, e.g., 0x02:...)
///
/// # Errors
///
/// Returns `Error::EthernetInit` if driver initialization fails.
pub fn create_ethernet(mac: [u8; 6]) -> Result<Lan9118> {
    use lan9118_smoltcp::Config;

    let config = Config {
        base_addr: MPS2_AN385_BASE,
        mac_addr: mac,
    };

    let mut eth = unsafe { Lan9118::new(config).map_err(|_| Error::EthernetInit)? };
    eth.init().map_err(|_| Error::EthernetInit)?;

    Ok(eth)
}

// Implement EthernetDevice for Lan9118
impl EthernetDevice for Lan9118 {
    fn mac_address(&self) -> [u8; 6] {
        Lan9118::mac_address(self)
    }
}

/// Exit QEMU with success status
pub fn exit_success() -> ! {
    cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_SUCCESS);
    #[allow(clippy::empty_loop)]
    loop {
        cortex_m::asm::wfi();
    }
}

/// Exit QEMU with failure status
pub fn exit_failure() -> ! {
    cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_FAILURE);
    #[allow(clippy::empty_loop)]
    loop {
        cortex_m::asm::wfi();
    }
}
