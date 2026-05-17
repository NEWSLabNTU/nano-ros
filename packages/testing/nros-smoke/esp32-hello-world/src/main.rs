//! Minimal hello world for ESP32-C3
//!
//! Prints messages over UART and toggles GPIO2 (LED on many dev boards).
//!
//! # Flashing
//!
//! ```bash
//! cargo +nightly run --release
//! ```

#![no_std]
#![no_main]

use esp_backtrace as _;
use esp_hal::{
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    main,
};

esp_bootloader_esp_idf::esp_app_desc!();

#[main]
fn main() -> ! {
    let peripherals = esp_hal::init(esp_hal::Config::default());

    let mut led = Output::new(peripherals.GPIO2, Level::Low, OutputConfig::default());
    let delay = Delay::new();

    esp_println::println!("ESP32-C3 hello world!");
    esp_println::println!("nros ESP32 dev environment ready.");

    let mut counter: u32 = 0;
    loop {
        led.toggle();
        counter = counter.wrapping_add(1);
        esp_println::println!("Blink #{counter}");
        delay.delay_millis(500);
    }
}
