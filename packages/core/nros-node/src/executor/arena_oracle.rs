//! phase-412 #4 — the arena's HOST ORACLE, held to at compile time.
//!
//! `nros-node/build.rs` is the one place that knows what this image's declared
//! entities cost: it sums the per-kind model over `NROS_ENTITY_COUNT_*` (and the
//! declared depths) and emits the result as [`arena_model::REQUIRED`] beside
//! the arena it derives from it (`cargo:arena_size`). Before this module that
//! requirement stopped at the derivation. An image that STATED
//! `NROS_EXECUTOR_ARENA_SIZE` below it built, linked, flashed and then died at
//! the first registration that did not fit, with `NodeError::BufferTooSmall` —
//! on a board whose only diagnostic channel may be a RAM record read back with
//! a debugger (issue 1036).
//!
//! Here the two numbers meet in one compilation, so the refusal is a build
//! error that names the knob, both numbers and the shortfall.
//!
//! # Which arms can be held, and which cannot
//!
//! The arena is a slice of caller-supplied backing, so "the arena" is really
//! one number per PLACEMENT. A compile-time check needs the arena and the
//! requirement to be constants in the same compilation; that decides it.
//!
//! | arm | arena comes from | checked |
//! | --- | --- | --- |
//! | Rust `.bss` `EXECUTOR_BACKING` | `ExecutorSizing::DEFAULT.arena` = `ARENA_SIZE` | compile time, the `ARENA_SIZE` const item below |
//! | Rust heap, default sizing (`open` / `open_multi` / `open_with_rmw` past the static) | the same `DEFAULT` | compile time, same assertion |
//! | C `nros_executor_t::_opaque` | `EXECUTOR_OPAQUE_U64S`, measured from `ExecutorInlineStorage` = `DEFAULT` | compile time, same assertion (`nros-c` compiles after it) |
//! | C++ `Node::GlobalStorageHolder` / `Executor::storage_` | `CPP_EXECUTOR_OPAQUE_U64S`, measured the same way | compile time, same assertion |
//! | Rust `open_in` over a caller's `const` sizing | the caller's `ExecutorSizing::arena` | compile time, OPT-IN: [`ExecutorSizing::assert_covers_model`](super::ExecutorSizing::assert_covers_model) |
//! | Rust heap, per-entry sizing (`open_sized`, `arena_size_for(cbs)`) | a RUNTIME `cbs` handed through `BoardEntry` | NO — runtime |
//! | Rust `open_in` over a runtime slice / runtime sizing | runtime values | NO — runtime |
//!
//! The runtime arms are not faked. Their arena is a value, not a constant, and
//! an executor there may hold a SUBSET of the image (a tier), so the
//! image-wide model is not even the right bound. What they get is the
//! registration-time refusal every arm already has: `arena_alloc` and
//! `arena_alloc_with_trailing` report the entity, the shortfall and the knob
//! (`report_arena_exhausted`, issues 0900 / 1036) and write the shortfall to
//! the boot record.
//!
//! # What the model is, and what it is not
//!
//! [`REQUIRED`](arena_model::REQUIRED) is the per-kind sum, whose terms
//! `executor::arena` asserts to be upper bounds on the entry types they stand
//! for. So an arena at or above it fits what was declared. It is NOT a lower
//! bound on what an image needs: the terms over-price (issue 1255 prices every
//! subscription at one image-wide type bound), so an arena stated below the
//! model may still fit. The refusal is deliberate anyway — a number below the
//! model is a claim the build cannot check, and the tree's answer to an
//! over-priced model is to correct the model's INPUTS (declared depths,
//! `NROS_PUBSUB_QOS_DEPTH`, per-type bounds), which re-derive the arena, rather
//! than to hand-state a number that goes stale the next time any term moves.
//!
//! An image that declares nothing has `REQUIRED == 0`: the derivation then
//! budgeted a worst case over `NROS_EXECUTOR_MAX_CBS` slots, which is a budget
//! and not a requirement, and stating less than a budget is the ordinary use of
//! the knob. Nothing is refused there.
//!
//! [`arena_model::REQUIRED`]: crate::config::arena_model::REQUIRED

use crate::config::arena_model;

