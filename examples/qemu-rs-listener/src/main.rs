//! QEMU Listener - zenoh-pico Subscriber for MPS2-AN385
//!
//! Simplified example using nano-ros-baremetal high-level API.

#![no_std]
#![no_main]

use core::ffi::c_void;
use core::ptr;
use core::sync::atomic::{AtomicU32, Ordering};

use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use panic_semihosting as _;

use nano_ros_baremetal::platform::qemu_mps2::{self, exit_failure, exit_success};
use nano_ros_baremetal::{create_interface, create_socket_set, BaremetalNode, NodeConfig};

// ============================================================================
// Network Configuration
// ============================================================================

/// Device MAC address (locally administered, based on TAP interface 1)
const MAC_ADDRESS: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];

// Docker mode: QEMU runs inside container with NAT to Docker network
#[cfg(feature = "docker")]
mod net_config {
    /// Device IP address (static) - internal container network
    /// NOTE: Must be different from talker (192.168.100.10) to avoid zenoh ID collisions
    pub const IP_ADDRESS: [u8; 4] = [192, 168, 100, 11];
    /// Default gateway (container bridge with NAT)
    pub const GATEWAY: [u8; 4] = [192, 168, 100, 1];
    /// Zenoh router locator (zenohd container on Docker network)
    pub const ZENOH_LOCATOR: &[u8] = b"tcp/172.20.0.2:7447\0";
}

// Manual mode: QEMU connects directly to host TAP bridge
#[cfg(not(feature = "docker"))]
mod net_config {
    /// Device IP address (static)
    pub const IP_ADDRESS: [u8; 4] = [192, 0, 2, 11];
    /// Default gateway (host bridge)
    pub const GATEWAY: [u8; 4] = [192, 0, 2, 1];
    /// Zenoh router locator (zenohd on host)
    pub const ZENOH_LOCATOR: &[u8] = b"tcp/192.0.2.1:7447\0";
}

use net_config::{GATEWAY, IP_ADDRESS, ZENOH_LOCATOR};

/// Topic to subscribe to
const TOPIC: &[u8] = b"demo/qemu\0";

// ============================================================================
// Subscriber Callback State
// ============================================================================

/// Message buffer for storing received messages
const MSG_BUFFER_SIZE: usize = 256;
static mut MSG_BUFFER: [u8; MSG_BUFFER_SIZE] = [0u8; MSG_BUFFER_SIZE];
static mut MSG_LEN: usize = 0;

/// Message count (atomic for safe callback access)
static MSG_COUNT: AtomicU32 = AtomicU32::new(0);

/// Subscriber callback - called when a message is received
#[allow(static_mut_refs)]
extern "C" fn subscriber_callback(data: *const u8, len: usize, _ctx: *mut c_void) {
    // Copy message to buffer (using unsafe block for static mut access)
    unsafe {
        let copy_len = len.min(MSG_BUFFER_SIZE);
        ptr::copy_nonoverlapping(data, MSG_BUFFER.as_mut_ptr(), copy_len);
        MSG_LEN = copy_len;
    }

    // Increment message count
    MSG_COUNT.fetch_add(1, Ordering::SeqCst);
}

// ============================================================================
// Main Entry Point
// ============================================================================

#[entry]
fn main() -> ! {
    hprintln!("");
    hprintln!("========================================");
    hprintln!("  QEMU Listener - nano-ros-baremetal");
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

    // Declare subscriber
    hprintln!("");
    hprintln!("Subscribing to topic: demo/qemu");

    let _subscriber = match unsafe {
        node.create_subscriber_raw(TOPIC, Some(subscriber_callback), ptr::null_mut())
    } {
        Ok(s) => s,
        Err(e) => {
            hprintln!("Error creating subscriber: {:?}", e);
            exit_failure();
        }
    };

    hprintln!("Subscriber declared");
    hprintln!("");
    hprintln!("Waiting for messages...");

    // Receive messages
    let mut last_count = 0u32;
    let mut poll_count = 0u32;

    loop {
        // Poll to process network events
        node.spin_once(10);

        // Check for new messages
        let current_count = MSG_COUNT.load(Ordering::SeqCst);
        if current_count > last_count {
            // New message received
            #[allow(static_mut_refs)]
            unsafe {
                let msg = &MSG_BUFFER[..MSG_LEN];
                if let Ok(s) = core::str::from_utf8(msg) {
                    hprintln!("Received [{}]: {}", current_count, s);
                } else {
                    hprintln!("Received [{}]: <{} bytes binary>", current_count, MSG_LEN);
                }
            }
            last_count = current_count;

            // Exit after receiving 10 messages
            if current_count >= 10 {
                hprintln!("");
                hprintln!("Received 10 messages, exiting.");
                break;
            }
        }

        poll_count += 1;

        // Safety timeout (exit after a long time with no messages)
        if poll_count > 100000 {
            hprintln!("");
            hprintln!("Timeout waiting for messages.");
            break;
        }
    }

    // Cleanup
    hprintln!("");
    hprintln!("Cleaning up...");
    node.shutdown();

    hprintln!("");
    hprintln!("========================================");
    hprintln!("  Test Complete: {} messages received", last_count);
    hprintln!("========================================");

    exit_success();
}
