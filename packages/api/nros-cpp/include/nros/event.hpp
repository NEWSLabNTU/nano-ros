/**
 * @file event.hpp
 * @ingroup grp_support
 * @brief Phase 108 — status events on `nros::Subscription` and
 *        `nros::Publisher`.
 *
 * Tier-1 status-event surface for the C++ user-facing API. Mirrors
 * the C surface in `<nros/event.h>` (cbindgen-generated from
 * `nros-c/src/event.rs`), wrapped as typed C++ callbacks.
 *
 * Backends opt in to specific event kinds; methods return
 * `Result<>` carrying `ErrorCode::Unsupported` until backend wiring
 * lands per-phase (109+).
 */

#ifndef NROS_CPP_EVENT_HPP
#define NROS_CPP_EVENT_HPP

#include <cstdint>

#include "nros/result.hpp"

namespace nros {

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

} // namespace nros

#endif // NROS_CPP_EVENT_HPP
