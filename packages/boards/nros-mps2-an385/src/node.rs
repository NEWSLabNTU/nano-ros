//! Platform initialization and `run()` entry point for QEMU bare-metal.
//!
//! Uses `zpico-smoltcp` for socket management and `lan9118-smoltcp` for
//! Ethernet when the `ethernet` feature is enabled, or `zpico-serial` +
//! `cmsdk-uart` for UART serial when the `serial` feature is enabled.

#[cfg(not(any(feature = "ethernet", feature = "serial")))]
compile_error!("Enable at least one transport: `ethernet` or `serial`");

use core::mem::MaybeUninit;

use cortex_m_semihosting::hprintln;

use nros_platform_mps2_an385::random;

#[cfg(feature = "ethernet")]
use nros_platform_mps2_an385::clock;
#[cfg(feature = "ethernet")]
use crate::network;

use crate::config::Config;
use crate::exit_failure;

#[cfg(feature = "ethernet")]
use crate::error::{Error, Result};

// ---- Ethernet static storage ----

#[cfg(feature = "ethernet")]
use lan9118_smoltcp::{Config as EthConfig, Lan9118, MPS2_AN385_BASE};
#[cfg(feature = "ethernet")]
use smoltcp::iface::{Interface, SocketSet};
#[cfg(feature = "ethernet")]
use smoltcp::phy::Device;
#[cfg(feature = "ethernet")]
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};
#[cfg(feature = "ethernet")]
use zpico_smoltcp::SmoltcpBridge;

#[cfg(feature = "ethernet")]
static mut ETH_DEVICE: MaybeUninit<Lan9118> = MaybeUninit::uninit();
#[cfg(feature = "ethernet")]
static mut NET_IFACE: MaybeUninit<Interface> = MaybeUninit::uninit();
#[cfg(feature = "ethernet")]
static mut NET_SOCKETS: MaybeUninit<SocketSet<'static>> = MaybeUninit::uninit();

// ---- Serial static storage ----

#[cfg(feature = "serial")]
static mut UART_DEVICE: MaybeUninit<cmsdk_uart::CmsdkUart> = MaybeUninit::uninit();

// ---- Semihosting helpers ----

/// Get host wall-clock time via ARM semihosting SYS_TIME (0x11).
///
/// Returns seconds since 1970-01-01. On QEMU, this reflects the host's
/// real clock, providing entropy that varies between QEMU runs (unlike
/// the DWT cycle counter which is deterministic in emulation).
fn semihosting_time() -> u32 {
    let result: u32;
    unsafe {
        core::arch::asm!(
            "bkpt #0xAB",
            inout("r0") 0x11_u32 => result,
            in("r1") 0_u32,
        );
    }
    result
}

// ---- Ethernet helpers ----

#[cfg(feature = "ethernet")]
/// Trait for Ethernet devices that can be used with the platform
trait EthernetDevice: Device {
    /// Get the MAC address
    fn mac_address(&self) -> [u8; 6];
}

#[cfg(feature = "ethernet")]
impl EthernetDevice for Lan9118 {
    fn mac_address(&self) -> [u8; 6] {
        Lan9118::mac_address(self)
    }
}

#[cfg(feature = "ethernet")]
/// Helper to create an smoltcp interface from an Ethernet device
fn create_interface<D: EthernetDevice>(eth: &mut D) -> Interface {
    let mac = eth.mac_address();
    let mac_addr = EthernetAddress::from_bytes(&mac);
    let now = clock::now();
    let iface_config = smoltcp::iface::Config::new(mac_addr.into());
    Interface::new(iface_config, eth, now)
}

#[cfg(feature = "ethernet")]
/// Helper to create a socket set with pre-allocated storage
unsafe fn create_socket_set() -> SocketSet<'static> {
    let storage = unsafe { zpico_smoltcp::get_socket_storage() };
    SocketSet::new(&mut storage[..])
}

