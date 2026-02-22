//! Platform initialization and `run()` entry point for STM32F4.
//!
//! Uses `zpico-smoltcp` for socket management and `stm32-eth` for Ethernet.

use smoltcp::iface::{Interface, SocketSet};
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};
use stm32_eth::{
    EthPins, Parts, PartsIn,
    dma::{RxRingEntry, TxRingEntry},
};
use stm32f4xx_hal::{gpio::GpioExt, pac, prelude::*, rcc::RccExt};
use zpico_smoltcp::SmoltcpBridge;

use zpico_platform_stm32f4::clock;
use crate::config::Config;
use crate::error::{Error, Result};
use zpico_platform_stm32f4::phy;
use zpico_platform_stm32f4::random;

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
// Public API
// ============================================================================

/// Run an application with the given configuration.
///
/// This is the main entry point for STM32F4 applications.
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
/// use nros_stm32f4::{Config, run};
/// use nros::prelude::*;
///
/// run(Config::nucleo_f429zi(), |config| {
///     let exec_config = ExecutorConfig::new(config.zenoh_locator)
///         .domain_id(config.domain_id);
///     let mut executor = Executor::<_, 0, 0>::open(&exec_config)?;
///     let mut node = executor.create_node("my_node")?;
///     // Full Executor API: publishers, subscriptions, services, actions...
///     Ok(())
/// })
/// ```
///
/// # Returns
///
/// Never returns (`-> !`). Enters idle loop on completion or error.
#[allow(static_mut_refs)]
pub fn run<F, E: core::fmt::Debug>(config: Config, f: F) -> !
where
    F: FnOnce(&Config) -> core::result::Result<(), E>,
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

    // Create and register TCP + UDP sockets via transport crate
    unsafe {
        zpico_smoltcp::create_and_register_sockets(&mut sockets);
        zpico_smoltcp::create_and_register_udp_sockets(&mut sockets);
    }

    // Store global state for poll callback (in zpico-platform-stm32f4)
    unsafe {
        zpico_platform_stm32f4::network::set_network_state(
            &mut iface as *mut Interface,
            &mut sockets as *mut SocketSet<'static>,
            &mut dma as *mut stm32_eth::dma::EthernetDMA<'static, 'static>,
        );

        zpico_smoltcp::set_poll_callback(zpico_platform_stm32f4::network::smoltcp_network_poll);
    }

    defmt::info!("Network ready.");

    // Run user application
    match f(&config) {
        Ok(()) => {
            defmt::info!("Application completed successfully");
        }
        Err(e) => {
            defmt::error!("Application error: {:?}", defmt::Debug2Format(&e));
        }
    }

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
) -> Result<(stm32_eth::dma::EthernetDMA<'static, 'static>, Interface, SocketSet<'static>)> {
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
