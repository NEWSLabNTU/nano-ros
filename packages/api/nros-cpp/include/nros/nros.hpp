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

// Phase 118.D — pull cbindgen-generated FFI before any wrapper hpp so
// `nros/qos.hpp`'s `#ifndef NROS_CPP_FFI_H` guard skips its local
// fallback definitions in favor of the canonical types.
#include "nros_cpp_ffi.h"

#include "nros/traits.hpp"
#include "nros/log.hpp"
#include "nros/result.hpp"
// Issue 0789 — the clock / time / duration surface. `node.now()` and
// `node.get_clock()->now()` are what a ported rclcpp publisher calls to
// stamp a header, so the umbrella carries all three. Phase 379 W5 moved
// `duration.hpp` ABOVE `qos.hpp`: the QoS deadline / lifespan / lease
// accessors take and return `nros::Duration`. `qos.hpp` also includes it
// directly, so this order is legibility rather than load-bearing.
#include "nros/duration.hpp"
#include "nros/time.hpp"
#include "nros/clock.hpp"
#include "nros/qos.hpp"
#include "nros/options.hpp"
#include "nros/future.hpp"
#include "nros/stream.hpp"
// RFC-0088 D5 — nros::SerializationFormat / format_of<M> / linked_format().
#include "nros/serialization_format.hpp"
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
#include "nros/polling_action_server.hpp"
#include "nros/polling_action_client.hpp"
#include "nros/polling_subscription.hpp"
#include "nros/parameter.hpp"
// phase-426 W4 — the ONE parameter facade. Forwards a node's
// declare/get/set/has onto the executor's store across the FFI, so a parameter
// means the same thing wherever it is declared. There is now one node type to
// wear it (phase-427 W4), which is the other half of that guarantee.
// Freestanding: `<string>`/`<vector>` behind `NROS_CPP_STD`.
#include "nros/node_parameters.hpp"
#include "nros/tick_ctx.hpp"
#include "nros/lifecycle.hpp"
// phase-427 W4 — `nros/component_node.hpp` IS GONE. `nros::ComponentNode` was a
// type that WRAPPED a node (RFC-0044 Q1's "wrap, not derive"), so a component
// was not a `Node` and every verb had to be forwarded. Its members are on
// `rclcpp::Node` now, its pool is the opt-in `nros::NodeWithTimers<N>`, and its
// macros are in `component.hpp` below. The rclcpp-shaped, value-returning
// parameter facade phase-417 W2.b pulled in here for is `Node`'s own.
// phase-417 stage 6 step A — `nros::create_subscription_raw`, the
// arena-registration entry point `rclcpp::Node::create_subscription`
// below uses so it has no dispatch of its own.
#include "nros/component.hpp"

namespace nros {

/// Get the global executor handle for Future::wait().
///
/// Returns the raw storage pointer used by the global `init()`/`spin_once()`
/// free functions. Use with `Future::wait(nros::global_handle(), ...)`.
///
/// @return Executor handle, or nullptr if not initialized.
inline void* global_handle() {
    if (!::rclcpp::Node::global_initialized()) return nullptr;
    return ::rclcpp::Node::global_storage();
}

/// Drive transport I/O and dispatch callbacks.
///
/// Call this periodically so subscriptions can receive data.
/// When using manual-poll (no callbacks), this drives the network layer.
///
/// @param timeout_ms  Maximum time to block waiting for I/O (default: 10ms).
/// @return Result indicating success or failure.
inline Result spin_once(int32_t timeout_ms = 10) {
    if (!::rclcpp::Node::global_initialized()) {
        return Result(ErrorCode::NotInitialized);
    }
    return Result(nros_cpp_spin_once(::rclcpp::Node::global_storage(), timeout_ms));
}

/// Register a callback to run BEFORE the global session's entities are torn
/// down — issue 0790.
///
/// rclcpp hangs the shutdown hooks on `Context`, which nano-ros does not have
/// (phase-379's init stage records the collapse into one support object), so
/// they live on the executor — here, the global one `nros::init()` opened and
/// `nros::shutdown()` closes.
///
/// This is the phase with no workaround: the callback runs while every entity
/// still works, so a node can publish a final state, answer a last request,
/// park an actuator or release a bus. `nros::on_shutdown` below runs after
/// teardown, when none of that is possible any more.
///
/// A CLEAN-STOP facility: a watchdog reset, a hard fault or an abort does not
/// come through `nros::shutdown()`, so it does not come through here either.
///
/// @param callback  Function to invoke. Must not be null.
/// @param context   Opaque pointer handed back to `callback`. Must stay valid
///                  until the callback runs or is removed.
/// @param out       Receives the removal handle. Optional.
inline Result pre_shutdown(ShutdownCallback callback, void* context = nullptr,
                           PreShutdownCallbackHandle* out = nullptr) {
    void* executor = global_handle();
    if (executor == nullptr) {
        return Result(ErrorCode::NotInitialized);
    }
    nros_cpp_shutdown_callback_handle_t raw = NROS_CPP_SHUTDOWN_CALLBACK_HANDLE_INVALID;
    nros_cpp_ret_t ret = nros_cpp_add_pre_shutdown_callback(executor, callback, context, &raw);
    if (out != nullptr) *out = PreShutdownCallbackHandle(raw);
    return Result(ret);
}

/// Register a callback to run AFTER the global session's entities are torn
/// down — `rclcpp::on_shutdown`. Issue 0790.
///
/// The entities are gone by the time it runs; use [`pre_shutdown`] for
/// anything that needs the wire.
///
/// @see pre_shutdown for the parameter and error contract.
inline Result on_shutdown(ShutdownCallback callback, void* context = nullptr,
                          OnShutdownCallbackHandle* out = nullptr) {
    void* executor = global_handle();
    if (executor == nullptr) {
        return Result(ErrorCode::NotInitialized);
    }
    nros_cpp_shutdown_callback_handle_t raw = NROS_CPP_SHUTDOWN_CALLBACK_HANDLE_INVALID;
    nros_cpp_ret_t ret = nros_cpp_add_on_shutdown_callback(executor, callback, context, &raw);
    if (out != nullptr) *out = OnShutdownCallbackHandle(raw);
    return Result(ret);
}

/// Remove a callback registered with [`pre_shutdown`]. `true` when `handle`
/// named a live one — "it was not there" is an ordinary answer, as in rclcpp.
inline bool remove_pre_shutdown_callback(PreShutdownCallbackHandle handle) {
    void* executor = global_handle();
    if (executor == nullptr) return false;
    return nros_cpp_remove_pre_shutdown_callback(executor, handle.value()) == 0;
}

/// Remove a callback registered with [`on_shutdown`].
/// @see remove_pre_shutdown_callback
inline bool remove_on_shutdown_callback(OnShutdownCallbackHandle handle) {
    void* executor = global_handle();
    if (executor == nullptr) return false;
    return nros_cpp_remove_on_shutdown_callback(executor, handle.value()) == 0;
}

/// Phase 123.B.2 — block until `nros::ok()` returns false.
///
/// Mirror of `rclcpp::spin(node)`. The typical pattern in user
/// code is: install a SIGINT handler that calls `nros::shutdown()`
/// (which flips `ok()` to false), then `nros::spin()` from `main`.
///
/// Returns the first non-success `spin_once` result, or
/// `Result::success()` after a clean shutdown.
inline Result spin() {
    if (!::rclcpp::Node::global_initialized()) {
        return Result(ErrorCode::NotInitialized);
    }
    Result last = Result::success();
    while (ok()) {
        last = Result(nros_cpp_spin_once(::rclcpp::Node::global_storage(), 10));
        if (!last.ok()) return last;
    }
    return last;
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
    if (!::rclcpp::Node::global_initialized()) {
        return Result(ErrorCode::NotInitialized);
    }
    // Issue 0329 — forward to the single budgeted-spin CFFI entry point. This
    // loop previously budgeted by ITERATION count (`elapsed += timeout`), which
    // collapsed to milliseconds when `spin_once` returned early on a signaled
    // wake — the exact bug `Executor::spin` fixed in Phase 118.C. The correct
    // wall-clock budget now lives once, Rust-side, in `nros_cpp_spin_for`.
    return Result(nros_cpp_spin_for(::rclcpp::Node::global_storage(), duration_ms, poll_ms));
}

} // namespace nros

