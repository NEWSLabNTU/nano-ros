//! Phase 99.F — verify SlotLending/SlotBorrowing trait conformance for
//! the zenoh-pico backend at compile time. End-to-end zero-copy tests
//! that exercise a real session are exercised by the
//! `examples/native/rust/zenoh/{talker,listener}` migrations in 99.J.

#![cfg(all(feature = "platform-posix", feature = "lending"))]

use nros_rmw::{SlotBorrowing, SlotLending};
use nros_rmw_zenoh::shim::{ZenohPublisher, ZenohSubscriber};

/// `ZenohPublisher: SlotLending` and `ZenohSubscriber: SlotBorrowing`
/// — proven by accepting them as bounded generic arguments. If the
/// trait impls go missing, the file fails to compile.
fn _publisher_implements_slot_lending<P: SlotLending>() {}
fn _subscriber_implements_slot_borrowing<S: SlotBorrowing>() {}

#[test]
fn zenoh_publisher_impls_slot_lending() {
    _publisher_implements_slot_lending::<ZenohPublisher>();
}

#[test]
fn zenoh_subscriber_impls_slot_borrowing() {
    _subscriber_implements_slot_borrowing::<ZenohSubscriber>();
}
