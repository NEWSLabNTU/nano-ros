//! Minimal PAC for ARM CMSDK Cortex-M3 (MPS2-AN385 FPGA image)
//!
//! Provides only interrupt bindings for use with RTIC v2. No register APIs.
//! Follows the [`lm3s6965`](https://crates.io/crates/lm3s6965) pattern.
//!
//! Interrupt map sourced from ARM `CMSDK_CM3.h` and verified against
//! QEMU `hw/arm/mps2.c` (32 external IRQs, Ethernet at IRQ 13).

#![deny(missing_docs)]
#![no_std]

pub use self::Interrupt as interrupt;
use cortex_m::interrupt::Nr;
pub use cortex_m_rt::interrupt;

/// Number of bits available in the NVIC for configuring priority
pub const NVIC_PRIO_BITS: u8 = 3;

/// CMSDK Cortex-M3 interrupts (MPS2-AN385)
///
/// QEMU configures 32 external NVIC interrupts for the AN385 variant.
/// Names match the ARM CMSDK_CM3.h header definitions.
#[allow(non_camel_case_types)]
#[derive(Clone, Copy)]
pub enum Interrupt {
    /// UART0 Receive
    UARTRX0,
    /// UART0 Transmit
    UARTTX0,
    /// UART1 Receive
    UARTRX1,
    /// UART1 Transmit
    UARTTX1,
    /// UART2 Receive
    UARTRX2,
    /// UART2 Transmit
    UARTTX2,
    /// GPIO Port 0 combined
    PORT0_ALL,
    /// GPIO Port 1 combined
    PORT1_ALL,
    /// CMSDK Timer 0
    TIMER0,
    /// CMSDK Timer 1
    TIMER1,
    /// CMSDK Dual Timer
    DUALTIMER,
    /// SPI
    SPI,
    /// UART 0/1/2 overflow (OR'd)
    UARTOVF,
    /// Ethernet (LAN9118 — wired at IRQ 13 in QEMU)
    ETHERNET,
    /// Audio I2S
    I2S,
    /// Touch Screen Controller
    TSC,
    /// GPIO Port 2 combined
    PORT2_ALL,
    /// GPIO Port 3 combined
    PORT3_ALL,
    /// UART3 Receive
    UARTRX3,
    /// UART3 Transmit
    UARTTX3,
    /// UART4 Receive
    UARTRX4,
    /// UART4 Transmit
    UARTTX4,
    /// ADC SPI
    ADCSPI,
    /// Shield SPI
    SHIELDSPI,
    /// GPIO Port 0 pin 0
    PORT0_0,
    /// GPIO Port 0 pin 1
    PORT0_1,
    /// GPIO Port 0 pin 2
    PORT0_2,
    /// GPIO Port 0 pin 3
    PORT0_3,
    /// GPIO Port 0 pin 4
    PORT0_4,
    /// GPIO Port 0 pin 5
    PORT0_5,
    /// GPIO Port 0 pin 6
    PORT0_6,
    /// GPIO Port 0 pin 7
    PORT0_7,
}

unsafe impl Nr for Interrupt {
    #[inline]
    fn nr(&self) -> u8 {
        *self as u8
    }
}

unsafe extern "C" {
    fn UARTRX0();
    fn UARTTX0();
    fn UARTRX1();
    fn UARTTX1();
    fn UARTRX2();
    fn UARTTX2();
    fn PORT0_ALL();
    fn PORT1_ALL();
    fn TIMER0();
    fn TIMER1();
    fn DUALTIMER();
    fn SPI();
    fn UARTOVF();
    fn ETHERNET();
    fn I2S();
    fn TSC();
    fn PORT2_ALL();
    fn PORT3_ALL();
    fn UARTRX3();
    fn UARTTX3();
    fn UARTRX4();
    fn UARTTX4();
    fn ADCSPI();
    fn SHIELDSPI();
    fn PORT0_0();
    fn PORT0_1();
    fn PORT0_2();
    fn PORT0_3();
    fn PORT0_4();
    fn PORT0_5();
    fn PORT0_6();
    fn PORT0_7();
}

union Vector {
    handler: unsafe extern "C" fn(),
    _reserved: u32,
}

#[unsafe(link_section = ".vector_table.interrupts")]
#[unsafe(no_mangle)]
static __INTERRUPTS: [Vector; 32] = [
    Vector { handler: UARTRX0 },
    Vector { handler: UARTTX0 },
    Vector { handler: UARTRX1 },
    Vector { handler: UARTTX1 },
    Vector { handler: UARTRX2 },
    Vector { handler: UARTTX2 },
    Vector { handler: PORT0_ALL },
    Vector { handler: PORT1_ALL },
    Vector { handler: TIMER0 },
    Vector { handler: TIMER1 },
    Vector { handler: DUALTIMER },
    Vector { handler: SPI },
    Vector { handler: UARTOVF },
    Vector { handler: ETHERNET },
    Vector { handler: I2S },
    Vector { handler: TSC },
    Vector { handler: PORT2_ALL },
    Vector { handler: PORT3_ALL },
    Vector { handler: UARTRX3 },
    Vector { handler: UARTTX3 },
    Vector { handler: UARTRX4 },
    Vector { handler: UARTTX4 },
    Vector { handler: ADCSPI },
    Vector { handler: SHIELDSPI },
    Vector { handler: PORT0_0 },
    Vector { handler: PORT0_1 },
    Vector { handler: PORT0_2 },
    Vector { handler: PORT0_3 },
    Vector { handler: PORT0_4 },
    Vector { handler: PORT0_5 },
    Vector { handler: PORT0_6 },
    Vector { handler: PORT0_7 },
];

/// All peripherals
///
/// Empty — this PAC provides only interrupt bindings.
/// Required by RTIC's `#[app(device = ...)]` attribute.
pub struct Peripherals {
    _0: (),
}

impl Peripherals {
    /// Steal the peripherals (required by RTIC)
    #[inline]
    pub unsafe fn steal() -> Self {
        Peripherals { _0: () }
    }
}
