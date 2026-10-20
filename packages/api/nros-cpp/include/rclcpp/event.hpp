/**
 * @file event.hpp
 * @ingroup grp_support
 * @brief Phase 108 — status events on `rclcpp::Subscription` and
 *        `rclcpp::Publisher`.
 *
 * Tier-1 status-event surface for the C++ user-facing API. Mirrors
 * the C surface in `<nros/event.h>` (cbindgen-generated from
 * `nros-c/src/event.rs`), wrapped as typed C++ callbacks.
 *
 * Backends opt in to specific event kinds. The Tier-1 setters still return
 * `ErrorCode::Unsupported` at this layer; the matched and incompatible-type
 * ones (issue 1972) reach the backend.
 */

#ifndef NROS_RCLCPP_EVENT_HPP
#define NROS_RCLCPP_EVENT_HPP

#include <cstdint>

#include "rclcpp/result.hpp"
#include "rclcpp/detail/nros_cpp_ffi.h"

namespace rclcpp {

/// Liveliness status payload.
struct LivelinessChangedStatus {
    uint16_t alive_count;
    uint16_t not_alive_count;
    int16_t alive_count_change;
    int16_t not_alive_count_change;
};

/// Count payload — used for deadline-missed and message-lost events.
struct CountStatus {
    uint32_t total_count;
    uint32_t total_count_change;
};

/// Type alias: deadline-missed payload shape is identical to
/// [`CountStatus`].
using DeadlineMissedStatus = CountStatus;

/// Type alias: message-lost payload shape is identical to
/// [`CountStatus`].
using MessageLostStatus = CountStatus;

/// Issue 1972 — rclcpp's `MatchedInfo` (upstream `rmw_matched_status_t`):
/// `{total_count, total_count_change, current_count, current_count_change}`.
/// The counts are `uint32_t` where upstream's are `size_t`.
using MatchedInfo = ::nros_cpp_matched_status_t;

/// Issue 1972 — rclcpp's `IncompatibleTypeInfo` (upstream
/// `rmw_incompatible_type_status_t`): `{total_count, total_count_change}`.
using IncompatibleTypeInfo = ::nros_cpp_incompatible_type_status_t;

/// Issue 1972 — rclcpp's `PublisherMatchedCallbackType`. A C function
/// pointer plus a `user_context`, not a `std::function`: the entity hands it
/// to the backend, which keeps no heap closure (RFC-0018, freestanding). The
/// first argument is the entity's storage, as for every C++ event callback.
using PublisherMatchedCallbackType = ::nros_cpp_matched_cb_t;
/// Issue 1972 — rclcpp's `SubscriptionMatchedCallbackType`. Same shape.
using SubscriptionMatchedCallbackType = ::nros_cpp_matched_cb_t;
/// Issue 1972 — rclcpp's `IncompatibleTypeCallbackType`, one type for both
/// sides as upstream's is.
using IncompatibleTypeCallbackType = ::nros_cpp_incompatible_type_cb_t;

} // namespace rclcpp

#endif // NROS_RCLCPP_EVENT_HPP
