//! STM32F4 USART driver
//!
//! Minimal MMIO driver for the STM32F4 USART peripherals. Uses raw register
//! access (PAC-level) rather than the HAL `Serial` type, which requires owning
//! GPIO pins and makes static storage difficult.
//!
//! # Supported peripherals
//!
//! | Peripheral | Base address    | Bus  | Clock (168 MHz sysclk) |
//! |------------|-----------------|------|------------------------|
//! | USART1     | `0x4001_1000`   | APB2 | 84 MHz                 |
//! | USART2     | `0x4000_4400`   | APB1 | 42 MHz                 |
//! | USART3     | `0x4000_4800`   | APB1 | 42 MHz                 |
//! | UART4      | `0x4000_4C00`   | APB1 | 42 MHz                 |
//! | UART5      | `0x4000_5000`   | APB1 | 42 MHz                 |
//! | USART6     | `0x4001_1400`   | APB2 | 84 MHz                 |
//!
//! # GPIO alternate function
//!
//! All STM32F4 USARTs use **AF7** for their TX/RX pins. The caller must
//! configure the appropriate GPIO pins as alternate function before enabling
//! the USART. This crate provides [`configure_gpio_af`] for raw MMIO GPIO
//! pin setup (no HAL pin ownership needed).
//!
//! # Usage
//!
//! ```ignore
//! use stm32f4_usart::{Stm32f4Usart, configure_gpio_af};
//!
//! // Enable GPIOB clock and configure PB10 (TX) / PB11 (RX) as AF7
//! configure_gpio_af(GPIOB_BASE, 10, 7); // TX
//! configure_gpio_af(GPIOB_BASE, 11, 7); // RX
//!
//! let mut usart = Stm32f4Usart::new(USART3_BASE);
//! usart.configure(42_000_000, 115200);
//! usart.enable();
//! usart.write(b"Hello\n");
//! ```

#![no_std]

use core::ptr;

use zpico_serial::SerialPort;

// ============================================================================
// Base addresses
// ============================================================================

/// USART1 base address (APB2, 84 MHz).
pub const USART1_BASE: usize = 0x4001_1000;
/// USART2 base address (APB1, 42 MHz).
pub const USART2_BASE: usize = 0x4000_4400;
/// USART3 base address (APB1, 42 MHz).
pub const USART3_BASE: usize = 0x4000_4800;
/// UART4 base address (APB1, 42 MHz).
pub const UART4_BASE: usize = 0x4000_4C00;
/// UART5 base address (APB1, 42 MHz).
pub const UART5_BASE: usize = 0x4000_5000;
/// USART6 base address (APB2, 84 MHz).
pub const USART6_BASE: usize = 0x4001_1400;

// ============================================================================
// GPIO base addresses
// ============================================================================

/// GPIOA base address.
pub const GPIOA_BASE: usize = 0x4002_0000;
/// GPIOB base address.
pub const GPIOB_BASE: usize = 0x4002_0400;
/// GPIOC base address.
pub const GPIOC_BASE: usize = 0x4002_0800;
/// GPIOD base address.
pub const GPIOD_BASE: usize = 0x4002_0C00;

// ============================================================================
// RCC base address and register offsets
// ============================================================================

/// RCC base address.
const RCC_BASE: usize = 0x4002_3800;
/// RCC AHB1 peripheral clock enable register offset.
const RCC_AHB1ENR: usize = 0x30;
/// RCC APB1 peripheral clock enable register offset.
const RCC_APB1ENR: usize = 0x40;
/// RCC APB2 peripheral clock enable register offset.
const RCC_APB2ENR: usize = 0x44;

// ============================================================================
// USART register offsets
// ============================================================================

/// Status register offset.
const SR: usize = 0x00;
/// Data register offset.
const DR: usize = 0x04;
/// Baud rate register offset.
const BRR: usize = 0x08;
/// Control register 1 offset.
const CR1: usize = 0x0C;

// ============================================================================
// SR register bits
// ============================================================================

/// RX register not empty.
const SR_RXNE: u32 = 1 << 5;
/// TX register empty.
const SR_TXE: u32 = 1 << 7;

// ============================================================================
// CR1 register bits
// ============================================================================

