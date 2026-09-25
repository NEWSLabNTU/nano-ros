//! phase-454 W10 — the DECLARED QoS history depth, and what Rust does with it.
//!
//! # What this is
//!
//! The QoS history DEPTH of a subscription multiplies the executor arena:
//! `sizeof(entry) + (depth + 1) * bound + (depth + 1) * 8`. A build that sizes
//! from a declared depth and an image that registers a different one are an
//! image sized for one number and running another — so the two have to be held
//! to one. C++ holds them with a `static_assert` inside `NROS_SUBSCRIBE`, and C
//! with a `_Static_assert` inside `NROS_ASSERT_DECLARED_DEPTH`.
//!
//! # Why Rust's half is at REGISTRATION and not at compile time
//!
//! Both C surfaces get a compile-time answer because a preprocessor macro sees
//! the call site's TOPIC as a token. Rust has neither of the two things that
//! would make the same answer possible:
//!
//!   * **No macro seam at the subscribe site.** `nros::main!` emits the boot
//!     scaffold and knows nothing about subscriptions; there is no
//!     `nros::subscribe!`. Every call site is an ordinary method call.
//!   * **The topic is a runtime `&str`**, not a const generic. A `const`
//!     assertion would need it in the type system.
//!
//! And the descriptor road differs too: a RUST component is registered as an
//! empty cmake INTERFACE library (`NanoRosNodeRegister.cmake`), compiled by the
//! workspace runtime crate rather than by that target, so there is no C
//! preprocessor in the lane and no include path to put a header on. That is why
//! `_nros_declared_qos_arm()` returns early for `RUST` — the header mechanism
//! is not available to it, not merely unimplemented.
//!
//! # TWO facts arrive, on two roads
//!
//! The declared depths reach `nros-node/build.rs` from whichever carrier the
//! road has, and it writes both into [`crate::config::DECLARED_QOS_ROWS`]:
//!
//!   * the **cmake road** (native C/C++, Zephyr west, NuttX) sets
//!     `NROS_ENTITY_DECLARED_DEPTHS` (`type|topic=depth`), written by the entity
//!     inventory from the resolved SystemModel;
//!   * the **single-package cargo leaf road** — the one phase-454 W12 taught to
//!     read a `system.contract.yaml` — sets no such variable and names a SIZING
//!     DESCRIPTOR instead, whose `[[endpoint]]` rows carry the same fact
//!     (RFC-0100 D4). W10 shipped reading only the first, so on exactly the road
//!     that had a contract this table was `None` and nothing could be compared.
//!
//! # And two things are done with them, because the surfaces differ
//!
//!   * [`check`](crate::declared_qos::check) REFUSES a disagreement, with
//!     [`NodeError::DeclaredDepthMismatch`] — the registration fails, the
//!     subscription never exists, and the entry's `?` carries it out. Reached
//!     from the C and C++ FFI seams, which took the declared depth at their own
//!     call site already.
//!   * [`honour`](crate::declared_qos::honour) is what a RUST registration calls: it TAKES a declared depth
//!     shallower than the ask (the C++ three-argument `NROS_SUBSCRIBE`'s answer,
//!     delivered one layer down because Rust has no call-site seam) and refuses
//!     a declaration DEEPER than the ask. Its doc comment has the whole rule and
//!     why the asymmetry is real.
//!
//! # ABSENCE IS NOT ZERO
//!
//! A `(type, topic)` with no row is "nobody declared this endpoint". Nothing is
//! checked for it, nothing is taken, and nothing is defaulted. An image with no
//! contract sidecar has `DECLARED_QOS_ROWS == None` and behaves exactly as it
//! did before — measured, not asserted: on `examples/native/rust/listener` with
//! the sidecar removed, `.bss + .data` and every attributed symbol are
//! identical to `origin/main`'s, and the image carries no `declared_qos` symbol
//! at all.

use crate::executor::NodeError;
use nros_rmw::QoSProfile;

/// Every declared endpoint this image's contract states a depth for, or `None`
/// when it declares none. `(type name, topic, depth)`; both the ROS and the
/// DDS-mangled spelling of a type appear, because which one a generated message
/// class carries is a property of the codegen that produced it.
pub use crate::config::DECLARED_QOS_ROWS;

