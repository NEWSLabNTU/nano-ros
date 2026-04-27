// nros-cpp: Umbrella header
// Include this single header to get the full nros C++ API.
//
// Freestanding C++ compatible — no STL, no exceptions, no RTTI required.

/**
 * @file nros.hpp
 * @ingroup grp_init
 * @brief Umbrella header — pulls in every public C++ API surface.
 */

#ifndef NROS_CPP_HPP
#define NROS_CPP_HPP

#include "nros/result.hpp"
#include "nros/qos.hpp"
#include "nros/future.hpp"
#include "nros/stream.hpp"
// Phase 84.G8: node.hpp no longer pulls in the heavy entity headers —
// each entity header carries its own out-of-line `Node::create_X<>()`
// template definition. The umbrella pulls in every entity explicitly so
// `#include <nros/nros.hpp>` still yields the full API.
#include "nros/node.hpp"
#include "nros/publisher.hpp"
#include "nros/subscription.hpp"
#include "nros/service.hpp"
#include "nros/client.hpp"
#include "nros/action_server.hpp"
#include "nros/action_client.hpp"

namespace nros {

/// Get the global executor handle for Future::wait().
///
/// Returns the raw storage pointer used by the global `init()`/`spin_once()`
/// free functions. Use with `Future::wait(nros::global_handle(), ...)`.
///
/// @return Executor handle, or nullptr if not initialized.
inline void* global_handle() {
    if (!Node::global_initialized()) return nullptr;
    return Node::global_storage();
}

/// Drive transport I/O and dispatch callbacks.
///
/// Call this periodically so subscriptions can receive data.
/// When using manual-poll (no callbacks), this drives the network layer.
///
/// @param timeout_ms  Maximum time to block waiting for I/O (default: 10ms).
/// @return Result indicating success or failure.
inline Result spin_once(int32_t timeout_ms = 10) {
    if (!Node::global_initialized()) {
        return Result(ErrorCode::NotInitialized);
    }
    return Result(nros_cpp_spin_once(Node::global_storage(), timeout_ms));
}

/// Spin for a duration (blocking).
///
/// Repeatedly calls `spin_once()` until `duration_ms` has elapsed.
/// Convenience wrapper around the global executor.
///
/// @param duration_ms  Total time to spin, in milliseconds.
/// @param poll_ms      Individual spin_once timeout (default: 10ms).
/// @return Result from the last spin_once call.
inline Result spin(uint32_t duration_ms, int32_t poll_ms = 10) {
    if (!Node::global_initialized()) {
        return Result(ErrorCode::NotInitialized);
    }
    uint32_t elapsed = 0;
    Result last = Result::success();
    while (elapsed < duration_ms) {
        int32_t remaining = static_cast<int32_t>(duration_ms - elapsed);
        int32_t timeout = remaining < poll_ms ? remaining : poll_ms;
        last = Result(nros_cpp_spin_once(Node::global_storage(), timeout));
        if (!last.ok()) return last;
        elapsed += static_cast<uint32_t>(timeout);
    }
    return last;
}

} // namespace nros

#ifdef NROS_CPP_STD
#include "nros/std_compat.hpp"
#endif

#endif // NROS_CPP_HPP