// ============================================================================
// rclcpp:: — the process-level surface (RFC-0089 stage 6, step A)
// ============================================================================
//
// `rclcpp::init` / `shutdown` / `ok` / `Node` / `spin` / `spin_some` / `Rate` /
// `spin_until_future_complete` used to live in `nros/rclcpp_compat.hpp`, a
// separate header a ported file had to be force-included with. RFC-0089
// §"Naming: replace, with alias as the migration step" makes the ROS 2 spelling
// a FIRST-CLASS name declared by the API headers themselves; §"End state: no
// compat layer survives" is what follows from it — with one spelling per
// concept there is nothing left for a shim to bridge, so it dissolves by
// construction rather than having to be argued away. `nros::` is untouched and
// both spellings work; deprecating `nros::` and migrating the in-tree call
// sites is step B.
//
// They land HERE, in the umbrella, and not in `node.hpp`, because that is where
// their dependencies are. `rclcpp::Node` reaches `nros::create_subscription_raw`
// (`component.hpp`), `nros::Timer`, `nros::ParameterServer`, `nros::Service` and
// `nros::Client`, every one of which includes `node.hpp` itself; `Rate::sleep`
// and `spin_until_future_complete` drive the `nros::spin` / `nros::spin_once`
// free functions defined a few lines above. This IS the header they were
// shimming: the process-level verbs `rclcpp::init` mirrors — `nros::init`,
// `nros::spin`, `nros::spin_once`, `nros::on_shutdown` — are all reached
// through `<nros/nros.hpp>`.
//
// FREESTANDING. Unlike the shim it replaces, this is not hosted-STL by
// construction. `init` / `shutdown` / `ok` and the `--ros-args` predicate need
// nothing but `<cstdlib>`, so they reach a `no_std` C++ build. `Node`, the spin
// forwarders and `Rate` are gated on the standard-library pieces their
// signatures are spelled in (`__has_include`, never `__STDC_HOSTED__` — issue
// 0112, rationale in `publisher.hpp`), so on a minimal libcpp they are absent
// rather than a parse error.

#include <cstdlib>     // std::abort -- the runtime half of RFC-0089 W3.b
#include <type_traits> // the SFINAE guards on create_service / create_client