/// The arm every default-sized backing shares, held in the one compilation
/// every image makes.
///
/// The Rust `.bss` static (`executor::backing`), the heap fallback of the
/// `alloc` constructors, the C `nros_executor_t::_opaque` and the C++
/// `Node::GlobalStorageHolder` storage are all sized from
/// `ExecutorSizing::DEFAULT`, whose arena IS `ARENA_SIZE`, and each already
/// refuses at compile time to be smaller than that sizing. So this one item
/// covers all four.
///
/// Here rather than beside `ExecutorSizing` because `storage` is compiled only
/// with an RMW seam, and this must fire in every build of the crate.
///
/// It can fire only when `NROS_EXECUTOR_ARENA_SIZE` is STATED: a derived arena
/// is the model, floored.
const _: () = assert_covers(
    crate::config::ARENA_SIZE,
    arena_model::REQUIRED,
    "NROS_EXECUTOR_ARENA_SIZE (Zephyr: CONFIG_NROS_EXECUTOR_ARENA_SIZE)",
    Remedy::Knob,
);

/// Bytes `arena` falls short of this image's modelled requirement; `0` when it
/// covers it, or when the image declared nothing.
pub const fn shortfall(arena: usize) -> usize {
    arena_model::REQUIRED.saturating_sub(arena)
}

/// Refuse, during CONST EVALUATION, an arena below this image's model.
///
/// `source` names where `arena` came from — the knob for the default sizing,
/// the caller's own constant otherwise — because a refusal that does not say
/// which number to move is the half-diagnostic issue 1036 was about.
///
/// Call it from a `const _: () = ...;` item; that is what makes it a build
/// error. Called at run time it is an ordinary panic with the same text.
pub const fn assert_covers_model(arena: usize, source: &'static str) {
    assert_covers(arena, arena_model::REQUIRED, source, Remedy::Caller)
}

/// Which number the reader has to move, because the two arms differ: the
/// default arena is a KNOB the build derives when it is left alone, while a
/// caller's sizing is the caller's own constant and unsetting the knob does
/// nothing for it (the bench's `arena_size_for(2)` failed with the knob unset).
#[derive(Clone, Copy)]
enum Remedy {
    Knob,
    Caller,
}

/// [`assert_covers_model`] against an explicit requirement. Split out so the
/// comparison and its message are testable without a build that declares
/// entities.
const fn assert_covers(arena: usize, required: usize, source: &str, remedy: Remedy) {
    if arena >= required {
        return;
    }
    let msg = Refusal::new(source, arena, required, remedy);
    // `panic!("{}", <&str>)` is the one formatted panic const evaluation
    // accepts, so the numbers are rendered into the buffer first.
    panic!("{}", msg.as_str())
}

/// The refusal text, built without `alloc` or `core::fmt` so it can be built in
/// a `const` context — a build error that said "arena too small" and not by how
/// much would send the reader to `build.rs` to redo the arithmetic.
struct Refusal {
    buf: [u8; Self::CAP],
    len: usize,
}

impl Refusal {
    /// Generous for the longest `source` in the tree plus three 20-digit
    /// numbers; a longer one truncates rather than failing to build.
    const CAP: usize = 768;

    const fn new(source: &str, arena: usize, required: usize, remedy: Remedy) -> Self {
        let head = Self {
            buf: [0; Self::CAP],
            len: 0,
        }
        .text("nros arena oracle (phase-412): ")
        .text(source)
        .text(" = ")
        .num(arena)
        .text(" B, but the entities this image declares are modelled at ")
        .num(required)
        .text(" B (")
        .num(required - arena)
        .text(" B short; nros-node/build.rs, arena_model::REQUIRED). Every entity")
        .text(" registration past it fails at run time with BufferTooSmall. Give ")
        .text(source)
        .text(" at least ")
        .num(required)
        .text(" B");
        let head = match remedy {
            Remedy::Knob => {
                head.text(", or leave it unset so the build derives it from the same model.")
            }
            Remedy::Caller => head
                .text(": this is the caller's own sizing, which the knob does not reach")
                .text(" (arena_size_for(cbs) scales the default by cbs / MAX_CBS; it is")
                .text(" not the model). Size it from ExecutorSizing::DEFAULT.arena instead."),
        };
        head.text(" If the model over-prices this image, fix its inputs (declared QoS")
            .text(" depths, NROS_PUBSUB_QOS_DEPTH; issue 1255) instead of stating less.")
    }

    const fn text(mut self, s: &str) -> Self {
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() && self.len < Self::CAP {
            self.buf[self.len] = b[i];
            self.len += 1;
            i += 1;
        }
        self
    }