#[cfg(feature = "ethernet")]
/// Create LAN9118 Ethernet driver for QEMU MPS2-AN385
fn create_ethernet(mac: [u8; 6]) -> Result<Lan9118> {
    let config = EthConfig {
        base_addr: MPS2_AN385_BASE,
        mac_addr: mac,
    };

    let mut eth = unsafe { Lan9118::new(config).map_err(|_| Error::EthernetInit)? };
    eth.init().map_err(|_| Error::EthernetInit)?;

    Ok(eth)
}

#[cfg(feature = "ethernet")]
/// Initialize network stack (IP, gateway, smoltcp bridge, sockets)
fn init_network<D: EthernetDevice + 'static>(
    eth: &mut D,
    iface: &mut Interface,
    sockets: &mut SocketSet<'static>,
    config: &Config,
) -> Result<()> {
    // Configure IP address
    let ip_addr = Ipv4Address::new(config.ip[0], config.ip[1], config.ip[2], config.ip[3]);
    iface.update_ip_addrs(|addrs| {
        addrs
            .push(IpCidr::new(IpAddress::Ipv4(ip_addr), config.prefix))
            .ok();
    });

    // Set default gateway (skip if 0.0.0.0, which indicates link-local mode)
    if config.gateway != [0, 0, 0, 0] {
        let gw = Ipv4Address::new(
            config.gateway[0],
            config.gateway[1],
            config.gateway[2],
            config.gateway[3],
        );
        iface
            .routes_mut()
            .add_default_ipv4_route(gw)
            .map_err(|_| Error::Route)?;
    }

    // Initialize the transport crate's bridge
    SmoltcpBridge::init();

    // Seed ephemeral port counter to avoid TCP 4-tuple collisions.
    // Without this, smoltcp always starts at port 49152, and a stale
    // FIN-WAIT-1 socket in the host kernel from a previous QEMU run
    // blocks the new SYN on the same (src_ip:49152 → dst_ip:7447) tuple.
    //
    // QEMU's DWT cycle counter is deterministic (TCG replays the same
    // instruction count every run), so we use the host's wall clock via
    // ARM semihosting SYS_TIME for real entropy that varies between runs.
    let host_time = semihosting_time() as u16;
    let ip_byte = config.ip[3] as u16;
    zpico_smoltcp::seed_ephemeral_port(host_time.wrapping_add(ip_byte.wrapping_mul(251)));

    // Seed RNG with IP to avoid zenoh ID collisions
    let ip_seed = u32::from_be_bytes(config.ip);
    random::seed(ip_seed);

    // Create and register TCP + UDP sockets via transport crate
    unsafe {
        zpico_smoltcp::create_and_register_sockets(sockets);
        zpico_smoltcp::create_and_register_udp_sockets(sockets);
    }

    // Store global state for poll callback
    unsafe {
        network::set_network_state(
            iface as *mut Interface,
            sockets as *mut SocketSet<'static>,
            eth as *mut D as *mut (),
        );

        zpico_smoltcp::set_poll_callback(network::smoltcp_network_poll);

        // Register the network poll as the sleep callback so busy-wait
        // sleep polls the network stack to avoid missing packets.
        nros_platform_mps2_an385::sleep::set_poll_callback(network::smoltcp_network_poll);
    }

    Ok(())
}

// ---- Init functions ----

/// Initialize Ethernet transport.
#[cfg(feature = "ethernet")]
#[allow(static_mut_refs)]
fn init_ethernet(config: &Config) {
    hprintln!("Initializing LAN9118 Ethernet...");
    let eth = match create_ethernet(config.mac) {
        Ok(e) => e,
        Err(e) => {
            hprintln!("Error creating Ethernet: {:?}", e);
            exit_failure();
        }
    };

    // Move into static storage so pointers remain valid
    unsafe { ETH_DEVICE.write(eth) };
    let eth = unsafe { ETH_DEVICE.assume_init_mut() };

    hprintln!(
        "  MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        config.mac[0],
        config.mac[1],
        config.mac[2],
        config.mac[3],
        config.mac[4],
        config.mac[5]
    );

    // Create smoltcp interface and socket set
    hprintln!("");
    hprintln!("Creating network interface...");
    let iface = create_interface(eth);
    unsafe { NET_IFACE.write(iface) };
    let sockets = unsafe { create_socket_set() };
    unsafe { NET_SOCKETS.write(sockets) };

    hprintln!(
        "  IP: {}.{}.{}.{}",
        config.ip[0],
        config.ip[1],
        config.ip[2],
        config.ip[3]
    );

    // Initialize network stack
    let eth = unsafe { ETH_DEVICE.assume_init_mut() };
    let iface = unsafe { NET_IFACE.assume_init_mut() };
    let sockets = unsafe { NET_SOCKETS.assume_init_mut() };

    if let Err(e) = init_network(eth, iface, sockets, config) {
        hprintln!("Error initializing network: {:?}", e);
        exit_failure();
    }

    hprintln!("Ethernet ready.");
}