/// Do two topic spellings name the same endpoint?
///
/// One leading `/` is ignored on each side: a contract states `/chatter` and a
/// call site may write either `"/chatter"` or the relative `"chatter"` (
/// `examples/workspaces/launch/src/listener_pkg` writes the second). Nothing
/// deeper is normalised — a namespace is a different endpoint, and guessing at
/// one here would make the check match rows nobody meant.
fn topic_eq(a: &str, b: &str) -> bool {
    let a = a.strip_prefix('/').unwrap_or(a);
    let b = b.strip_prefix('/').unwrap_or(b);
    a == b
}

/// The declared depth for `(type_name, topic)` in `rows`, or `None`.
///
/// Split out from [`declared_depth`] so the lookup is testable against a table
/// this build did not generate: in-tree, `DECLARED_QOS_ROWS` is `None` (no
/// contract reaches a unit-test build), so a test that could only call the
/// public form would assert nothing while passing.
fn depth_in(rows: &[(&str, &str, u32)], type_name: &str, topic: &str) -> Option<u32> {
    rows.iter()
        .find(|(t, tp, _)| *t == type_name && topic_eq(tp, topic))
        .map(|(_, _, d)| *d)
}

/// The declared depth for `(type_name, topic)`, or `None` when nobody declared
/// that endpoint.
///
/// Linear over a handful of rows, on a path that runs once per subscription at
/// registration. `None` when this image declares nothing at all.
pub fn declared_depth(type_name: &str, topic: &str) -> Option<u32> {
    depth_in(DECLARED_QOS_ROWS?, type_name, topic)
}

/// Refuse a subscription whose QoS depth disagrees with what the contract
/// declared for its topic.
///
/// `Ok(())` when they agree AND when nothing was declared — an image that has
/// not opted in is not an image in error. On a disagreement the TOPIC and BOTH
/// numbers go to the log first (a `NodeError` is a unit variant and can carry
/// none of the three), then the registration is refused.
///
/// Reached from the **C and C++ FFI registration seams**, which have already
/// taken the declared depth at their own call site (`NROS_SUBSCRIBE`'s
/// three-argument form → `qos_from_declared_depth`; C's `_Static_assert`), so a
/// disagreement arriving here is a real one. The Rust seams call [`honour`]
/// instead — see its doc comment for why the two surfaces differ.
///
/// # Keep the message inside ONE log line
///
/// `nros_log`'s formatting buffer is 256 bytes by default (`buffer-size-256`)
/// and OVERFLOW TRUNCATES with a single `…`. This message used to run past 450
/// characters, so on a real image the half a reader needs — which file to edit —
/// was the half that got cut, and the diagnostic ended mid-sentence. The
/// numbers and the topic come first for the same reason: a long type name may
/// still push the tail off, and the tail is the least load-bearing part.
pub fn check(type_name: &str, topic: &str, depth: u32) -> Result<(), NodeError> {
    match declared_depth(type_name, topic) {
        Some(declared) if declared != depth => {
            nros_log::log_error!(
                nros_log::get_logger("nros_node"),
                "declared depth: `{}` registers KEEP_LAST({}) but this image's contract \
                 declares {}, and the arena was sized from {}. Fix the contract row or the \
                 QoS at the call site (type `{}`).",
                topic,
                depth,
                declared,
                declared,
                type_name
            );
            Err(NodeError::DeclaredDepthMismatch)
        }
        _ => Ok(()),
    }
}

