//! Platform-specific hardware support
//!
//! Each platform module provides:
//! - `create_ethernet(mac: [u8; 6])` - Initialize the Ethernet driver
//! - Implementation of `EthernetDevice` trait
//!
//! # Available Platforms
//!
//! - `qemu_mps2` - QEMU MPS2-AN385 with LAN9118 Ethernet (feature: `qemu-mps2`)

#[cfg(feature = "qemu-mps2")]
pub mod qemu_mps2;