namespace rclcpp {

// --- Process-level lifecycle -------------------------------------------------
//
// `rclcpp::init()` is a process-level handshake → `nros::init()`.
// `rclcpp::shutdown()` → `nros::shutdown()`. `rclcpp::ok()` → `nros::ok()`
// (nros tracks the shutdown flag).
//
// The TWO-ARGUMENT form is REFUSE-LOUD (RFC-0089 W3.b). It used to forward to
// the same `nros::init()` and discard `argv`, so `--ros-args -r
// chatter:=/other` — the single most common thing a ported `main` passes —
// silently became a wrong-topic bug at runtime. Honouring it is remap
// resolution, RFC-0020 violation class 4, and belongs beside
// `nros::resolve_name` rather than here; until it lands, the honest answer is a
// loud abort carrying the migration.

namespace detail {
/// Does this argv carry `--ros-args`? Factored out of `init` so the decision is
/// TESTABLE: an abort inside `init` can only be exercised by a process that then
/// dies, which is the shape of a check nothing ever runs.
/// `constexpr`, and that is the point: it makes the predicate assertable with a
/// `static_assert`, so its cases run in every `check cpp` with no link and no
/// process. `std::strcmp` is not constexpr in C++14, hence the open-coded compare.
constexpr bool is_ros_args_flag(char const* s, int i = 0) {
    return s == nullptr              ? false
           : "--ros-args"[i] == '\0' ? s[i] == '\0'
           : s[i] != "--ros-args"[i] ? false
                                     : is_ros_args_flag(s, i + 1);
}

constexpr bool argv_has_ros_args(int argc, char const* const* argv, int i = 0) {
    return i >= argc                   ? false
           : argv == nullptr           ? false
           : is_ros_args_flag(argv[i]) ? true
                                       : argv_has_ros_args(argc, argv, i + 1);
}

/// What upstream's `throw` becomes here — phase-428 W5 finding 9.
///
/// The `create_*` verbs on `rclcpp::Node` (reached as `rclcpp::Node`, which is an
/// alias for it since phase-427 W1-W3) used to write
/// `(void)this->create_…(…)` and return the `shared_ptr` regardless. The
/// `(void)` was not incidental: `nros::Result` carries `[[nodiscard]]`
/// (`result.hpp`, phase-428 W6), and the casts SUPPRESSED the one signal that
/// existed. Measured, because the claim is easy to overstate: no C++ lane in
/// this tree compiles with `-Werror`, so a bare discard here is a
/// `-Wunused-result` WARNING and `just check cpp` stays green through it —
/// the Rust-side `-D warnings` has no C++ counterpart. Even at full strength
/// it would only have told the AUTHOR of this header, never the porting user,
/// which is why the fix is a runtime refusal rather than a lint.
///
/// Every alternative was weighed against RFC-0089 Part I ("a difference must
/// be one the compiler points at, and where it cannot be, it must be loud by
/// other means"):
///
/// * a `Result`-returning overload makes the compiler point at it — and at
///   every SUCCEEDING call too, so a ported `auto pub = node->create_publisher
///   <M>("chatter", 10);` no longer compiles. That trades clause 2 (port
///   upstream: same name, same shape) for a diagnostic on the path that is
///   not broken. Rejected.
/// * a null `shared_ptr` says nothing at compile time and, at runtime, says
///   nothing either: `create_wall_timer`'s result is stored and never
///   dereferenced, so a null there is a timer that silently never fires —
///   the same "reads as success" one level down. Rejected.
/// * an `ok()` flag on the node is opt-in, and a ported file never asks. It
///   would also be a member written by us and read by nobody, which is
///   finding 10 in the same sweep. Rejected.
///
/// So: the loudest thing available, at the earliest point the defect is
/// knowable, which is the CALL. This is the identical shape
/// `rclcpp::init(argc, argv)` uses for `--ros-args` a few lines below, and it
/// is what an uncaught upstream throw actually does to a tutorial `main` —
/// terminate with a diagnostic, rather than continue with a dead object.
///
/// The failure is still HANDLEABLE: the underlying out-ref `rclcpp::Node` verbs
/// return `nros::Result` into caller-owned storage, and the message names them.
[[noreturn]] inline void abort_failed_create(const char* verb, const char* name, int32_t code) {
    NROS_ERROR("rclcpp::Node::%s(\"%s\") failed with nros::ErrorCode %d. %s", verb, name,
               static_cast<int>(code), NROS_RCLCPP_ABORT_FAILED_CREATE);
    ::std::abort();
}

/// The one spelling every `create_*` verb uses. Takes the `Result` by value so
/// the `[[nodiscard]]` is consumed HERE and cannot be silently dropped again.
inline void require_created(::nros::Result r, const char* verb, const char* name) {
    if (!r.ok()) {
        abort_failed_create(verb, name, r.raw());
    }
}
} // namespace detail

/// `rclcpp::init(argc, argv)` — ADOPT-BOUNDED, refused at RUNTIME when it must be.
///
/// **Why this is not a `static_assert`.** Whether the process was given
/// `--ros-args` is a value, not a type: the compiler cannot see it. A compile-time
/// refusal would reject every caller — including the overwhelmingly common
/// embedded one that passes `argc`/`argv` straight through from `main` and has no
/// ROS arguments at all — to catch a case that may never occur, and it would make
/// upstream's own tutorial `main` unportable for a reason unrelated to what the
/// program does.
///
/// So the refusal fires where the information is. RFC-0089's rule is that a
/// contract must never silently drop configuration; it is satisfied by being
/// LOUD, and compile time is simply the earliest point loudness is available. When
/// only the value carries the defect, the earliest point is the call.
///
/// * no `--ros-args` in `argv` → identical to `rclcpp::init()`. Nothing is
///   dropped, because nothing was passed.
/// * `--ros-args` present → the process ABORTS with a diagnostic naming the flag,
///   rather than proceeding with a remap it did not apply. A wrong-topic bug that
///   surfaces three hours into a run is the outcome this exists to prevent.
///
/// Remaps and parameter overrides reach a nano-ros process from the LAUNCHER,
/// which projects them into the environment before exec. Honouring them from
/// `argv` is remap resolution — RFC-0020 violation class 4 — so the parser
/// belongs beside `nros::resolve_name`, and phase-417 W3.b tracks it. Until it
/// lands this call is honest about what it cannot do.
inline void init(int argc, char const* const* argv) {
    if (detail::argv_has_ros_args(argc, argv)) {
        // `std::abort`, not a return code: this call site has nowhere to put a
        // failure -- upstream's `init` returns void, and the ported `main` does
        // not check it. Continuing is the one outcome the rule forbids.
        NROS_ERROR("%s", NROS_RCLCPP_REFUSE_INIT_ARGV);
        ::std::abort();
    }
    (void)::nros::init();
}

/// `rclcpp::init()` — the zero-argument form. ADOPT.
inline void init() {
    (void)::nros::init();
}
/// `rclcpp::shutdown()` — ADOPT-BOUNDED. Upstream's channel here is `bool`, so
/// ours is too (RFC-0089's error-channel rule: a ported API keeps upstream's
/// channel even when that is `bool`).
///
/// It used to discard `nros::shutdown()`'s `Result` and answer `true`
/// unconditionally (phase-428 W5 finding 9, one site over from the seven
/// `create_*` verbs and the only one the compiler already pointed at: `just
/// check cpp` carried a standing `-Wunused-result`). `NROS_NODISCARD` on
/// `Result` is what pointed at it (phase-427 W8) — a fini that failed reported
/// success, which is the exact silence the attribute exists to break.
///
/// The bound: upstream returns `false` when the context was never initialised;
/// `nros::shutdown()` answers `success()` for that case (it is a no-op), so we
/// return `true` there. `false` here means the teardown itself failed.
inline bool shutdown() {
    return ::nros::shutdown().ok();
}
inline bool ok() {
    return ::nros::ok();
}

/// `rclcpp::spin_once(timeout_ms)` — INVENTION, kept (RFC-0089 §"Review of the
/// invented parts", item 2), and now reachable under the `rclcpp::` spelling a
/// user is meant to write.
///
/// Upstream's one-cycle verb is `spin_some(node)` — drain what is ready, never
/// wait — and that one is already ported (`rclcpp::spin_some`, `nros.hpp`, a
/// 0-timeout call to this). This is a BLOCKING WAIT WITH A BUDGET, which
/// rclcpp has no verb for and an RTOS task needs: a task that must not
/// spin-poll sleeps until work arrives or the budget expires.
///
/// COLLISION NOTE, recorded rather than hidden: `rclpy.spin_once` exists with
/// the signature `(node, timeout_sec=None)`. A user carrying that habit writes
/// `spin_once(node, 0.1)` here and gets no matching overload — mechanical, not
/// silent. The collision gate watches in case rclcpp ever adds one.
using ::nros::spin_once;

/// Mirror of `rclcpp::FutureReturnCode` (issue 0339).
///
/// `spin_until_future_complete` used to return `void`, so a caller could not
/// tell success from timeout and the standard idiom
///
/// ```cpp
/// if (rclcpp::spin_until_future_complete(node, fut) == rclcpp::FutureReturnCode::SUCCESS) { … }
/// ```
///
/// could not be written against the shim at all.
enum class FutureReturnCode {
    SUCCESS,
    /// The deadline passed with the future still pending.
    TIMEOUT,
    /// `::nros::ok()` went false (shutdown) before the future was ready.
    INTERRUPTED,
};

} // namespace rclcpp

// The node call-shape adapter and the verbs that take one. `std::shared_ptr` is
// in `rclcpp::Node`'s every signature — `std::make_shared<rclcpp::Node>(…)` is
// how a ported file constructs it and `create_publisher` hands one back — so
// where `<memory>` / `<string>` / `<vector>` / `<functional>` are absent the
// type is absent with them.
#if defined(NROS_CPP_HAS_SHARED_PTR) && defined(NROS_CPP_HAS_STD_STRING) &&                        \
    defined(NROS_CPP_HAS_STD_VECTOR) && defined(NROS_CPP_HAS_STD_FUNCTION)

