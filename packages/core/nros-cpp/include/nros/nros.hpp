// nros-cpp: Umbrella header
// Include this single header to get the full nros C++ API.
//
// Freestanding C++ compatible — no STL, no exceptions, no RTTI required.

#ifndef NROS_CPP_HPP
#define NROS_CPP_HPP

#include "nros/result.hpp"
#include "nros/qos.hpp"
#include "nros/node.hpp" // includes publisher, subscription, service, client, action headers

namespace nros {

/// Drive transport I/O and dispatch callbacks.
///
/// Call this periodically so subscriptions can receive data.
/// When using manual-poll (no callbacks), this drives the network layer.
///
/// @param timeout_ms  Maximum time to block waiting for I/O (default: 10ms).
/// @return Result indicating success or failure.
inline Result spin_once(int32_t timeout_ms = 10) {
    void* handle = Node::global_executor();
    if (!handle) {
        return Result(ErrorCode::NotInitialized);
    }
    return Result(nros_cpp_spin_once(handle, timeout_ms));
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
    void* handle = Node::global_executor();
    if (!handle) {
        return Result(ErrorCode::NotInitialized);
    }
    uint32_t elapsed = 0;
    Result last = Result::success();
    while (elapsed < duration_ms) {
        int32_t remaining = static_cast<int32_t>(duration_ms - elapsed);
        int32_t timeout = remaining < poll_ms ? remaining : poll_ms;
        last = Result(nros_cpp_spin_once(handle, timeout));
        if (!last.ok()) return last;
        elapsed += static_cast<uint32_t>(timeout);
    }
    return last;
}

} // namespace nros

#endif // NROS_CPP_HPP
