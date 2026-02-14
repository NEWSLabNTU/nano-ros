//! High-level Node API and platform initialization for STM32F4
//!
//! Uses `zpico-smoltcp` for socket management instead of
//! the legacy BSP bridge approach.

use core::ffi::{c_char, c_void};
use core::fmt::Write as _;
use core::ptr;

use heapless::String;
use nros_core::RosMessage;
use zpico_smoltcp::SmoltcpBridge;
use smoltcp::iface::{Interface, SocketSet};
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};
use stm32_eth::{
    EthPins, Parts, PartsIn,
    dma::{EthernetDMA, RxRingEntry, TxRingEntry},
};
use stm32f4xx_hal::{gpio::GpioExt, pac, prelude::*, rcc::RccExt};

use zpico_sys::{
    zenoh_shim_close, zenoh_shim_declare_publisher, zenoh_shim_declare_subscriber,
    zenoh_shim_init, zenoh_shim_is_open, zenoh_shim_open, zenoh_shim_spin_once,
};

use crate::clock;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::phy;
use crate::publisher::Publisher;
use crate::random;
use crate::subscriber::{Subscription, subscription_trampoline};

// ============================================================================
// Static Buffer Allocation
// ============================================================================

/// Number of RX DMA descriptors
const RX_DESC_COUNT: usize = 4;

/// Number of TX DMA descriptors
const TX_DESC_COUNT: usize = 4;

// Ethernet DMA descriptors - must be in normal RAM (not CCM)
#[unsafe(link_section = ".ethram")]
static mut RX_RING: [RxRingEntry; RX_DESC_COUNT] = [RxRingEntry::INIT; RX_DESC_COUNT];
#[unsafe(link_section = ".ethram")]
static mut TX_RING: [TxRingEntry; TX_DESC_COUNT] = [TxRingEntry::INIT; TX_DESC_COUNT];

// ============================================================================
// Global state for poll callback
// ============================================================================

static mut GLOBAL_IFACE: *mut Interface = ptr::null_mut();
static mut GLOBAL_SOCKETS: *mut SocketSet<'static> = ptr::null_mut();
static mut GLOBAL_DMA: *mut EthernetDMA<'static, 'static> = ptr::null_mut();

/// Network poll callback called by the transport crate's smoltcp_poll()
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smoltcp_network_poll() {
    unsafe {
        if GLOBAL_IFACE.is_null() || GLOBAL_SOCKETS.is_null() || GLOBAL_DMA.is_null() {
            return;
        }

        let dma = &mut *GLOBAL_DMA;
        let iface = &mut *GLOBAL_IFACE;
        let sockets = &mut *GLOBAL_SOCKETS;

        // stm32-eth implements Device for &mut EthernetDMA, not EthernetDMA directly
        let mut dma_ref = dma;
        SmoltcpBridge::poll(iface, &mut dma_ref, sockets);
        clock::update_from_dwt();
    }
}

// ============================================================================
// Public API
// ============================================================================

/// High-level node handle for pub/sub operations
pub struct Node {
    domain_id: u32,
}

impl Node {
    /// Create a typed publisher for a ROS 2 topic
    ///
    /// Constructs the ROS 2 keyexpr from topic name and `M::TYPE_NAME`:
    /// `<domain_id>/<topic>/<type_name>/TypeHashNotSupported`
    pub fn create_publisher<M: RosMessage>(&mut self, topic: &str) -> Result<Publisher<M>> {
        let mut key = format_ros2_keyexpr(self.domain_id, topic, M::TYPE_NAME);
        key.push('\0').map_err(|_| Error::TopicTooLong)?;
        let handle =
            unsafe { zenoh_shim_declare_publisher(key.as_bytes().as_ptr() as *const c_char) };
        if handle < 0 {
            defmt::error!("Failed to create publisher: {}", handle);
            return Err(Error::PublisherDeclare);
        }
        defmt::info!("Publisher created (handle={})", handle);
        Ok(unsafe { Publisher::from_handle(handle) })
    }

