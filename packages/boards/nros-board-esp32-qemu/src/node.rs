//! Platform initialization and `run()` entry point for ESP32-C3 QEMU.
//!
//! Uses `nros-smoltcp` for socket management and `openeth-smoltcp` for
//! Ethernet when the `ethernet` feature is enabled, or zenoh-pico's
//! built-in serial when the `serial` feature is enabled.

#[cfg(not(any(feature = "ethernet", feature = "serial")))]
compile_error!("Enable at least one transport: `ethernet` or `serial`");

// Phase 214.E.2 — at-most-one-transport guard.
#[cfg(all(feature = "ethernet", feature = "serial"))]
compile_error!("Pick exactly one transport: `ethernet` and `serial` are mutually exclusive");

use esp_hal::rng::Rng;

use nros_platform_esp32_qemu::random;

use crate::config::Config;

// NOTE: We intentionally do NOT import `type Result<T>` in this module.
// The `esp_println::println!` macro uses `?` internally which expands to
// `Result<(), core::fmt::Error>`. A `type Result<T>` alias here would shadow
// `core::result::Result` and cause "expected 1 generic argument but 2 supplied" errors.

fn network_identity_seed(config: &Config) -> u32 {
    let mut seed = 0x9e37_79b9u32;
    for byte in config.mac_addr.iter().chain(config.ip.iter()) {
        seed ^= u32::from(*byte);
        seed = seed.rotate_left(5).wrapping_mul(0x85eb_ca6b);
    }
    seed
}

// ---- Ethernet imports and static storage ----

#[cfg(feature = "ethernet")]
use core::mem::MaybeUninit;

#[cfg(feature = "ethernet")]
use nros_smoltcp::SmoltcpBridge;
#[cfg(feature = "ethernet")]
use openeth_smoltcp::OpenEth;
#[cfg(feature = "ethernet")]
use smoltcp::iface::{Interface, SocketSet};
#[cfg(feature = "ethernet")]
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};

// Static storage for network objects (initialized by init_hardware, must
// outlive the function call so set_network_state pointers remain valid).
#[cfg(feature = "ethernet")]
static mut ETH_DEVICE: MaybeUninit<OpenEth> = MaybeUninit::uninit();
#[cfg(feature = "ethernet")]
static mut NET_IFACE: MaybeUninit<Interface> = MaybeUninit::uninit();
#[cfg(feature = "ethernet")]
static mut NET_SOCKETS: MaybeUninit<SocketSet<'static>> = MaybeUninit::uninit();

/// Helper to create a socket set with pre-allocated storage
///
/// # Safety
///
/// Must be called at most once during board init. `nros_smoltcp::get_socket_storage`
/// hands out an aliasable `&'static mut [SocketStorage<'static>]`; calling this
/// twice would produce two mutable references to the same backing storage.
#[cfg(feature = "ethernet")]
unsafe fn create_socket_set() -> SocketSet<'static> {
    let storage = unsafe { nros_smoltcp::get_socket_storage() };
    SocketSet::new(&mut storage[..])
}

// ---- Ethernet init ----

