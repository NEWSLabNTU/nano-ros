//! Phase 99.G — verify SlotLending/SlotBorrowing trait conformance for
//! the XRCE-DDS backend at compile time. End-to-end zero-copy tests
//! against a real Micro-XRCE-DDS-Agent are tracked under 99.J.
//!
//! Run with:
//!   cargo test -p nros-rmw-xrce --features lending,xrce-udp --test lending_traits

#![cfg(feature = "lending")]

use nros_rmw::{SlotBorrowing, SlotLending};
use nros_rmw_xrce::{XrcePublisher, XrceSubscriber};

fn _publisher_implements_slot_lending<P: SlotLending>() {}
fn _subscriber_implements_slot_borrowing<S: SlotBorrowing>() {}

#[test]
fn xrce_publisher_impls_slot_lending() {
    _publisher_implements_slot_lending::<XrcePublisher>();
}

#[test]
fn xrce_subscriber_impls_slot_borrowing() {
    _subscriber_implements_slot_borrowing::<XrceSubscriber>();
}
