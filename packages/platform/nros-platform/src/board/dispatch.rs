//! Phase 216.A.1 — `DispatchStrategy` enum.
//!
//! Declares how a Node pkg expects its callbacks to fire. The board
//! crate's `NodeDispatchRuntime::dispatch_strategy()` (Phase 216.A.2,
//! lives at `super::runtime`) tells the codegen + check layers which
//! strategies it can serve; `Node::DISPATCH` (Phase 216.A.3, lives at
//! `nros::Node`) tells which strategy a Node requires.
//!
//! `nros check` (Phase 216.D.1) cross-validates Node `DISPATCH` against
//! the Entry pkg's board framework on a `(framework, strategy)` matrix
//! — see the doc body in `docs/roadmap/phase-216-baremetal-framework-
//! integration.md`.
//!
//! `#[repr(u8)]` for FFI stability: the `nros::node!()` macro (Phase
//! 216.A.5) emits a per-Node `__nros_node_<pkg>_dispatch_strategy()
//! -> u8` ABI symbol so `nros check` can read the strategy without
//! linking the Node crate.

/// How a Node expects its callbacks to fire.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum DispatchStrategy {
    /// Callbacks fire from the executor's spin loop (current default).
    /// Served by every runtime: POSIX, RTOS (FreeRTOS/NuttX/Zephyr/
    /// ThreadX), bare-metal, RTIC (proxied via `__nros_dispatch` task
    /// when the board demands it), Embassy (likewise). The default for
    /// every existing Node pkg — preserves backward compat.
    Inline = 0,

    /// Callbacks fire from a framework-owned task (RTIC dispatcher /
    /// Embassy task). The board-side `NodeDispatchRuntime` enqueues
    /// signaled callbacks; the framework's scheduler dequeues + drives
    /// `ExecutableNode::on_callback` from its own task context. Needed
    /// for Nodes whose callbacks must not run from the spin task (e.g.
    /// callbacks that take RTIC locks or share priority with custom
    /// tasks).
    Deferred = 1,

    /// Callbacks fire directly from an ISR handler. Design slot only —
    /// impl deferred to Phase 216.E.1. Requires a reentrancy audit of
    /// the dispatch path + a lock-free SPSC variant tolerant of
    /// ISR-priority producers + a per-Node `#[isr_safe]` proof
    /// contract. Reserved here so the matrix in Phase 216.D.1 has a
    /// stable discriminant to reject against.
    FromIsr = 2,
}

impl DispatchStrategy {
    /// Compile-time default for `Node::DISPATCH`. Inline preserves
    /// every existing Node pkg unchanged.
    pub const DEFAULT: Self = Self::Inline;

    /// FFI round-trip discriminant. Mirrors `as u8` but expressible in
    /// `const` contexts where `as` casts on enums currently require an
    /// `#[allow(non_upper_case_globals)]` dance.
    #[inline]
    pub const fn to_u8(self) -> u8 {
        self as u8
    }

    /// Inverse of `to_u8`. Returns `None` for unknown discriminants —
    /// `nros check` surfaces the rejection with a clear diagnostic
    /// rather than silently treating a future strategy as `Inline`.
    #[inline]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Inline),
            1 => Some(Self::Deferred),
            2 => Some(Self::FromIsr),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_inline() {
        assert_eq!(DispatchStrategy::DEFAULT, DispatchStrategy::Inline);
    }

    #[test]
    fn u8_round_trip() {
        for s in [
            DispatchStrategy::Inline,
            DispatchStrategy::Deferred,
            DispatchStrategy::FromIsr,
        ] {
            assert_eq!(DispatchStrategy::from_u8(s.to_u8()), Some(s));
        }
    }

    #[test]
    fn from_u8_rejects_unknown() {
        assert_eq!(DispatchStrategy::from_u8(3), None);
        assert_eq!(DispatchStrategy::from_u8(255), None);
    }
}