/// Receiver enable.
const CR1_RE: u32 = 1 << 2;
/// Transmitter enable.
const CR1_TE: u32 = 1 << 3;
/// USART enable.
const CR1_UE: u32 = 1 << 13;

// ============================================================================
// GPIO register offsets
// ============================================================================

/// GPIO mode register offset.
const GPIO_MODER: usize = 0x00;
/// GPIO output speed register offset.
const GPIO_OSPEEDR: usize = 0x08;
/// GPIO pull-up/pull-down register offset.
const GPIO_PUPDR: usize = 0x0C;
/// GPIO alternate function low register offset (pins 0-7).
const GPIO_AFRL: usize = 0x20;
/// GPIO alternate function high register offset (pins 8-15).
const GPIO_AFRH: usize = 0x24;

// ============================================================================
// USART clock enable bits
// ============================================================================

/// RCC APB1ENR bit for USART2.
const RCC_APB1ENR_USART2EN: u32 = 1 << 17;
/// RCC APB1ENR bit for USART3.
const RCC_APB1ENR_USART3EN: u32 = 1 << 18;
/// RCC APB1ENR bit for UART4.
const RCC_APB1ENR_UART4EN: u32 = 1 << 19;
/// RCC APB1ENR bit for UART5.
const RCC_APB1ENR_UART5EN: u32 = 1 << 20;
/// RCC APB2ENR bit for USART1.
const RCC_APB2ENR_USART1EN: u32 = 1 << 4;
/// RCC APB2ENR bit for USART6.
const RCC_APB2ENR_USART6EN: u32 = 1 << 5;

// ============================================================================
// Driver
// ============================================================================

/// STM32F4 USART driver.
///
/// Provides polled (non-interrupt) TX and RX for any STM32F4 USART/UART
/// peripheral. The driver busy-waits on TX (acceptable for serial baud rates)
/// and returns immediately for RX if no data is available.
pub struct Stm32f4Usart {
    base: usize,
}

impl Stm32f4Usart {
    /// Create a new USART driver for the given base address.
    ///
    /// Does **not** enable the peripheral — call [`configure`](Self::configure)
    /// and [`enable`](Self::enable) after construction.
    pub const fn new(base: usize) -> Self {
        Self { base }
    }

    /// Configure the baud rate.
    ///
    /// `pclk_hz` is the peripheral bus clock frequency in Hz:
    /// - APB1 (42 MHz for 168 MHz sysclk): USART2, USART3, UART4, UART5
    /// - APB2 (84 MHz for 168 MHz sysclk): USART1, USART6
    ///
    /// The BRR register is computed as: `mantissa.fraction` where
    /// `USARTDIV = pclk / (16 * baudrate)`.
    pub fn configure(&mut self, pclk_hz: u32, baudrate: u32) {
        // USARTDIV = pclk / (16 * baudrate)
        // BRR = (mantissa << 4) | fraction
        // Using fixed-point: multiply by 16 to get 4 fractional bits
        //   usartdiv_x16 = (pclk * 16) / (16 * baudrate) = pclk / baudrate
        // But we need sub-integer precision, so:
        //   usartdiv_x16 = (2 * pclk + baudrate) / (2 * baudrate)  [rounded]
        let usartdiv_x16 = (2 * pclk_hz + baudrate) / (2 * baudrate);
        let mantissa = usartdiv_x16 >> 4;
        let fraction = usartdiv_x16 & 0x0F;
        let brr = (mantissa << 4) | fraction;
        self.write_reg(BRR, brr);
    }

    /// Enable the USART with both TX and RX.
    pub fn enable(&mut self) {
        let cr1 = self.read_reg(CR1);
        self.write_reg(CR1, cr1 | CR1_UE | CR1_TE | CR1_RE);
    }

    /// Disable the USART.
    pub fn disable(&mut self) {
        let cr1 = self.read_reg(CR1);
        self.write_reg(CR1, cr1 & !(CR1_UE | CR1_TE | CR1_RE));
    }

    /// Check if the TX data register is empty (ready for next byte).
    #[inline]
    fn tx_empty(&self) -> bool {
        self.read_reg(SR) & SR_TXE != 0
    }