namespace rclcpp {

namespace detail {

// phase-426 W4 — `adopt_executor_param_seed` IS GONE, along with the store it
// existed to reconcile.
//
// It re-read every launch-seeded parameter out of the EXECUTOR's store and
// copied it over the code default, because `rclcpp::Node`'s own
// `nros::ParameterServer` was a DIFFERENT object and would otherwise have
// answered 0.15 while launch said 0.03 (issue 0745). `nros::ComponentNode`
// carried the same dispatch written a second time, in C++17 `if constexpr`,
// and both headers flagged the duplication against themselves.
//
// With one store there is nothing to reconcile: `declare_parameter` declares
// into the store the seed was written to, finds the name already present, and
// reads back the seeded value. The adoption is the FFI's `ALREADY_EXISTS`
// path, not a helper — and it is now one path rather than two that could
// disagree.

} // namespace detail

// --- Node ---------------------------------------------------------------------
//
// phase-427 W1-W3 — THE NODE TYPES ARE MERGED. `rclcpp::Node` is not a class
// here any more; it is an ALIAS for `rclcpp::Node`, and the hosted call shape
// that used to be a separate adapter class in this file is now a set of
// overloads on that one type (declared in `node.hpp`, defined below).
//
// What that closes, in order of how much it cost:
//
//   * THE DUPLICATE PARAMETER FACADE. `nros.hpp` said so about itself ("KNOWN
//     DUPLICATION... There should be ONE helper"): the same rclcpp-shaped
//     `declare_parameter<T>` existed twice, once in C++14 here and once in
//     C++17 `if constexpr` on `ComponentNode`. One node type is what makes one
//     facade possible.
//   * THE `get_logger()` COLLISION. Two accessors with identical signatures
//     and different observable behaviour — a real node logger and the
//     hardcoded `"nros.compat"` sentinel. One type can hold only one, so the
//     merge forced the decision W5 records.
//   * THE TWO-VOCABULARY SPLIT. A ported file wrote `rclcpp::Node` and got a
//     type with no graph queries, no lifecycle, no callback groups, no action
//     entities and no out-ref creators; a native file wrote `rclcpp::Node` and
//     got no `shared_ptr` creators and no parameters. Neither list was a
//     design; both were what the other file happened to have.
//
// The class holds no dispatch state and runs no loop — every entity it creates
// is registered on the executor arena `rclcpp::init()` opened (issue 0465, one
// session per image), which is all RFC-0019 permits a wrapper to be.
//
// Threading: callbacks fire on whatever thread services a spin verb, mirroring
// the rclcpp default. A node cannot move between executors (RFC-0002, one
// executor per RTOS task), which is why the executor is decided at construction
// and never afterwards.

} // namespace rclcpp

namespace nros {

// The HOSTED half of `rclcpp::Node`, defined here rather than in `node.hpp`
// because every body below needs a complete entity type (`Publisher<M>`,
// `Subscription<M>`, `Service<S>`, `Client<S>`) or a helper the umbrella pulls
// in (`create_subscription_raw` from `component.hpp`, the callback cells from
// `subscription.hpp` / `timer.hpp`). That is the same split Phase 84.G8 already
// uses for the out-ref `Node::create_X<>` templates, which live in the header
// that owns each entity.
//
// The declarations — with the default arguments — are in `node.hpp` under
// `NROS_CPP_NODE_HOSTED`, the one predicate both files share.

#ifdef NROS_CPP_NODE_HOSTED

// -- publishers ---------------------------------------------------------------

} // namespace nros

namespace rclcpp {
template <typename M>
inline ::std::shared_ptr<Publisher<M>> Node::create_publisher(const ::std::string& topic,
                                                              const ::nros::QoS& qos) {
    auto p = ::std::make_shared<Publisher<M>>();
    ::rclcpp::detail::require_created(this->create_publisher<M>(*p, topic.c_str(), qos),
                                      "create_publisher", topic.c_str());
    // OWNERSHIP: the arena stores `&entity` as its dispatch context and there
    // is no unregister, so the cell must outlive the registration whatever the
    // caller does with the pointer we hand back.
    this->hosted().owned_entities.push_back(p);
    return p;
}
} // namespace rclcpp

namespace nros {} // namespace nros

namespace rclcpp {
template <typename M>
inline ::std::shared_ptr<Publisher<M>> Node::create_publisher(const ::std::string& topic,
                                                              ::size_t depth) {
    return this->create_publisher<M>(topic, ::nros::QoS(static_cast<uint32_t>(depth)));
}
} // namespace rclcpp

namespace nros {

// -- subscriptions ------------------------------------------------------------
//
// phase-417 — ONE DISPATCH PATH. This ARENA-REGISTERS the subscription through
// `nros::create_subscription_raw` (`component.hpp`), the same
// `nros_cpp_subscription_register` call the native callback-style
// `create_subscription` makes, so the executor owns the subscriber and
// dispatches the callback during `spin_once` — whichever spin verb the caller
// drives. It used to create a POLL-mode subscription and drain it from a
// node-local `pump()`, which only `rclcpp::spin` / `spin_some` called: a file
// that spun `nros::spin_once()` instead got zero callbacks and no diagnostic.
//
// Why not the out-ref `create_subscription(sub, topic, cb, qos)` overload: that
// one is SFINAE-restricted to `void(*)(const M&)` — a plain function pointer
// with NO context slot — and every ported rclcpp callback captures.
//
// The receive-buffer hint is `rx_buffer_capacity<M>` — the same number the poll
// path's `take()` stacks — and NOT the strict `rx_size_bound<M>`, whose
// unbounded-type arm is a deliberate compile error (issue 0964). A ported file
// must not stop compiling because its message has an unbounded string.
//
// ONE THING THIS LOSES, stated rather than left silent:
// `create_subscription_raw` hardcodes an EMPTY type hash (it takes no such
// parameter), which `normalize_type_hash` turns into `"TypeHashNotSupported"`.
// It does not affect DELIVERY — a subscriber's data keyexpr puts `*` in the
// hash slot — but under `ros-iron` / `ros-jazzy` the subscription's LIVELINESS
// token advertises the placeholder instead of the real hash, so
// `ros2 topic info --verbose` reads differently.
//
// WHAT THE RETURNED POINTER IS: a keep-alive, which is all upstream source does
// with it (`rclcpp::Subscription<M>::SharedPtr sub_;`). The executor owns the
// real subscriber, so `sub->take(msg)` on it answers `NotInitialized` — the
// sample went to your callback.
} // namespace nros

namespace rclcpp {
template <typename M, typename Cb>
inline typename Subscription<M>::SharedPtr
Node::create_subscription(const ::std::string& topic, const ::nros::QoS& qos, Cb cb) {
    // phase-456 W2 — the arena owns everything, so this allocates nothing.
    //
    // This used to `make_shared` a `detail::SubscriptionCallback<M>` cell whose
    // only job was to give a capturing lambda a stable address, push it into
    // `owned_entities` to keep it alive, and hand back a `shared_ptr` aliasing
    // into it. W1 put the capture in the arena entry, so the cell has nothing
    // left to hold; W2 stops pretending the result is a pointer to an object.
    using Capture = ::nros::InplaceFn<void(const M&)>;
    Capture captured(::nros::tr::forward_rvalue(cb));
    ::size_t handle_id = 0;
    ::rclcpp::detail::require_created(::nros::detail::register_subscription_capturing<M>(
                                          *this, topic.c_str(), qos, captured, &handle_id),
                                      "create_subscription", topic.c_str());
    return typename Subscription<M>::SharedPtr(this->executor_handle(), handle_id);
}
} // namespace rclcpp

namespace nros {} // namespace nros

