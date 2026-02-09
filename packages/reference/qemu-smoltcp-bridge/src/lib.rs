//! Common code for QEMU bare-metal examples with smoltcp + zenoh-pico
//!
//! This crate provides the bridge between smoltcp (TCP/IP stack) and
//! zenoh-pico's socket operations for bare-metal QEMU examples.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                    Application                          │
//! │               (qemu-rs-talker/listener)                 │
//! └─────────────────────┬───────────────────────────────────┘
//!                       │
//! ┌─────────────────────▼───────────────────────────────────┐
//! │                  zenoh-pico                             │
//! │              (C library, FFI)                           │
//! └─────────────────────┬───────────────────────────────────┘
//!                       │ smoltcp_socket_* FFI calls
//! ┌─────────────────────▼───────────────────────────────────┐
//! │              SmoltcpZenohBridge                         │
//! │      (this crate - bridges FFI to smoltcp)              │
//! └─────────────────────┬───────────────────────────────────┘
//!                       │
//! ┌─────────────────────▼───────────────────────────────────┐
//! │                   smoltcp                               │
//! │              (TCP/IP stack)                             │
//! └─────────────────────┬───────────────────────────────────┘
//!                       │
//! ┌─────────────────────▼───────────────────────────────────┐
//! │               lan9118-smoltcp                           │
//! │           (Ethernet driver)                             │
//! └─────────────────────────────────────────────────────────┘
//! ```

#![no_std]

pub mod bridge;
pub mod clock;
pub mod libc_stubs;

pub use bridge::SmoltcpZenohBridge;
pub use clock::{clock_ms, set_clock_ms};

// Re-export commonly used types
pub use lan9118_smoltcp::{Config as EthConfig, Lan9118, MPS2_AN385_BASE};
pub use smoltcp::iface::{Config as IfaceConfig, Interface, SocketSet};
pub use smoltcp::socket::tcp::{Socket as TcpSocket, SocketBuffer as TcpSocketBuffer};
pub use smoltcp::time::Instant;
pub use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};
