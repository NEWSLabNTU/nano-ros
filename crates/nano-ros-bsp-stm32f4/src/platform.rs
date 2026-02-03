//! Platform initialization for STM32F4
//!
//! This module handles all low-level hardware setup including:
//! - Clock configuration
//! - GPIO pin setup for Ethernet RMII
//! - Ethernet peripheral initialization
//! - smoltcp interface creation
//! - DWT cycle counter for timing

use crate::config::Config;
use crate::{Error, Result};

use stm32_eth::{
    dma::{EthernetDMA, RxRingEntry, TxRingEntry},
    EthPins, Parts, PartsIn,
};
use stm32f4xx_hal::{gpio::GpioExt, pac, prelude::*, rcc::RccExt};

use smoltcp::{
    iface::{Config as IfaceConfig, Interface, SocketSet},
    socket::tcp::{Socket as TcpSocket, SocketBuffer as TcpSocketBuffer},
    time::Instant,
    wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address},
};

// ============================================================================
// Static Buffer Allocation
// ============================================================================

/// Number of RX DMA descriptors
const RX_DESC_COUNT: usize = 4;

/// Number of TX DMA descriptors
const TX_DESC_COUNT: usize = 4;

/// TCP socket RX buffer size
const TCP_RX_BUFFER_SIZE: usize = 2048;

/// TCP socket TX buffer size
const TCP_TX_BUFFER_SIZE: usize = 2048;

/// Maximum number of sockets
const MAX_SOCKETS: usize = 2;

// Ethernet DMA descriptors - must be in normal RAM (not CCM)
#[unsafe(link_section = ".ethram")]
static mut RX_RING: [RxRingEntry; RX_DESC_COUNT] = [RxRingEntry::INIT; RX_DESC_COUNT];
#[unsafe(link_section = ".ethram")]
static mut TX_RING: [TxRingEntry; TX_DESC_COUNT] = [TxRingEntry::INIT; TX_DESC_COUNT];

// TCP socket buffers for zenoh
static mut TCP_RX_BUFFER: [u8; TCP_RX_BUFFER_SIZE] = [0u8; TCP_RX_BUFFER_SIZE];
static mut TCP_TX_BUFFER: [u8; TCP_TX_BUFFER_SIZE] = [0u8; TCP_TX_BUFFER_SIZE];