    /// Create a typed subscription for a ROS 2 topic
    ///
    /// Messages are deserialized from CDR and delivered to the callback.
    ///
    /// # Limitations
    ///
    /// The callback is a function pointer (`fn(&M)`), not a closure.
    /// Use `static` variables for external state — the standard bare-metal pattern.
    pub fn create_subscription<M: RosMessage>(
        &mut self,
        topic: &str,
        callback: fn(&M),
    ) -> Result<Subscription<M>> {
        let mut key = format_ros2_keyexpr_wildcard(self.domain_id, topic, M::TYPE_NAME);
        key.push('\0').map_err(|_| Error::TopicTooLong)?;
        let ctx = callback as *mut c_void;
        let handle = unsafe {
            zenoh_shim_declare_subscriber(
                key.as_bytes().as_ptr() as *const c_char,
                subscription_trampoline::<M>,
                ctx,
            )
        };
        if handle < 0 {
            defmt::error!("Failed to create subscriber: {}", handle);
            return Err(Error::SubscriberDeclare);
        }
        defmt::info!("Subscriber created (handle={})", handle);
        Ok(unsafe { Subscription::from_handle(handle) })
    }

    /// Process network events and callbacks
    ///
    /// This must be called periodically to:
    /// - Handle network traffic
    /// - Dispatch subscriber callbacks
    /// - Process zenoh protocol messages
    ///
    /// # Arguments
    ///
    /// * `timeout_ms` - Maximum time to spend processing (0 = non-blocking)
    pub fn spin_once(&mut self, timeout_ms: u32) {
        unsafe {
            zenoh_shim_spin_once(timeout_ms);
        }
    }

    /// Get current uptime in milliseconds
    pub fn now_ms(&self) -> u64 {
        clock::clock_ms()
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        defmt::info!("Shutting down node...");
        unsafe {
            zenoh_shim_close();

            GLOBAL_IFACE = ptr::null_mut();
            GLOBAL_SOCKETS = ptr::null_mut();
            GLOBAL_DMA = ptr::null_mut();
        }
    }
}

// ============================================================================
// Initialization
// ============================================================================

