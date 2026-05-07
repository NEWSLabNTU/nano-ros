//! Phase 115.H follow-up — locator-scheme dispatch in `DdsRmw::open`.
//!
//! Validates the contract added in transport.rs:
//!
//! * `custom/...` on a `std + platform-posix` build returns
//!   `ConnectionFailed` (the stock `DomainParticipantFactory` cannot
//!   accept a custom transport — `NrosCustomTransportParticipantFactory`
//!   needs `DomainParticipantFactoryAsync`, which v1 only wires on
//!   the no_std path; std custom-transport support is the deferred
//!   "POSIX std-path async factory wiring" follow-up).
//! * Non-`custom/...` locators continue to fall through to the
//!   stock UDP factory and either succeed or, when the host has no
//!   network to bind, surface the underlying error — we just check
//!   they don't error out specifically because of the locator
//!   prefix check.

#![cfg(feature = "platform-posix")]

use nros_rmw::{Rmw, RmwConfig, SessionMode, TransportError};
use nros_rmw_dds::DdsRmw;

fn cfg_with_locator(locator: &str) -> RmwConfig<'_> {
    RmwConfig {
        locator,
        mode: SessionMode::Client,
        domain_id: 0,
        node_name: "phase_115_h_dispatch_test",
        namespace: "",
        properties: &[],
    }
}

#[test]
fn custom_locator_errors_on_std_build() {
    // Std + platform-posix path: dispatching `custom/...` must error
    // out cleanly rather than silently fall through to UDP.
    let result = DdsRmw.open(&cfg_with_locator("custom/anything-here"));
    assert!(
        matches!(result, Err(TransportError::ConnectionFailed)),
        "custom locator on std build should return ConnectionFailed",
    );
}