    /// Check if the RX data register has data.
    #[inline]
    fn rx_ready(&self) -> bool {
        self.read_reg(SR) & SR_RXNE != 0
    }

    #[inline]
    fn read_reg(&self, offset: usize) -> u32 {
        unsafe { ptr::read_volatile((self.base + offset) as *const u32) }
    }

    #[inline]
    fn write_reg(&self, offset: usize, val: u32) {
        unsafe { ptr::write_volatile((self.base + offset) as *mut u32, val) }
    }
}

impl SerialPort for Stm32f4Usart {
    fn write(&mut self, data: &[u8]) -> usize {
        for &byte in data {
            // Busy-wait until TX data register is empty
            while !self.tx_empty() {
                core::hint::spin_loop();
            }
            self.write_reg(DR, byte as u32);
        }
        data.len()
    }

    fn read(&mut self, buf: &mut [u8]) -> usize {
        let mut n = 0;
        while n < buf.len() && self.rx_ready() {
            buf[n] = (self.read_reg(DR) & 0xFF) as u8;
            n += 1;
        }
        n
    }
}

// ============================================================================
// GPIO and clock helpers
// ============================================================================

/// Enable the clock for a GPIO port via RCC AHB1ENR.
///
/// `port_index`: 0 = GPIOA, 1 = GPIOB, 2 = GPIOC, 3 = GPIOD, etc.
///
/// # Safety
///
/// Writes to RCC registers. Must not race with other RCC configuration.
pub unsafe fn enable_gpio_clock(port_index: u8) {
    let rcc_ahb1enr = (RCC_BASE + RCC_AHB1ENR) as *mut u32;
    unsafe {
        let val = ptr::read_volatile(rcc_ahb1enr);
        ptr::write_volatile(rcc_ahb1enr, val | (1 << port_index));
    }
}

/// Enable the clock for a USART peripheral via RCC APBxENR.
///
/// `usart_index`: 1-based USART number (1 = USART1, 2 = USART2, etc.)
///
/// # Safety
///
/// Writes to RCC registers. Must not race with other RCC configuration.
///
/// # Panics
///
/// Panics if `usart_index` is 0 or greater than 6.
pub unsafe fn enable_usart_clock(usart_index: u8) {
    match usart_index {
        1 => unsafe {
            let reg = (RCC_BASE + RCC_APB2ENR) as *mut u32;
            let val = ptr::read_volatile(reg);
            ptr::write_volatile(reg, val | RCC_APB2ENR_USART1EN);
        },
        2 => unsafe {
            let reg = (RCC_BASE + RCC_APB1ENR) as *mut u32;
            let val = ptr::read_volatile(reg);
            ptr::write_volatile(reg, val | RCC_APB1ENR_USART2EN);
        },
        3 => unsafe {
            let reg = (RCC_BASE + RCC_APB1ENR) as *mut u32;
            let val = ptr::read_volatile(reg);
            ptr::write_volatile(reg, val | RCC_APB1ENR_USART3EN);
        },
        4 => unsafe {
            let reg = (RCC_BASE + RCC_APB1ENR) as *mut u32;
            let val = ptr::read_volatile(reg);
            ptr::write_volatile(reg, val | RCC_APB1ENR_UART4EN);
        },
        5 => unsafe {
            let reg = (RCC_BASE + RCC_APB1ENR) as *mut u32;
            let val = ptr::read_volatile(reg);
            ptr::write_volatile(reg, val | RCC_APB1ENR_UART5EN);
        },
        6 => unsafe {
            let reg = (RCC_BASE + RCC_APB2ENR) as *mut u32;
            let val = ptr::read_volatile(reg);
            ptr::write_volatile(reg, val | RCC_APB2ENR_USART6EN);
        },
        _ => panic!("Invalid USART index"),
    }
}

