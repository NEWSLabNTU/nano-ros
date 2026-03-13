//! Platform initialization and `run()` entry point for STM32F4.
//!
//! Uses `zpico-smoltcp` for socket management and `stm32-eth` for Ethernet
//! when the `ethernet` feature is enabled, or `zpico-serial` for UART serial
//! when the `serial` feature is enabled.

#[cfg(not(any(feature = "ethernet", feature = "serial")))]
compile_error!("Enable at least one transport: `ethernet` or `serial`");

#[cfg(feature = "ethernet")]
use core::mem::MaybeUninit;

#[cfg(feature = "ethernet")]
use stm32f4xx_hal::gpio::GpioExt;
use stm32f4xx_hal::{pac, prelude::*, rcc::RccExt};

use zpico_platform_stm32f4::clock;
use zpico_platform_stm32f4::random;

use crate::config::Config;

#[cfg(feature = "ethernet")]
use smoltcp::iface::{Interface, SocketSet};
#[cfg(feature = "ethernet")]
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};
#[cfg(feature = "ethernet")]
use stm32_eth::{
    EthPins, Parts, PartsIn,
    dma::{RxRingEntry, TxRingEntry},
};
#[cfg(feature = "ethernet")]
use zpico_smoltcp::SmoltcpBridge;
#[cfg(feature = "ethernet")]
use zpico_platform_stm32f4::phy;

// ============================================================================
// Static Buffer Allocation (ethernet)
// ============================================================================

#[cfg(feature = "ethernet")]
const RX_DESC_COUNT: usize = 4;
#[cfg(feature = "ethernet")]
const TX_DESC_COUNT: usize = 4;

#[cfg(feature = "ethernet")]
#[unsafe(link_section = ".ethram")]
static mut RX_RING: [RxRingEntry; RX_DESC_COUNT] = [RxRingEntry::INIT; RX_DESC_COUNT];
#[cfg(feature = "ethernet")]
#[unsafe(link_section = ".ethram")]
static mut TX_RING: [TxRingEntry; TX_DESC_COUNT] = [TxRingEntry::INIT; TX_DESC_COUNT];

#[cfg(feature = "ethernet")]
static mut ETH_DMA: MaybeUninit<stm32_eth::dma::EthernetDMA<'static, 'static>> =
    MaybeUninit::uninit();
#[cfg(feature = "ethernet")]
static mut NET_IFACE: MaybeUninit<Interface> = MaybeUninit::uninit();
#[cfg(feature = "ethernet")]
static mut NET_SOCKETS: MaybeUninit<SocketSet<'static>> = MaybeUninit::uninit();

// ============================================================================
// Public API
// ============================================================================

