//! phase-454 W10 — the DECLARED QoS history depth, and the check over it, for
//! Rust.
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
//! The facts arrive the way every other derived fact does on the cargo road:
//! `NROS_ENTITY_DECLARED_DEPTHS` (`type|topic=depth`) reaches
//! `nros-node/build.rs`, which writes [`crate::config::DECLARED_QOS_ROWS`].
//! This module reads it, and a disagreement is
//! [`NodeError::DeclaredDepthMismatch`] — which fails the registration, so the
//! subscription never exists and the entry's `?` carries it out.
//!
//! # ABSENCE IS NOT ZERO
//!
//! A `(type, topic)` with no row is "nobody declared this endpoint". Nothing is
//! checked for it and nothing is defaulted. An image with no contract sidecar
//! has `DECLARED_QOS_ROWS == None` and behaves exactly as it did before.

use crate::executor::NodeError;

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
/// Called at every place a subscription is created from a topic and a QoS.
/// `check-declared-qos-registration` holds that set to the whole set.
pub fn check(type_name: &str, topic: &str, depth: u32) -> Result<(), NodeError> {
    match declared_depth(type_name, topic) {
        Some(declared) if declared != depth => {
            nros_log::log_error!(
                nros_log::get_logger("nros_node"),
                "nros: subscription on topic `{}` (type `{}`) is being registered with QoS \
                 depth {}, but this system's contract sidecar DECLARES depth {} for that \
                 topic. Depth multiplies the executor arena, which was sized from the \
                 declared number -- so the image is sized for {} and would run {}. Fix \
                 whichever is wrong: the contract row \
                 (<bringup>/launch/<stem>.contract.yaml, contracts.sub_endpoints.<ep>.qos) \
                 or the QoS at the call site.",
                topic,
                type_name,
                depth,
                declared,
                declared,
                depth
            );
            Err(NodeError::DeclaredDepthMismatch)
        }
        _ => Ok(()),
    }
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
    }
}