// Socket storage
static mut SOCKET_STORAGE: [smoltcp::iface::SocketStorage<'static>; MAX_SOCKETS] =
    [smoltcp::iface::SocketStorage::EMPTY; MAX_SOCKETS];

// ============================================================================
// Simple Timer using DWT
// ============================================================================

/// Simple timing tracker using DWT cycle counter
pub struct Timer {
    last_tick: u32,
    ticks_per_ms: u32,
    start_tick: u32,
}

impl Timer {
    /// Create a new timer
    pub fn new(sysclk_hz: u32) -> Self {
        let start = cortex_m::peripheral::DWT::cycle_count();
        Self {
            last_tick: start,
            ticks_per_ms: sysclk_hz / 1000,
            start_tick: start,
        }
    }

    /// Get current tick count (wraps around)
    pub fn now(&self) -> u32 {
        cortex_m::peripheral::DWT::cycle_count()
    }

    /// Check if the given number of milliseconds have elapsed since last check
    #[allow(dead_code)]
    pub fn elapsed_ms(&mut self, ms: u32) -> bool {
        let now = self.now();
        let elapsed_ticks = now.wrapping_sub(self.last_tick);
        let required_ticks = ms * self.ticks_per_ms;

        if elapsed_ticks >= required_ticks {
            self.last_tick = now;
            true
        } else {
            false
        }
    }

    /// Get elapsed milliseconds since timer creation
    pub fn elapsed_ms_total(&self) -> u64 {
        let now = self.now();
        (now.wrapping_sub(self.start_tick) as u64 * 1000) / (self.ticks_per_ms as u64)
    }
}

// ============================================================================
// Platform State
// ============================================================================

/// Initialized platform state holding all hardware references
pub struct Platform {
    pub dma: EthernetDMA<'static, 'static>,
    pub iface: Interface,
    pub sockets: SocketSet<'static>,
    pub tcp_handle: smoltcp::iface::SocketHandle,
    pub timer: Timer,
}

impl Platform {
    /// Get current time in milliseconds since platform init
    pub fn now_ms(&self) -> u64 {
        self.timer.elapsed_ms_total()
    }

    /// Poll the network interface and bridge data to zenoh-pico buffers
    pub fn poll(&mut self) {
        use zenoh_pico_shim::platform_smoltcp;

        // Update platform clock for zenoh-pico
        platform_smoltcp::smoltcp_set_clock_ms(self.now_ms());

        // Get smoltcp timestamp
        let timestamp = Instant::from_millis(self.now_ms() as i64);

        // Poll smoltcp
        let mut dma_ref = &mut self.dma;
        let _activity = self.iface.poll(timestamp, &mut dma_ref, &mut self.sockets);

        // Bridge smoltcp socket to zenoh-pico platform buffers
        let socket = self.sockets.get_mut::<TcpSocket>(self.tcp_handle);

        if socket.is_active() {
            // Read from smoltcp socket → push to shim RX buffer
            if socket.can_recv() {
                let mut buf = [0u8; 256];
                match socket.recv_slice(&mut buf) {
                    Ok(n) if n > 0 => {
                        defmt::debug!("Received {} bytes from TCP", n);
                        let ret = platform_smoltcp::smoltcp_socket_push_rx(0, buf.as_ptr(), n);
                        if ret < 0 {
                            defmt::warn!("Failed to push RX data to shim");
                        }
                    }
                    Ok(_) => {}
                    Err(_) => defmt::warn!("TCP recv error"),
                }
            }

            // Pop from shim TX buffer → send through smoltcp socket
            if socket.can_send() {
                let mut buf = [0u8; 256];
                let n = platform_smoltcp::smoltcp_socket_pop_tx(0, buf.as_mut_ptr(), buf.len());
                if n > 0 {
                    defmt::debug!("Sending {} bytes via TCP", n);
                    if socket.send_slice(&buf[..n as usize]).is_err() {
                        defmt::warn!("TCP send error");
                    }
                }
            }
        }

        // Poll zenoh-pico platform
        let _ret = platform_smoltcp::smoltcp_poll();
    }
}

// ============================================================================
// Initialization
// ============================================================================

/// Initialize the STM32F4 platform
///
/// This function:
/// 1. Configures clocks for maximum performance (168 MHz sysclk)
/// 2. Enables DWT cycle counter for timing
/// 3. Configures GPIO pins for Ethernet RMII (NUCLEO-F429ZI pinout)
/// 4. Initializes the Ethernet peripheral
/// 5. Creates smoltcp interface with the configured IP address
/// 6. Creates TCP socket for zenoh connection
/// 7. Initializes zenoh-pico platform layer
///
/// # Safety
///
/// Must be called only once at startup. Accesses static mutable buffers.
///
/// # Note
///
/// Currently supports NUCLEO-F429ZI pin configuration only. Other board
/// configurations will be added in future versions.
#[allow(static_mut_refs)]
pub unsafe fn init(config: &Config) -> Result<Platform> {
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

    let timer = Timer::new(sysclk_hz);

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

    let Parts { mut dma, .. } = unsafe {
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

    // Create smoltcp interface
    defmt::info!("Creating smoltcp interface...");

    let mac_addr = EthernetAddress::from_bytes(&config.mac);
    let iface_config = IfaceConfig::new(mac_addr.into());

    // Create interface - requires &mut Device
    let mut dma_ref = &mut dma;
    let mut iface = Interface::new(iface_config, &mut dma_ref, Instant::from_millis(0));

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
        .map_err(|_| Error::NetworkInit)?;

    defmt::info!(
        "IP address: {}.{}.{}.{}",
        config.ip[0],
        config.ip[1],
        config.ip[2],
        config.ip[3]
    );

    // Create socket set
    let mut sockets = unsafe { SocketSet::new(&mut SOCKET_STORAGE[..]) };

    // Create TCP socket for zenoh connection
    let tcp_rx_buffer = unsafe { TcpSocketBuffer::new(&mut TCP_RX_BUFFER[..]) };
    let tcp_tx_buffer = unsafe { TcpSocketBuffer::new(&mut TCP_TX_BUFFER[..]) };
    let tcp_socket = TcpSocket::new(tcp_rx_buffer, tcp_tx_buffer);

    let tcp_handle = sockets.add(tcp_socket);

    // Initialize zenoh-pico platform layer
    use zenoh_pico_shim::platform_smoltcp;
    platform_smoltcp::smoltcp_set_clock_ms(0);
    platform_smoltcp::smoltcp_set_poll_callback(Some(network_poll_callback));

    let ret = platform_smoltcp::smoltcp_init();
    if ret < 0 {
        defmt::error!("Failed to initialize smoltcp platform: {}", ret);
        return Err(Error::NetworkInit);
    }
    defmt::info!("smoltcp platform initialized");

    Ok(Platform {
        dma,
        iface,
        sockets,
        tcp_handle,
        timer,
    })
}

/// Network poll callback (no-op - polling is done in main loop)
unsafe extern "C" fn network_poll_callback() {
    // Polling is done in the main loop where we have access to all state
}
