//! nros STM32F4 Talker Example (`nros::main!()` BoardEntry shape).
//!
//! Publishes an incrementing `std_msgs/Int32` on `/chatter` once per second,
//! compatible with ROS 2 nodes via rmw_zenoh. The application logic lives in
//! the sibling `talker_pkg` Node pkg; the boot scaffold (reset → executor →
//! spin) is owned by `nros::main!()` + `nros-board-stm32f4`.
//!
//! # Hardware
//!
//! - Board: NUCLEO-F429ZI (or similar STM32F4 with Ethernet)
//! - Connect Ethernet cable to the board's RJ45 port
//!
//! # Network Configuration
//!
//! Static IP (baked into the deploy overlay in `Cargo.toml`):
//! - Device IP: 192.168.1.10/24
//! - Gateway: 192.168.1.1
//! - Zenoh Router: 192.168.1.1:7447
//!
//! # Logging
//!
//! STM32F4 logs via defmt (not semihosting): `nros-board-stm32f4` forwards
//! every `nros-log` record to `defmt::{info,warn,error,…}!`, so the
//! `defmt_rtt` + `probe-rs attach` workflow keeps working. The defmt global
//! logger (`defmt_rtt`), the timestamp function, and a defmt panic handler
//! (`panic_probe`) therefore stay in this entry bin.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

defmt::timestamp!("{=u64:us}", { 0 });

nros::main!();