/// Initialize all STM32F4 hardware and the transport stack.
///
/// Sets up clocks, GPIO, and the selected transport (Ethernet and/or serial
/// depending on enabled features). After calling this, you can create an
/// `Executor` and start using nano-ros.
///
/// Accepts device and core peripherals by value, avoiding ownership
/// conflicts with frameworks like RTIC that also take peripherals.
/// Returns the SysTick peripheral for use with monotonic timers.
///
/// This is automatically called by [`run()`]. Call it directly when
/// using an alternative execution model (e.g., RTIC) that needs hardware
/// initialized before returning control to the framework.
///
/// # Panics
///
/// Panics if hardware initialization fails (clocks, Ethernet, PHY).
/// Must be called exactly once before any nros operations.
#[allow(static_mut_refs)]
pub fn init_hardware(
    config: &Config,
    dp: pac::Peripherals,
    cp: cortex_m::Peripherals,
) -> cortex_m::peripheral::SYST {
    defmt::info!("nros STM32F4 platform starting...");

    // Initialize platform hardware (clocks + ethernet if enabled)
    let syst = match unsafe { setup_hardware(config, dp, cp) } {
        Ok(result) => result,
        Err(e) => {
            defmt::error!("Platform init failed: {:?}", defmt::Debug2Format(&e));
            loop {
                cortex_m::asm::wfi();
            }
        }
    };

    #[cfg(feature = "serial")]
    {
        defmt::info!("Serial transport configured.");
        defmt::info!("  USART{}, {} baud", config.usart_index, config.baudrate);

        // Note: The actual USART peripheral setup and zpico_serial::register_port()
        // call must be done by the application or a board-specific USART driver,
        // since STM32F4 USART initialization requires GPIO pin configuration that
        // varies by board layout.

        #[cfg(not(feature = "ethernet"))]
        random::seed(config.usart_index as u32);
    }

    syst
}

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
///     let mut executor = Executor::open(&exec_config)?;
///     let mut node = executor.create_node("my_node")?;
///     // Full Executor API: publishers, subscriptions, services, actions...
///     Ok(())
/// })
/// ```
///
/// # Returns
///
/// Never returns (`-> !`). Enters idle loop on completion or error.
pub fn run<F, E: core::fmt::Debug>(config: Config, f: F) -> !
where
    F: FnOnce(&Config) -> core::result::Result<(), E>,
{
    let dp = pac::Peripherals::take().unwrap_or_else(|| {
        defmt::error!("Device peripherals already taken");
        loop {
            cortex_m::asm::wfi();
        }
    });
    let cp = cortex_m::Peripherals::take().unwrap_or_else(|| {
        defmt::error!("Core peripherals already taken");
        loop {
            cortex_m::asm::wfi();
        }
    });
    let _syst = init_hardware(&config, dp, cp);

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

/// Initialize STM32F4 hardware.
///
/// Configures clocks, DWT, and Ethernet (if enabled). Returns the SysTick
/// peripheral for the caller to use with a monotonic timer.
///
/// # Safety
///
/// Must be called only once at startup. Accesses static mutable buffers.
#[allow(static_mut_refs)]
unsafe fn setup_hardware(
    config: &Config,
    dp: pac::Peripherals,
    cp: cortex_m::Peripherals,
) -> core::result::Result<cortex_m::peripheral::SYST, SetupError> {
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

    // Enable DWT cycle counter for timing; keep SYST for caller
    let syst = cp.SYST;
    let mut dcb = cp.DCB;
    let mut dwt = cp.DWT;
    dcb.enable_trace();
    dwt.enable_cycle_counter();

    // Initialize DWT-based clock
    clock::init(sysclk_hz);

    #[cfg(feature = "ethernet")]
    {
        defmt::info!(
            "  IP: {}.{}.{}.{}",
            config.ip[0],
            config.ip[1],
            config.ip[2],
            config.ip[3]
        );

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
            .map_err(|_| SetupError::HardwareInit)?
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
        let mut iface =
            Interface::new(iface_config, &mut dma_ref, smoltcp::time::Instant::from_millis(0));

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
            .map_err(|_| SetupError::Route)?;

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

        // Move into static storage so pointers remain valid
        unsafe {
            ETH_DMA.write(dma);
            NET_IFACE.write(iface);
            NET_SOCKETS.write(sockets);
        }

        // Initialize the transport crate's bridge
        SmoltcpBridge::init();

        // Seed RNG with IP to avoid zenoh ID collisions
        let ip_seed = u32::from_be_bytes(config.ip);
        random::seed(ip_seed);

        // Create and register TCP + UDP sockets via transport crate
        let sockets = unsafe { NET_SOCKETS.assume_init_mut() };
        unsafe {
            zpico_smoltcp::create_and_register_sockets(sockets);
            zpico_smoltcp::create_and_register_udp_sockets(sockets);
        }

        // Store global state for poll callback (in zpico-platform-stm32f4)
        let iface = unsafe { NET_IFACE.assume_init_mut() };
        let dma = unsafe { ETH_DMA.assume_init_mut() };
        unsafe {
            zpico_platform_stm32f4::network::set_network_state(
                iface as *mut Interface,
                sockets as *mut SocketSet<'static>,
                dma as *mut stm32_eth::dma::EthernetDMA<'static, 'static>,
            );

            zpico_smoltcp::set_poll_callback(
                zpico_platform_stm32f4::network::smoltcp_network_poll,
            );
        }

        defmt::info!("Network ready.");
    }

    Ok(syst)
}

/// Internal error type for setup_hardware
#[derive(Debug)]
enum SetupError {
    #[cfg(feature = "ethernet")]
    HardwareInit,
    #[cfg(feature = "ethernet")]
    Route,
    /// Placeholder
    #[allow(dead_code)]
    _Never,
}

impl core::fmt::Display for SetupError {
    #[cfg_attr(not(feature = "ethernet"), allow(unused_variables))]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            #[cfg(feature = "ethernet")]
            SetupError::HardwareInit => write!(f, "Hardware init failed"),
            #[cfg(feature = "ethernet")]
            SetupError::Route => write!(f, "Failed to add route"),
            SetupError::_Never => unreachable!(),
        }
    }
}