/// Configure a GPIO pin as alternate function with high speed and pull-up.
///
/// Sets the pin to:
/// - Mode: Alternate function (MODER = 0b10)
/// - Speed: High (OSPEEDR = 0b10)
/// - Pull: Pull-up (PUPDR = 0b01)
/// - Alternate function: `af` (0–15)
///
/// `gpio_base` is the GPIO port base address (e.g., [`GPIOB_BASE`]).
/// `pin` is the pin number (0–15).
/// `af` is the alternate function number (e.g., 7 for USART).
///
/// # Safety
///
/// Writes to GPIO registers. The GPIO port clock must be enabled first
/// (see [`enable_gpio_clock`]).
pub unsafe fn configure_gpio_af(gpio_base: usize, pin: u8, af: u8) {
    let pin = pin as usize;

    // Set mode to alternate function (0b10)
    let moder = (gpio_base + GPIO_MODER) as *mut u32;
    unsafe {
        let val = ptr::read_volatile(moder);
        let val = val & !(0b11 << (pin * 2)); // Clear
        let val = val | (0b10 << (pin * 2)); // Set AF mode
        ptr::write_volatile(moder, val);
    }

    // Set speed to high (0b10)
    let ospeedr = (gpio_base + GPIO_OSPEEDR) as *mut u32;
    unsafe {
        let val = ptr::read_volatile(ospeedr);
        let val = val & !(0b11 << (pin * 2));
        let val = val | (0b10 << (pin * 2));
        ptr::write_volatile(ospeedr, val);
    }

    // Set pull-up (0b01) — needed for USART idle-high line
    let pupdr = (gpio_base + GPIO_PUPDR) as *mut u32;
    unsafe {
        let val = ptr::read_volatile(pupdr);
        let val = val & !(0b11 << (pin * 2));
        let val = val | (0b01 << (pin * 2));
        ptr::write_volatile(pupdr, val);
    }

    // Set alternate function
    let af = af as u32;
    if pin < 8 {
        let afrl = (gpio_base + GPIO_AFRL) as *mut u32;
        unsafe {
            let val = ptr::read_volatile(afrl);
            let val = val & !(0x0F << (pin * 4));
            let val = val | (af << (pin * 4));
            ptr::write_volatile(afrl, val);
        }
    } else {
        let afrh = (gpio_base + GPIO_AFRH) as *mut u32;
        let pos = (pin - 8) * 4;
        unsafe {
            let val = ptr::read_volatile(afrh);
            let val = val & !(0x0F << pos);
            let val = val | (af << pos);
            ptr::write_volatile(afrh, val);
        }
    }
}

/// Return the base address for a 1-based USART index.
///
/// # Panics
///
/// Panics if `usart_index` is 0 or greater than 6.
pub const fn usart_base(usart_index: u8) -> usize {
    match usart_index {
        1 => USART1_BASE,
        2 => USART2_BASE,
        3 => USART3_BASE,
        4 => UART4_BASE,
        5 => UART5_BASE,
        6 => USART6_BASE,
        _ => panic!("Invalid USART index"),
    }
}

/// Return the peripheral bus clock frequency (Hz) for a 1-based USART index,
/// assuming 168 MHz sysclk, 42 MHz APB1, 84 MHz APB2.
///
/// # Panics
///
/// Panics if `usart_index` is 0 or greater than 6.
pub const fn usart_pclk_hz(usart_index: u8) -> u32 {
    match usart_index {
        // APB2: USART1, USART6
        1 | 6 => 84_000_000,
        // APB1: USART2, USART3, UART4, UART5
        2..=5 => 42_000_000,
        _ => panic!("Invalid USART index"),
    }
}