    const fn num(mut self, mut v: usize) -> Self {
        let mut digits = [0u8; 20];
        let mut n = 0;
        loop {
            digits[n] = b'0' + (v % 10) as u8;
            n += 1;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        while n > 0 && self.len < Self::CAP {
            n -= 1;
            self.buf[self.len] = digits[n];
            self.len += 1;
        }
        self
    }

    const fn as_str(&self) -> &str {
        let (head, _) = self.buf.split_at(self.len);
        match core::str::from_utf8(head) {
            Ok(s) => s,
            // Only reachable if a non-ASCII `source` was cut mid-character at
            // the cap. The numbers are then lost, which is why the cap is large.
            Err(_) => "nros arena oracle (phase-412): the arena is below the declared model",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Refusal, Remedy, assert_covers, shortfall};

    /// The numbers the refusal must carry, from the reference island's shape
    /// (46,272 modelled) against a stated 8,192.
    ///
    /// The source is a neutral label, not the knob's name: `config-knob-census`
    /// reads this crate's sources and counts a call taking a knob-shaped literal
    /// first as a knob READ. The real knob name in the real build error is
    /// asserted by `just check node-std-tests`, against a real build.
    #[test]
    fn the_refusal_names_the_source_both_numbers_and_the_shortfall() {
        for remedy in [Remedy::Knob, Remedy::Caller] {
            let r = Refusal::new("the arena knob", 8_192, 46_272, remedy);
            let s = r.as_str();
            assert!(s.contains("the arena knob = 8192 B"), "{s}");
            assert!(s.contains("modelled at 46272 B"), "{s}");
            assert!(s.contains("(38080 B short"), "{s}");
            assert!(s.contains("at least 46272 B"), "{s}");
            assert!(
                s.len() < Refusal::CAP,
                "the message reached the cap and was cut: {s}"
            );
        }
    }

    /// The two arms need different advice, and the wrong advice is worse than
    /// none: "leave the knob unset" was printed for the bench's own sizing,
    /// whose build had the knob unset already.
    #[test]
    fn only_the_knob_arm_is_told_to_unset_the_knob() {
        let knob = Refusal::new("K", 1, 2, Remedy::Knob);
        let caller = Refusal::new("S", 1, 2, Remedy::Caller);
        assert!(
            knob.as_str().contains("leave it unset"),
            "{}",
            knob.as_str()
        );
        assert!(!caller.as_str().contains("unset"), "{}", caller.as_str());
        assert!(
            caller.as_str().contains("ExecutorSizing::DEFAULT.arena"),
            "{}",
            caller.as_str()
        );
    }

    #[test]
    fn zero_and_extreme_numbers_render() {
        let s = Refusal::new("x", 0, usize::MAX, Remedy::Caller);
        assert!(s.as_str().contains("x = 0 B"), "{}", s.as_str());
        assert!(
            s.as_str().contains(&std::format!("{} B", usize::MAX)),
            "{}",
            s.as_str()
        );
    }

    /// The comparison, both sides of the boundary. The panic IS the build
    /// error when this runs in a `const` item; here it is observable.
    #[test]
    fn an_arena_at_the_model_passes_and_one_byte_below_is_refused() {
        assert_covers(46_272, 46_272, "arena", Remedy::Knob);
        assert_covers(1 << 20, 46_272, "arena", Remedy::Knob);
        let refused =
            std::panic::catch_unwind(|| assert_covers(46_271, 46_272, "arena", Remedy::Knob));
        let payload = refused.expect_err("one byte under the model must be refused");
        let text = payload
            .downcast_ref::<std::string::String>()
            .map(std::string::String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or_default();
        assert!(text.contains("(1 B short"), "{text}");
    }

    /// No declaration, no requirement: the worst-case budget is not a claim
    /// about the image, so nothing an operator states can be short of it.
    #[test]
    fn an_image_that_declares_nothing_requires_nothing() {
        assert_covers(0, 0, "arena", Remedy::Caller);
        if crate::config::arena_model::REQUIRED == 0 {
            assert_eq!(shortfall(0), 0);
        } else {
            // A test build that declared entities: the derived default must
            // still cover them, which is the property the const item asserts.
            assert_eq!(shortfall(crate::config::ARENA_SIZE), 0);
        }
    }
}