/// The QoS a Rust subscription actually registers with, once the contract has
/// had its say — phase-454 W13, the Rust half of W10.
///
/// # Why Rust TAKES the declared depth where C and C++ ASSERT it
///
/// C++ settles this at the call site, by arity: `NROS_SUBSCRIBE(Msg, m, topic)`
/// states no QoS, so `qos_from_declared_depth` BUILDS one from the declaration
/// ("nothing to assert — there is only ever one number"), and the four-argument
/// form, where the call site did state one, `static_assert`s that the two agree.
/// C does the same with `NROS_ASSERT_DECLARED_DEPTH`.
///
/// Rust has neither half of that seam: there is no `nros::subscribe!`, and the
/// topic is a runtime `&str`, so nothing at the call site can be told apart
/// from anything else at compile time — and by the time a QoS reaches a
/// registration, "the caller wrote this" and "a constructor defaulted it" are
/// the same `QoSProfile`. `Node::create_subscription` hands
/// `QoSProfile::default()` — `rmw_qos_profile_default`, KEEP_LAST(10) — to the
/// same `_with_qos` entry point a caller with an opinion reaches, and the
/// declarative road folds both into one `EntityMetadata::qos` field long before
/// this point.
///
/// So the distinction C++ makes is not available here, and the two candidate
/// rules that remain each fail on their own:
///
/// * **Check only.** Every `QoSProfile::default()` call site in the tree would
///   be refused the moment its endpoint is declared at anything but 10 — which
///   is the entire point of declaring one. A declaration would become a way to
///   stop an image booting.
/// * **Take, always.** A caller who narrowed a queue on purpose
///   (`create_subscription_viewable` requires KEEP_LAST(1); a control loop that
///   wants the newest sample and no backlog) would be silently widened by a
///   contract that over-declared.
///
/// # The rule, and why the asymmetry is real
///
/// **The contract states what the BUILD RESERVED.** W12 made that literal: the
/// executor arena, the zenoh receive ring and both payload pools on a declared
/// image are all sized from the declared depth, so the declaration is not a
/// preference — it is the size of the storage that exists.
///
/// * `asked > declared` — the registration wants more slots than were bought.
///   It cannot have them, and nothing is gained by refusing: this is every
///   unstated call site in the tree (`QoSProfile::default()` asks 10; every
///   in-tree contract declares less). **TAKEN**, and reported.
/// * `asked < declared` — the registration fits, so nothing is at risk of
///   overrunning; but the image paid for a queue deeper than anything wants,
///   and taking would WIDEN a depth somebody narrowed. Two statements about one
///   fact disagreeing, with no default that explains either (RFC-0100 D8).
///   **REFUSED**, naming both.
/// * equal, or nothing declared — returned unchanged. An image with no contract
///   is byte-identical to every build before this wave.
///
/// # And it says so
///
/// A take is REPORTED, once per endpoint that diverges — the C++ surface's
/// number is visible in the source at the call site and Rust's is not, so a
/// registration that quietly stopped being what `lib.rs` reads would be the lie
/// this whole model exists to remove. The severity is the DIRECTION, the same
/// rule `nros-rmw-zenoh`'s `shim/qos.rs` states for its own grants: taking a
/// SHALLOWER depth can drop a sample under a burst, so it is `WARN`; a deeper
/// one costs a peer nothing and is `INFO`.
///
/// Called instead of [`check`] at every Rust subscription registration. The C
/// and C++ FFI seams keep [`check`] — they have already taken the declared
/// depth at their own call site, so a disagreement reaching THEM is a real
/// refusal and not a default nobody wrote.
/// # `#[inline]` is load-bearing, and it was MEASURED
///
/// An image with no contract must be byte-identical to every build before this
/// wave, and "the table is empty so the work folds away" is a claim about the
/// optimiser, not a fact. It was FALSE as first written: with the table
/// threaded in as an `unwrap_or(&[])` argument, `honour` stayed out of line —
/// a real `T nros_node::declared_qos::honour` symbol, +144 bytes of text and
/// +8 of data on the no-contract listener against `origin/main`, because a
/// 40-byte `QoSProfile` moving in and out by value is past LLVM's inlining
/// budget. Matching the `Option` HERE, behind `#[inline]`, is what lets a
/// `None` table fold the call to `Ok(qos)` at every one of the fourteen
/// registration sites and leave `honour_in` unreferenced. Verified by
/// rebuilding both trees and comparing the linked binary byte for byte.
#[inline]
pub fn honour(type_name: &str, topic: &str, qos: QoSProfile) -> Result<QoSProfile, NodeError> {
    let Some(rows) = DECLARED_QOS_ROWS else {
        return Ok(qos);
    };
    honour_in(rows, type_name, topic, qos)
}

