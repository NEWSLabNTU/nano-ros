//! QEMU Talker - zenoh-pico Publisher for MPS2-AN385
//!
//! Simplified example using nano-ros-baremetal high-level API.

#![no_std]
#![no_main]

use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use panic_semihosting as _;

use nano_ros_baremetal::platform::qemu_mps2::{self, exit_failure, exit_success};
use nano_ros_baremetal::{BaremetalNode, NodeConfig, create_interface, create_socket_set};

// ============================================================================
// Network Configuration
// ============================================================================

/// Device MAC address (locally administered)
const MAC_ADDRESS: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x00];

// Docker mode: QEMU runs inside container with NAT to Docker network
#[cfg(feature = "docker")]
mod net_config {
    /// Device IP address (static) - internal container network
    pub const IP_ADDRESS: [u8; 4] = [192, 168, 100, 10];
    /// Default gateway (container bridge with NAT)
    pub const GATEWAY: [u8; 4] = [192, 168, 100, 1];
    /// Zenoh router locator (zenohd container on Docker network)
    pub const ZENOH_LOCATOR: &[u8] = b"tcp/172.20.0.2:7447\0";
}

// Manual mode: QEMU connects directly to host TAP bridge
#[cfg(not(feature = "docker"))]
mod net_config {
    /// Device IP address (static)
    pub const IP_ADDRESS: [u8; 4] = [192, 0, 2, 10];
    /// Default gateway (host bridge)
    pub const GATEWAY: [u8; 4] = [192, 0, 2, 1];
    /// Zenoh router locator (zenohd on host)
    pub const ZENOH_LOCATOR: &[u8] = b"tcp/192.0.2.1:7447\0";
}

use net_config::{GATEWAY, IP_ADDRESS, ZENOH_LOCATOR};

/// Topic to publish on
const TOPIC: &[u8] = b"demo/qemu\0";

// ============================================================================
// Main Entry Point
// ============================================================================

#[entry]
fn main() -> ! {
    hprintln!("");
    hprintln!("========================================");
    hprintln!("  QEMU Talker - nano-ros-baremetal");
    hprintln!("========================================");
    hprintln!("");

    // Initialize Ethernet driver
    hprintln!("Initializing LAN9118 Ethernet...");
    let mut eth = match qemu_mps2::create_ethernet(MAC_ADDRESS) {
        Ok(e) => e,
        Err(e) => {
            hprintln!("Error creating Ethernet: {:?}", e);
            exit_failure();
        }
    };

    hprintln!(
        "  MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        MAC_ADDRESS[0],
        MAC_ADDRESS[1],
        MAC_ADDRESS[2],
        MAC_ADDRESS[3],
        MAC_ADDRESS[4],
        MAC_ADDRESS[5]
    );

    // Create smoltcp interface and socket set
    hprintln!("");
    hprintln!("Creating network interface...");
    let mut iface = create_interface(&mut eth);
    let mut sockets = unsafe { create_socket_set() };

    hprintln!(
        "  IP: {}.{}.{}.{}",
        IP_ADDRESS[0],
        IP_ADDRESS[1],
        IP_ADDRESS[2],
        IP_ADDRESS[3]
    );

    // Create bare-metal node
    hprintln!("");
    hprintln!("Connecting to zenoh router...");
    let config = NodeConfig::new(IP_ADDRESS, GATEWAY, ZENOH_LOCATOR);

    let mut node = match BaremetalNode::new(&mut eth, &mut iface, &mut sockets, config) {
        Ok(n) => n,
        Err(e) => {
            hprintln!("Error creating node: {:?}", e);
            exit_failure();
        }
    };

    hprintln!("Connected!");

    // Declare publisher
    hprintln!("");
    hprintln!("Declaring publisher on topic: demo/qemu");

    let publisher = match node.create_publisher(TOPIC) {
        Ok(p) => p,
        Err(e) => {
            hprintln!("Error creating publisher: {:?}", e);
            exit_failure();
        }
    };

    hprintln!("Publisher declared (handle: {})", publisher.handle());

    // Publish messages
    hprintln!("");
    hprintln!("Publishing messages...");

    let mut count = 0u32;
    let mut msg_buf = [0u8; 64];

    loop {
        // Poll to process network events
        node.spin_once(10);

        // Publish a message every ~100 polls
        #[allow(clippy::manual_is_multiple_of)]
        if count % 100 == 0 {
            let msg_num = count / 100;
            if msg_num < 10 {
                // Format message: "Hello from QEMU #N"
                let msg = format_message(&mut msg_buf, msg_num);

                if let Err(e) = publisher.publish(msg) {
                    hprintln!("Publish failed: {:?}", e);
                } else {
                    hprintln!("Published: {}", core::str::from_utf8(msg).unwrap_or("?"));
                }
            } else if msg_num == 10 {
                hprintln!("");
                hprintln!("Done publishing 10 messages.");
                break;
            }
        }

        count += 1;

        // Safety timeout
        if count > 10000 {
            hprintln!("Timeout!");
            break;
        }
    }

    // Cleanup
    hprintln!("");
    hprintln!("Cleaning up...");
    node.shutdown();

    hprintln!("");
    hprintln!("========================================");
    hprintln!("  Test Complete");
    hprintln!("========================================");

    exit_success();
}

/// Format a message into the buffer
fn format_message(buf: &mut [u8], num: u32) -> &[u8] {
    let prefix = b"Hello from QEMU #";
    let mut pos = 0;

    // Copy prefix
    for &b in prefix {
        if pos < buf.len() {
            buf[pos] = b;
            pos += 1;
        }
    }

    // Convert number to string
    if num == 0 {
        if pos < buf.len() {
            buf[pos] = b'0';
            pos += 1;
        }
    } else {
        let mut n = num;
        let mut digits = [0u8; 10];
        let mut digit_count = 0;

        while n > 0 {
            digits[digit_count] = b'0' + (n % 10) as u8;
            n /= 10;
            digit_count += 1;
        }

        for i in (0..digit_count).rev() {
            if pos < buf.len() {
                buf[pos] = digits[i];
                pos += 1;
            }
        }
    }

    &buf[..pos]
}