/// Default USART pin mapping for common boards.
///
/// Returns `(gpio_base, tx_pin, rx_pin, af)` for the given USART index.
/// These are the most common pin assignments on NUCLEO-F429ZI and
/// STM32F4-Discovery boards:
///
/// | USART  | TX pin | RX pin | AF |
/// |--------|--------|--------|----|
/// | USART1 | PA9    | PA10   | 7  |
/// | USART2 | PA2    | PA3    | 7  |
/// | USART3 | PB10   | PB11   | 7  |
/// | UART4  | PA0    | PA1    | 8  |
/// | UART5  | PC12   | PD2    | 8  |
/// | USART6 | PC6    | PC7    | 8  |
///
/// # Panics
///
/// Panics if `usart_index` is 0 or greater than 6.
pub const fn default_pins(usart_index: u8) -> UsartPins {
    match usart_index {
        1 => UsartPins {
            tx_gpio_base: GPIOA_BASE,
            tx_pin: 9,
            rx_gpio_base: GPIOA_BASE,
            rx_pin: 10,
            af: 7,
        },
        2 => UsartPins {
            tx_gpio_base: GPIOA_BASE,
            tx_pin: 2,
            rx_gpio_base: GPIOA_BASE,
            rx_pin: 3,
            af: 7,
        },
        3 => UsartPins {
            tx_gpio_base: GPIOB_BASE,
            tx_pin: 10,
            rx_gpio_base: GPIOB_BASE,
            rx_pin: 11,
            af: 7,
        },
        4 => UsartPins {
            tx_gpio_base: GPIOA_BASE,
            tx_pin: 0,
            rx_gpio_base: GPIOA_BASE,
            rx_pin: 1,
            af: 8,
        },
        5 => UsartPins {
            tx_gpio_base: GPIOC_BASE,
            tx_pin: 12,
            rx_gpio_base: GPIOD_BASE,
            rx_pin: 2,
            af: 8,
        },
        6 => UsartPins {
            tx_gpio_base: GPIOC_BASE,
            tx_pin: 6,
            rx_gpio_base: GPIOC_BASE,
            rx_pin: 7,
            af: 8,
        },
        _ => panic!("Invalid USART index"),
    }
}

/// Pin configuration for a USART peripheral.
#[derive(Clone, Copy)]
pub struct UsartPins {
    /// GPIO port base address for the TX pin.
    pub tx_gpio_base: usize,
    /// TX pin number (0–15).
    pub tx_pin: u8,
    /// GPIO port base address for the RX pin.
    pub rx_gpio_base: usize,
    /// RX pin number (0–15).
    pub rx_pin: u8,
    /// Alternate function number.
    pub af: u8,
}

/// GPIO port index from a GPIO base address.
///
/// Returns 0 for GPIOA, 1 for GPIOB, etc.
const fn gpio_port_index(gpio_base: usize) -> u8 {
    ((gpio_base - GPIOA_BASE) / 0x400) as u8
}

/// Initialize a USART peripheral with GPIO and clock setup.
///
/// This is a convenience function that:
/// 1. Enables the GPIO port clock(s)
/// 2. Enables the USART peripheral clock
/// 3. Configures TX and RX pins as alternate function
/// 4. Sets the baud rate and enables the USART
///
/// Uses [`default_pins`] for pin assignment. For custom pin mappings,
/// use the individual functions directly.
///
/// # Safety
///
/// Must not race with other RCC or GPIO configuration. Should be called
/// once during hardware initialization.
///
/// # Panics
///
/// Panics if `usart_index` is 0 or greater than 6.
pub unsafe fn init_usart(usart_index: u8, baudrate: u32) -> Stm32f4Usart {
    let pins = default_pins(usart_index);
    unsafe { init_usart_with_pins(usart_index, baudrate, &pins) }
}

/// Initialize a USART peripheral with explicit pin configuration.
///
/// Like [`init_usart`] but allows specifying custom pins.
///
/// # Safety
///
/// Must not race with other RCC or GPIO configuration.
pub unsafe fn init_usart_with_pins(
    usart_index: u8,
    baudrate: u32,
    pins: &UsartPins,
) -> Stm32f4Usart {
    // Enable GPIO port clocks
    unsafe {
        enable_gpio_clock(gpio_port_index(pins.tx_gpio_base));
        enable_gpio_clock(gpio_port_index(pins.rx_gpio_base));
    }

    // Enable USART peripheral clock
    unsafe {
        enable_usart_clock(usart_index);
    }

    // Configure GPIO pins as alternate function
    unsafe {
        configure_gpio_af(pins.tx_gpio_base, pins.tx_pin, pins.af);
        configure_gpio_af(pins.rx_gpio_base, pins.rx_pin, pins.af);
    }

    // Configure and enable the USART
    let pclk = usart_pclk_hz(usart_index);
    let mut usart = Stm32f4Usart::new(usart_base(usart_index));
    usart.configure(pclk, baudrate);
    usart.enable();

    usart
}