/// Entry point for platform-based applications
///
/// Initializes all hardware and zenoh infrastructure, then calls the
/// user-provided closure with a ready-to-use `Node`.
///
/// # Example
///
/// ```no_run
/// use nano_ros_platform_stm32f4::prelude::*;
///
/// // Define a message type
/// struct Int32 { data: i32 }
/// // ... impl Serialize, Deserialize, RosMessage ...
///
/// #[entry]
/// fn main() -> ! {
///     run_node(Config::nucleo_f429zi(), |node| {
///         let pub_ = node.create_publisher::<Int32>("/chatter")?;
///         pub_.publish(&Int32 { data: 42 })?;
///         Ok(())
///     })
/// }
/// ```
///
/// # Panics
///
/// Panics if hardware initialization fails.
#[allow(static_mut_refs)]
pub fn run_node<F>(config: Config, f: F) -> !
where
    F: FnOnce(&mut Node) -> Result<()>,
{
    defmt::info!("nros STM32F4 platform starting...");
    defmt::info!(
        "  IP: {}.{}.{}.{}",
        config.ip[0],
        config.ip[1],
        config.ip[2],
        config.ip[3]
    );

    // Initialize platform hardware
    let (mut dma, mut iface, mut sockets) = match unsafe { init_hardware(&config) } {
        Ok(result) => result,
        Err(e) => {
            defmt::error!("Platform init failed: {:?}", e);
            loop {
                cortex_m::asm::wfi();
            }
        }
    };

    // Initialize the transport crate's bridge
    SmoltcpBridge::init();

    // Seed RNG with IP to avoid zenoh ID collisions
    let ip_seed = u32::from_be_bytes(config.ip);
    random::seed(ip_seed);

    // Create and register TCP sockets via transport crate
    unsafe {
        zpico_smoltcp::create_and_register_sockets(&mut sockets);
    }

    // Store global state for poll callback
    unsafe {
        GLOBAL_DMA = &mut dma as *mut EthernetDMA<'static, 'static>;
        GLOBAL_IFACE = &mut iface as *mut Interface;
        GLOBAL_SOCKETS = &mut sockets as *mut SocketSet<'static>;

        zpico_smoltcp::set_poll_callback(smoltcp_network_poll);
    }

    // Initialize zenoh session
    defmt::info!("Connecting to zenoh router...");
    let ret = unsafe { zenoh_shim_init(config.zenoh_locator.as_ptr() as *const c_char) };
    if ret < 0 {
        defmt::error!("zenoh_shim_init failed: {}", ret);
        loop {
            cortex_m::asm::wfi();
        }
    }

    let ret = unsafe { zenoh_shim_open() };
    if ret < 0 {
        defmt::error!("zenoh_shim_open failed: {}", ret);
        loop {
            cortex_m::asm::wfi();
        }
    }

    // Verify session is open
    if unsafe { zenoh_shim_is_open() } == 0 {
        defmt::error!("zenoh session not open after open()");
        loop {
            cortex_m::asm::wfi();
        }
    }
    defmt::info!("Zenoh session opened");

    // Create node
    let mut node = Node {
        domain_id: config.domain_id,
    };

    // Run user code
    match f(&mut node) {
        Ok(()) => {
            defmt::info!("Application completed successfully");
        }
        Err(e) => {
            defmt::error!("Application error: {:?}", e);
        }
    }

    // Node will be dropped here, closing zenoh session

    defmt::info!("Entering idle loop");
    loop {
        cortex_m::asm::wfi();
    }
}

// ============================================================================
// Hardware initialization
// ============================================================================

