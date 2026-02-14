//! RTIC Example for nros on STM32F4 with nano-ros-transport-zenoh
//!
//! This example demonstrates nros with nano-ros-transport-zenoh on an STM32F4
//! microcontroller using smoltcp for TCP/IP networking.
//!
//! # Architecture
//!
//! ```text
//! RTIC Tasks
//! ├── poll_network (priority 2)
//! │   └── Polls smoltcp, bridges to zenoh-pico platform buffers
//! ├── zenoh_poll (priority 2)
//! │   └── Calls ShimContext::spin_once() for zenoh processing
//! └── publisher_task (priority 1)
//!     └── Periodic publishing of sensor data
//! ```
//!
//! # Hardware
//!
//! - Board: NUCLEO-F429ZI (or similar STM32F4 with Ethernet)
//! - Connect Ethernet cable to the board's RJ45 port
//!
//! # Network Configuration
//!
//! Default (static IP):
//! - Device IP: 192.168.1.10
//! - Gateway: 192.168.1.1
//! - Zenoh Router: 192.168.1.1:7447
//!
//! # Building
//!
//! ```bash
//! cargo build --release
//! # Flash with probe-rs
//! cargo run --release
//! ```
//!
//! # Note
//!
//! This example requires the zenoh-pico C library to be cross-compiled for
//! ARM Cortex-M. The current implementation shows the correct integration
//! pattern but zenoh operations are stubbed until cross-compilation is set up.

#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use panic_probe as _;

use rtic::app;
use rtic_monotonics::systick::prelude::*;

use stm32f4xx_hal::{gpio::GpioExt, prelude::*, rcc::RccExt};

use stm32_eth::{
    EthPins, Parts, PartsIn,
    dma::{EthernetDMA, RxRingEntry, TxRingEntry},
};

use smoltcp::{
    iface::{Config, Interface, SocketSet},
    time::Instant,
    wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address},
};

// Import nano-ros-link-smoltcp for TCP bridge
use nano_ros_link_smoltcp::SmoltcpBridge;

// ============================================================================
// Network Configuration
// ============================================================================

/// Device MAC address (locally administered)
const MAC_ADDRESS: [u8; 6] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];

/// Device IP address (static)
const IP_ADDRESS: [u8; 4] = [192, 168, 1, 10];

/// Network gateway
const GATEWAY: Ipv4Address = Ipv4Address::new(192, 168, 1, 1);

/// Zenoh router address
#[allow(dead_code)] // Used when zenoh-pico is available
const ZENOH_ROUTER: &[u8] = b"tcp/192.168.1.1:7447\0";

// ============================================================================
// Static Buffer Allocation
// ============================================================================

/// Number of RX DMA descriptors
const RX_DESC_COUNT: usize = 4;

/// Number of TX DMA descriptors
const TX_DESC_COUNT: usize = 4;

/// Maximum number of sockets (matches nano-ros-link-smoltcp::MAX_SOCKETS)
#[allow(dead_code)] // Used in documentation only; actual value comes from link crate
const MAX_SOCKETS: usize = 4;

/// Poll interval in milliseconds
const POLL_INTERVAL_MS: u32 = 10;

/// Publish interval in milliseconds
const PUBLISH_INTERVAL_MS: u32 = 1000;

// Ethernet DMA descriptors - must be in normal RAM (not CCM)
#[unsafe(link_section = ".ethram")]
static mut RX_RING: [RxRingEntry; RX_DESC_COUNT] = [RxRingEntry::INIT; RX_DESC_COUNT];
#[unsafe(link_section = ".ethram")]
static mut TX_RING: [TxRingEntry; TX_DESC_COUNT] = [TxRingEntry::INIT; TX_DESC_COUNT];

// Clock state for smoltcp_clock_now_ms (updated by poll task)
static mut CLOCK_MS: u64 = 0;

/// Provide the millisecond clock for nano-ros-link-smoltcp's bridge.
/// Called internally by `SmoltcpBridge::poll()` for smoltcp timestamping.
#[unsafe(no_mangle)]
pub extern "C" fn smoltcp_clock_now_ms() -> u64 {
    unsafe { CLOCK_MS }
}

// ============================================================================
// Platform Callback for smoltcp Integration
// ============================================================================

