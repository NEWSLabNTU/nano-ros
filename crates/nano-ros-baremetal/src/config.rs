//! Node configuration types

/// Network and zenoh configuration for a bare-metal node
#[derive(Clone, Copy, Debug)]
pub struct NodeConfig<'a> {
    /// IPv4 address as 4 octets (e.g., [192, 168, 100, 10])
    pub ip: [u8; 4],
    /// Gateway address as 4 octets (e.g., [192, 168, 100, 1])
    pub gateway: [u8; 4],
    /// Network prefix length (e.g., 24 for /24)
    pub prefix: u8,
    /// Zenoh router locator (null-terminated, e.g., b"tcp/192.168.1.1:7447\0")
    pub zenoh_locator: &'a [u8],
}

impl<'a> NodeConfig<'a> {
    /// Create a new node configuration
    pub const fn new(ip: [u8; 4], gateway: [u8; 4], zenoh_locator: &'a [u8]) -> Self {
        Self {
            ip,
            gateway,
            prefix: 24,
            zenoh_locator,
        }
    }

    /// Set the network prefix length
    pub const fn with_prefix(mut self, prefix: u8) -> Self {
        self.prefix = prefix;
        self
    }
}