namespace rclcpp {
template <typename M, typename Cb>
inline typename Subscription<M>::SharedPtr Node::create_subscription(const ::std::string& topic,
                                                                     ::size_t depth, Cb cb) {
    return this->create_subscription<M>(topic, ::nros::QoS(static_cast<uint32_t>(depth)),
                                        ::nros::tr::forward_rvalue(cb));
}
} // namespace rclcpp

namespace nros {

// -- wall timer ---------------------------------------------------------------

#ifdef NROS_CPP_HAS_STD_CHRONO
/// `create_wall_timer(period, callback)` — fires `callback()` every `period`,
/// dispatched by the EXECUTOR during `spin_once`, i.e. under whichever spin
/// verb the caller drives.
///
/// The `std::chrono::duration` -> milliseconds conversion is the only work this
/// function does beyond delegating; that is ergonomics and permitted, the
/// schedule is not. See the envelope on `rclcpp::Timer` (`timer.hpp`) for the
/// two things it costs (millisecond resolution, and catch-up rather than rcl's
/// drop-the-backlog on a missed deadline).
///
/// RETURNS `Timer::SharedPtr`, not the deleted `TimerBase::SharedPtr`
/// (phase-430 W7). The pointer is an ALIASING co-owner of the private
/// `detail::WallTimer` cell, pointing at its `timer` member — the same shape
/// `create_subscription` returns, which is why the cell needs no base class to
/// be handed out.
} // namespace nros

namespace rclcpp {
template <typename Rep, typename Period, typename Cb>
inline ::std::shared_ptr<::nros::Timer>
Node::create_wall_timer(::std::chrono::duration<Rep, Period> period, Cb cb) {
    auto t = ::std::make_shared<::rclcpp::detail::WallTimer>();
    t->callback = ::nros::tr::forward_rvalue(cb);
    const auto ms = ::std::chrono::duration_cast<::std::chrono::milliseconds>(period).count();
    // A null return would be silent HERE above all: a ported node stores the
    // timer handle and never dereferences it, so a dead timer would simply
    // never fire.
    ::rclcpp::detail::require_created(
        this->create_wall_timer(t->timer, ms > 0 ? static_cast<uint64_t>(ms) : uint64_t(0),
                                &::rclcpp::detail::WallTimer::trampoline, t.get()),
        "create_wall_timer", "");
    // The arena holds `t.get()`; the node keeps the cell alive, and
    // `~nros::Timer` cancels the slot when it finally drops. `owned_entities`,
    // NOT a typed `timers_` member — the typed vector existed so the deleted
    // `pump()` could iterate it, and it was the member that broke the
    // capability-layout rule.
    this->hosted().owned_entities.push_back(t);
    return ::std::shared_ptr<::nros::Timer>(t, &t->timer);
}
} // namespace rclcpp

namespace nros {
#endif // NROS_CPP_HAS_STD_CHRONO

// -- parameters ---------------------------------------------------------------
//
// MOVED OUT OF THIS BLOCK (phase-426 W4). The definitions are below, after
// `#endif // NROS_CPP_HAS_SHARED_PTR && ...`, because a node's parameters are
// not a hosted capability: the scalar FFI is `<cstdint>` and a `const char*`,
// and `nros::Seq<T, N>` carries arrays with no STL either. Only the
// `std::string`-KEYED overloads (declared on the class in `node.hpp`) need the
// STL, and only those keep the gate.
//
// `parameters()` IS GONE (phase-426 W4). It handed out a reference to the node's
// own `nros::ParameterServer`, described as the escape hatch for "the C-API
// helpers that take an `nros_parameter_server_t*`" - and no such helper
// existed: `nros_executor_register_parameter_services` takes the executor,
// never a standalone store. That class is deleted now too, so there is nothing
// to return and nothing that took it. A node's parameters are reached through
// `declare_parameter` / `get_parameter` / `set_parameter`, which name the store
// the services actually read.

// -- services and clients -----------------------------------------------------
//
// A callback of upstream's `shared_ptr<Request>, shared_ptr<Response>` shape is
// REFUSE-LOUD rather than "no matching function": that signature needs a
// per-request heap allocation on the delivery path, which is a second delivery
// path, not a spelling.

} // namespace nros

namespace rclcpp {
template <typename S>
inline ::std::shared_ptr<Service<S>> Node::create_service(const ::std::string& name,
                                                          const ::nros::QoS& qos) {
    auto s = ::std::make_shared<Service<S>>();
    ::rclcpp::detail::require_created(this->template create_service<S>(*s, name.c_str(), qos),
                                      "create_service", name.c_str());
    this->hosted().owned_entities.push_back(s);
    return s;
}
} // namespace rclcpp

namespace nros {} // namespace nros

namespace rclcpp {
template <typename S, typename F, typename>
inline ::std::shared_ptr<Service<S>> Node::create_service(const ::std::string& name, F callback,
                                                          const ::nros::QoS& qos) {
    auto s = ::std::make_shared<Service<S>>();
    this->hosted().owned_entities.push_back(s);
    ::rclcpp::detail::require_created(
        this->template create_service<S>(*s, name.c_str(), callback, qos), "create_service",
        name.c_str());
    return s;
}
} // namespace rclcpp

namespace nros {} // namespace nros

namespace rclcpp {
template <typename S, typename F, typename, typename>
inline ::std::shared_ptr<Service<S>> Node::create_service(const ::std::string&, F,
                                                          const ::nros::QoS&) {
    static_assert(::rclcpp::detail::refuse<F>::value,
                  NROS_RCLCPP_REFUSE_SHARED_PTR_SERVICE_CALLBACK);
    return ::std::shared_ptr<Service<S>>();
}
} // namespace rclcpp

namespace nros {} // namespace nros

namespace rclcpp {
template <typename S>
inline ::std::shared_ptr<Client<S>> Node::create_client(const ::std::string& name,
                                                        const ::nros::QoS& qos) {
    auto c = ::std::make_shared<Client<S>>();
    ::rclcpp::detail::require_created(this->template create_client<S>(*c, name.c_str(), qos),
                                      "create_client", name.c_str());
    this->hosted().owned_entities.push_back(c);
    return c;
}
} // namespace rclcpp

namespace nros {} // namespace nros

namespace rclcpp {
template <typename S, typename F, typename>
inline ::std::shared_ptr<Client<S>> Node::create_client(const ::std::string& name, F callback,
                                                        const ::nros::QoS& qos) {
    auto c = ::std::make_shared<Client<S>>();
    this->hosted().owned_entities.push_back(c);
    ::rclcpp::detail::require_created(
        this->template create_client<S>(*c, name.c_str(), callback, qos), "create_client",
        name.c_str());
    return c;
}
} // namespace rclcpp

namespace nros {} // namespace nros

namespace rclcpp {
template <typename S, typename F, typename, typename>
inline ::std::shared_ptr<Client<S>> Node::create_client(const ::std::string&, F,
                                                        const ::nros::QoS&) {
    static_assert(::rclcpp::detail::refuse<F>::value,
                  NROS_RCLCPP_REFUSE_SHARED_PTR_SERVICE_CALLBACK);
    return ::std::shared_ptr<Client<S>>();
}
} // namespace rclcpp