/// Network poll callback
///
/// This is called by zenoh-pico's platform layer when it needs to poll
/// the network. We bridge smoltcp's socket operations to the shim's buffers.
unsafe extern "C" fn network_poll_callback() {
    // This callback is invoked from zenoh-pico's network layer.
    // It should poll smoltcp and transfer data between smoltcp sockets
    // and the shim's internal buffers.
    //
    // The actual implementation would:
    // 1. Poll the smoltcp interface
    // 2. For each active zenoh socket:
    //    - Read from smoltcp socket → push to shim RX buffer
    //    - Pop from shim TX buffer → send through smoltcp socket
    //
    // This is handled in the poll_network RTIC task instead, as it has
    // access to the shared resources.
}

// ============================================================================
// RTIC Application
// ============================================================================

systick_monotonic!(Mono, 1000); // 1 kHz = 1ms resolution

#[app(device = stm32f4xx_hal::pac, peripherals = true, dispatchers = [USART1, USART2, USART3])]
mod app {
    use super::*;

    #[shared]
    struct Shared {
        eth_dma: EthernetDMA<'static, 'static>,
        iface: Interface,
        sockets: SocketSet<'static>,
        counter: u32,
    }

    #[local]
    struct Local {}

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        info!("nros RTIC + nano-ros-transport-zenoh example starting...");

        let dp = cx.device;

        // Configure clocks using stm32f4xx-hal 0.21 builder API
        let rcc = dp.RCC.constrain();
        let clocks = rcc
            .cfgr
            .use_hse(8.MHz()) // NUCLEO-F429ZI has 8 MHz HSE
            .sysclk(168.MHz())
            .hclk(168.MHz())
            .pclk1(42.MHz())
            .pclk2(84.MHz())
            .freeze();

        // Initialize monotonic timer
        Mono::start(cx.core.SYST, clocks.sysclk().to_Hz());

        // Configure GPIO for Ethernet (RMII mode)
        let gpioa = dp.GPIOA.split();
        let gpiob = dp.GPIOB.split();
        let gpioc = dp.GPIOC.split();
        let gpiog = dp.GPIOG.split();

        // RMII pins for NUCLEO-F429ZI
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

        // Initialize Ethernet peripheral
        info!("Initializing Ethernet...");

        let eth_parts_in = PartsIn {
            dma: dp.ETHERNET_DMA,
            mac: dp.ETHERNET_MAC,
            mmc: dp.ETHERNET_MMC,
            ptp: dp.ETHERNET_PTP,
        };