/// Initialize serial (UART) transport.
#[cfg(feature = "serial")]
#[allow(static_mut_refs)]
fn init_serial(config: &Config) {
    hprintln!("Initializing CMSDK UART serial...");
    hprintln!("  Base: 0x{:08x}", config.uart_base);
    hprintln!("  Baud: {}", config.baudrate);

    let mut uart = cmsdk_uart::CmsdkUart::new(config.uart_base);
    uart.enable();

    // Move into static storage
    unsafe {
        UART_DEVICE.write(uart);
        zpico_serial::register_port(0, UART_DEVICE.assume_init_mut());
    }

    // Seed RNG with host wall-clock time via semihosting to generate unique
    // zenoh IDs. QEMU's virtual clock (-icount shift=auto) starts at 0 for
    // each instance, so hardware timers are deterministic. Semihosting
    // SYS_TIME returns the host's real UNIX timestamp which varies between
    // QEMU runs started at different times.
    let host_time = semihosting_time();
    random::seed(host_time);

    hprintln!("Serial ready.");
}

/// Initialize all MPS2-AN385 hardware and the transport stack.
///
/// Sets up the DWT cycle counter and initializes the selected transport
/// (Ethernet and/or serial depending on enabled features). After calling
/// this, you can create an `Executor` and start using nano-ros.
///
/// This is automatically called by [`run()`]. Call it directly only
/// when using an alternative execution model (e.g., RTIC) that needs
/// hardware initialized before returning control to the framework.
///
/// # Panics
///
/// Panics if hardware initialization fails (Ethernet, network stack).
/// Must be called exactly once before any nros operations.
pub fn init_hardware(config: &Config) {
    // Initialize CMSDK Timer0 as the monotonic clock source.
    // This must happen before any clock reads (including smoltcp interface
    // creation which calls clock::now()). On QEMU, pair with
    // `-icount shift=auto` to keep virtual time aligned with wall-clock
    // time. See docs/reference/qemu-icount.md.
    nros_platform_mps2_an385::clock::init_hardware_timer();

    // Enable DWT cycle counter for timing measurements
    nros_platform_mps2_an385::timing::CycleCounter::enable();

    hprintln!("");
    hprintln!("========================================");
    hprintln!("  nros QEMU Platform");
    hprintln!("========================================");
    hprintln!("");

    #[cfg(feature = "ethernet")]
    init_ethernet(config);

    #[cfg(feature = "serial")]
    init_serial(config);

    hprintln!("");
}

/// Run an application with the given configuration.
///
/// This is the main entry point for QEMU bare-metal applications.
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
/// use nros_mps2_an385::{Config, run};
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
/// Never returns (`-> !`). Calls `exit_success()` on Ok, `exit_failure()` on Err.
pub fn run<F, E: core::fmt::Debug>(config: Config, f: F) -> !
where
    F: FnOnce(&Config) -> core::result::Result<(), E>,
{
    init_hardware(&config);

    // Run user application
    match f(&config) {
        Ok(()) => {
            hprintln!("");
            hprintln!("Application completed successfully.");
            hprintln!("");
            hprintln!("========================================");
            hprintln!("  Done");
            hprintln!("========================================");
            crate::exit_success();
        }
        Err(e) => {
            hprintln!("");
            hprintln!("Application error: {:?}", e);
            exit_failure();
        }
    }
}