/// Initialize Ethernet transport via OpenETH + smoltcp.
#[cfg(feature = "ethernet")]
#[allow(static_mut_refs)]
fn init_ethernet(config: &Config) {
    // Initialize OpenETH driver
    esp_println::println!("Initializing OpenETH...");
    let openeth_config = openeth_smoltcp::Config {
        base_addr: openeth_smoltcp::ESP32C3_BASE,
        mac_addr: config.mac_addr,
    };
    // Construct the driver directly in static storage. OpenEth::init() writes
    // the addresses of its internal DMA buffers (tx_buf/rx_buf, which live
    // inside the struct) into hardware TX/RX descriptors — so it MUST be
    // called after the struct has reached its final address. Calling init()
    // before the move left the descriptors pointing at stale stack memory,
    // causing QEMU to transmit all-zero frames.
    //
    // Issue #64 — use `new_in_place` rather than `OpenEth::new(...)` +
    // `ETH_DEVICE.write(...)`: the by-value `new` materialises an ~11 KB
    // `OpenEth` (tx_buf + rx_bufs[4] + rx_frame) on the stack, which overflows
    // the esp32-c3 ~18 KB stack into `.bss` and silently corrupts whatever lives
    // there — it was wiping the esp-alloc heap metadata (Size 98304 → 0, then
    // `memory allocation of N bytes failed`) and clobbering the zenoh connect
    // locator (the 0xffffffff Load-access-fault). Constructing in place removes
    // the temporary entirely.
    let eth = unsafe {
        OpenEth::new_in_place(ETH_DEVICE.as_mut_ptr(), openeth_config);
        ETH_DEVICE.assume_init_mut()
    };
    eth.init();

    let mac = eth.mac_address();
    esp_println::println!(
        "  MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0],
        mac[1],
        mac[2],
        mac[3],
        mac[4],
        mac[5]
    );

    // Create smoltcp interface
    esp_println::println!("");
    esp_println::println!("Creating network interface...");

    let mac_addr = EthernetAddress::from_bytes(&mac);
    let iface_config = smoltcp::iface::Config::new(mac_addr.into());
    let iface = Interface::new(
        iface_config,
        eth,
        smoltcp::time::Instant::from_millis(nros_platform_esp32_qemu::clock::clock_ms() as i64),
    );
    unsafe { NET_IFACE.write(iface) };
    let sockets = unsafe { create_socket_set() };
    unsafe { NET_SOCKETS.write(sockets) };

    // Configure static IP (no DHCP in QEMU)
    let iface = unsafe { NET_IFACE.assume_init_mut() };
    let ip_addr = Ipv4Address::new(config.ip[0], config.ip[1], config.ip[2], config.ip[3]);
    iface.update_ip_addrs(|addrs| {
        addrs
            .push(IpCidr::new(IpAddress::Ipv4(ip_addr), config.prefix))
            .ok();
    });

    let gw = Ipv4Address::new(
        config.gateway[0],
        config.gateway[1],
        config.gateway[2],
        config.gateway[3],
    );
    let _ = iface.routes_mut().add_default_ipv4_route(gw);

    esp_println::println!(
        "  IP: {}.{}.{}.{}/{}",
        config.ip[0],
        config.ip[1],
        config.ip[2],
        config.ip[3],
        config.prefix
    );
    esp_println::println!(
        "  Gateway: {}.{}.{}.{}",
        config.gateway[0],
        config.gateway[1],
        config.gateway[2],
        config.gateway[3]
    );

    // Initialize transport bridge
    SmoltcpBridge::init().expect("SmoltcpBridge::init double-call");

    // Create and register TCP + UDP sockets via transport crate
    let sockets = unsafe { NET_SOCKETS.assume_init_mut() };
    unsafe {
        nros_smoltcp::create_and_register_sockets(sockets);
        nros_smoltcp::create_and_register_udp_sockets(sockets);
    }

    // Store global state for poll callback
    let eth = unsafe { ETH_DEVICE.assume_init_mut() };
    unsafe {
        crate::network::set_network_state(
            iface as *mut Interface,
            sockets as *mut SocketSet<'static>,
            eth as *mut OpenEth as *mut (),
        );

        nros_smoltcp::set_poll_callback(crate::network::smoltcp_network_poll);

        // Register the network poll as the sleep callback so busy-wait
        // sleep polls the network stack to avoid missing packets during
        // zenoh-pico's connect handshake.
        nros_platform_esp32_qemu::sleep::set_poll_callback(crate::network::smoltcp_network_poll);
    }

    esp_println::println!(
        "  smoltcp poll callback registered: {}",
        nros_smoltcp::has_poll_callback()
    );
    esp_println::println!("Ethernet ready.");
}

// ---- Serial init ----

/// Initialize serial transport.
///
/// ESP32-C3 QEMU uses zenoh-pico's built-in serial support — no additional
/// driver crates are needed. The zenoh locator string (e.g.,
/// `serial/UART_0#baudrate=115200`) tells zenoh-pico which UART to use.
#[cfg(feature = "serial")]
fn init_serial(config: &Config) {
    esp_println::println!("Initializing serial transport...");
    esp_println::println!("  Baud: {}", config.baudrate);
    esp_println::println!("  Locator: {}", config.zenoh_locator);
    esp_println::println!("Serial ready.");
}

// ---- Main init + run ----

/// Initialize all ESP32-C3 QEMU hardware and the transport stack.
///
/// Sets up ESP32 peripherals, heap allocator, RNG, and the selected
/// transport (Ethernet and/or serial depending on enabled features).
/// After calling this, you can create an `Executor` and start using nano-ros.
///
/// This is automatically called by [`run()`]. Call it directly only when
/// using an alternative execution model (e.g., RTIC) that needs hardware
/// initialized before returning control to the framework.
///
/// # Panics
///
/// Panics if hardware initialization fails. Must be called exactly once
/// before any nros operations.
pub fn init_hardware(config: &Config) {
    esp_println::println!("");
    esp_println::println!("========================================");
    esp_println::println!("  nros ESP32-C3 QEMU Platform");
    esp_println::println!("========================================");
    esp_println::println!("");

    // Step 1: Initialize ESP32 peripherals
    esp_println::println!("Initializing ESP32-C3...");
    let _peripherals = esp_hal::init(esp_hal::Config::default());

    // Step 2: Set up heap allocator. For zenoh / non-DDS builds,
    // esp-alloc carves 96 KB out of DRAM at runtime; zenoh-pico +
    // nros publisher/subscriber setup can exceed the previous 64 KB
    // carve-out after session open. For DDS builds the example crate
    // enables `nros-platform/global-allocator`, which registers a
    // 256 KB static `FreeListHeap` instead — calling
    // `esp_alloc::heap_allocator!` on top of that produces the
    // "the `#[global_allocator]` in nros_platform conflicts with
    // global allocator in: esp_alloc" link error (Phase 101.7).
    #[cfg(not(feature = "dds-heap"))]
    esp_alloc::heap_allocator!(size: 96 * 1024);

    // Step 3: Register the monotonic clock with the shared busy-wait sleep
    // loop in `nros-baremetal-common`. Without this, `sleep_ms` silently
    // no-ops and zenoh-pico's connect handshake polls the network zero
    // times → Transport(ConnectionFailed).
    nros_platform_esp32_qemu::sleep::init_clock();

    // Step 4: Initialize hardware RNG (for zenoh-pico session ID)
    let rng = Rng::new();
    let rng_seed = rng.random() ^ network_identity_seed(config);
    random::seed(rng_seed);
    #[cfg(feature = "ethernet")]
    nros_smoltcp::seed_ephemeral_port((rng_seed as u16) ^ u16::from(config.mac_addr[5]));

    // Step 4: Initialize selected transport(s)
    #[cfg(feature = "ethernet")]
    init_ethernet(config);

    #[cfg(feature = "serial")]
    init_serial(config);

    esp_println::println!("");
}