namespace nros {

#endif // NROS_CPP_NODE_HOSTED

} // namespace nros

namespace rclcpp {

// `rclcpp::Node` is declared at the bottom of `node.hpp`, UNCONDITIONALLY —
// see there for why. It is not declared here, because a freestanding target
// gets the class and must get its ROS 2 spelling with it.
//
// phase-417 — `pump()` IS GONE and does not come back. It ran a node's wall
// timers and drained its polling subscriptions, and only `rclcpp::spin` /
// `spin_some` called it, so a node driven by any other spin verb dispatched
// nothing. Every entity a node creates is registered on the executor arena, so
// there is nothing left for a node-local sweep to do and mixing spin spellings
// is harmless. A second dispatch path here is the RFC-0019 violation the whole
// item was about, and it cannot be made visible by a diagnostic.

// --- rclcpp::create_timer (phase-430 W6) -------------------------------------
//
// Humble has NO `Node::create_timer` member — the clock-taking verb is a FREE
// function, `rclcpp::create_timer(node, clock, period, callback[, group])`, and
// that is the only form a Humble-era file can be written in. Ours takes the
// same arguments in the same order and returns the same cell
// `create_wall_timer` returns.
//
// The clock is `nros::Clock*`, which is exactly what `node->get_clock()` hands
// back, so the ported line binds with no conversion. Upstream's parameter is
// `rclcpp::Clock::SharedPtr`; there is no allocator here (RFC-0022) and the
// clock is a member of the node, so the pointer is the counterpart — the same
// ADOPT-BOUNDED trade `get_clock()` itself makes.
//
// WHAT THE CLOCK DOES, and it is the whole point of the verb: a
// `NROS_CLOCK_ROS_TIME` clock follows `/clock`, so the timer stops when a bag
// stops and re-times when the bag is replayed at another rate; a
// `NROS_CLOCK_STEADY_TIME` one does not. `create_wall_timer` is the steady
// verb and is unaffected by simulated time. Upstream distinguishes the two
// cases by TYPE (`WallTimer` vs `GenericTimer<ClockT>`) *and* by VERB; the type
// distinction is unportable here (the clock is a runtime field on one flat
// `Timer` — RFC-0089 §"Timer, studied against RTOS semantics"), the verb
// distinction is portable and is what ported code writes, so that is what we
// take.
//
// NOT ADDED: a clock-less `Node::create_timer(period, callback)`. That member
// arrives in Iron, and the captured surface here is Humble; adding it now would
// claim a spelling the recorded upstream does not have.

namespace detail {

/// `NodeT` in upstream's signature is anything node-shaped — `this`, a
/// `shared_ptr`, a reference. One overload set, so the free function does not
/// need three copies.
inline ::rclcpp::Node& as_node_ref(::rclcpp::Node& n) {
    return n;
}
inline ::rclcpp::Node& as_node_ref(::rclcpp::Node* n) {
    return *n;
}
inline ::rclcpp::Node& as_node_ref(const ::std::shared_ptr<::rclcpp::Node>& n) {
    return *n;
}

} // namespace detail

#ifdef NROS_CPP_NODE_HOSTED

/// `rclcpp::create_timer(node, clock, period, callback)` — humble's only
/// clock-taking timer verb.
template <typename NodeT, typename CallbackT>
inline ::std::shared_ptr<::nros::Timer>
create_timer(NodeT&& node, ::nros::Clock* clock, ::nros::Duration period, CallbackT&& callback) {
    ::rclcpp::Node& n = detail::as_node_ref(node);
    auto t = ::std::make_shared<detail::WallTimer>();
    t->callback = ::nros::tr::relay<CallbackT>(callback);
    const int64_t ns = period.nanoseconds();
    const uint64_t ms = ns > 0 ? static_cast<uint64_t>(ns / 1000000) : uint64_t(0);
    // Same refusal as the seven `create_*` verbs: `rclcpp::create_timer` is a
    // free function main added after this branch's sweep, and it had the same
    // discarded `Result`. A dead timer never fires and is never dereferenced,
    // so nothing downstream would say so.
    detail::require_created(n.create_timer(t->timer, clock != nullptr ? *clock : *n.get_clock(), ms,
                                           &detail::WallTimer::trampoline, t.get()),
                            "create_timer", "");
    // Same ownership rule as `create_wall_timer`: the arena holds `t.get()` and
    // has no unregister, so the node keeps the cell alive and the returned
    // pointer is an ALIASING co-owner of its `nros::Timer` member.
    n.own_entity(t);
    return ::std::shared_ptr<::nros::Timer>(t, &t->timer);
}

#ifdef NROS_CPP_HAS_STD_CHRONO
/// `rclcpp::create_timer(node, clock, 100ms, callback)` — the `std::chrono`
/// spelling, which is what a ported file actually writes. `rclcpp::Duration` is
/// implicitly constructible from a chrono duration upstream; `nros::Duration`
/// is not (it reaches freestanding targets where `<chrono>` does not exist), so
/// the conversion is an overload rather than a constructor.
template <typename NodeT, typename Rep, typename Period, typename CallbackT>
inline ::std::shared_ptr<::nros::Timer> create_timer(NodeT&& node, ::nros::Clock* clock,
                                                     ::std::chrono::duration<Rep, Period> period,
                                                     CallbackT&& callback) {
    const auto ns = ::std::chrono::duration_cast<::std::chrono::nanoseconds>(period).count();
    return create_timer(::nros::tr::relay<NodeT>(node), clock,
                        ::nros::Duration::from_nanoseconds(static_cast<int64_t>(ns)),
                        ::nros::tr::relay<CallbackT>(callback));
}
#endif // NROS_CPP_HAS_STD_CHRONO

#endif // NROS_CPP_NODE_HOSTED

// --- spin / spin_some --------------------------------------------------------
//
// phase-417 — FORWARDERS, and nothing else. `rclcpp::spin(node)` is
// `nros::spin()` (block until `ok()` goes false) and `rclcpp::spin_some(node)`
// is one 0-timeout `nros::spin_once`. Both used to call a node-local `pump()`
// first, which was the ONLY thing that ran a ported node's timers and
// subscriptions; now those live on the executor, so these two dispatch nothing
// of their own and a file that reaches for `nros::spin_once()` or drives an
// `nros::Executor` gets exactly the same callbacks. That equivalence is the
// structural prerequisite for step B: after it both node types share a name,
// and a mismatch would be invisible.
//
// One behaviour change for an existing caller: `nros::spin()` RETURNS on the
// first failing `spin_once`, where this loop used to discard the result and
// keep going. A dead session now ends the spin instead of looking alive
// forever, which is what `nros::spin()` and `Executor::spin` already promise.
//
// The `node` argument is still checked, and still otherwise unused: there is
// one session per image (issue 0465), so every `rclcpp::Node` is on the global
// executor these verbs drive. Upstream takes the node for the same reason and
// spins the executor it belongs to.

inline void spin(const Node::SharedPtr& node) {
    if (!node || !node->initialized()) {
        return;
    }
    (void)::nros::spin();
}

inline void spin_some(const Node::SharedPtr& node) {
    if (!node || !node->initialized()) {
        return;
    }
    (void)::nros::spin_once(0);
}

// Future type is templated rather than `const auto& future` so the header
// stays parseable under `-std=c++14` (the C++20 abbreviated-function-template
// syntax breaks `just check cpp`'s freestanding probe).
//
// issue 0339 — the bounded branch used to call `Executor::spin(timeout_ms)`
// and never consult the future, so it BURNED THE WHOLE TIMEOUT even when the
// future completed on the first spin: a `wait_for_service` / `send_request`
// sequence ported from rclcpp paid the full timeout on every SUCCESSFUL call.
// The unbounded branch directly below already had the right shape; both now
// share it, differing only in whether a deadline exists.
template <typename Future>
inline FutureReturnCode spin_until_future_complete(const Node::SharedPtr& node,
                                                   const Future& future, int32_t timeout_ms = -1) {
    if (!node || !node->initialized()) {
        return FutureReturnCode::INTERRUPTED;
    }
    // Poll slice: same 10 ms the unbounded loop always used.
    constexpr int32_t kPollMs = 10;
    const bool bounded = timeout_ms >= 0;
    const uint64_t start_ns = nros_cpp_time_ns();
    const uint64_t budget_ns = bounded ? static_cast<uint64_t>(timeout_ms) * 1000000ull : 0ull;

    while (::nros::ok()) {
        if (future.is_ready()) {
            return FutureReturnCode::SUCCESS;
        }
        (void)::nros::spin_once(kPollMs);
        // Re-check before the deadline test: a future that became ready on the
        // spin just above must report SUCCESS even if the budget expired in
        // the same slice.
        if (future.is_ready()) {
            return FutureReturnCode::SUCCESS;
        }
        if (bounded && nros_cpp_time_ns() - start_ns >= budget_ns) {
            return FutureReturnCode::TIMEOUT;
        }
    }
    return FutureReturnCode::INTERRUPTED;
}

} // namespace rclcpp

