//! QEMU Talker - zenoh-pico Publisher for MPS2-AN385
//!
//! This example demonstrates zenoh-pico publishing with the LAN9118 Ethernet
//! controller in QEMU. It connects to a zenohd router and publishes messages.
//!
//! # Network Configuration
//!
//! - Device IP: 192.0.2.10/24
//! - Gateway: 192.0.2.1 (host bridge)
//! - Zenoh router: 192.0.2.1:7447 (zenohd on host)
//!
//! # Running
//!
//! 1. Setup network:
//!    ```bash
//!    sudo ./scripts/qemu/setup-network.sh
//!    ```
//!
//! 2. Start zenohd on host:
//!    ```bash
//!    zenohd --listen tcp/0.0.0.0:7447
//!    ```
//!
//! 3. Build zenoh-pico library:
//!    ```bash
//!    ./scripts/qemu/build-zenoh-pico.sh
//!    ```
//!
//! 4. Run QEMU with networking:
//!    ```bash
//!    ./scripts/qemu/launch-mps2-an385.sh \
//!        --tap tap-qemu0 \
//!        --ip 192.0.2.10 \
//!        --binary target/thumbv7m-none-eabi/release/qemu-rs-talker
//!    ```

#![no_std]
#![no_main]

use core::ptr;

use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use panic_semihosting as _;

use lan9118_smoltcp::{Config, Lan9118, MPS2_AN385_BASE};
use qemu_rs_common::{clock, SmoltcpZenohBridge};

use smoltcp::{
    iface::{Config as IfaceConfig, Interface, SocketSet},
    socket::tcp::{Socket as TcpSocket, SocketBuffer as TcpSocketBuffer},
    wire::{EthernetAddress, IpAddress, IpCidr},
};

// ============================================================================
// zenoh-pico Shim FFI
// ============================================================================

// Error codes
const ZENOH_SHIM_OK: i32 = 0;

extern "C" {
    fn zenoh_shim_init(locator: *const i8) -> i32;
    fn zenoh_shim_open() -> i32;
    fn zenoh_shim_close() -> i32;
    fn zenoh_shim_is_open() -> i32;
    fn zenoh_shim_declare_publisher(keyexpr: *const i8) -> i32;
    fn zenoh_shim_publish(handle: i32, data: *const u8, len: usize) -> i32;
    fn zenoh_shim_undeclare_publisher(handle: i32) -> i32;
    fn zenoh_shim_spin_once(timeout_ms: u32) -> i32;
}

// ============================================================================
// Network Configuration
// ============================================================================

/// Device MAC address (locally administered)
const MAC_ADDRESS: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x00];

// Docker mode: QEMU runs inside container with NAT to Docker network
#[cfg(feature = "docker")]
mod net_config {
    use smoltcp::wire::Ipv4Address;
    /// Device IP address (static) - internal container network
    pub const IP_ADDRESS: Ipv4Address = Ipv4Address::new(192, 168, 100, 10);
    /// Network prefix length
    pub const IP_PREFIX: u8 = 24;
    /// Default gateway (container bridge with NAT)
    pub const GATEWAY: Ipv4Address = Ipv4Address::new(192, 168, 100, 1);
    /// Zenoh router locator (zenohd container on Docker network)
    pub const ZENOH_LOCATOR: &[u8] = b"tcp/172.20.0.2:7447\0";
}

// Manual mode: QEMU connects directly to host TAP bridge
#[cfg(not(feature = "docker"))]
mod net_config {
    use smoltcp::wire::Ipv4Address;
    /// Device IP address (static)
    pub const IP_ADDRESS: Ipv4Address = Ipv4Address::new(192, 0, 2, 10);
    /// Network prefix length
    pub const IP_PREFIX: u8 = 24;
    /// Default gateway (host bridge)
    pub const GATEWAY: Ipv4Address = Ipv4Address::new(192, 0, 2, 1);
    /// Zenoh router locator (zenohd on host)
    pub const ZENOH_LOCATOR: &[u8] = b"tcp/192.0.2.1:7447\0";
}

use net_config::{GATEWAY, IP_ADDRESS, IP_PREFIX, ZENOH_LOCATOR};

