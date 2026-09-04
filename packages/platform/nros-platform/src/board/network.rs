//! [`NetworkError`] — the vocabulary for a link-up failure.
//!
//! **The `NetworkWait` TRAIT was removed in phase-206 W4 (issue 1067).** It had
//! one implementation and no callers, and the one place that would have called
//! it — the `nros::main!` Zephyr arm — deliberately routed around it, because
//! `ZephyrBoard::wait_link_up` calls `net_if_is_up` / `k_msleep`, `static
//! inline` header functions with no link symbol, so the native_sim final link
//! failed on undefined references.
//!
//! The documented boot order it belonged to could not be built either: the
//! shape the book described,
//! `impl<B: Board + TransportBringup + NetworkWait> BoardEntry for B`, OVERLAPS
//! the twelve direct `BoardEntry` impls, and Rust has no "call this method if
//! the type happens to implement it". "Skipped if the board doesn't impl the
//! mixin" was not expressible.
//!
//! What boards actually do — and the contract that now stands — is
//! [`super::BoardEntry::run`], whose body (usually a family helper such as
//! `nros_board_freertos::run_entry`) owns bring-up in the order that board
//! needs. `ZephyrBoard::wait_link_up` survives as an INHERENT method, which is
//! how its one caller (its own README example) already used it.
//!
//! The error type stays: it is the shared vocabulary a board reports a link
//! failure in, and it is used whether or not a trait wraps it.

/// Network bringup failure mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NetworkError {
    /// PHY linked but no DHCP lease before the board's deadline.
    DhcpTimeout,
    /// Static-IP configuration referenced a non-existent interface
    /// or duplicate address.
    ConfigInvalid,
    /// No default route or gateway unreachable.
    NoRoute,
    /// Board-specific failure not covered by the above.
    Other,
}