#endif // NROS_CPP_HAS_SHARED_PTR && ...

// --- node parameters ---------------------------------------------------------
//
// Out here rather than in the hosted block above: see the note at the
// `-- parameters --` marker there, and `node.hpp` for why the gate moved to
// the `std::string`-keyed overloads alone (phase-426 W4).

// -- parameters ---------------------------------------------------------------
//
// Forwarders onto THE parameter store — the `nros_params::ParameterServer` the
// EXECUTOR owns, reached through `nros/node_parameters.hpp`. There is no other
// one any more.
//
// What changed in phase-426 W4, and why it is not a detail: these used to
// forward to an inline `nros::ParameterServer<NROS_RCLCPP_MAX_PARAMS>` member
// on the hosted block — a second store, node-local, which the six
// `rcl_interfaces/srv/*` servers could not read. So a parameter declared here
// was invisible to `ros2 param get`, a sibling node did not share it, and the
// launch seed had to be copied across by a helper because the two stores could
// not be the same object. The member is deleted; the seam is one FFI call;
// `ros2 param get <node> <name>` sees what `declare_parameter` wrote. Issue
// 0793 / RFC-0089 §"Parameters".
//
// ADOPT-BOUNDED still, and the envelope is now about TYPES rather than scope:
// bool / int / int64_t / double reach the store, `std::string` and
// `std::vector<T>` do under `NROS_CPP_STD`, and rclcpp's `ParameterDescriptor`
// / `ignore_override` / callback arguments remain absent (the
// compile-time-options rule). The WIRE half of the acceptance — that the six
// services answer per node FQN — is phase-426 W3/W6.
//
// Where the image declares no `param_services` capability there is no store at
// all, and every call answers `ErrorCode::Unsupported`. This facade wears
// upstream's value-returning signature, which has nowhere to report that, so
// `declare_parameter` returns the code default. The loud half USED to be
// `nros::ComponentNode`, whose own facade recorded the failure on an `ok()`
// flag and made it boot-fatal; phase-427 W4 deleted that type, so no C++ path
// is boot-fatal on a missing store any more. `rclcpp::Node` still carries the
// flag (`set_error` / `ok()`); routing the parameter path back onto it is a
// separate decision, because it changes what an upstream-shaped
// `declare_parameter` does.

/// `rclcpp::Node::declare_parameter<T>(name, default)` — declare, then read
/// back, returning the value in effect.
///
/// Re-declaring is not an error: a launch-seeded parameter is DECLARED before
/// user code runs, and upstream's contract is that `declare` adopts the
/// override. That adoption is now the store's own `ALREADY_EXISTS` answer
/// followed by the read-back below, rather than a helper that copied a value
/// between two stores. On any other failure the code default is returned.

namespace rclcpp {
template <typename T> inline T Node::declare_parameter(const char* name, T default_value) {
    // phase-446 W6 -- a declaration the contract's `params:` does not make, or
    // makes with another type, refuses the boot through `set_error` before
    // anything reaches the store (see `Node::check_declared_param`).
    if (!this->check_declared_param(name, ::nros::detail::node_param_type<T>::value)) {
        return default_value;
    }
    const ::nros_cpp_node_t* h = this->ffi_handle();
    Result r = ::nros::detail::node_param_declare(h, name, default_value);
    if (!r.ok() && r.raw() != NROS_RET_ALREADY_EXISTS) {
        return default_value;
    }
    T out = T();
    if (!::nros::detail::node_param_get(h, name, out).ok()) {
        return default_value;
    }
    return out;
}
} // namespace rclcpp

namespace nros {} // namespace nros

namespace rclcpp {
template <typename T> inline bool Node::get_parameter(const char* name, T& out) const {
    return ::nros::detail::node_param_get(this->ffi_handle(), name, out).ok();
}
} // namespace rclcpp

namespace nros {} // namespace nros

namespace rclcpp {
template <typename T> inline T Node::get_parameter(const char* name) const {
    T out = T();
    (void)::nros::detail::node_param_get(this->ffi_handle(), name, out);
    return out;
}
} // namespace rclcpp

namespace nros {

/// phase-426 W4 — `set_parameter` now goes through the SAME
/// `ParameterServer::apply` a remote `ros2 param set` does, so a read-only
/// parameter is refused here exactly as it is on the wire, and the value a
/// service reports back is the value this wrote.
} // namespace nros

namespace rclcpp {
template <typename T> inline Result Node::set_parameter(const char* name, T value) {
    return ::nros::detail::node_param_set(this->ffi_handle(), name, value);
}
} // namespace rclcpp

namespace nros {} // namespace nros

namespace rclcpp {
inline bool Node::has_parameter(const char* name) const {
    return ::nros::detail::node_param_has(this->ffi_handle(), name);
}
} // namespace rclcpp

