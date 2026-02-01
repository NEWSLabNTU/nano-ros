//! QEMU Listener - zenoh-pico Subscriber for MPS2-AN385
//!
//! This example demonstrates zenoh-pico subscribing with the LAN9118 Ethernet
//! controller in QEMU. It connects to a zenohd router and receives messages.
//!
//! # Network Configuration
//!
//! - Device IP: 192.0.2.11/24
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
//!        --tap tap-qemu1 \
//!        --ip 192.0.2.11 \
//!        --binary target/thumbv7m-none-eabi/release/qemu-rs-listener
//!    ```

#![no_std]
#![no_main]

use core::ffi::c_void;
use core::ptr;
use core::sync::atomic::{AtomicU32, Ordering};

use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use panic_semihosting as _;

use lan9118_smoltcp::{Config, Lan9118, MPS2_AN385_BASE};
use qemu_rs_common::{clock, SmoltcpZenohBridge};

use smoltcp::{
    iface::{Config as IfaceConfig, Interface, SocketSet},
    socket::tcp::{Socket as TcpSocket, SocketBuffer as TcpSocketBuffer},
    wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address},
};

// ============================================================================
// zenoh-pico Shim FFI
// ============================================================================

// Error codes
const ZENOH_SHIM_OK: i32 = 0;

/// Subscriber callback function type
/// (keyexpr, data, len, attachment, att_len, ctx)
type ShimCallback =
    Option<unsafe extern "C" fn(*const i8, *const u8, usize, *const u8, usize, *mut c_void)>;

extern "C" {
    fn zenoh_shim_init(locator: *const i8) -> i32;
    fn zenoh_shim_open() -> i32;
    fn zenoh_shim_close() -> i32;
    fn zenoh_shim_is_open() -> i32;
    fn zenoh_shim_declare_subscriber(
        keyexpr: *const i8,
        callback: ShimCallback,
        ctx: *mut c_void,
    ) -> i32;
    fn zenoh_shim_undeclare_subscriber(handle: i32) -> i32;
    fn zenoh_shim_spin_once(timeout_ms: u32) -> i32;
}

// ============================================================================
// Network Configuration
// ============================================================================

/// Device MAC address (locally administered, based on TAP interface 1)
const MAC_ADDRESS: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];

/// Device IP address (static)
const IP_ADDRESS: Ipv4Address = Ipv4Address::new(192, 0, 2, 11);

/// Network prefix length
const IP_PREFIX: u8 = 24;

/// Default gateway (host bridge)
const GATEWAY: Ipv4Address = Ipv4Address::new(192, 0, 2, 1);

/// Zenoh router locator (null-terminated C string)
const ZENOH_LOCATOR: &[u8] = b"tcp/192.0.2.1:7447\0";

/// Topic to subscribe to (null-terminated C string)
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
unsafe extern "C" fn subscriber_callback(
    _keyexpr: *const i8,
    data: *const u8,
    len: usize,
    _attachment: *const u8,
    _att_len: usize,
    _ctx: *mut c_void,
) {
    // Copy message to buffer
    let copy_len = len.min(MSG_BUFFER_SIZE);
    ptr::copy_nonoverlapping(data, MSG_BUFFER.as_mut_ptr(), copy_len);
    MSG_LEN = copy_len;

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
    hprintln!("  QEMU Listener - zenoh-pico Subscriber");
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

    // Create socket set with pre-allocated TCP sockets
    #[allow(static_mut_refs)]
    let mut sockets = unsafe { SocketSet::new(&mut SOCKET_STORAGE[..]) };

    // Create TCP sockets for zenoh-pico
    #[allow(static_mut_refs)]
    unsafe {
        let tcp0 = TcpSocket::new(
            TcpSocketBuffer::new(&mut TCP_RX_BUFFER_0[..]),
            TcpSocketBuffer::new(&mut TCP_TX_BUFFER_0[..]),
        );
        sockets.add(tcp0);

        let tcp1 = TcpSocket::new(
            TcpSocketBuffer::new(&mut TCP_RX_BUFFER_1[..]),
            TcpSocketBuffer::new(&mut TCP_TX_BUFFER_1[..]),
        );
        sockets.add(tcp1);
    }

    // Store pointers for poll callback
    unsafe {
        IFACE_PTR = &mut iface as *mut _;
        SOCKETS_PTR = &mut sockets as *mut _;
        ETH_PTR = &mut eth as *mut _;
    }

    // Initialize the zenoh-pico bridge
    hprintln!("");
    hprintln!("Initializing zenoh-pico bridge...");
    SmoltcpZenohBridge::init();

    // Register our poll callback
    extern "C" {
        fn smoltcp_set_poll_callback(cb: Option<unsafe extern "C" fn()>);
    }
    unsafe {
        smoltcp_set_poll_callback(Some(smoltcp_network_poll));
    }

    // Initialize zenoh session
    hprintln!("");
    hprintln!("Connecting to zenoh router at tcp/192.0.2.1:7447...");

    let ret = unsafe { zenoh_shim_init(ZENOH_LOCATOR.as_ptr() as *const i8) };
    if ret != ZENOH_SHIM_OK {
        hprintln!("zenoh_shim_init failed: {}", ret);
        exit_failure();
    }

    let ret = unsafe { zenoh_shim_open() };
    if ret != ZENOH_SHIM_OK {
        hprintln!("zenoh_shim_open failed: {}", ret);
        exit_failure();
    }

    // Verify session is open
    if unsafe { zenoh_shim_is_open() } == 0 {
        hprintln!("Session not open!");
        exit_failure();
    }

    hprintln!("Connected!");

    // Declare subscriber
    hprintln!("");
    hprintln!("Subscribing to topic: demo/qemu");

    let sub_handle = unsafe {
        zenoh_shim_declare_subscriber(
            TOPIC.as_ptr() as *const i8,
            Some(subscriber_callback),
            ptr::null_mut(),
        )
    };
    if sub_handle < 0 {
        hprintln!("zenoh_shim_declare_subscriber failed: {}", sub_handle);
        exit_failure();
    }

    hprintln!("Subscriber declared (handle: {})", sub_handle);
    hprintln!("");
    hprintln!("Waiting for messages...");

    // Receive messages
    let mut last_count = 0u32;
    let mut poll_count = 0u32;

    loop {
        // Poll to process network events
        unsafe {
            zenoh_shim_spin_once(10);
        }

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
    unsafe {
        zenoh_shim_undeclare_subscriber(sub_handle);
        zenoh_shim_close();
    }

    hprintln!("");
    hprintln!("========================================");
    hprintln!("  Test Complete: {} messages received", last_count);
    hprintln!("========================================");

    exit_success();
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