/// Run an application with the given configuration.
///
/// This is the main entry point for ESP32-C3 QEMU applications.
/// It handles all hardware and network initialization, then calls
/// your application code with a reference to the config.
///
/// Inside the closure, use `Executor::open()` to create an executor
/// with full API access (publishers, subscriptions, services, actions,
/// timers, callbacks).
///
/// # Example
///
/// ```ignore
/// use nros_board_esp32_qemu::{Config, run};
/// use nros::prelude::*;
///
/// run(Config::default(), |config| {
///     let exec_config = ExecutorConfig::new(config.zenoh_locator)
///         .domain_id(config.domain_id);
///     let mut executor = Executor::open(&exec_config)?;
///     let mut node = executor.create_node("my_node")?;
///     // Full Executor API: publishers, subscriptions, services, actions...
///     Ok(())
/// })
/// ```
///
/// # Returns
///
/// Never returns (`-> !`). Loops forever after the application function completes
/// or on error.
pub fn run<F, E: core::fmt::Debug>(config: Config, f: F) -> !
where
    F: FnOnce(&Config) -> core::result::Result<(), E>,
{
    // Phase 173.1 — delegate to the shared direct-exec driver via the
    // `Board` trait. `Esp32Qemu::init_hardware` folds in the log-writer
    // registration; `exit_*` is the ESP32 no-exit spin loop.
    nros_board_common::run::<Esp32Qemu, F, E>(config, f)
}

/// Phase 173.1 — board ZST carrying the `Board` super-trait impls so
/// `nros_board_common::run` drives ESP32-C3 QEMU boot.
pub struct Esp32Qemu;

impl nros_board_common::BoardInit for Esp32Qemu {
    type Config = Config;

    fn init_hardware(cfg: &Config) {
        init_hardware(cfg);
        register_log_writer();
    }
}

impl nros_board_common::BoardPrint for Esp32Qemu {
    fn println(args: core::fmt::Arguments<'_>) {
        esp_println::println!("{}", args);
    }
}

impl nros_board_common::BoardExit for Esp32Qemu {
    fn exit_success() -> ! {
        // ESP32 has no process exit — spin forever (the test harness
        // kills QEMU once it sees the completion banner).
        #[allow(clippy::empty_loop)]
        loop {
            core::hint::spin_loop();
        }
    }

    fn exit_failure() -> ! {
        #[allow(clippy::empty_loop)]
        loop {
            core::hint::spin_loop();
        }
    }
}

/// Phase 88.15.f — register an `esp_println`-backed writer with
/// `nros-platform-esp32-qemu`'s log fn-ptr slot. Called once from
/// [`run()`] right after `init_hardware`. Mirrors the wifi board's
/// shape from Phase 88.16.E.
///
/// `pub(crate)` so the Phase 225.O `BoardEntry::run` shim in
/// `board_entry.rs` can route nros log records to the esp-println
/// console (node-registration / executor diagnostics).
pub(crate) fn register_log_writer() {
    fn writer(severity: u8, name: &[u8], message: &[u8]) {
        let label = match severity {
            0 => "TRACE",
            1 => "DEBUG",
            2 => "INFO",
            3 => "WARN",
            4 => "ERROR",
            5 => "FATAL",
            _ => "?",
        };
        let name_str = core::str::from_utf8(name).unwrap_or("");
        let msg_str = core::str::from_utf8(message).unwrap_or("");
        if !name_str.is_empty() {
            esp_println::println!("[{}] {}: {}", label, name_str, msg_str);
        } else {
            esp_println::println!("[{}] {}", label, msg_str);
        }
    }
    nros_platform_esp32_qemu::register_log_writer(Some(writer));
}