/// Initialize STM32F4 hardware and create smoltcp interface
///
/// Returns (EthernetDMA, Interface, SocketSet) with all hardware configured.
///
/// # Safety
///
/// Must be called only once at startup. Accesses static mutable buffers.
#[allow(static_mut_refs)]
unsafe fn init_hardware(
    config: &Config,
) -> Result<(EthernetDMA<'static, 'static>, Interface, SocketSet<'static>)> {
    // Get access to device peripherals
    let dp = pac::Peripherals::take().ok_or(Error::HardwareInit)?;
    let cp = cortex_m::Peripherals::take().ok_or(Error::HardwareInit)?;

    // Configure clocks
    let rcc = dp.RCC.constrain();
    let clocks = rcc
        .cfgr
        .use_hse((config.hse_freq_mhz as u32).MHz())
        .sysclk(168.MHz())
        .hclk(168.MHz())
        .pclk1(42.MHz())
        .pclk2(84.MHz())
        .freeze();

    let sysclk_hz = clocks.sysclk().to_Hz();
    defmt::info!("Clocks configured: sysclk = {} Hz", sysclk_hz);

    // Enable DWT cycle counter for timing
    let mut dcb = cp.DCB;
    let mut dwt = cp.DWT;
    dcb.enable_trace();
    dwt.enable_cycle_counter();

    // Initialize DWT-based clock
    clock::init(sysclk_hz);

    // Split GPIO ports
    let gpioa = dp.GPIOA.split();
    let gpiob = dp.GPIOB.split();
    let gpioc = dp.GPIOC.split();
    let gpiog = dp.GPIOG.split();

    // Configure RMII pins for NUCLEO-F429ZI
    // Note: Currently only this pin configuration is supported.
    // The PinConfig enum is reserved for future board support.
    let ref_clk = gpioa.pa1.into_floating_input();
    let crs = gpioa.pa7.into_floating_input();
    let tx_en = gpiog.pg11.into_floating_input();
    let tx_d0 = gpiog.pg13.into_floating_input();
    let tx_d1 = gpiob.pb13.into_floating_input();
    let rx_d0 = gpioc.pc4.into_floating_input();
    let rx_d1 = gpioc.pc5.into_floating_input();

    let eth_pins = EthPins {
        ref_clk,
        crs,
        tx_en,
        tx_d0,
        tx_d1,
        rx_d0,
        rx_d1,
    };

    // MDC/MDIO for PHY management
    let mdio = gpioa.pa2.into_alternate();
    let mdc = gpioc.pc1.into_alternate();

    defmt::info!("Initializing Ethernet...");

    let eth_parts_in = PartsIn {
        dma: dp.ETHERNET_DMA,
        mac: dp.ETHERNET_MAC,
        mmc: dp.ETHERNET_MMC,
        ptp: dp.ETHERNET_PTP,
    };

    let Parts { mut dma, mac, .. } = unsafe {
        stm32_eth::new_with_mii(
            eth_parts_in,
            &mut RX_RING,
            &mut TX_RING,
            clocks,
            eth_pins,
            mdio,
            mdc,
        )
        .map_err(|_| Error::HardwareInit)?
    };

    defmt::info!("Ethernet initialized");

    // PHY detection using SMI (Station Management Interface)
    let detected_phy = {
        let mut mac_ref = mac;
        phy::scan_for_phy(|addr, reg| mac_ref.read(addr, reg))
    };

    match detected_phy {
        Some((addr, phy_type)) => {
            defmt::info!("Detected {} PHY at address {}", phy_type.name(), addr);
        }
        None => {
            defmt::warn!("No PHY detected, assuming LAN8742A at address 0");
        }
    }

    // Create smoltcp interface
    defmt::info!("Creating smoltcp interface...");

    let mac_addr = EthernetAddress::from_bytes(&config.mac);
    let iface_config = smoltcp::iface::Config::new(mac_addr.into());

    let mut dma_ref = &mut dma;
    let mut iface = Interface::new(iface_config, &mut dma_ref, smoltcp::time::Instant::from_millis(0));

    // Set IP address
    iface.update_ip_addrs(|addrs| {
        addrs
            .push(IpCidr::new(
                IpAddress::v4(config.ip[0], config.ip[1], config.ip[2], config.ip[3]),
                config.prefix,
            ))
            .ok();
    });

    // Set default gateway
    let gateway = Ipv4Address::new(
        config.gateway[0],
        config.gateway[1],
        config.gateway[2],
        config.gateway[3],
    );
    iface
        .routes_mut()
        .add_default_ipv4_route(gateway)
        .map_err(|_| Error::Route)?;

    defmt::info!(
        "IP address: {}.{}.{}.{}",
        config.ip[0],
        config.ip[1],
        config.ip[2],
        config.ip[3]
    );

    // Create socket set from transport crate's pre-allocated storage
    let storage = unsafe { zpico_smoltcp::get_socket_storage() };
    let sockets = SocketSet::new(&mut storage[..]);

    Ok((dma, iface, sockets))
}

// ============================================================================
// Helpers
// ============================================================================

/// Format a ROS 2 data keyexpr: `<domain_id>/<topic>/<type_name>/TypeHashNotSupported`
fn format_ros2_keyexpr(domain_id: u32, topic: &str, type_name: &str) -> String<256> {
    let mut key = String::<256>::new();
    let topic_stripped = topic.trim_matches('/');
    let _ = write!(
        key,
        "{}/{}/{}/TypeHashNotSupported",
        domain_id, topic_stripped, type_name
    );
    key
}

/// Format a ROS 2 subscriber keyexpr with wildcard: `<domain_id>/<topic>/<type_name>/*`
fn format_ros2_keyexpr_wildcard(domain_id: u32, topic: &str, type_name: &str) -> String<256> {
    let mut key = String::<256>::new();
    let topic_stripped = topic.trim_matches('/');
    let _ = write!(key, "{}/{}/{}/*", domain_id, topic_stripped, type_name);
    key
}
