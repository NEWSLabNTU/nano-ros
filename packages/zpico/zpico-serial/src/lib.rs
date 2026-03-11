//! Serial (UART) link layer for nros bare-metal systems
//!
//! Provides the zenoh-pico serial platform symbols (`_z_open_serial_from_dev`,
//! `_z_read_serial_internal`, `_z_send_serial_internal`, etc.) for bare-metal
//! targets where zenoh-pico has no built-in serial backend.
//!
//! # Usage
//!
//! Board crates implement the [`SerialPort`] trait for their UART peripheral,
//! then register the port during hardware init:
//!
//! ```ignore
//! use zpico_serial::{SerialPort, register_port};
//!
//! struct MyUart { /* ... */ }
//!
//! impl SerialPort for MyUart {
//!     fn write(&mut self, data: &[u8]) -> usize { /* ... */ }
//!     fn read(&mut self, buf: &mut [u8]) -> usize { /* ... */ }
//! }
//!
//! // In init_hardware():
//! static mut UART: MaybeUninit<MyUart> = MaybeUninit::uninit();
//! unsafe {
//!     UART.write(MyUart::new(/* ... */));
//!     register_port(0, UART.assume_init_mut());
//! }
//! ```
//!
//! # Scope
//!
//! This crate is only needed for the bare-metal (smoltcp) platform backend.
//! All other platforms (POSIX, Zephyr, FreeRTOS, NuttX, ThreadX) use
//! zenoh-pico's built-in serial implementation.

#![no_std]

mod ffi;
mod port;

pub use port::{SerialPort, register_port, MAX_SERIAL_PORTS};
