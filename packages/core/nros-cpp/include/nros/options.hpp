// nros-cpp: entity named-options structs (Phase 189.M3)
// Freestanding C++ — no exceptions, no STL required

/**
 * @file options.hpp
 * @ingroup grp_node
 * @brief rclcpp-style named-options structs for entity creation.
 *
 * Mirrors `rclcpp::SubscriptionOptions` / `rclcpp::PublisherOptions`:
 * the options struct sits **alongside** the `QoS` argument (rclcpp
 * convention — QoS is its own positional parameter) and carries the
 * non-QoS creation axes. The `QoS` value class lives in `qos.hpp`.
 *
 * Phase 189.M3.1 introduces a single live field — `sched_context` — on
 * `SubscriptionOptions`, plus a reserved `message_info` flag. The
 * `PublisherOptions` struct is deliberately empty-with-comment: a
 * publisher has no callback, so it has neither a scheduling context nor
 * a message-info axis. It exists for rclcpp symmetry and as the future
 * home for intra-process / loaned-message knobs.
 */

#ifndef NROS_CPP_OPTIONS_HPP
#define NROS_CPP_OPTIONS_HPP

#include <cstdint>

namespace nros {

/// Sentinel meaning "no scheduling context selected" — the entity
/// inherits the executor / Node default `Fifo` context. Matches the
/// `int sched_context = -1` unset convention; valid SchedContext ids are
/// `0..=255` (the FFI `nros_cpp_bind_handle_to_sched_context` takes a
/// `uint8_t`). `0` is the auto-created default `Fifo` SC
/// (`nros_cpp_default_sched_context_id()`), so the sentinel is `-1`, not
/// `0`, to keep "bind to the default explicitly" expressible.
static constexpr int SCHED_CONTEXT_UNSET = -1;

/// rclcpp-style named options for `Node::create_subscription<M>()`.
///
/// Sits alongside the positional `QoS` argument:
/// ```cpp
/// nros::SubscriptionOptions opts;
/// opts.sched_context = my_sc_id;
/// NROS_TRY(node.create_subscription<Msg>(sub, "/topic",
///                                         nros::QoS::default_profile(),
///                                         opts));
/// ```
///
/// Every existing 2-/3-arg `create_subscription` call keeps compiling —
/// the options parameter defaults to `{}` (all fields at their unset /
/// reserved defaults), which is observably identical to the pre-M3
/// behaviour.
struct SubscriptionOptions {
    /// Scheduling-context id to bind this subscription's dispatch onto.
    ///
    /// `SCHED_CONTEXT_UNSET` (the default) leaves the entity on the
    /// executor / Node default `Fifo` context — no bind call is made.
    /// A value in `0..=255` lowers to
    /// `nros_cpp_bind_handle_to_sched_context(executor, handle, sc)`
    /// after the subscription is created (create-then-bind); a failing
    /// bind surfaces as the create call's `Result`.
    ///
    /// NOTE (Phase 189.M3.1): the C++ subscription is a *thin wrapper*
    /// over a bare `RmwSubscriber` polled via `try_recv_raw` — it does
    /// **not** register a callback entry in the executor, so today's
    /// `nros_cpp_subscription_create` exposes no bindable executor
    /// `HandleId`. The lowering therefore only fires when a handle id is
    /// available (see `Subscription<M>::sched_handle_id_`); for the
    /// poll-style thin wrapper it is a documented no-op until a
    /// handle-returning create FFI lands (tracked alongside M3.4). The
    /// option field + overload are wired now so the rclcpp-shaped call
    /// site is stable and the bind path activates transparently once the
    /// FFI grows a handle id.
    int sched_context = SCHED_CONTEXT_UNSET;

    /// RESERVED — not yet implemented.
    ///
    /// When wired, this selects the with-`MessageInfo` delivery path so
    /// callbacks observe per-sample metadata (source timestamp, GID,
    /// sequence number). That requires a new "with-info" subscription
    /// create FFI + a `SubBufferedRawInfoCEntry`-style arena entry in
    /// `nros-node` — none of which exists yet.
    ///
    /// TODO(M3.4): wire `message_info` to the new arena with-info path.
    /// Setting it today has no effect (it is intentionally ignored).
    bool message_info = false;
};

/// rclcpp-style named options for `Node::create_publisher<M>()`.
///
/// Deliberately empty (reserved): a publisher has no callback, so it
/// carries neither a scheduling context nor a message-info axis. The
/// struct exists for rclcpp symmetry and is the planned home for future
/// intra-process / loaned-message publisher knobs. Passing `{}` is
/// observably identical to the pre-M3 `qos`-only create.
struct PublisherOptions {
    // Reserved for future use (intra-process, loaned-message tuning).
    // No live fields today.
};

} // namespace nros

#endif // NROS_CPP_OPTIONS_HPP