/// Topic to publish on (null-terminated C string)
const TOPIC: &[u8] = b"demo/qemu\0";

// ============================================================================
// Static Buffer Allocation
// ============================================================================

/// Maximum number of sockets
const MAX_SOCKETS: usize = 4;

/// TCP socket buffer size
const TCP_BUFFER_SIZE: usize = 2048;

// Socket storage
static mut SOCKET_STORAGE: [smoltcp::iface::SocketStorage<'static>; MAX_SOCKETS] =
    [smoltcp::iface::SocketStorage::EMPTY; MAX_SOCKETS];

// TCP buffers for each socket
static mut TCP_RX_BUFFER_0: [u8; TCP_BUFFER_SIZE] = [0u8; TCP_BUFFER_SIZE];
static mut TCP_TX_BUFFER_0: [u8; TCP_BUFFER_SIZE] = [0u8; TCP_BUFFER_SIZE];
static mut TCP_RX_BUFFER_1: [u8; TCP_BUFFER_SIZE] = [0u8; TCP_BUFFER_SIZE];
static mut TCP_TX_BUFFER_1: [u8; TCP_BUFFER_SIZE] = [0u8; TCP_BUFFER_SIZE];

// ============================================================================
// Global State for Poll Callback
// ============================================================================

// These need to be accessible from the poll callback
static mut IFACE_PTR: *mut Interface = ptr::null_mut();
static mut SOCKETS_PTR: *mut SocketSet<'static> = ptr::null_mut();
static mut ETH_PTR: *mut Lan9118 = ptr::null_mut();

/// Poll callback called by zenoh-pico to drive the network stack
///
/// # Safety
///
/// This function must only be called after the global pointers (IFACE_PTR,
/// SOCKETS_PTR, ETH_PTR) have been initialized in main(). It accesses
/// mutable statics and must not be called concurrently.
#[no_mangle]
pub unsafe extern "C" fn smoltcp_network_poll() {
    if IFACE_PTR.is_null() || SOCKETS_PTR.is_null() || ETH_PTR.is_null() {
        return;
    }

    let iface = &mut *IFACE_PTR;
    let sockets = &mut *SOCKETS_PTR;
    let eth = &mut *ETH_PTR;

    // Poll the smoltcp interface
    SmoltcpZenohBridge::poll(iface, eth, sockets);

    // Advance the clock a little bit for each poll
    clock::advance_clock_ms(1);
}

// ============================================================================
// Main Entry Point
// ============================================================================

#[entry]
fn main() -> ! {
    hprintln!("");
    hprintln!("========================================");
    hprintln!("  QEMU Talker - zenoh-pico Publisher");
    hprintln!("========================================");
    hprintln!("");

    // Initialize Ethernet driver
    hprintln!("Initializing LAN9118 Ethernet...");
    let config = Config {
        base_addr: MPS2_AN385_BASE,
        mac_addr: MAC_ADDRESS,
    };

    let mut eth = match unsafe { Lan9118::new(config) } {
        Ok(e) => e,
        Err(e) => {
            hprintln!("Error creating driver: {:?}", e);
            exit_failure();
        }
    };

    if let Err(e) = eth.init() {
        hprintln!("Error initializing driver: {:?}", e);
        exit_failure();
    }

    hprintln!(
        "  MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        MAC_ADDRESS[0],
        MAC_ADDRESS[1],
        MAC_ADDRESS[2],
        MAC_ADDRESS[3],
        MAC_ADDRESS[4],
        MAC_ADDRESS[5]
    );

    // Create smoltcp interface
    hprintln!("");
    hprintln!("Creating smoltcp interface...");

    let mac_addr = EthernetAddress::from_bytes(&MAC_ADDRESS);
    let iface_config = IfaceConfig::new(mac_addr.into());
    let mut iface = Interface::new(iface_config, &mut eth, clock::now());

    // Configure IP address
    iface.update_ip_addrs(|addrs| {
        addrs
            .push(IpCidr::new(IpAddress::Ipv4(IP_ADDRESS), IP_PREFIX))
            .ok();
    });

    // Set default gateway
    iface
        .routes_mut()
        .add_default_ipv4_route(GATEWAY)
        .expect("Failed to add default route");

    hprintln!("  IP: {}", IP_ADDRESS);
    hprintln!("  Gateway: {}", GATEWAY);

    // Initialize the zenoh-pico bridge BEFORE registering sockets
    hprintln!("");
    hprintln!("Initializing zenoh-pico bridge...");
    SmoltcpZenohBridge::init();

    // Seed the RNG with a unique value based on IP address to avoid zenoh ID collisions
    extern "C" {
        fn smoltcp_seed_random(seed: u32);
    }
    let ip_seed = u32::from_be_bytes(IP_ADDRESS.octets());
    unsafe { smoltcp_seed_random(ip_seed) };

    // Create socket set with pre-allocated TCP sockets
    #[allow(static_mut_refs)]
    let mut sockets = unsafe { SocketSet::new(&mut SOCKET_STORAGE[..]) };

    // Register socket function from bridge
    extern "C" {
        fn smoltcp_register_socket(handle: usize) -> i32;
    }

    // Create TCP sockets for zenoh-pico and register them with the bridge
    #[allow(static_mut_refs)]
    unsafe {
        let tcp0 = TcpSocket::new(
            TcpSocketBuffer::new(&mut TCP_RX_BUFFER_0[..]),
            TcpSocketBuffer::new(&mut TCP_TX_BUFFER_0[..]),
        );
        let handle0 = sockets.add(tcp0);
        // Convert SocketHandle to usize (they have the same layout)
        smoltcp_register_socket(core::mem::transmute::<smoltcp::iface::SocketHandle, usize>(
            handle0,
        ));

        let tcp1 = TcpSocket::new(
            TcpSocketBuffer::new(&mut TCP_RX_BUFFER_1[..]),
            TcpSocketBuffer::new(&mut TCP_TX_BUFFER_1[..]),
        );
        let handle1 = sockets.add(tcp1);
        smoltcp_register_socket(core::mem::transmute::<smoltcp::iface::SocketHandle, usize>(
            handle1,
        ));
    }

    // Store pointers for poll callback
    unsafe {
        IFACE_PTR = &mut iface as *mut _;
        SOCKETS_PTR = &mut sockets as *mut _;
        ETH_PTR = &mut eth as *mut _;
    }

    // Register our poll callback
    extern "C" {
        fn smoltcp_set_poll_callback(cb: Option<unsafe extern "C" fn()>);
    }
    unsafe {
        smoltcp_set_poll_callback(Some(smoltcp_network_poll));
    }

    // Initialize zenoh session
    hprintln!("");
    // Print zenoh locator (strip null terminator for display)
    let locator_str =
        core::str::from_utf8(&ZENOH_LOCATOR[..ZENOH_LOCATOR.len() - 1]).unwrap_or("?");
    hprintln!("Connecting to zenoh router at {}...", locator_str);

    // Debug: Check poll callback and socket registration
    extern "C" {
        fn smoltcp_has_poll_callback() -> i32;
        fn smoltcp_get_poll_count() -> u32;
        fn smoltcp_get_socket_open_count() -> u32;
        fn smoltcp_get_socket_connect_count() -> u32;
    }

    hprintln!("  Callback registered: {}", unsafe {
        smoltcp_has_poll_callback()
    });
    hprintln!("  Socket open count: {}", unsafe {
        smoltcp_get_socket_open_count()
    });
    hprintln!("  Socket connect count: {}", unsafe {
        smoltcp_get_socket_connect_count()
    });

    let ret = unsafe { zenoh_shim_init(ZENOH_LOCATOR.as_ptr() as *const i8) };
    if ret != ZENOH_SHIM_OK {
        hprintln!("zenoh_shim_init failed: {}", ret);
        exit_failure();
    }
    hprintln!("  zenoh_shim_init OK");

    extern "C" {
        fn smoltcp_get_is_connected_check_count() -> u32;
        fn smoltcp_get_is_connected_true_count() -> u32;
    }

    extern "C" {
        fn smoltcp_get_send_count() -> u32;
        fn smoltcp_get_recv_count() -> u32;
        fn smoltcp_get_bytes_sent() -> u32;
        fn smoltcp_get_bytes_recv() -> u32;
        fn smoltcp_get_tcp_tx_count() -> u32;
        fn smoltcp_get_tcp_rx_count() -> u32;
        fn smoltcp_get_tcp_tx_bytes() -> u32;
        fn smoltcp_get_tcp_rx_bytes() -> u32;
    }

    hprintln!("  Calling zenoh_shim_open...");
    let ret = unsafe { zenoh_shim_open() };
    if ret != ZENOH_SHIM_OK {
        hprintln!("zenoh_shim_open failed: {}", ret);
        hprintln!("  Bridge poll count: {}", unsafe {
            smoltcp_get_poll_count()
        });
        hprintln!("  Socket open count: {}", unsafe {
            smoltcp_get_socket_open_count()
        });
        hprintln!("  Socket connect count: {}", unsafe {
            smoltcp_get_socket_connect_count()
        });
        hprintln!("  Is connected checks: {}", unsafe {
            smoltcp_get_is_connected_check_count()
        });
        hprintln!("  Is connected true: {}", unsafe {
            smoltcp_get_is_connected_true_count()
        });
        hprintln!(
            "  FFI send/recv: {}/{} ({}/{} bytes)",
            unsafe { smoltcp_get_send_count() },
            unsafe { smoltcp_get_recv_count() },
            unsafe { smoltcp_get_bytes_sent() },
            unsafe { smoltcp_get_bytes_recv() }
        );
        hprintln!(
            "  TCP tx/rx: {}/{} ({}/{} bytes)",
            unsafe { smoltcp_get_tcp_tx_count() },
            unsafe { smoltcp_get_tcp_rx_count() },
            unsafe { smoltcp_get_tcp_tx_bytes() },
            unsafe { smoltcp_get_tcp_rx_bytes() }
        );
        exit_failure();
    }

    // Verify session is open
    if unsafe { zenoh_shim_is_open() } == 0 {
        hprintln!("Session not open!");
        exit_failure();
    }

    hprintln!("Connected!");

    // Declare publisher
    hprintln!("");
    hprintln!("Declaring publisher on topic: demo/qemu");

    let pub_handle = unsafe { zenoh_shim_declare_publisher(TOPIC.as_ptr() as *const i8) };
    if pub_handle < 0 {
        hprintln!("zenoh_shim_declare_publisher failed: {}", pub_handle);
        exit_failure();
    }

    hprintln!("Publisher declared (handle: {})", pub_handle);

    // Publish messages
    hprintln!("");
    hprintln!("Publishing messages...");

    let mut count = 0u32;
    let mut msg_buf = [0u8; 64];

    loop {
        // Poll to process network events
        unsafe {
            zenoh_shim_spin_once(10);
        }

        // Publish a message every ~100 polls
        #[allow(clippy::manual_is_multiple_of)]
        if count % 100 == 0 {
            let msg_num = count / 100;
            if msg_num < 10 {
                // Format message: "Hello from QEMU #N"
                let msg = format_message(&mut msg_buf, msg_num);
                let msg_len = msg.len();

                let ret = unsafe { zenoh_shim_publish(pub_handle, msg.as_ptr(), msg_len) };
                if ret == ZENOH_SHIM_OK {
                    hprintln!("Published: {}", core::str::from_utf8(msg).unwrap_or("?"));
                } else {
                    hprintln!("Publish failed: {}", ret);
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
    unsafe {
        zenoh_shim_undeclare_publisher(pub_handle);
        zenoh_shim_close();
    }

    hprintln!("");
    hprintln!("========================================");
    hprintln!("  Test Complete");
    hprintln!("========================================");

    exit_success();
}

/// Format a message into the buffer
fn format_message(buf: &mut [u8], num: u32) -> &[u8] {
    // "Hello from QEMU #N"
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

fn exit_success() -> ! {
    cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_SUCCESS);
    loop {
        cortex_m::asm::wfi();
    }
}

fn exit_failure() -> ! {
    cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_FAILURE);
    loop {
        cortex_m::asm::wfi();
    }
}
