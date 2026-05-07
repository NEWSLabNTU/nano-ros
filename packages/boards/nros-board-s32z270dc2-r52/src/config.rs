//! Board configuration for NXP S32Z270-DC RTU0 Cortex-R52.
//!
//! Defaults match the eval board's stock ENETC port wiring + the
//! 192.0.3.0/24 subnet the rest of the nano-ros QEMU/FVP fleet uses,
//! so a single host bridge can run cross-board DDS interop trials
//! without subnet juggling.

#[derive(Clone)]
pub struct Config {
    #[cfg(feature = "ethernet")]
    pub mac: [u8; 6],
    #[cfg(feature = "ethernet")]
    pub ip: [u8; 4],
    #[cfg(feature = "ethernet")]
    pub prefix: u8,
    #[cfg(feature = "ethernet")]
    pub gateway: [u8; 4],

    pub locator: &'static str,
    pub domain_id: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            #[cfg(feature = "ethernet")]
            mac: [0x02, 0x00, 0x00, 0x00, 0x52, 0x01],
            #[cfg(feature = "ethernet")]
            ip: [192, 0, 3, 30],
            #[cfg(feature = "ethernet")]
            prefix: 24,
            #[cfg(feature = "ethernet")]
            gateway: [192, 0, 3, 1],
            locator: "tcp/192.0.3.1:7447",
            domain_id: 0,
        }
    }
}

impl Config {
    pub fn with_ip(mut self, ip: [u8; 4]) -> Self {
        #[cfg(feature = "ethernet")]
        {
            self.ip = ip;
        }
        let _ = ip;
        self
    }

    pub fn with_locator(mut self, locator: &'static str) -> Self {
        self.locator = locator;
        self
    }

    pub fn with_domain_id(mut self, domain_id: u32) -> Self {
        self.domain_id = domain_id;
        self
    }
}