        // Safety: These static mutable references are only created once during init
        // and the DMA hardware requires them to remain valid for the lifetime of the program.
        #[allow(static_mut_refs)]
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
            .expect("Failed to initialize Ethernet")
        };

        // Enable Ethernet interrupts
        dma.enable_interrupt();

        // Create smoltcp interface
        info!("Creating smoltcp interface...");

        let mac_addr = EthernetAddress::from_bytes(&MAC_ADDRESS);
        let config = Config::new(mac_addr.into());

        let mut dma_ref = &mut dma;
        let mut iface = Interface::new(config, &mut dma_ref, Instant::from_millis(0));

        // Set IP address
        iface.update_ip_addrs(|addrs| {
            addrs
                .push(IpCidr::new(
                    IpAddress::v4(IP_ADDRESS[0], IP_ADDRESS[1], IP_ADDRESS[2], IP_ADDRESS[3]),
                    24,
                ))
                .ok();
        });

        // Set default gateway
        iface
            .routes_mut()
            .add_default_ipv4_route(GATEWAY)
            .expect("Failed to add default route");

        info!(
            "IP address: {}.{}.{}.{}",
            IP_ADDRESS[0], IP_ADDRESS[1], IP_ADDRESS[2], IP_ADDRESS[3]
        );

        // Create socket set using link crate's pre-allocated storage
        let storage = unsafe { nano_ros_link_smoltcp::get_socket_storage() };
        let mut sockets = SocketSet::new(&mut storage[..]);

        // ═══════════════════════════════════════════════════════════════════════
        // Initialize nano-ros-link-smoltcp bridge
        // ═══════════════════════════════════════════════════════════════════════
        info!("Initializing smoltcp bridge...");

        SmoltcpBridge::init();

        // Create TCP sockets and register with the bridge
        unsafe { nano_ros_link_smoltcp::create_and_register_sockets(&mut sockets) };

        // Set the network poll callback
        // Note: The actual polling is done in poll_network task via SmoltcpBridge::poll()
        nano_ros_link_smoltcp::set_poll_callback(network_poll_callback);

        info!("smoltcp bridge initialized");

        // ═══════════════════════════════════════════════════════════════════════
        // Initialize zenoh-pico shim session
        // ═══════════════════════════════════════════════════════════════════════
        //
        // Note: The following code requires zenoh-pico to be cross-compiled for
        // ARM Cortex-M. When available, uncomment this section:
        //
        // use nano_ros_transport_zenoh::ShimContext;
        //
        // info!("Connecting to zenoh router...");
        // match ShimContext::new(ZENOH_ROUTER) {
        //     Ok(ctx) => {
        //         info!("Connected to zenoh router!");
        //
        //         // Declare publisher
        //         match ctx.declare_publisher(b"nros/rtic/counter\0") {
        //             Ok(publisher) => {
        //                 info!("Publisher declared for nros/rtic/counter");
        //                 // Store publisher in shared resources
        //             }
        //             Err(e) => error!("Failed to declare publisher: {}", e),
        //         }
        //     }
        //     Err(e) => error!("Failed to connect to zenoh: {}", e),
        // }
        //
        // For now, we demonstrate the network stack integration without zenoh.

        info!("Starting periodic tasks...");

        // Start polling tasks
        poll_network::spawn().ok();
        zenoh_poll::spawn().ok();
        publisher_task::spawn().ok();

        (
            Shared {
                eth_dma: dma,
                iface,
                sockets,
                counter: 0,
            },
            Local {},
        )
    }

    /// Periodic network polling task
    ///
    /// Calls `SmoltcpBridge::poll()` which handles both smoltcp interface
    /// polling and data transfer between smoltcp sockets and zenoh-pico's
    /// staging buffers.
    #[task(shared = [eth_dma, iface, sockets], priority = 2)]
    async fn poll_network(mut cx: poll_network::Context) {
        loop {
            // Update platform clock for zenoh-pico
            let now = Mono::now();
            unsafe { CLOCK_MS = now.ticks() as u64 };

            // Poll smoltcp + bridge data to/from zenoh-pico staging buffers
            (
                &mut cx.shared.eth_dma,
                &mut cx.shared.iface,
                &mut cx.shared.sockets,
            )
                .lock(|mut eth_dma, iface, sockets| {
                    SmoltcpBridge::poll(iface, &mut eth_dma, sockets);
                });

            Mono::delay(POLL_INTERVAL_MS.millis()).await;
        }
    }

    /// Zenoh polling task
    ///
    /// Calls zenoh-pico's spin_once to process incoming messages and
    /// invoke subscriber callbacks.
    #[task(priority = 2)]
    async fn zenoh_poll(_cx: zenoh_poll::Context) {
        loop {
            // When zenoh-pico is available, this would be:
            // if let Some(ctx) = &ZENOH_CONTEXT {
            //     match ctx.spin_once(0) {
            //         Ok(events) if events > 0 => {
            //             debug!("Processed {} zenoh events", events);
            //         }
            //         Ok(_) => {}
            //         Err(e) => warn!("Zenoh spin error: {}", e),
            //     }
            // }

            // For now, just poll the platform layer
            let ret = nano_ros_link_smoltcp::smoltcp_poll();
            if ret < 0 {
                // Poll callback not set or error - expected for stub
            }

            Mono::delay(POLL_INTERVAL_MS.millis()).await;
        }
    }

    /// Publish messages periodically
    ///
    /// Demonstrates publishing ROS 2 messages at a fixed rate.
    #[task(shared = [counter], priority = 1)]
    async fn publisher_task(mut cx: publisher_task::Context) {
        loop {
            // Increment counter
            let count = cx.shared.counter.lock(|c| {
                *c = c.wrapping_add(1);
                *c
            });

            // When zenoh-pico is available, this would publish:
            // if let Some(publisher) = &PUBLISHER {
            //     // Create Int32 message
            //     use nros_core::Serialize;
            //     use nros_serdes::CdrWriter;
            //     use std_msgs::msg::Int32;
            //
            //     let msg = Int32 { data: count as i32 };
            //     let mut buffer = [0u8; 64];
            //     let mut writer = CdrWriter::new(&mut buffer);
            //     if msg.serialize(&mut writer).is_ok() {
            //         let len = writer.position();
            //         if publisher.publish(&buffer[..len]).is_ok() {
            //             info!("Published: counter = {}", count);
            //         }
            //     }
            // }

            info!("Counter = {} (zenoh publish stubbed)", count);

            Mono::delay(PUBLISH_INTERVAL_MS.millis()).await;
        }
    }

    /// Ethernet interrupt handler
    #[task(binds = ETH, shared = [eth_dma], priority = 3)]
    fn eth_interrupt(mut cx: eth_interrupt::Context) {
        cx.shared.eth_dma.lock(|_eth_dma| {
            EthernetDMA::interrupt_handler();
        });
    }
}