// --- Rate / WallRate (phase-417 W2.d) ----------------------------------------
//
// A FORWARDER onto `nros::spin(remaining_ms, poll_ms)`, which budgets by wall
// clock inside `nros_cpp_spin_for` — Rust-side, once.
//
// The obvious fifteen-line class with its own sleep loop is NOT admissible
// here, and the distinction is the whole point of this item: a loop in the
// wrapper that spins the executor is RFC-0020 violation class 2, and a wrapper
// that BLOCKS without spinning is what RFC-0021 forbids. Same capability, and
// only one of the two shapes is allowed. So `sleep()` computes a deadline and
// makes exactly one call into the entry point that already exists.
//
// ADOPT-BOUNDED, and the envelope is load-bearing enough that a ported loop
// behaves DIFFERENTLY here even though it compiles unchanged:
//
//   * **Your callbacks RUN during the sleep.** `nros::spin` drives the
//     executor, so subscriptions and timers registered on the arena fire while
//     `rate.sleep()` is blocked. Under a single-threaded rclcpp executor they
//     do not — `Rate::sleep()` there is a pure `sleep_for`, and callbacks only
//     run when you call `spin_some`. The nano-ros behaviour is the one a
//     `while (ok()) { work(); rate.sleep(); }` loop usually WANTS (it is why
//     that loop needs a second thread upstream), but it is not the same
//     contract, and a callback that races your loop body is the way it shows.
//     Since phase-417 that includes a `rclcpp::Node`'s OWN timers and
//     subscriptions: they are executor entities now, so `rate.sleep()` runs
//     them.
//   * `Rate` and `WallRate` are the SAME clock. Upstream's `Rate` measures ROS
//     time and `WallRate` steady time; both here read the monotonic
//     `nros_cpp_time_ns()`, so a `Rate` does not slow down under a sim clock.
//     `WallRate` is faithful; `Rate` is `WallRate` under a second name.
//   * Resolution is ONE MILLISECOND — the FFI budget is `uint32_t` ms — and the
//     remaining time is rounded UP, so a period below 1 ms cannot be met and a
//     rate faster than 1 kHz is not expressible.
//   * Before `nros::init()` there is no executor to spin, so `sleep()` returns
//     immediately rather than blocking. A rate loop that runs before init busy
//     spins; upstream would have slept.
//
// Gated on `<chrono>` alone: the duration constructor and `period()` are spelled
// in `std::chrono`, and everything else it touches is an integer.
// phase-442 W5 — `Rate` EXISTS ON EVERY TARGET.
//
// It used to sit inside `#ifdef NROS_CPP_HAS_STD_CHRONO`, which made the whole
// type a property of the toolchain: a node written against `rclcpp::Rate`
// compiled on a host and vanished on ThreadX. Measured, the gate was paying for
// two members out of five — `Rate(double)`, `sleep()` and `reset()` are integer
// arithmetic over `nros_cpp_time_ns()` and never needed `<chrono>` at all.
//
// The two that did are handled differently, and the difference is the rule
// RFC-0096 D1 states. `period()` returns `nros::Duration`, which is ours and
// exists everywhere, so the type's SHAPE no longer follows a probe. The
// `std::chrono` CONSTRUCTOR stays behind the capability macro, which is a gate
// on a METHOD — permitted, since `sizeof(Rate)` is `int64_t` + `uint64_t` in
// every configuration and two translation units cannot disagree about it.
//
// That constructor is the one thing here W8 still has to resolve: its parameter
// names `std::chrono::duration`, and W8's acceptance is zero `NROS_CPP_HAS_*` in
// the tree. Recorded rather than quietly left, because a gated method is exactly
// the kind of residue that reads as finished work.

namespace rclcpp {

/// `rclcpp::Rate` — a periodic loop rate, driven by the nano-ros executor.
/// See the envelope above; the differences from upstream are real.
class Rate {
  public:
    /// Construct from a frequency in Hz. `0` or negative disables the rate:
    /// `sleep()` becomes a no-op returning `false`.
    explicit Rate(double frequency_hz)
        : period_ns_(frequency_hz > 0.0 ? static_cast<int64_t>(1000000000.0 / frequency_hz) : 0) {
        reset();
    }

    /// Construct from a period — `rclcpp::Rate(nros::Duration::from_nanoseconds(n))`.
    /// The always-available spelling, on every target.
    explicit Rate(::nros::Duration period) : period_ns_(period.nanoseconds()) {
        if (period_ns_ < 0) period_ns_ = 0;
        reset();
    }

#ifdef NROS_CPP_HAS_STD_CHRONO
    /// Construct from a period — `rclcpp::Rate(std::chrono::milliseconds(100))`.
    ///
    /// A gated METHOD, not a gated type: `sizeof(Rate)` is the same in every
    /// configuration. Where `<chrono>` is absent, the `nros::Duration` overload
    /// above is the same operation.
    template <typename Rep, typename Period>
    explicit Rate(std::chrono::duration<Rep, Period> period)
        : period_ns_(std::chrono::duration_cast<std::chrono::nanoseconds>(period).count()) {
        if (period_ns_ < 0) period_ns_ = 0;
        reset();
    }
#endif

    /// Spin the executor until the next tick is due.
    ///
    /// @return `true` if there was time left to wait, `false` if the loop had
    ///         already overrun the period (upstream's contract) or the rate is
    ///         disabled.
    bool sleep() {
        if (period_ns_ <= 0) {
            return false;
        }
        const uint64_t now_ns = nros_cpp_time_ns();
        if (now_ns >= next_tick_ns_) {
            // Overran. Re-anchor on now rather than firing a burst to catch up
            // — upstream's `Rate` contract, and the choice the retired
            // node-local `pump()` used to make for wall timers before they
            // became executor timers (which catch up instead; see the envelope
            // on `rclcpp::Timer`).
            next_tick_ns_ = now_ns + static_cast<uint64_t>(period_ns_);
            return false;
        }
        const uint64_t remaining_ns = next_tick_ns_ - now_ns;
        // Round UP: truncating a 0.4 ms remainder to 0 would turn `sleep()`
        // into a busy loop.
        uint32_t remaining_ms = static_cast<uint32_t>((remaining_ns + 999999ull) / 1000000ull);
        if (remaining_ms == 0) remaining_ms = 1;
        next_tick_ns_ += static_cast<uint64_t>(period_ns_);
        const int32_t poll_ms = remaining_ms < 10u ? static_cast<int32_t>(remaining_ms) : 10;
        (void)::nros::spin(remaining_ms, poll_ms);
        return true;
    }

    /// `rclcpp::Rate::reset()` — anchor the next tick one period from now.
    void reset() { next_tick_ns_ = nros_cpp_time_ns() + static_cast<uint64_t>(period_ns_); }

    /// The configured period.
    ///
    /// `nros::Duration` rather than `std::chrono::nanoseconds` (phase-442 W5):
    /// a return type that exists only where `<chrono>` does would keep the whole
    /// class hostage to the toolchain, which is what this work item removed.
    /// `.nanoseconds()` is the same number upstream's `.count()` gives.
    ::nros::Duration period() const { return ::nros::Duration::from_nanoseconds(period_ns_); }

  private:
    int64_t period_ns_;
    uint64_t next_tick_ns_;
};

/// `rclcpp::WallRate` — the steady-clock rate. Identical to `Rate` here; see
/// the "same clock" bullet in the envelope above.
using WallRate = Rate;

} // namespace rclcpp

#endif // NROS_CPP_HPP