/// [`honour`] against a table this build did not generate.
///
/// Split out for the reason [`depth_in`] is: in-tree `DECLARED_QOS_ROWS` is
/// `None`, so a test that could only call the public form would exercise the
/// "nothing declared" arm and assert nothing about the take or the refusal
/// while reading as coverage of both.
fn honour_in(
    rows: &[(&str, &str, u32)],
    type_name: &str,
    topic: &str,
    qos: QoSProfile,
) -> Result<QoSProfile, NodeError> {
    let Some(declared) = depth_in(rows, type_name, topic) else {
        return Ok(qos);
    };
    if declared == qos.depth {
        return Ok(qos);
    }
    if declared > qos.depth {
        nros_log::log_error!(
            nros_log::get_logger("nros_node"),
            "declared depth: `{}` registers KEEP_LAST({}) and the contract declares {} -- \
             DEEPER. Taking it would widen a queue the code narrowed; fix the contract row \
             or the call site (type `{}`).",
            topic,
            qos.depth,
            declared,
            type_name
        );
        return Err(NodeError::DeclaredDepthMismatch);
    }
    let mut granted = qos;
    granted.depth = declared;
    nros_log::log_warn!(
        nros_log::get_logger("nros_node"),
        "declared depth: taking KEEP_LAST({}) on `{}`; the call site asked {} and the build \
         reserved {}. Raise the contract row to keep more (type `{}`).",
        declared,
        topic,
        qos.depth,
        declared,
        type_name
    );
    Ok(granted)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape `build.rs` writes: the ROS spelling and the DDS-mangled one,
    /// for each declared endpoint.
    const ROWS: &[(&str, &str, u32)] = &[
        ("std_msgs/msg/Int32", "/chatter", 1),
        ("std_msgs::msg::dds_::Int32_", "/chatter", 1),
    ];

    #[test]
    fn either_type_spelling_finds_the_row() {
        assert_eq!(depth_in(ROWS, "std_msgs/msg/Int32", "/chatter"), Some(1));
        assert_eq!(
            depth_in(ROWS, "std_msgs::msg::dds_::Int32_", "/chatter"),
            Some(1),
            "a generated Rust message class reports the DDS-mangled TYPE_NAME; a table that \
             carried only the ROS spelling would match nothing and check nothing"
        );
    }

    #[test]
    fn a_relative_topic_matches_the_declared_absolute_one() {
        assert_eq!(depth_in(ROWS, "std_msgs/msg/Int32", "chatter"), Some(1));
    }

    #[test]
    fn absence_is_not_a_depth() {
        assert_eq!(
            depth_in(ROWS, "std_msgs/msg/Int32", "/undeclared"),
            None,
            "an endpoint nobody declared has no depth -- not 0, and not the ROS default of 10"
        );
        assert_eq!(
            depth_in(ROWS, "std_msgs/msg/Bool", "/chatter"),
            None,
            "the table is keyed on the PAIR: a declared topic under another type is not a hit, \
             or one declaration would size every type carried on that topic"
        );
        assert_eq!(depth_in(&[], "std_msgs/msg/Int32", "/chatter"), None);
    }

    #[test]
    fn a_namespace_is_a_different_endpoint() {
        assert_eq!(
            depth_in(ROWS, "std_msgs/msg/Int32", "/ns/chatter"),
            None,
            "only ONE leading slash is ignored; matching a namespaced topic against a bare \
             declaration would check a row nobody meant"
        );
    }

    /// phase-454 W13 — the TAKE. An unstated call site asks the ROS default 10; a
    /// contract that declares 1 is the size the build reserved, so the
    /// registration comes back at 1 and every other policy is untouched.
    ///
    /// Untouched matters as much as the depth: `honour` rebuilds the profile a
    /// backend is handed, so a reliability or durability quietly reset here
    /// would change the wire behaviour of every declared endpoint.
    #[test]
    fn an_endpoint_asking_deeper_than_its_declaration_registers_at_the_declared_depth() {
        let asked = QoSProfile::QOS_PROFILE_DEFAULT;
        assert_eq!(
            asked.depth, 10,
            "the ROS default an unstated call site gets"
        );
        let granted = honour_in(ROWS, "std_msgs/msg/Int32", "/chatter", asked)
            .expect("taking the declared depth is not an error");
        assert_eq!(
            granted.depth, 1,
            "the contract declares KEEP_LAST(1) and the build reserved for it; a registration \
             that kept its 10 would claim a receive region the arena never bought"
        );
        assert_eq!(granted.reliability, asked.reliability);
        assert_eq!(granted.durability, asked.durability);
        assert_eq!(granted.history, asked.history);
        assert_eq!(granted.deadline_ms, asked.deadline_ms);
        assert_eq!(granted.lifespan_ms, asked.lifespan_ms);
    }

    /// The DDS-mangled spelling is the one a generated Rust message class
    /// carries, so it is the one a typed call site actually hands `honour` —
    /// a take that only matched the ROS spelling would fire for nothing.
    #[test]
    fn the_take_matches_the_spelling_a_typed_call_site_carries() {
        let granted = honour_in(
            ROWS,
            "std_msgs::msg::dds_::Int32_",
            "/chatter",
            QoSProfile::QOS_PROFILE_DEFAULT,
        )
        .expect("admissible");
        assert_eq!(granted.depth, 1);
    }

    /// phase-454 W13 — the REFUSAL, and the reason the rule is asymmetric.
    ///
    /// A registration asking for LESS than the declaration fits inside what the
    /// build reserved, so nothing overruns — but taking would WIDEN a queue the
    /// code narrowed on purpose (`create_subscription_viewable` requires
    /// KEEP_LAST(1)), and the image has paid for slots nothing wants. Two
    /// statements about one fact disagreeing, with no default that explains
    /// either: RFC-0100 D8.
    #[test]
    fn a_declaration_deeper_than_the_registration_is_refused_not_taken() {
        const DEEP: &[(&str, &str, u32)] = &[("std_msgs/msg/Int32", "/chatter", 20)];
        let mut asked = QoSProfile::QOS_PROFILE_DEFAULT;
        asked.depth = 10;
        assert_eq!(
            honour_in(DEEP, "std_msgs/msg/Int32", "/chatter", asked),
            Err(NodeError::DeclaredDepthMismatch),
            "THE control for the other direction: declared 20, registered 10. If this returns \
             Ok the rule has collapsed into an unconditional take, and a contract can silently \
             deepen any queue in the image."
        );
    }

    /// Equal is not a divergence, and an undeclared endpoint is not one either
    /// — the two arms that must stay silent, or every image in the tree gains
    /// a report it cannot act on.
    #[test]
    fn agreement_and_absence_both_pass_the_profile_through_unchanged() {
        let mut asked = QoSProfile::QOS_PROFILE_DEFAULT;
        asked.depth = 1;
        assert_eq!(
            honour_in(ROWS, "std_msgs/msg/Int32", "/chatter", asked),
            Ok(asked)
        );
        let deep = QoSProfile::QOS_PROFILE_DEFAULT;
        assert_eq!(
            honour_in(ROWS, "std_msgs/msg/Int32", "/undeclared", deep),
            Ok(deep),
            "absence is not zero and it is not ten either -- an endpoint nobody declared keeps \
             exactly the profile the call site built"
        );
        assert_eq!(
            honour_in(&[], "std_msgs/msg/Int32", "/chatter", deep),
            Ok(deep)
        );
    }

    /// The public entry point over THIS build's table. In-tree that table is
    /// `None` (no contract reaches a unit-test build), so this asserts the
    /// no-declaration behaviour rather than a lookup -- and says so, because a
    /// reader would otherwise take it for coverage of the lookup above.
    #[cfg(not(nros_declared_qos_table))]
    #[test]
    fn an_image_that_declares_nothing_checks_nothing() {
        assert!(
            DECLARED_QOS_ROWS.is_none(),
            "`nros_declared_qos_table` is unset, so the build wrote no table. If this fires, \
             the cfg and the const have come apart and the declared-image test below is \
             being skipped on a build that HAS a table."
        );
        assert_eq!(declared_depth("std_msgs/msg/Int32", "/chatter"), None);
        assert_eq!(check("std_msgs/msg/Int32", "/chatter", 10), Ok(()));
        // phase-454 W13 — and the take is inert too, which is acceptance 4: an
        // image with no contract registers exactly the profile it built.
        let asked = QoSProfile::QOS_PROFILE_DEFAULT;
        assert_eq!(honour("std_msgs/msg/Int32", "/chatter", asked), Ok(asked));
    }

    /// THE NEGATIVE CONTROL, over the build's OWN table.
    ///
    /// Everything above tests `depth_in` against a table this module wrote,
    /// which proves the search and nothing about DELIVERY: a carrier cmake
    /// never forwards, a `build.rs` parse that drops every row, and a table
    /// keyed on a spelling no call site writes all look exactly like "no
    /// mismatch here". Six mechanisms in this campaign have been correct and
    /// unreachable for precisely that reason.
    ///
    /// So this module is compiled only where `build.rs` wrote rows, and
    /// `just check declared-qos-registration` builds this crate with
    /// `NROS_ENTITY_DECLARED_DEPTHS` set to make that happen. It also asserts
    /// that the test does NOT exist without the variable -- otherwise a cfg
    /// that was always on would make the lane pass while proving nothing.
    #[cfg(nros_declared_qos_table)]
    mod declared_image {
        use super::*;

        #[test]
        fn refuses_a_disagreeing_depth_and_accepts_an_agreeing_one() {
            let rows = DECLARED_QOS_ROWS.expect("the cfg says this build wrote a table");
            assert!(
                !rows.is_empty(),
                "an empty table with the cfg set is a parse that dropped every row -- which \
                 would disable the check while reading as if it were on"
            );
            assert_eq!(
                declared_depth("std_msgs/msg/Int32", "/chatter"),
                Some(1),
                "the lane forwards `std_msgs/msg/Int32|/chatter=1`; if this is None the \
                 carrier, the parse or the key spelling is wrong, and every registration in \
                 a real image would go unchecked"
            );
            assert_eq!(
                declared_depth("std_msgs::msg::dds_::Int32_", "/chatter"),
                Some(1),
                "build.rs must emit the DDS-mangled spelling too -- it is what a generated \
                 Rust message class reports as TYPE_NAME, and the only spelling a typed call \
                 site ever hands `check`"
            );
            assert_eq!(check("std_msgs/msg/Int32", "/chatter", 1), Ok(()));
            assert_eq!(
                check("std_msgs::msg::dds_::Int32_", "/chatter", 10),
                Err(NodeError::DeclaredDepthMismatch),
                "THE control: declared 1, registered 10. If this passes, the registration \
                 check does nothing on a real image and every lane above it stays green."
            );
            assert_eq!(
                check("std_msgs/msg/Int32", "/undeclared", 10),
                Ok(()),
                "and an endpoint nobody declared is still not an error"
            );
        }

        /// phase-454 W13 — the TAKE, over the build's own table.
        ///
        /// The sibling above proves the ROWS arrived. This proves the thing a
        /// Rust image actually depends on: that an unstated call site's
        /// `QoSProfile::default()` comes back at the DECLARED depth. A table
        /// that is delivered and never consulted by the registration would pass
        /// every assertion above it, which is the shape this campaign keeps
        /// finding.
        ///
        /// `just check declared-qos-registration` runs this against BOTH
        /// carriers — the cmake road's `NROS_ENTITY_DECLARED_DEPTHS` and the
        /// cargo leaf road's sizing descriptor — because a carrier that reaches
        /// one road and not the other is exactly how W10 shipped with its
        /// registration check unable to compare anything on the road W12 taught
        /// to read a contract.
        #[test]
        fn an_unstated_call_site_registers_at_the_declared_depth() {
            let asked = QoSProfile::QOS_PROFILE_DEFAULT;
            assert_eq!(
                asked.depth, 10,
                "the profile `Node::create_subscription` hands a registration when the call \
                 site states none"
            );
            let granted = honour("std_msgs::msg::dds_::Int32_", "/chatter", asked)
                .expect("the declared depth is shallower than the ask, so it is TAKEN");
            assert_eq!(
                granted.depth, 1,
                "this build's table declares KEEP_LAST(1) for `/chatter`; if this is 10 the \
                 registration ignored the declaration and the image runs a depth the build did \
                 not reserve for"
            );
        }
    }
}
