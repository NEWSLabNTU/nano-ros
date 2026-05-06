// nros-cpp: SchedContext API
// Freestanding C++14 — no exceptions, no STL required.
//
// Phase 110.B / 110.C — register first-class scheduling capabilities
// on the executor and bind callbacks to them.
//
// The default Fifo SC is auto-created at executor init; every callback
// registered without an explicit `bind_to_sched_context` call binds to
// it. Single-thread non-preemption applies — see Phase 110.D for
// preemption semantics.

/**
 * @file sched_context.hpp
 * @ingroup grp_executor
 * @brief Phase 110.B / 110.C — `nros::SchedContext` API.
 */

#ifndef NROS_CPP_SCHED_CONTEXT_HPP
#define NROS_CPP_SCHED_CONTEXT_HPP

#include <cstdint>

#include "nros/result.hpp"

extern "C" {

// Mirrors the Rust `nros_cpp_*` FFI in `nros-cpp/src/lib.rs`.
struct nros_cpp_sched_context_ffi {
    uint8_t class_;
    uint8_t priority;
    uint8_t deadline_policy;
    uint32_t period_us;
    uint32_t budget_us;
    uint32_t deadline_us;
};

uint8_t nros_cpp_default_sched_context_id(void);
int nros_cpp_create_sched_context(void* handle,
                                  const struct nros_cpp_sched_context_ffi* cfg,
                                  uint8_t* out_sc_id);
int nros_cpp_bind_handle_to_sched_context(void* handle,
                                          size_t callback_handle,
                                          uint8_t sc_id);

} // extern "C"

namespace nros {

/// Scheduling class — picks the runtime queue + selection policy.
enum class SchedClass : uint8_t {
    Fifo = 0,
    Edf = 1,
    Sporadic = 2,
    BestEffort = 3,
    TimeTriggered = 4,
};

/// Criticality bucket. Lower numeric value = higher priority.
enum class Priority : uint8_t {
    Critical = 0,
    Normal = 1,
    BestEffort = 2,
};

/// Deadline-interpretation policy. `Activated` is the default for
/// event-triggered subscriptions; `Released` for timer-triggered;
/// `Inherited` carries deadline in the message header.
enum class DeadlinePolicy : uint8_t {
    Released = 0,
    Activated = 1,
    Inherited = 2,
};

/// Identifier of a registered scheduling context. Slot 0 is the
/// auto-created default `Fifo` SC.
using SchedContextId = uint8_t;

/// Scheduling-context descriptor. Time fields use `0` as the
/// "absent" sentinel.
struct SchedContext {
    SchedClass class_ = SchedClass::Fifo;
    Priority priority = Priority::Normal;
    DeadlinePolicy deadline_policy = DeadlinePolicy::Activated;
    uint32_t period_us = 0;
    uint32_t budget_us = 0;
    uint32_t deadline_us = 0;
};

/// Identifier of the auto-created default `Fifo`-class SC.
inline SchedContextId default_sched_context_id() {
    return nros_cpp_default_sched_context_id();
}

namespace detail {
inline ::nros_cpp_sched_context_ffi to_ffi(const SchedContext& sc) {
    ::nros_cpp_sched_context_ffi ffi{};
    ffi.class_ = static_cast<uint8_t>(sc.class_);
    ffi.priority = static_cast<uint8_t>(sc.priority);
    ffi.deadline_policy = static_cast<uint8_t>(sc.deadline_policy);
    ffi.period_us = sc.period_us;
    ffi.budget_us = sc.budget_us;
    ffi.deadline_us = sc.deadline_us;
    return ffi;
}
} // namespace detail

/// Register a new scheduling context with the executor.
///
/// @param executor_handle Raw executor handle from `Executor::handle()`.
/// @param sc              SC descriptor to register.
/// @param out_sc_id       Receives the new id on success.
/// @return Result indicating success or failure (`Full` if `MAX_SC`
///         is exhausted; `InvalidArgument` for null pointers).
inline Result create_sched_context(void* executor_handle, const SchedContext& sc,
                                   SchedContextId& out_sc_id) {
    auto ffi = detail::to_ffi(sc);
    return Result(nros_cpp_create_sched_context(executor_handle, &ffi, &out_sc_id));
}

/// Bind a registered callback (by handle id) to a scheduling context.
inline Result bind_handle_to_sched_context(void* executor_handle, size_t callback_handle,
                                           SchedContextId sc_id) {
    return Result(nros_cpp_bind_handle_to_sched_context(executor_handle, callback_handle, sc_id));
}

} // namespace nros

#endif // NROS_CPP_SCHED_CONTEXT_HPP
