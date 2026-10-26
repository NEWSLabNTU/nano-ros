// nros-cpp: Umbrella header
// Include this single header to get the full nros C++ API.
//
// Freestanding C++ compatible — no STL, no exceptions, no RTTI required.

/**
 * @file nros.hpp
 * @ingroup grp_init
 * @brief Umbrella header — pulls in every public C++ API surface.
 */

#ifndef NROS_RCLCPP_RCLCPP_HPP
#define NROS_RCLCPP_RCLCPP_HPP

// Phase 118.D — pull cbindgen-generated FFI before any wrapper hpp so
// `rclcpp/qos.hpp`'s `#ifndef NROS_CPP_FFI_H` guard skips its local
// fallback definitions in favor of the canonical types.
#include "rclcpp/detail/nros_cpp_ffi.h"

#include "rclcpp/detail/traits.hpp"
#include "rclcpp/logging.hpp"
#include "rclcpp/result.hpp"
// Issue 0789 — the clock / time / duration surface. `node.now()` and
// `node.get_clock()->now()` are what a ported rclcpp publisher calls to
// stamp a header, so the umbrella carries all three. Phase 379 W5 moved
// `duration.hpp` ABOVE `qos.hpp`: the QoS deadline / lifespan / lease
// accessors take and return `rclcpp::Duration`. `qos.hpp` also includes it
// directly, so this order is legibility rather than load-bearing.
#include "rclcpp/duration.hpp"
#include "rclcpp/time.hpp"
#include "rclcpp/clock.hpp"
#include "rclcpp/qos.hpp"
#include "rclcpp/options.hpp"
#include "rclcpp/future.hpp"
#include "rclcpp/stream.hpp"
// RFC-0088 D5 — rclcpp::SerializationFormat / format_of<M> / linked_format().
#include "rclcpp/detail/serialization_format.hpp"
// Phase 84.G8: node.hpp no longer pulls in the heavy entity headers —
// each entity header carries its own out-of-line `Node::create_X<>()`
// template definition. The umbrella pulls in every entity explicitly so
// `#include <rclcpp/rclcpp.hpp>` still yields the full API.
#include "rclcpp/node.hpp"
#include "rclcpp/publisher.hpp"
#include "rclcpp/subscription.hpp"
#include "rclcpp/service.hpp"
#include "rclcpp/client.hpp"
#include "rclcpp_action/server.hpp"
#include "rclcpp_action/client.hpp"
#include "rclcpp/polling_action_server.hpp"
#include "rclcpp/polling_action_client.hpp"
#include "rclcpp/polling_client.hpp"
#include "rclcpp/polling_service.hpp"
#include "rclcpp/polling_subscription.hpp"
#include "rclcpp/parameter.hpp"
// phase-426 W4 — the ONE parameter facade. Forwards a node's
// declare/get/set/has onto the executor's store across the FFI, so a parameter
// means the same thing wherever it is declared. There is now one node type to
// wear it (phase-427 W4), which is the other half of that guarantee.
// Freestanding: `<string>`/`<vector>` behind `NROS_CPP_STD`.
#include "rclcpp/node_parameters.hpp"
#include "rclcpp/tick_ctx.hpp"
#include "rclcpp_lifecycle/lifecycle_node.hpp"
#include "rclcpp/rosout.hpp" // issue 1589 — `/rosout` from C++
// phase-427 W4 — `nros/component_node.hpp` IS GONE. `nros::ComponentNode` was a
// type that WRAPPED a node (RFC-0044 Q1's "wrap, not derive"), so a component
// was not a `Node` and every verb had to be forwarded. Its members are on
// `rclcpp::Node` now, its pool is the opt-in `rclcpp::NodeWithTimers<N>`, and its
// macros are in `component.hpp` below. The rclcpp-shaped, value-returning
// parameter facade phase-417 W2.b pulled in here for is `Node`'s own.
// phase-417 stage 6 step A — `rclcpp::create_subscription_raw`, the
// arena-registration entry point `rclcpp::Node::create_subscription`
// below uses so it has no dispatch of its own.
#include "rclcpp/detail/component.hpp"

namespace rclcpp {

/// Get the global executor handle for Future::wait().
///
/// Returns the raw storage pointer used by the global `init()`/`spin_once()`
/// free functions. Use with `Future::wait(rclcpp::global_handle(), ...)`.
///
/// @return Executor handle, or nullptr if not initialized.
///
/// DEFINED in `<rclcpp/node.hpp>`, beside the free functions `Node` befriends:
/// `rclcpp::init_in(argc, argv)` (node.hpp) calls it, and an inline function used
/// in a TU that never sees its definition is a link error waiting for the
/// first caller (issue 1682's branch, found by `-Wall`).
inline void* global_handle();

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

/// phase-474 I2 -- the application's start-up is over: arm the contract
/// monitors of every executor that waits for this
/// (`CONFIG_NROS_MONITOR_ARM_ON_CALL`). Call it where the application enters
/// its running state, e.g. an INIT/RUN state machine's RUN transition.
///
/// Safe from any thread and from inside a callback (it bumps one atomic; each
/// executor arms at its next spin). Before arming a verdict is counted
/// (`suppressed_before_arm`) but not stored, logged or traced. Does nothing on
/// an image whose monitors arm at the first spin, which is the default.
inline void arm_monitors() {
    nros_cpp_monitors_arm();
}

/// Register a callback to run BEFORE the global session's entities are torn
/// down — issue 0790.
///
/// rclcpp hangs the shutdown hooks on `Context`, which nano-ros does not have
/// (phase-379's init stage records the collapse into one support object), so
/// they live on the executor — here, the global one `rclcpp::init()` opened and
/// `rclcpp::shutdown()` closes.
///
/// This is the phase with no workaround: the callback runs while every entity
/// still works, so a node can publish a final state, answer a last request,
/// park an actuator or release a bus. `rclcpp::on_shutdown` below runs after
/// teardown, when none of that is possible any more.
///
/// A CLEAN-STOP facility: a watchdog reset, a hard fault or an abort does not
/// come through `rclcpp::shutdown()`, so it does not come through here either.
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

/// `rclcpp::Context` (phase-488) — a VIEW of one executor's lifetime: its
/// pre- and on-shutdown hooks (issue 0790) and whether it is still running.
///
/// Upstream's context owns the rcl init state and is reached through a
/// `shared_ptr`. nano-ros's executor owns that state, so this names one by its
/// handle: `SharedPtr` is the view itself and `->` reaches it, so
/// `rclcpp::Context::SharedPtr c = node->get_node_base_interface()->get_context();
/// c->add_pre_shutdown_callback(cb)` spells as upstream's does. The hooks share
/// the executor's fixed table (`NROS_EXECUTOR_MAX_SHUTDOWN_CBS`, default 2): a
/// full table returns an INVALID handle on the std surface, where upstream
/// would allocate.
class Context {
  public:
    using SharedPtr = Context;
    Context() : executor_(nullptr) {}
    explicit Context(void* executor) : executor_(executor) {}
    Context* operator->() { return this; }
    const Context* operator->() const { return this; }
    /// The executor handle this context names; null for an empty one.
    void* executor_handle() const { return executor_; }
    /// `Context::is_valid()` — named, and not yet shut down.
    bool is_valid() const { return executor_ != nullptr && ::rclcpp::ok(); }

    /// The C-callback forms, on every target. See `rclcpp::pre_shutdown`.
    Result add_pre_shutdown_callback(ShutdownCallback callback, void* context,
                                     PreShutdownCallbackHandle* out = nullptr) {
        if (executor_ == nullptr) return Result(ErrorCode::NotInitialized);
        nros_cpp_shutdown_callback_handle_t raw = NROS_CPP_SHUTDOWN_CALLBACK_HANDLE_INVALID;
        nros_cpp_ret_t ret = nros_cpp_add_pre_shutdown_callback(executor_, callback, context, &raw);
        if (out != nullptr) *out = PreShutdownCallbackHandle(raw);
        return Result(ret);
    }
    /// @see add_pre_shutdown_callback
    Result add_on_shutdown_callback(ShutdownCallback callback, void* context,
                                    OnShutdownCallbackHandle* out = nullptr) {
        if (executor_ == nullptr) return Result(ErrorCode::NotInitialized);
        nros_cpp_shutdown_callback_handle_t raw = NROS_CPP_SHUTDOWN_CALLBACK_HANDLE_INVALID;
        nros_cpp_ret_t ret = nros_cpp_add_on_shutdown_callback(executor_, callback, context, &raw);
        if (out != nullptr) *out = OnShutdownCallbackHandle(raw);
        return Result(ret);
    }
    /// `true` when `handle` named a live callback.
    bool remove_pre_shutdown_callback(const PreShutdownCallbackHandle& handle) {
        if (executor_ == nullptr) return false;
        const bool removed = nros_cpp_remove_pre_shutdown_callback(executor_, handle.value()) == 0;
#if defined(NROS_CPP_STD) // hosted-family: shutdown-interop
        if (removed) release_closure(handle.value());
#endif
        return removed;
    }
    /// @see remove_pre_shutdown_callback
    bool remove_on_shutdown_callback(const OnShutdownCallbackHandle& handle) {
        if (executor_ == nullptr) return false;
        const bool removed = nros_cpp_remove_on_shutdown_callback(executor_, handle.value()) == 0;
#if defined(NROS_CPP_STD) // hosted-family: shutdown-interop
        if (removed) release_closure(handle.value());
#endif
        return removed;
    }

#if defined(NROS_CPP_STD) // hosted-family: shutdown-interop
    /// Upstream's `add_pre_shutdown_callback(std::function<void()>)`. The
    /// closure is held on the heap until the hook is removed.
    PreShutdownCallbackHandle add_pre_shutdown_callback(::std::function<void()> callback) {
        PreShutdownCallbackHandle h(NROS_CPP_SHUTDOWN_CALLBACK_HANDLE_INVALID);
        auto* fn = new ::std::function<void()>(static_cast<::std::function<void()>&&>(callback));
        if (!add_pre_shutdown_callback(&Context::run_closure, fn, &h).ok()) {
            delete fn;
            return PreShutdownCallbackHandle(NROS_CPP_SHUTDOWN_CALLBACK_HANDLE_INVALID);
        }
        closures()[h.value()].reset(fn);
        return h;
    }
    /// Upstream's `add_on_shutdown_callback(std::function<void()>)`.
    OnShutdownCallbackHandle add_on_shutdown_callback(::std::function<void()> callback) {
        OnShutdownCallbackHandle h(NROS_CPP_SHUTDOWN_CALLBACK_HANDLE_INVALID);
        auto* fn = new ::std::function<void()>(static_cast<::std::function<void()>&&>(callback));
        if (!add_on_shutdown_callback(&Context::run_closure, fn, &h).ok()) {
            delete fn;
            return OnShutdownCallbackHandle(NROS_CPP_SHUTDOWN_CALLBACK_HANDLE_INVALID);
        }
        closures()[h.value()].reset(fn);
        return h;
    }

  private:
    static void run_closure(void* fn) {
        if (fn != nullptr) (*static_cast<::std::function<void()>*>(fn))();
    }
    using ClosureMap =
        ::std::map<nros_cpp_shutdown_callback_handle_t, ::std::unique_ptr<::std::function<void()>>>;
    static ClosureMap& closures() {
        static ClosureMap m;
        return m;
    }
    static void release_closure(nros_cpp_shutdown_callback_handle_t h) { closures().erase(h); }
#else
  private:
#endif
    void* executor_;
};

namespace contexts {
/// `rclcpp::contexts::get_global_default_context()` — the context of the
/// executor `rclcpp::init` created.
inline Context get_global_default_context() {
    return Context(global_handle());
}
} // namespace contexts

namespace node_interfaces {
template <int Tag> inline Context NodeInterfaceHandle<Tag>::get_context() const {
    return Context(node_ != nullptr ? node_->executor_handle() : nullptr);
}
} // namespace node_interfaces

/// Phase 123.B.2 — block until `rclcpp::ok()` returns false.
///
/// Mirror of `rclcpp::spin(node)`. The typical pattern in user
/// code is: install a SIGINT handler that calls `rclcpp::shutdown()`
/// (which flips `ok()` to false), then `rclcpp::spin()` from `main`.
///
/// Returns the first non-success `spin_once` result, or
/// `Result::success()` after a clean shutdown.
inline Result spin_in() {
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
inline Result spin_in(uint32_t duration_ms, int32_t poll_ms = 10) {
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

} // namespace rclcpp

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
// construction rather than having to be argued away. Step B — retiring the
// in-tree `nros::` spelling — landed in phase-483 W1: `rclcpp::` is now the only
// namespace.
//
// They land HERE, in the umbrella, and not in `node.hpp`, because that is where
// their dependencies are. `rclcpp::Node` reaches `rclcpp::create_subscription_raw`
// (`component.hpp`), `rclcpp::Timer`, `rclcpp::Service` and `rclcpp::Client`,
// every one of which includes `node.hpp` itself; `Rate::sleep` and
// `spin_until_future_complete` drive the `rclcpp::spin` / `rclcpp::spin_once`
// free functions defined a few lines above. This IS the header they were
// shimming: the process-level verbs — `rclcpp::init`, `rclcpp::spin`,
// `rclcpp::spin_once`, `rclcpp::on_shutdown` — are all reached through
// `<rclcpp/rclcpp.hpp>`.
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
// `rclcpp::init()` / `rclcpp::shutdown()` / `rclcpp::ok()` are the
// process-level handshake (nros tracks the shutdown flag).
//
// The TWO-ARGUMENT form is REFUSE-LOUD (RFC-0089 W3.b). It used to forward to
// the one-argument `init()` and discard `argv`, so `--ros-args -r
// chatter:=/other` — the single most common thing a ported `main` passes —
// silently became a wrong-topic bug at runtime. Honouring it is remap
// resolution, RFC-0020 violation class 4, and belongs beside
// `rclcpp::resolve_name` rather than here; until it lands, the honest answer is a
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
/// The `create_*` verbs on `rclcpp::Node` (then `nros::Node`, reached as
/// `rclcpp::Node` through an alias since phase-427 W1-W3) used to write
/// `(void)this->create_…(…)` and return the `shared_ptr` regardless. The
/// `(void)` was not incidental: `rclcpp::Result` carries `[[nodiscard]]`
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
/// return `rclcpp::Result` into caller-owned storage, and the message names them.
[[noreturn]] inline void abort_failed_create(const char* verb, const char* name, int32_t code) {
    // issue 1303 — two records through `nros_log`, each short enough for its
    // format buffer: the call (the name bounded here), then the fixed text,
    // whose length `say_refused` checks at compile time. As one `NROS_ERROR`
    // record it was over 1180 bytes, which a real sink cannot carry: the body
    // is dropped and only an ellipsis is printed, so the process aborted
    // without saying why.
    RCLC_LOG_ERROR(log_handle(nullptr), "rclcpp::Node::%s(\"%.64s\") failed: rclcpp::ErrorCode %d",
                   verb, name != nullptr ? name : "", static_cast<int>(code));
    NROS_RCLCPP_SAY_REFUSED(NROS_RCLCPP_ABORT_FAILED_CREATE);
    ::std::abort();
}

/// The one spelling every `create_*` verb uses. Takes the `Result` by value so
/// the `[[nodiscard]]` is consumed HERE and cannot be silently dropped again.
inline void require_created(::rclcpp::Result r, const char* verb, const char* name) {
    if (!r.ok()) {
        abort_failed_create(verb, name, r.raw());
    }
}
} // namespace detail

/// `rclcpp::init(argc, argv)` — ADOPT-BOUNDED: `--ros-args -r` is honoured,
/// every other ROS argument ABORTS the process naming it.
///
/// `argv` is parsed with rcl's grammar by the same code as Rust's
/// `nros::Context::new` (`nros_node::ros_args`, reached through
/// `nros_cpp_install_argv_remaps`):
///
/// * `-r` / `--remap` `[node:]from:=to` inside `--ros-args ... --` — HONOURED,
///   as the FALLBACK beneath any remap the launch file projected for the same
///   name (the generated entry's `nros_cpp_declare_remap` calls). That is rcl's
///   own local-before-global order (`rcl_remap_topic_name`).
/// * `-p` / `--param` / `--params-file`, `__node` / `__name` / `__ns`,
///   `-e` / `--enclave`, the log flags, any unknown token — REFUSED. Runtime
///   parameters belong to RFC-0015 section 9's channel; node identity is
///   launch-authoritative (RFC-0046). rclcpp throws `UnknownROSArgsError` for
///   what rcl leaves unparsed, so refusing is the upstream behaviour too.
/// * no `--ros-args` → identical to `rclcpp::init()`.
///
/// **Why a refusal ABORTS rather than returns.** Upstream's `init` returns
/// `void` and a ported `main` does not check it, so this call site has nowhere
/// to put a failure, and continuing is the one outcome RFC-0089 forbids: a
/// `-p` that compiles and is silently dropped. The vector is validated BEFORE
/// the session opens. `rclcpp::init_in(argc, argv)` honours the
/// same arguments and RETURNS the refusal as a `Result`, for a caller that
/// wants to handle it.
inline void init(int argc, char const* const* argv) {
    // Issue 2073 -- the image's baked `ros_args` / `params_files` apply too,
    // with or without a `--ros-args` on the command line.
    const bool any = detail::argv_has_ros_args(argc, argv) || ::nros_cpp_baked_ros_args_present();
    if (any && ::rclcpp::detail::apply_ros_args(nullptr, argc, argv) != 0) {
        ::std::abort();
    }
    (void)::rclcpp::init_in();
    // An init that failed leaves no executor to install into; nothing here is
    // dropped that the failed init did not already drop with the session.
    void* const executor = ::rclcpp::global_handle();
    if (any && executor != nullptr && ::rclcpp::detail::apply_ros_args(executor, argc, argv) != 0) {
        ::std::abort();
    }
}

/// `rclcpp::init()` — the zero-argument form. ADOPT.
inline void init() {
    (void)::rclcpp::init_in();
}
/// `rclcpp::shutdown()` — ADOPT-BOUNDED. Upstream's channel here is `bool`, so
/// ours is too (RFC-0089's error-channel rule: a ported API keeps upstream's
/// channel even when that is `bool`).
///
/// It used to discard `rclcpp::shutdown()`'s `Result` and answer `true`
/// unconditionally (phase-428 W5 finding 9, one site over from the seven
/// `create_*` verbs and the only one the compiler already pointed at: `just
/// check cpp` carried a standing `-Wunused-result`). `NROS_NODISCARD` on
/// `Result` is what pointed at it (phase-427 W8) — a fini that failed reported
/// success, which is the exact silence the attribute exists to break.
///
/// The bound: upstream returns `false` when the context was never initialised;
/// `rclcpp::shutdown()` answers `success()` for that case (it is a no-op), so we
/// return `true` there. `false` here means the teardown itself failed.
inline bool shutdown() {
    return ::rclcpp::shutdown_in().ok();
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
using ::rclcpp::spin_once;

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
    /// `::rclcpp::ok()` went false (shutdown) before the future was ready.
    INTERRUPTED,
};

} // namespace rclcpp

// -- the ported publisher line, UNGATED — phase-456 W5 ------------------------
//
// `auto pub = node->create_publisher<M>("chatter", 10);` compiles on every
// target now, including `-ffreestanding -nostdinc++` against Zephyr's and
// ThreadX's minimal libcpp. It returns `Publisher<M>::SharedPtr`, which is
// `rclcpp::Owned<Publisher<M>>` — the publisher by value, `operator->` for
// `pub->publish(m)`, no allocator, no control block, no `<memory>`.
//
// WHAT THE NODE STOPPED DOING, stated because it is a behaviour change.
// The hosted body used to `make_shared` the publisher and push a second
// reference into `hosted().owned_entities`, so the node co-owned it and a
// ported body could drop the returned pointer mid-expression
// (`node->create_publisher<M>(…)->publish(m);`) and still have a live
// publisher afterwards. `Owned<T>` is SOLE ownership, so there is no second
// reference to keep: the entity is destroyed when the holder is. For the
// expression above that is still correct — the temporary outlives the
// full-expression — and for the member pattern the corpus actually uses
// (`publisher_ = this->create_publisher<M>(…)`) it is identical to upstream,
// where the node is the last owner. What is gone is a publisher that survives
// its own handle, which upstream does not give either once the node drops it.
//
// phase-456 W4 measured that the node was never an ARENA owner here: nothing
// registers a publisher and `EntryKind` has no publisher kind, so the retention
// was retention and nothing more. The comment that used to sit on it claimed
// the arena stored `&entity`, which was false in both halves and is the kind of
// copied rationale that teaches the next reader the wrong model.
namespace rclcpp {
template <typename M>
inline typename Publisher<M>::SharedPtr Node::create_publisher(const char* topic,
                                                               const ::rclcpp::QoS& qos) {
    Publisher<M> p;
    ::rclcpp::detail::require_created(this->create_publisher<M>(p, topic, qos), "create_publisher",
                                      topic);
    return typename Publisher<M>::SharedPtr(::rclcpp::tr::forward_rvalue(p));
}

template <typename M>
inline typename Publisher<M>::SharedPtr Node::create_publisher(const char* topic, ::size_t depth) {
    return this->create_publisher<M>(topic, ::rclcpp::QoS(static_cast<uint32_t>(depth)));
}
} // namespace rclcpp

// The `std::string`-keyed forwarders, then the `Node::SharedPtr`-taking
// verbs. phase-476 W3 split what was one block behind all four capability
// probes: each half now names the one capability its signatures use. Every
// verb here forwards to (or refuses in place of) a freestanding form defined
// after these blocks.
#if defined(NROS_CPP_HAS_STD_STRING) // hosted-family: string-interop

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
// phase-427 W1-W3 — THE NODE TYPES ARE MERGED. `rclcpp::Node` stopped being a
// separate adapter class here (it became an alias for `nros::Node`, and since
// phase-483 W1 it IS the one class), and the hosted call shape that adapter
// carried is now a set of overloads on that one type (declared in `node.hpp`,
// defined below).
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
//     entities and no out-ref creators; a native file wrote `nros::Node` and
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

namespace rclcpp {

// The HOSTED half of `rclcpp::Node`, defined here rather than in `node.hpp`
// because every body below needs a complete entity type (`Publisher<M>`,
// `Subscription<M>`, `Service<S>`, `Client<S>`) or a helper the umbrella pulls
// in (`create_subscription_raw` from `component.hpp`, the callback cells from
// `subscription.hpp` / `timer.hpp`). That is the same split Phase 84.G8 already
// uses for the out-ref `Node::create_X<>` templates, which live in the header
// that owns each entity.
//
// The declarations — with the default arguments — are in `node.hpp` under
// `NROS_CPP_HAS_STD_STRING`, the one predicate both files share.

// -- publishers ---------------------------------------------------------------

} // namespace rclcpp

namespace rclcpp {
template <typename M>
inline typename Publisher<M>::SharedPtr Node::create_publisher(const ::std::string& topic,
                                                               const ::rclcpp::QoS& qos) {
    return this->create_publisher<M>(topic.c_str(), qos);
}
} // namespace rclcpp

namespace rclcpp {} // namespace rclcpp

namespace rclcpp {
template <typename M>
inline typename Publisher<M>::SharedPtr Node::create_publisher(const ::std::string& topic,
                                                               ::size_t depth) {
    return this->create_publisher<M>(topic.c_str(), ::rclcpp::QoS(static_cast<uint32_t>(depth)));
}
} // namespace rclcpp

namespace rclcpp {

// -- subscriptions ------------------------------------------------------------
//
// phase-417 — ONE DISPATCH PATH. This ARENA-REGISTERS the subscription through
// `rclcpp::create_subscription_raw` (`component.hpp`), the same
// `nros_cpp_subscription_register` call the native callback-style
// `create_subscription` makes, so the executor owns the subscriber and
// dispatches the callback during `spin_once` — whichever spin verb the caller
// drives. It used to create a POLL-mode subscription and drain it from a
// node-local `pump()`, which only `rclcpp::spin` / `spin_some` called: a file
// that spun `rclcpp::spin_once()` instead got zero callbacks and no diagnostic.
//
// Why not the out-ref `create_subscription(sub, topic, qos, cb)` overload: that
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
// WHAT THE RETURNED HANDLE IS: a keep-alive, which is all upstream source does
// with it (`rclcpp::Subscription<M>::SharedPtr sub_;`). The executor owns the
// real subscriber. This comment used to say `sub->take(msg)` on it "answers
// `NotInitialized`"; it did not — `initialized_` was true and the call reached
// `&mut *(storage as *mut RmwSubscriber)` over bytes the arena never filled.
// phase-456 W2 made the handle a type with no `operator->`, and W2b moved the
// taking API to `rclcpp::PollSubscription<M>`, so neither half of that sentence
// has a subject any more.
} // namespace rclcpp

namespace rclcpp {
template <typename M, typename Cb>
inline typename Subscription<M>::SharedPtr
Node::create_subscription(const ::std::string& topic, const ::rclcpp::QoS& qos, Cb cb) {
    // phase-476 W1 — forwards to the freestanding `const char*` overload.
    return this->create_subscription<M>(topic.c_str(), qos, ::rclcpp::tr::forward_rvalue(cb));
}
} // namespace rclcpp

namespace rclcpp {} // namespace rclcpp

namespace rclcpp {
template <typename M, typename Cb>
inline typename Subscription<M>::SharedPtr Node::create_subscription(const ::std::string& topic,
                                                                     ::size_t depth, Cb cb) {
    return this->create_subscription<M>(topic.c_str(), depth, ::rclcpp::tr::forward_rvalue(cb));
}
} // namespace rclcpp

namespace rclcpp {

// -- wall timer ---------------------------------------------------------------

// phase-476 W2 — `create_wall_timer` is defined after the hosted block now
// (`-- timers --`): it returns a `TimerHandle` and allocates nothing, so it is
// not a hosted capability.

// -- parameters ---------------------------------------------------------------
//
// MOVED OUT OF THIS BLOCK (phase-426 W4). The definitions are below, after
// `#endif // NROS_CPP_HAS_SHARED_PTR && ...`, because a node's parameters are
// not a hosted capability: the scalar FFI is `<cstdint>` and a `const char*`,
// and `rclcpp::Seq<T, N>` carries arrays with no STL either. Only the
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

} // namespace rclcpp

namespace rclcpp {
template <typename S>
inline ::rclcpp::Owned<::rclcpp::PollService<S>> Node::create_service(const ::std::string& name,
                                                                      const ::rclcpp::QoS& qos) {
    // phase-476 W1 — forwards to the freestanding `const char*` overload.
    return this->template create_service<S>(name.c_str(), qos);
}
} // namespace rclcpp

namespace rclcpp {} // namespace rclcpp

namespace rclcpp {
template <typename S, typename F, typename>
inline typename Service<S>::SharedPtr Node::create_service(const ::std::string& name, F callback,
                                                           const ::rclcpp::QoS& qos) {
    return this->template create_service<S>(name.c_str(), callback, qos);
}
} // namespace rclcpp

namespace rclcpp {} // namespace rclcpp

namespace rclcpp {
template <typename S, typename F, typename, typename>
inline typename Service<S>::SharedPtr Node::create_service(const ::std::string& name, F callback,
                                                           const ::rclcpp::QoS& qos) {
    // phase-488 — the `const char*` overload decides: it takes upstream's
    // shared_ptr handler and refuses the rest.
    return this->template create_service<S>(name.c_str(), callback, qos);
}
} // namespace rclcpp

namespace rclcpp {} // namespace rclcpp

namespace rclcpp {
template <typename S>
inline ::rclcpp::Owned<::rclcpp::PollClient<S>> Node::create_client(const ::std::string& name,
                                                                    const ::rclcpp::QoS& qos) {
    // phase-476 W1 — forwards to the freestanding `const char*` overload.
    return this->template create_client<S>(name.c_str(), qos);
}
} // namespace rclcpp

namespace rclcpp {} // namespace rclcpp

namespace rclcpp {
template <typename S, typename F, typename>
inline typename Client<S>::SharedPtr Node::create_client(const ::std::string& name, F callback,
                                                         const ::rclcpp::QoS& qos) {
    return this->template create_client<S>(name.c_str(), callback, qos);
}
} // namespace rclcpp

namespace rclcpp {} // namespace rclcpp

namespace rclcpp {
template <typename S, typename F, typename, typename>
inline typename Client<S>::SharedPtr Node::create_client(const ::std::string&, F,
                                                         const ::rclcpp::QoS&) {
    static_assert(::rclcpp::detail::refuse<F>::value,
                  NROS_RCLCPP_REFUSE_SHARED_PTR_SERVICE_CALLBACK);
    return typename Client<S>::SharedPtr();
}
} // namespace rclcpp

namespace rclcpp {} // namespace rclcpp

#endif // NROS_CPP_HAS_STD_STRING

#if defined(NROS_CPP_HAS_SHARED_PTR) // hosted-family: shared-ptr-interop
namespace rclcpp {

// `rclcpp::Node` is defined in `node.hpp`, UNCONDITIONALLY, not here, because
// a freestanding target gets the class and must get its ROS 2 spelling with it.
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
// The clock is `rclcpp::Clock*`, which is exactly what `node->get_clock()` hands
// back, so the ported line binds with no conversion. Upstream's parameter is
// `rclcpp::Clock::SharedPtr`; there is no allocator here (RFC-0022) and the
// clock is a member of the node, so the pointer is the counterpart — the same
// ADOPT-BOUNDED trade `get_clock()` itself makes.
//
// WHAT THE CLOCK DOES, and it is the whole point of the verb: a
// `RCL_ROS_TIME` clock follows `/clock`, so the timer stops when a bag
// stops and re-times when the bag is replayed at another rate; a
// `RCL_STEADY_TIME` one does not. `create_wall_timer` is the steady
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

/// The `shared_ptr` member of `as_node_ref`'s overload set; the reference and
/// pointer ones are freestanding and live with `rclcpp::create_timer`, after
/// this block (phase-476 W2). This one needs `<memory>`.
/// A template (phase-482 W2) so a `shared_ptr<MyNode>` binds here exactly, not
/// through a conversion that would tie with the `Node::SharedPtr` overload.
template <typename N, typename ::rclcpp::tr::enable_if<
                          ::rclcpp::tr::is_convertible<N*, ::rclcpp::Node*>::value, int>::type = 0>
inline ::rclcpp::Node& as_node_ref(const ::std::shared_ptr<N>& n) {
    return *n;
}

} // namespace detail

// phase-476 W2 — `rclcpp::create_timer` moved after this block (`-- timers --`).

// --- spin / spin_some --------------------------------------------------------
//
// phase-417 — FORWARDERS, and nothing else. `rclcpp::spin(node)` is
// `rclcpp::spin()` (block until `ok()` goes false) and `rclcpp::spin_some(node)`
// is the process executor's `spin_some` drain (phase-488; it was one 0-timeout
// `rclcpp::spin_once`). Both used to call a node-local `pump()`
// first, which was the ONLY thing that ran a ported node's timers and
// subscriptions; now those live on the executor, so these two dispatch nothing
// of their own and a file that reaches for `rclcpp::spin_once()` or drives an
// `rclcpp::Executor` gets exactly the same callbacks. That equivalence is the
// structural prerequisite for step B: after it both node types share a name,
// and a mismatch would be invisible.
//
// One behaviour change for an existing caller: `rclcpp::spin()` RETURNS on the
// first failing `spin_once`, where this loop used to discard the result and
// keep going. A dead session now ends the spin instead of looking alive
// forever, which is what `rclcpp::spin()` and `Executor::spin` already promise.
//
// The `node` argument is still checked, and still otherwise unused: there is
// one session per image (issue 0465), so every `rclcpp::Node` is on the global
// executor these verbs drive. Upstream takes the node for the same reason and
// spins the executor it belongs to.

} // namespace rclcpp

#endif // NROS_CPP_HAS_SHARED_PTR

// --- spin verbs, freestanding (phase-482 W2) ----------------------------------
//
// They take `Node::SharedPtr`, which is `rclcpp::Handle<Node>` on every target
// since phase-482 W2, so they left the `shared_ptr` block: a freestanding port's
// `main` spells `rclcpp::spin(node)` exactly as a hosted one does.
namespace rclcpp {

inline void spin(const Node::SharedPtr& node) {
    if (!node || !node->initialized()) {
        return;
    }
    (void)::rclcpp::spin_in();
}

/// `rclcpp::spin(node->get_node_base_interface())` (phase-488; Nav2's
/// `main`s write it) — the node the handle names.
inline void spin(const node_interfaces::NodeBaseInterface& base) {
    spin(Node::SharedPtr(base.node()));
}

inline void spin_some(const Node::SharedPtr& node) {
    if (!node || !node->initialized()) {
        return;
    }
    // phase-488 — the process executor's `spin_some`, the same drain as
    // `Executor::spin_some` and Rust's (`nros_cpp_spin_some`). It was one
    // `spin_once(0)` pass, which leaves work a callback made ready for the
    // next call.
    void* const executor = ::rclcpp::global_handle();
    if (executor == nullptr) return;
    (void)nros_cpp_spin_some(executor, 0);
}

/// `rclcpp::spin_some(node->get_node_base_interface())`; see `spin`.
inline void spin_some(const node_interfaces::NodeBaseInterface& base) {
    spin_some(Node::SharedPtr(base.node()));
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
/// `rclcpp::spin_until_future_complete(node, future[, timeout])`.
///
/// ADOPT-BOUNDED: the timeout is `int32_t` MILLISECONDS (`-1` = unbounded, the
/// default), not a `std::chrono::duration`, so a ported `1s` argument does not
/// compile. It spins the PROCESS executor in 10 ms `spin_once` slices — there is
/// one executor per task (RFC-0002) and no executor argument — so the deadline
/// is quantised to 10 ms. Returns upstream's `FutureReturnCode`.
///
/// phase-488 — the future is taken by FORWARDING reference. It was `const
/// Future&`, which no `rclcpp::Future` could bind: its `is_ready()` caches the
/// reply and is non-const, so `spin_until_future_complete(node,
/// client->send_request(req))` — the pairing the poll client's own doc
/// recommends — did not compile, and the only thing the drift snippet ever
/// passed was a fake with a `const` `is_ready()`. Both now bind.
namespace detail {
/// Issue 2027 — "has this future completed?" for each future shape
/// `spin_until_future_complete` takes: anything with `is_ready()`
/// (`rclcpp::Future`, `rclcpp_action::detail::DrivenSharedFuture`, which also
/// drives its client) and, hosted, a bare `std::shared_future` / `std::future`
/// (a zero-length `wait_for`).
template <typename F> inline bool future_is_ready(F& f) {
    return f.is_ready();
}
#if defined(NROS_CPP_HAS_STD_FUTURE) // hosted-family: future-interop
template <typename T> inline bool future_is_ready(const ::std::shared_future<T>& f) {
    return f.valid() && f.wait_for(::std::chrono::seconds(0)) == ::std::future_status::ready;
}
template <typename T> inline bool future_is_ready(::std::shared_future<T>& f) {
    return f.valid() && f.wait_for(::std::chrono::seconds(0)) == ::std::future_status::ready;
}
template <typename T> inline bool future_is_ready(::std::future<T>& f) {
    return f.valid() && f.wait_for(::std::chrono::seconds(0)) == ::std::future_status::ready;
}
#endif
} // namespace detail

template <typename Future>
inline FutureReturnCode spin_until_future_complete(const Node::SharedPtr& node, Future&& future,
                                                   int32_t timeout_ms = -1) {
    if (!node || !node->initialized()) {
        return FutureReturnCode::INTERRUPTED;
    }
    // Poll slice: same 10 ms the unbounded loop always used.
    constexpr int32_t kPollMs = 10;
    const bool bounded = timeout_ms >= 0;
    const uint64_t start_ns = nros_cpp_time_ns();
    const uint64_t budget_ns = bounded ? static_cast<uint64_t>(timeout_ms) * 1000000ull : 0ull;

    while (::rclcpp::ok()) {
        if (::rclcpp::detail::future_is_ready(future)) {
            return FutureReturnCode::SUCCESS;
        }
        (void)::rclcpp::spin_once(kPollMs);
        // Re-check before the deadline test: a future that became ready on the
        // spin just above must report SUCCESS even if the budget expired in
        // the same slice.
        if (::rclcpp::detail::future_is_ready(future)) {
            return FutureReturnCode::SUCCESS;
        }
        if (bounded && nros_cpp_time_ns() - start_ns >= budget_ns) {
            return FutureReturnCode::TIMEOUT;
        }
    }
    return FutureReturnCode::INTERRUPTED;
}

namespace detail {
/// phase-515 P1 — `spin_until_future_complete` on ONE executor handle: the
/// free function's loop (10 ms `spin_once` slices, the future re-checked
/// after each, a deadline only when `timeout_ms >= 0`), shared by
/// `rclcpp::Executor` and `executors::SingleThreadedExecutor`. `process` says
/// which "still running?" applies: the process executor's `rclcpp::ok()`, or,
/// for an explicit executor that `rclcpp::init` never saw, only the
/// termination request (SIGINT) every spin honours.
template <typename Future>
inline FutureReturnCode spin_executor_until_future_complete(void* executor, Future& future,
                                                            int32_t timeout_ms, bool process) {
    if (executor == nullptr) return FutureReturnCode::INTERRUPTED;
    constexpr int32_t kPollMs = 10;
    const bool bounded = timeout_ms >= 0;
    const uint64_t start_ns = nros_cpp_time_ns();
    const uint64_t budget_ns = bounded ? static_cast<uint64_t>(timeout_ms) * 1000000ull : 0ull;
    while (process ? ::rclcpp::ok() : !::nros_cpp_termination_requested()) {
        if (::rclcpp::detail::future_is_ready(future)) return FutureReturnCode::SUCCESS;
        (void)nros_cpp_spin_once(executor, kPollMs);
        if (::rclcpp::detail::future_is_ready(future)) return FutureReturnCode::SUCCESS;
        if (bounded && nros_cpp_time_ns() - start_ns >= budget_ns) return FutureReturnCode::TIMEOUT;
    }
    return FutureReturnCode::INTERRUPTED;
}

/// phase-515 P2 — the `add_node` check, against one executor handle.
template <typename NodePtr>
inline void check_node_on_executor(const NodePtr& node, void* executor, bool process) {
    const bool live = node && node->initialized();
    const char* name = live ? node->get_name() : "";
    if (!live) {
        ::rclcpp::detail::abort_failed_create("add_node", name,
                                              static_cast<int32_t>(ErrorCode::NotInitialized));
    }
    if (node->executor_handle() != executor) {
        // `say_refused` takes a LITERAL (its length is checked against the
        // log buffer), so the two refusals are spelled out, not passed in.
        if (process) {
            NROS_RCLCPP_SAY_REFUSED(NROS_RCLCPP_REFUSE_FOREIGN_EXECUTOR_NODE);
        } else {
            NROS_RCLCPP_SAY_REFUSED(NROS_RCLCPP_REFUSE_FOREIGN_NODE);
        }
        ::rclcpp::detail::abort_failed_create("add_node", name,
                                              static_cast<int32_t>(ErrorCode::Unsupported));
    }
}
} // namespace detail

template <typename Future>
inline FutureReturnCode Executor::spin_until_future_complete(Future&& future, int32_t timeout_ms) {
    if (!initialized_) return FutureReturnCode::INTERRUPTED;
    return ::rclcpp::detail::spin_executor_until_future_complete(storage_, future, timeout_ms,
                                                                 false);
}

template <typename NodePtr> inline void Executor::add_node(const NodePtr& node, bool notify) {
    (void)notify;
    ::rclcpp::detail::check_node_on_executor(node, static_cast<void*>(storage_), false);
}

// --- rclcpp::executors (phase-488) --------------------------------------------
namespace executors {

/// `rclcpp::executors::SingleThreadedExecutor` — phase-488. ADOPT-BOUNDED.
///
/// A FACADE over the process executor, which is what every `rclcpp::Node`
/// already lives on: there is one session per image (issue 0465) and a node's
/// entities are created in that executor's arena when the node is constructed
/// (RFC-0002). So the three lines a stock `main` writes keep their meaning —
///
///     rclcpp::executors::SingleThreadedExecutor executor;
///     executor.add_node(node);
///     executor.spin();
///
/// — with `add_node` CHECKING the attachment that construction already made
/// rather than making one. The bound, stated: there is no second executor to
/// build, so two of these are the same executor; `add_node` aborts, naming the
/// node, when the node failed to create or was created on an explicit
/// `rclcpp::Executor` instead (spinning this would never reach it); there is
/// no `remove_node`, because arena registration is one-way (the
/// `cpp:Executor::add_node` row). `spin` / `spin_some` are `rclcpp::spin_in()` /
/// the process executor's `spin_some`.
class SingleThreadedExecutor {
  public:
    SingleThreadedExecutor() {}

    /// Check that `node` is on the process executor. `notify` is accepted and
    /// ignored, as there is no waiting executor to wake.
    void add_node(const ::rclcpp::Node::SharedPtr& node, bool notify = true) {
        (void)notify;
        ::rclcpp::detail::check_node_on_executor(node, ::rclcpp::global_handle(), true);
    }

    /// **REFUSED** — `remove_node`; see `rclcpp::Executor::remove_node`.
    /// phase-515 P2.
    template <typename NodePtr, typename T = void>
    void remove_node(const NodePtr& node, bool notify = true) {
        (void)node;
        (void)notify;
        static_assert(::rclcpp::detail::refuse<T>::value, NROS_RCLCPP_REFUSE_REMOVE_NODE);
    }

    /// `spin_until_future_complete(future, timeout)` on the process executor —
    /// phase-515 P1, the envelope of `rclcpp::spin_until_future_complete`
    /// (`int32_t` milliseconds, `-1` = unbounded, 10 ms slices).
    template <typename Future>
    FutureReturnCode spin_until_future_complete(Future&& future, int32_t timeout_ms = -1) {
        return ::rclcpp::detail::spin_executor_until_future_complete(::rclcpp::global_handle(),
                                                                     future, timeout_ms, true);
    }

    /// `spin_all(max_duration)` — phase-515 P2; see `rclcpp::Executor::spin_all`.
    /// A zero bound is refused, as upstream throws for it.
    void spin_all(uint32_t max_duration_ms) {
        if (max_duration_ms == 0) {
            NROS_RCLCPP_SAY_REFUSED("spin_all(0) REFUSED: max_duration must be positive, as in "
                                    "rclcpp.");
            return;
        }
        spin_some(max_duration_ms);
    }

    /// `spin_node_some(node)` — phase-515 P2: `add_node`'s check, then
    /// `spin_some`, which drains every node on the process executor.
    void spin_node_some(const ::rclcpp::Node::SharedPtr& node) {
        add_node(node);
        spin_some();
    }

    /// `spin_node_once(node, timeout)` — phase-515 P2: `add_node`'s check, then
    /// one `spin_once` of the process executor (every node's ready work). The
    /// budget is required, as for `rclcpp::spin_once`.
    void spin_node_once(const ::rclcpp::Node::SharedPtr& node, int32_t timeout_ms) {
        add_node(node);
        (void)::rclcpp::spin_once(timeout_ms);
    }

    /// Block until `rclcpp::ok()` goes false — `rclcpp::spin_in()`.
    void spin() { (void)::rclcpp::spin_in(); }

    /// Drain what is ready, without waiting — `Executor::spin_some` on the
    /// process executor. `max_ms` is MILLISECONDS (`0` = no limit).
    void spin_some(uint32_t max_ms = 0) {
        void* const executor = ::rclcpp::global_handle();
        if (executor == nullptr) return;
        (void)nros_cpp_spin_some(executor, max_ms);
    }
};

/// The refusal behind `MultiThreadedExecutor`; see that alias.
template <typename T = void> class MultiThreadedExecutorRefused {
  public:
    template <typename... Args> explicit MultiThreadedExecutorRefused(Args&&...) {
        static_assert(::rclcpp::detail::refuse<T>::value,
                      NROS_RCLCPP_REFUSE_MULTI_THREADED_EXECUTOR);
    }
};

/// `rclcpp::executors::MultiThreadedExecutor` — phase-488. REFUSED.
///
/// The NAME exists so a ported `main` fails here, with the alternative, rather
/// than on an unknown identifier. Declaring one is the compile error. An
/// executor dispatches on ONE task with run-to-completion callbacks (RFC-0002),
/// and nano-ros starts no threads behind the caller's back, so a pool of
/// workers is not a shape it has. Parallelism is per TIER — one executor per
/// RTOS task, declared in the system description — not per callback group.
using MultiThreadedExecutor = MultiThreadedExecutorRefused<>;

} // namespace executors

} // namespace rclcpp

#if defined(NROS_CPP_HAS_SHARED_PTR) // hosted-family: shared-ptr-interop
namespace rclcpp {

// phase-482 W2 — the upstream `main` is `rclcpp::spin(std::make_shared<MyNode>())`,
// a TEMPORARY `shared_ptr`. Binding it to a function parameter is safe (it lives
// for the call), but converting it to a `Node::SharedPtr` handle is refused
// because a STORED handle would dangle. These overloads take the `shared_ptr` by
// reference instead, so that line compiles unchanged and owns the node for
// exactly as long as the spin runs.
template <typename N, typename ::rclcpp::tr::enable_if<
                          ::rclcpp::tr::is_convertible<N*, Node*>::value, int>::type = 0>
inline void spin(const ::std::shared_ptr<N>& node) {
    spin(Node::SharedPtr(node));
}

template <typename N, typename ::rclcpp::tr::enable_if<
                          ::rclcpp::tr::is_convertible<N*, Node*>::value, int>::type = 0>
inline void spin_some(const ::std::shared_ptr<N>& node) {
    spin_some(Node::SharedPtr(node));
}

template <
    typename N, typename Future,
    typename ::rclcpp::tr::enable_if<::rclcpp::tr::is_convertible<N*, Node*>::value, int>::type = 0>
inline FutureReturnCode spin_until_future_complete(const ::std::shared_ptr<N>& node,
                                                   Future&& future, int32_t timeout_ms = -1) {
    return spin_until_future_complete(Node::SharedPtr(node), future, timeout_ms);
}

} // namespace rclcpp
#endif // NROS_CPP_HAS_SHARED_PTR

// -- dispatch verbs, freestanding (phase-476 W1) -------------------------------
//
// The `const char*` overloads of `create_subscription` / `create_service` /
// `create_client`. Each result is a handle or an owned poll object, none of them
// allocates, and their bodies are the ones the hosted `std::string` overloads
// used to carry; those overloads now forward here.

namespace rclcpp {

namespace detail {

/// The one-argument callback path (`void(const M&)`, or a hosted pointer shape).
template <typename M, typename Cb>
inline typename Subscription<M>::SharedPtr
create_subscription_dispatch(::rclcpp::Node& node, const char* topic, const ::rclcpp::QoS& qos,
                             Cb cb, ::rclcpp::tr::false_type /* takes MessageInfo */) {
    // phase-456 W2 — the arena owns everything, so this allocates nothing.
    //
    // This used to `make_shared` a `detail::SubscriptionCallback<M>` cell whose
    // only job was to give a capturing lambda a stable address, push it into
    // `owned_entities` to keep it alive, and hand back a `shared_ptr` aliasing
    // into it. W1 put the capture in the arena entry, so the cell has nothing
    // left to hold; W2 stops pretending the result is a pointer to an object.
    //
    // issue 1995 — the callback's SHAPE decides what the arena holds: a
    // `const M&` callable as-is, or (hosted only) an adapter that hands a
    // pointer-shaped callback a heap copy of each message. Anything else is
    // refused here, naming the shapes, rather than as an `InplaceFn` conversion.
    constexpr int shape = ::rclcpp::detail::subscription_callback_shape<M, Cb>::value;
    static_assert(shape != ::rclcpp::detail::SUB_SHAPE_NONE || ::rclcpp::detail::refuse<Cb>::value,
                  NROS_RCLCPP_REFUSE_SUBSCRIPTION_CALLBACK_SHAPE);
    using Capture = ::rclcpp::InplaceFn<void(const M&)>;
    Capture captured(::rclcpp::detail::subscription_callback_adapter<M, Cb, shape>::wrap(
        ::rclcpp::tr::forward_rvalue(cb)));
    ::size_t handle_id = 0;
    const ::rclcpp::Result r = ::rclcpp::detail::register_subscription_capturing<M>(
        node, topic, qos, captured, &handle_id);
    // phase-476 W2 — the arena's copy owns the callable now and destroys it on
    // release; ours must not destroy it too.
    if (r.ok()) {
        captured.relinquish();
    }
    ::rclcpp::detail::require_created(r, "create_subscription", topic);
    return typename Subscription<M>::SharedPtr(node.executor_handle(), handle_id);
}

/// Issue 2036 — the two-argument path: the callback also takes
/// `const rclcpp::MessageInfo&`, so the runtime takes each sample with its
/// metadata. Hosted only (the callable lives on the heap).
template <typename M, typename Cb>
inline typename Subscription<M>::SharedPtr
create_subscription_dispatch(::rclcpp::Node& node, const char* topic, const ::rclcpp::QoS& qos,
                             Cb cb, ::rclcpp::tr::true_type /* takes MessageInfo */) {
#if defined(NROS_CPP_HAS_SHARED_PTR) // hosted-family: shared-ptr-interop
    constexpr int shape = ::rclcpp::detail::subscription_info_callback_shape<M, Cb>::value;
    auto fn = ::rclcpp::detail::subscription_info_adapter<M, Cb, shape>::wrap(
        ::rclcpp::tr::forward_rvalue(cb));
    ::size_t handle_id = 0;
    const ::rclcpp::Result r = ::rclcpp::detail::register_subscription_message_info<M>(
        node, topic, qos, static_cast<decltype(fn)&&>(fn), &handle_id);
    ::rclcpp::detail::require_created(r, "create_subscription", topic);
    return typename Subscription<M>::SharedPtr(node.executor_handle(), handle_id);
#else
    static_assert(::rclcpp::detail::refuse<Cb>::value, NROS_RCLCPP_REFUSE_MESSAGE_INFO_CALLBACK);
    (void)topic;
    (void)qos;
    (void)cb;
    return typename Subscription<M>::SharedPtr(node.executor_handle(), 0);
#endif // NROS_CPP_HAS_SHARED_PTR
}

} // namespace detail

template <typename M, typename Cb>
inline typename Subscription<M>::SharedPtr
Node::create_subscription(const char* topic, const ::rclcpp::QoS& qos, Cb cb) {
    // issue 2036 — a callback that takes no one-argument shape but does take
    // `(message, const MessageInfo&)` goes down the metadata path; every other
    // callback (including one that would take both) keeps the one-argument path.
    constexpr bool takes_info = ::rclcpp::detail::subscription_callback_shape<M, Cb>::value ==
                                    ::rclcpp::detail::SUB_SHAPE_NONE &&
                                ::rclcpp::detail::subscription_info_callback_shape<M, Cb>::value !=
                                    ::rclcpp::detail::SUB_SHAPE_NONE;
    return ::rclcpp::detail::create_subscription_dispatch<M>(
        *this, topic, qos, ::rclcpp::tr::forward_rvalue(cb),
        ::rclcpp::tr::integral_constant<bool, takes_info>());
}

template <typename M, typename Cb>
inline typename Subscription<M>::SharedPtr Node::create_subscription(const char* topic,
                                                                     ::size_t depth, Cb cb) {
    return this->create_subscription<M>(topic, ::rclcpp::QoS(static_cast<uint32_t>(depth)),
                                        ::rclcpp::tr::forward_rvalue(cb));
}

template <typename M, typename Cb>
inline typename Subscription<M>::SharedPtr
Node::create_subscription(const char* topic, const ::rclcpp::QoS& qos, Cb cb,
                          const ::rclcpp::SubscriptionOptions& options) {
    // phase-488 — `options.callback_group` is upstream's anonymous group, which
    // selects the node default (`rclcpp::CallbackGroupToken`), so it needs no
    // handling here. A scheduling context is a different matter: this overload
    // cannot bind one, and dropping it would schedule the callback somewhere
    // the caller did not ask for.
    if (options.sched_context != ::rclcpp::SCHED_CONTEXT_UNSET) {
        ::rclcpp::detail::require_created(::rclcpp::Result(::rclcpp::ErrorCode::Unsupported),
                                          "create_subscription(..., options.sched_context)", topic);
    }
    // issue 2036 — metadata delivery is chosen by the CALLBACK, as upstream
    // chooses it: a callback taking `const rclcpp::MessageInfo&` gets it.
    // `options.message_info = true` with a callback that cannot receive it
    // would configure nothing, so it is refused rather than ignored.
    constexpr bool cb_takes_info =
        ::rclcpp::detail::subscription_callback_shape<M, Cb>::value ==
            ::rclcpp::detail::SUB_SHAPE_NONE &&
        ::rclcpp::detail::subscription_info_callback_shape<M, Cb>::value !=
            ::rclcpp::detail::SUB_SHAPE_NONE;
    if (options.message_info && !cb_takes_info) {
        ::rclcpp::detail::require_created(
            ::rclcpp::Result(::rclcpp::ErrorCode::Unsupported),
            "create_subscription(..., options.message_info) with a callback taking no "
            "rclcpp::MessageInfo",
            topic);
    }
    return this->create_subscription<M>(topic, qos, ::rclcpp::tr::forward_rvalue(cb));
}

template <typename M, typename Cb>
inline typename Subscription<M>::SharedPtr
Node::create_subscription(const char* topic, ::size_t depth, Cb cb,
                          const ::rclcpp::SubscriptionOptions& options) {
    return this->create_subscription<M>(topic, ::rclcpp::QoS(static_cast<uint32_t>(depth)),
                                        ::rclcpp::tr::forward_rvalue(cb), options);
}

template <typename S>
inline ::rclcpp::Owned<::rclcpp::PollService<S>> Node::create_service(const char* name,
                                                                      const ::rclcpp::QoS& qos) {
    // phase-456 W5 — the POLL server, by value. `Service<S>::SharedPtr` is a
    // dispatch handle now, and this factory exists precisely to be
    // `->take_request()`'d, so it must hand back something dereferenceable.
    // `Owned<PollService<S>>` is that and costs no allocator; the node keeps no
    // second reference because there is no second reference to keep.
    ::rclcpp::PollService<S> s;
    ::rclcpp::detail::require_created(this->template create_service<S>(s, name, qos),
                                      "create_service", name);
    return ::rclcpp::Owned<::rclcpp::PollService<S>>(::rclcpp::tr::forward_rvalue(s));
}

template <typename S, typename F, typename>
inline typename Service<S>::SharedPtr Node::create_service(const char* name, F callback,
                                                           const ::rclcpp::QoS& qos) {
    // phase-456 W5 — the arena owns the server and (W3) the handler, so this
    // allocates nothing and there is no cell to retain. The `owned_entities`
    // push that used to stand here was LOAD-BEARING while the arena held
    // `&*s` — dropping the caller's pointer would have dangled it — and became
    // pure retention when W3 moved the context to the handler.
    Service<S> s;
    ::rclcpp::detail::require_created(this->template create_service<S>(s, name, callback, qos),
                                      "create_service", name);
    return typename Service<S>::SharedPtr(this->executor_handle(), s.handle_id());
}

template <typename S, typename F, typename, typename>
inline typename Service<S>::SharedPtr Node::create_service(const char* name, F callback,
                                                           const ::rclcpp::QoS& qos) {
#if defined(NROS_CPP_HAS_SHARED_PTR) // hosted-family: shared-ptr-interop
    // phase-488 — upstream's `shared_ptr` handler, lent from the dispatch frame
    // (`detail::shared_ptr_service_trampoline`, no heap), when it converts to a
    // function pointer. A handler with STATE — `std::bind(&X::on_req, this, _1,
    // _2)`, a capturing lambda — is copied into the service's arena entry
    // instead (`detail::stateful_service_trampoline`), in any of the three
    // shapes. Anything invocable as none of them is refused, naming them.
    constexpr int shape = ::rclcpp::detail::service_ptr_shape<S, F>::value;
    constexpr int stateful = ::rclcpp::detail::stateful_service_shape<S, F>::value;
    static_assert(shape != 0 || stateful != 0 || ::rclcpp::detail::refuse<F>::value,
                  NROS_RCLCPP_REFUSE_SERVICE_CALLBACK_SHAPE);
    Service<S> s;
    ::rclcpp::detail::require_created(
        this->template register_hosted_service_<S>(
            s, name, ::rclcpp::tr::forward_rvalue(callback), qos,
            ::rclcpp::tr::integral_constant<int, (shape != 0 ? 0 : 1)>()),
        "create_service", name);
    return typename Service<S>::SharedPtr(this->executor_handle(), s.handle_id());
#else
    (void)name;
    (void)callback;
    (void)qos;
    static_assert(::rclcpp::detail::refuse<F>::value, NROS_RCLCPP_REFUSE_SERVICE_CALLBACK_SHAPE);
    return typename Service<S>::SharedPtr();
#endif // NROS_CPP_HAS_SHARED_PTR
}

#if defined(NROS_CPP_HAS_SHARED_PTR) // hosted-family: shared-ptr-interop
template <typename S, typename F>
inline Result Node::register_hosted_service_(Service<S>& out, const char* name, F callback,
                                             const ::rclcpp::QoS& qos,
                                             ::rclcpp::tr::integral_constant<int, 0>) {
    constexpr int shape = ::rclcpp::detail::service_ptr_shape<S, F>::value;
    return this->template register_service_<S>(
        out, name, &::rclcpp::detail::shared_ptr_service_trampoline<S, shape>::call,
        ::rclcpp::detail::shared_ptr_service_context<S>(
            callback, ::rclcpp::tr::integral_constant<int, (shape == 3 ? 3 : 2)>()),
        qos, ::rclcpp::ServiceOptions());
}

template <typename S, typename F>
inline Result Node::register_hosted_service_(Service<S>& out, const char* name, F callback,
                                             const ::rclcpp::QoS& qos,
                                             ::rclcpp::tr::integral_constant<int, 1>) {
    constexpr int stateful = ::rclcpp::detail::stateful_service_shape<S, F>::value;
    using Fn = ::rclcpp::detail::StatefulServiceFn<S>;
    Fn captured(::rclcpp::detail::stateful_service_adapter<S, (stateful != 0 ? stateful : 1)>::wrap(
        ::rclcpp::tr::forward_rvalue(callback)));
    const Result r = this->template register_service_<S>(
        out, name, &::rclcpp::detail::stateful_service_trampoline<S>::call, nullptr, qos,
        ::rclcpp::ServiceOptions(), &captured, sizeof(Fn),
        &::rclcpp::detail::stateful_service_trampoline<S>::destroy);
    // The arena's copy owns the callable now and destroys it on release.
    if (r.ok()) {
        captured.relinquish();
    }
    return r;
}
#endif // NROS_CPP_HAS_SHARED_PTR

template <typename S, typename F>
inline typename Service<S>::SharedPtr Node::create_service(const char* name, F callback,
                                                           const ::rclcpp::QoS& qos,
                                                           const ::rclcpp::CallbackGroup* group) {
    // phase-488 — see the declaration: an anonymous group is null here.
    if (group != nullptr && group->get_name() != nullptr) {
        ::rclcpp::detail::require_created(::rclcpp::Result(::rclcpp::ErrorCode::Unsupported),
                                          "create_service(..., named callback group)", name);
    }
    return this->template create_service<S>(name, ::rclcpp::tr::forward_rvalue(callback), qos);
}

template <typename S>
inline ::rclcpp::Owned<::rclcpp::PollClient<S>> Node::create_client(const char* name,
                                                                    const ::rclcpp::QoS& qos) {
    // phase-456 W9 — the FUTURE-style client, by value. `Client<S>::SharedPtr`
    // is a dispatch handle now, and this factory exists precisely to be
    // `->send_request()`'d, so it must hand back something dereferenceable.
    // `Owned<PollClient<S>>` is that and costs no allocator; the node keeps no
    // second reference because there is no second reference to keep.
    ::rclcpp::PollClient<S> c;
    ::rclcpp::detail::require_created(this->template create_client<S>(c, name, qos),
                                      "create_client", name);
    return ::rclcpp::Owned<::rclcpp::PollClient<S>>(::rclcpp::tr::forward_rvalue(c));
}

template <typename S, typename F, typename>
inline typename Client<S>::SharedPtr Node::create_client(const char* name, F callback,
                                                         const ::rclcpp::QoS& qos) {
    // phase-456 W9 — the arena owns the client and (W3) the handler, so this
    // allocates nothing and there is no cell to retain. The `owned_entities`
    // push that used to stand here was LOAD-BEARING while the arena held
    // `&*c` — dropping the caller's pointer would have dangled it — and became
    // pure retention when W3 moved the context to the handler. Same removal W5
    // made one entity over.
    Client<S> c;
    ::rclcpp::detail::require_created(this->template create_client<S>(c, name, callback, qos),
                                      "create_client", name);
    return typename Client<S>::SharedPtr(this->executor_handle(), c.handle_id());
}

template <typename S, typename F, typename, typename>
inline typename Client<S>::SharedPtr Node::create_client(const char*, F, const ::rclcpp::QoS&) {
    static_assert(::rclcpp::detail::refuse<F>::value,
                  NROS_RCLCPP_REFUSE_SHARED_PTR_SERVICE_CALLBACK);
    return typename Client<S>::SharedPtr();
}

} // namespace rclcpp

// -- timers -------------------------------------------------------------------
//
// phase-476 W2 — FREESTANDING. The callable is copied into the executor arena
// (`nros_cpp_timer_create_capturing`), which destroys it when the timer is
// released, so these allocate nothing and return a two-word `TimerHandle`.
// Defined after the hosted block so that, on a hosted target, the
// `shared_ptr` member of `as_node_ref`'s overload set is already declared when
// `rclcpp::create_timer`'s body names it.

namespace rclcpp {
namespace detail {

/// Whether a timer callable is invocable as `cb(const rclcpp::TimerInfo&)` —
/// issue 1974, upstream's `TimerInfoCallbackType` overload. Overload resolution
/// in `sizeof`, as `tr::is_convertible` does: nothing is evaluated.
template <typename F> class timer_takes_info {
    typedef char yes_t[1];
    typedef char no_t[2];
    template <typename G>
    static yes_t& probe(G* g, decltype((*g)(*static_cast<const TimerInfo*>(nullptr)), 0));
    template <typename G> static no_t& probe(...);

  public:
    static const bool value = sizeof(probe<F>(nullptr, 0)) == sizeof(yes_t);
};

/// The one callable the arena stores for a timer: it takes the FFI call info
/// and either converts it to a `TimerInfo` (on the timer's own clock, which
/// the info carries, so the adapter adds no capture of its own) or ignores it.
template <typename F, bool TakesInfo = timer_takes_info<F>::value> struct TimerAdapter {
    F f;
    void operator()(const nros_cpp_timer_call_info_t& raw) {
        const rcl_clock_type_t clock = static_cast<rcl_clock_type_t>(raw.clock_type);
        const TimerInfo info{Time(raw.expected_call_time, clock),
                             Time(raw.actual_call_time, clock)};
        f(info);
    }
};
template <typename F> struct TimerAdapter<F, false> {
    F f;
    void operator()(const nros_cpp_timer_call_info_t&) { f(); }
};

/// The capture capacity a timer adapter needs: the usual knob, except for a
/// callable that is ITSELF an `InplaceFn` (a `TimerInfoCallbackType` variable
/// passed by name), whose size is the knob plus two pointers by construction.
/// The arena copies any length, so this only stops that one shape from tripping
/// the capacity `static_assert` meant for oversized lambdas.
template <typename F> struct timer_capture_capacity {
    static constexpr tr::size_type value = NROS_CPP_CALLBACK_CAPACITY;
};
template <typename Sig, tr::size_type Cap> struct timer_capture_capacity<InplaceFn<Sig, Cap>> {
    static constexpr tr::size_type value = sizeof(TimerAdapter<InplaceFn<Sig, Cap>>);
};

/// Register `cb` as a timer on `clock_type`, its callable held by the arena.
///
/// The bridge between `rclcpp::InplaceFn` and
/// `nros_cpp_timer_create_capturing_with_info`, the timer twin of
/// `register_subscription_capturing`. The `InplaceFn`'s bytes are COPIED into
/// the arena; on success this object relinquishes them without destroying,
/// because the arena's copy now owns the callable and its `destroy` hook runs
/// `clear()` on that copy when the timer is released. A capture that owns
/// something (a `std::string`, a `shared_ptr`) is destroyed exactly once, there.
///
/// Issue 1974: every callable goes through the call-info entry point. One
/// taking `const TimerInfo&` receives the executor's expected/actual call
/// times; a `void()` one ignores them at no capture cost. `group` (may be
/// null) and `autostart` are Jazzy's trailing `create_*timer` arguments:
/// `autostart == false` registers the timer cancelled until `reset()`.
template <typename Cb>
inline TimerHandle register_timer_capturing(::rclcpp::Node& node, rcl_clock_type_t clock_type,
                                            Duration period, Cb&& cb, const char* verb,
                                            const CallbackGroup* group = nullptr,
                                            bool autostart = true) {
    using F = typename tr::decay<Cb>::type;
    using Adapter = TimerAdapter<F>;
    using Capture =
        InplaceFn<void(const nros_cpp_timer_call_info_t&), timer_capture_capacity<F>::value>;
    Capture captured(Adapter{tr::relay<Cb>(cb)});
    // Non-capturing lambdas, so each decays to the plain function pointer the
    // arena stores. `ctx` is the ARENA's copy of `captured`, never ours.
    nros_cpp_timer_info_callback_t invoke = [](const nros_cpp_timer_call_info_t* info, void* ctx) {
        (*static_cast<Capture*>(ctx))(*info);
    };
    nros_cpp_timer_callback_t destroy = [](void* ctx) { static_cast<Capture*>(ctx)->clear(); };
    const int64_t ns = period.nanoseconds();
    const uint64_t ms = ns > 0 ? static_cast<uint64_t>(ns / 1000000) : uint64_t(0);
    size_t handle_id = 0;
    const nros_cpp_node_t* h = node.ffi_handle();
    const char* group_name = group != nullptr ? group->get_name() : nullptr;
    Result r = h == nullptr ? Result(ErrorCode::NotInitialized)
                            : Result(nros_cpp_timer_create_capturing_with_info(
                                  h, static_cast<uint8_t>(clock_type), ms, invoke,
                                  reinterpret_cast<const uint8_t*>(&captured), sizeof(Capture),
                                  destroy, group_name, autostart, &handle_id));
    if (r.ok()) {
        captured.relinquish();
    }
    // A null return would be silent HERE above all: a ported node stores the
    // timer handle and never dereferences it, so a dead timer would simply
    // never fire.
    ::rclcpp::detail::require_created(r, verb, "");
    return TimerHandle(node.executor_handle(), handle_id);
}

} // namespace detail
} // namespace rclcpp

namespace rclcpp {

template <typename Cb>
inline ::rclcpp::TimerHandle Node::create_wall_timer(::rclcpp::Duration period, Cb cb,
                                                     const ::rclcpp::CallbackGroup* group,
                                                     bool autostart) {
    return ::rclcpp::detail::register_timer_capturing(*this, RCL_STEADY_TIME, period,
                                                      ::rclcpp::tr::forward_rvalue(cb),
                                                      "create_wall_timer", group, autostart);
}

#ifdef NROS_CPP_HAS_STD_CHRONO // hosted-family: chrono-interop
template <typename Rep, typename Period, typename Cb>
inline ::rclcpp::TimerHandle Node::create_wall_timer(::std::chrono::duration<Rep, Period> period,
                                                     Cb cb, const ::rclcpp::CallbackGroup* group,
                                                     bool autostart) {
    const auto ns = ::std::chrono::duration_cast<::std::chrono::nanoseconds>(period).count();
    return this->create_wall_timer(::rclcpp::Duration::from_nanoseconds(static_cast<int64_t>(ns)),
                                   ::rclcpp::tr::forward_rvalue(cb), group, autostart);
}
#endif // NROS_CPP_HAS_STD_CHRONO

namespace detail {
/// `NodeT` in upstream's signature is anything node-shaped — `this`, a
/// reference, or (hosted) a `shared_ptr`, whose overload is in the hosted block
/// above. One overload set, so the free function does not need three copies.
/// phase-482 W2 — `Node::SharedPtr` is a freestanding handle.
inline ::rclcpp::Node& as_node_ref(const ::rclcpp::Node::SharedPtr& n) {
    return *n;
}
inline ::rclcpp::Node& as_node_ref(::rclcpp::Node& n) {
    return n;
}
inline ::rclcpp::Node& as_node_ref(::rclcpp::Node* n) {
    return *n;
}
} // namespace detail

/// `rclcpp::create_timer(node, clock, period, callback)` — humble's only
/// clock-taking timer verb. FREESTANDING since phase-476 W2.
///
/// ADOPT-BOUNDED: the clock is `rclcpp::Clock*` (what `node->get_clock()` returns),
/// not `rclcpp::Clock::SharedPtr`; the period is truncated to whole MILLISECONDS;
/// Humble's optional trailing `group` argument is not taken; and the result is
/// a `rclcpp::TimerHandle` (`rclcpp::TimerBase::SharedPtr` names the same type).
///
/// Jazzy's trailing `group` and `autostart` are taken (issue 1974): `group` is a
/// `const rclcpp::CallbackGroup*` (null = the node default — a ported `nullptr`
/// binds), and `autostart = false` creates the timer cancelled until `reset()`.
/// A callback invocable as `void(const rclcpp::TimerInfo&)` receives the
/// activation's expected and actual call times on `clock`.
template <typename NodeT, typename CallbackT>
inline ::rclcpp::TimerHandle
create_timer(NodeT&& node, ::rclcpp::Clock* clock, ::rclcpp::Duration period, CallbackT&& callback,
             const ::rclcpp::CallbackGroup* group = nullptr, bool autostart = true) {
    ::rclcpp::Node& n = detail::as_node_ref(node);
    const rcl_clock_type_t type =
        clock != nullptr ? clock->get_clock_type() : n.get_clock()->get_clock_type();
    return ::rclcpp::detail::register_timer_capturing(n, type, period,
                                                      ::rclcpp::tr::relay<CallbackT>(callback),
                                                      "create_timer", group, autostart);
}

#ifdef NROS_CPP_HAS_STD_CHRONO // hosted-family: chrono-interop
/// `rclcpp::create_timer(node, clock, 100ms, callback)` — the `std::chrono`
/// spelling, which is what a ported file actually writes. Upstream's
/// `rclcpp::Duration` is implicitly constructible from a chrono duration; ours
/// is not (it reaches freestanding targets where `<chrono>` does not exist), so
/// the conversion is an overload rather than a constructor.
template <typename NodeT, typename Rep, typename Period, typename CallbackT>
inline ::rclcpp::TimerHandle
create_timer(NodeT&& node, ::rclcpp::Clock* clock, ::std::chrono::duration<Rep, Period> period,
             CallbackT&& callback, const ::rclcpp::CallbackGroup* group = nullptr,
             bool autostart = true) {
    const auto ns = ::std::chrono::duration_cast<::std::chrono::nanoseconds>(period).count();
    return create_timer(::rclcpp::tr::relay<NodeT>(node), clock,
                        ::rclcpp::Duration::from_nanoseconds(static_cast<int64_t>(ns)),
                        ::rclcpp::tr::relay<CallbackT>(callback), group, autostart);
}
#endif // NROS_CPP_HAS_STD_CHRONO

} // namespace rclcpp

// --- node parameters ---------------------------------------------------------
//
// Out here rather than in the hosted block above: see the note at the
// `-- parameters --` marker there, and `node.hpp` for why the gate moved to
// the `std::string`-keyed overloads alone (phase-426 W4).

// -- parameters ---------------------------------------------------------------
//
// Forwarders onto THE parameter store — the `nros_params::ParameterServer` the
// EXECUTOR owns, reached through `rclcpp/node_parameters.hpp`. There is no other
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
    if (!this->check_declared_param(name, ::rclcpp::detail::node_param_type<T>::value)) {
        return default_value;
    }
    const ::nros_cpp_node_t* h = this->ffi_handle();
    Result r = ::rclcpp::detail::node_param_declare(h, name, default_value);
    if (!r.ok() && r.raw() != RCLC_RET_ALREADY_EXISTS) {
        return default_value;
    }
    T out = T();
    if (!::rclcpp::detail::node_param_get(h, name, out).ok()) {
        return default_value;
    }
    return out;
}
} // namespace rclcpp

namespace rclcpp {} // namespace rclcpp

namespace rclcpp {
template <typename T> inline bool Node::get_parameter(const char* name, T& out) const {
    return ::rclcpp::detail::node_param_get(this->ffi_handle(), name, out).ok();
}
} // namespace rclcpp

namespace rclcpp {} // namespace rclcpp

namespace rclcpp {
template <typename T> inline T Node::get_parameter(const char* name) const {
    T out = T();
    (void)::rclcpp::detail::node_param_get(this->ffi_handle(), name, out);
    return out;
}
} // namespace rclcpp

namespace rclcpp {

/// phase-426 W4 — `set_parameter` now goes through the SAME
/// `ParameterServer::apply` a remote `ros2 param set` does, so a read-only
/// parameter is refused here exactly as it is on the wire, and the value a
/// service reports back is the value this wrote.
} // namespace rclcpp

namespace rclcpp {
template <typename T> inline Result Node::set_parameter(const char* name, T value) {
    return ::rclcpp::detail::node_param_set(this->ffi_handle(), name, value);
}
} // namespace rclcpp

namespace rclcpp {} // namespace rclcpp

namespace rclcpp {
inline bool Node::has_parameter(const char* name) const {
    return ::rclcpp::detail::node_param_has(this->ffi_handle(), name);
}
} // namespace rclcpp

// --- the rest of rclcpp's parameter surface (phase-417 W4.a) -----------------
//
// Every one of these is a forwarder onto the executor's ONE store through
// `rclcpp/node_parameters.hpp`. None of them holds state; a wrapper that did
// would be the second store phase-426 removed, in a smaller spelling.

namespace rclcpp {

template <typename T>
inline T Node::declare_parameter(const char* name, T default_value,
                                 const ParameterDescriptor& descriptor) {
    T in_effect = this->template declare_parameter<T>(name, default_value);
    // The descriptor is attached AFTER the declaration, which is the store's
    // own shape (rclc's `rclc_add_parameter_description` & co.). A failure
    // here leaves the parameter declared with its value, which is what
    // upstream does when the descriptor is consistent with the default — and
    // when it is not, the attach refuses rather than advertising a bound the
    // value breaks.
    (void)::rclcpp::detail::node_param_apply_descriptor(
        this->ffi_handle(), name, descriptor, ::rclcpp::detail::node_param_type<T>::value);
    return in_effect;
}

template <typename T>
inline bool Node::get_parameter_or(const char* name, T& out, T fallback) const {
    if (::rclcpp::detail::node_param_get(this->ffi_handle(), name, out).ok()) {
        return true;
    }
    out = fallback;
    return false;
}

inline Result Node::undeclare_parameter(const char* name) {
    return ::rclcpp::detail::node_param_undeclare(this->ffi_handle(), name);
}

inline ParameterType Node::get_parameter_type(const char* name) const {
    int32_t code = 0;
    if (!::rclcpp::detail::node_param_get_type(this->ffi_handle(), name, code).ok()) {
        return PARAMETER_NOT_SET;
    }
    return static_cast<ParameterType>(code);
}

inline Result Node::get_parameter_types(const char* const* names, ::size_t count,
                                        ParameterType* out) const {
    if ((names == nullptr || out == nullptr) && count != 0) {
        return Result(RCL_RET_INVALID_ARGUMENT);
    }
    for (::size_t i = 0; i < count; ++i) {
        out[i] = this->get_parameter_type(names[i]);
    }
    return Result(RCL_RET_OK);
}

inline Result Node::describe_parameter(const char* name, ParameterDescriptor& out, char* text,
                                       ::size_t text_len) const {
    out = parameter_descriptor();
    // The buffer is split in half, one NUL-terminated string in each, so ONE
    // caller buffer serves both texts and neither can run into the other.
    char* description = nullptr;
    char* constraints = nullptr;
    ::size_t half = 0;
    if (text != nullptr && text_len >= 2) {
        half = text_len / 2;
        description = text;
        constraints = text + half;
    }
    int32_t type = 0;
    Result r = ::rclcpp::detail::node_param_describe(this->ffi_handle(), name, description, half,
                                                     constraints, half, &out.read_only, &type);
    if (r.raw() == RCLC_RET_NOT_FOUND) {
        return r;
    }
    out.description = description;
    out.additional_constraints = constraints;

    // The range, read through whichever accessor the TYPE picks. `NOT_FOUND`
    // from it means "no range", which is not a failure of `describe`.
    if (type == ::rclcpp::param_type::INTEGER) {
        out.has_range =
            ::rclcpp::detail::node_param_get_range(this->ffi_handle(), name, &out.integer_from,
                                                   &out.integer_to, &out.integer_step)
                .ok();
    } else if (type == ::rclcpp::param_type::DOUBLE) {
        out.has_range =
            ::rclcpp::detail::node_param_get_range(this->ffi_handle(), name, &out.double_from,
                                                   &out.double_to, &out.double_step)
                .ok();
    }
    return r;
}

inline Result Node::list_parameters(const char* prefix, char* out_names, ::size_t name_stride,
                                    ::size_t max_names, ::size_t& count) const {
    return ::rclcpp::detail::node_param_list(this->ffi_handle(), prefix, out_names, name_stride,
                                             max_names, &count);
}

inline Result Node::set_parameters_atomically(const ParameterWrite* writes, ::size_t count) {
    return ::rclcpp::detail::node_params_set_atomically(this->ffi_handle(), writes, count);
}

inline Result Node::add_on_set_parameters_callback(OnSetParametersCallbackType callback,
                                                   void* context,
                                                   ParameterCallbackHandle& out_handle) {
    return ::rclcpp::detail::node_param_add_on_set_callback(this->ffi_handle(), callback, context,
                                                            &out_handle);
}

inline Result Node::remove_on_set_parameters_callback(ParameterCallbackHandle handle) {
    return ::rclcpp::detail::node_param_remove_on_set_callback(this->ffi_handle(), handle);
}

} // namespace rclcpp

// --- issue 2056: the value-object parameter surface (hosted) -----------------
//
// Dispatchers from `ParameterValue` / `Parameter` onto the typed methods
// above. No state: a `Parameter` read here is built from the store each time.

#ifdef NROS_CPP_STD // hosted-family: parameter-interop
namespace rclcpp {
namespace detail {

template <typename T>
inline T node_declare_typed(Node& n, const char* name, T v, const ParameterDescriptor* d) {
    return d != nullptr ? n.template declare_parameter<T>(name, v, *d)
                        : n.template declare_parameter<T>(name, v);
}

inline ParameterValue node_declare_value(Node& n, const char* name, const ParameterValue& v,
                                         const ParameterDescriptor* d) {
    switch (v.get_type()) {
    case PARAMETER_BOOL:
        return ParameterValue(node_declare_typed<bool>(n, name, v.as_bool(), d));
    case PARAMETER_INTEGER:
        return ParameterValue(
            static_cast<long long>(node_declare_typed<int64_t>(n, name, v.as_int(), d)));
    case PARAMETER_DOUBLE:
        return ParameterValue(node_declare_typed<double>(n, name, v.as_double(), d));
    case PARAMETER_STRING:
        return ParameterValue(node_declare_typed<::std::string>(n, name, v.as_string(), d));
    case PARAMETER_BOOL_ARRAY:
        return ParameterValue(
            node_declare_typed<::std::vector<bool>>(n, name, v.as_bool_array(), d));
    case PARAMETER_INTEGER_ARRAY:
        return ParameterValue(
            node_declare_typed<::std::vector<int64_t>>(n, name, v.as_integer_array(), d));
    case PARAMETER_DOUBLE_ARRAY:
        return ParameterValue(
            node_declare_typed<::std::vector<double>>(n, name, v.as_double_array(), d));
    default:
        // NOT_SET, a byte array or a string array: no store form.
        return ParameterValue();
    }
}

/// `rcl_interfaces::msg::ParameterDescriptor` lowered to the borrowed-text
/// descriptor. The pointers borrow `m`'s strings, so the result must not
/// outlive `m`; one range of each kind is read, as upstream permits at most one.
template <typename D> inline ParameterDescriptor descriptor_from_message(const D& m) {
    ParameterDescriptor d = ::rclcpp::parameter_descriptor();
    d.description = m.description.c_str();
    d.additional_constraints = m.additional_constraints.c_str();
    d.read_only = m.read_only;
    for (const auto& r : m.floating_point_range) {
        d.has_range = true;
        d.double_from = r.from_value;
        d.double_to = r.to_value;
        d.double_step = r.step;
        break;
    }
    for (const auto& r : m.integer_range) {
        d.has_range = true;
        d.integer_from = r.from_value;
        d.integer_to = r.to_value;
        d.integer_step = static_cast<int64_t>(r.step);
        break;
    }
    return d;
}

inline Node::SetParametersOutcome node_set_outcome(Result r) {
    Node::SetParametersOutcome o;
    o.successful = r.ok();
    if (!o.successful) {
        o.reason = "set_parameter refused (nano-ros code " + ::std::to_string(r.raw()) + ")";
    }
    return o;
}

} // namespace detail

inline ParameterValue Node::declare_parameter(const char* name,
                                              const ParameterValue& default_value) {
    return ::rclcpp::detail::node_declare_value(*this, name, default_value, nullptr);
}

inline ParameterValue Node::declare_parameter(const char* name, const ParameterValue& default_value,
                                              const ParameterDescriptor& descriptor) {
    return ::rclcpp::detail::node_declare_value(*this, name, default_value, &descriptor);
}

template <typename D, typename>
inline ParameterValue Node::declare_parameter(const char* name, const ParameterValue& default_value,
                                              const D& descriptor) {
    const ParameterDescriptor d = ::rclcpp::detail::descriptor_from_message(descriptor);
    return ::rclcpp::detail::node_declare_value(*this, name, default_value, &d);
}

template <typename T, typename D, typename>
inline T Node::declare_parameter(const char* name, T default_value, const D& descriptor) {
    const ParameterDescriptor d = ::rclcpp::detail::descriptor_from_message(descriptor);
    return this->template declare_parameter<T>(name, default_value, d);
}

inline bool Node::get_parameter(const char* name, Parameter& out) const {
    ParameterValue v;
    switch (this->get_parameter_type(name)) {
    case PARAMETER_BOOL: {
        bool x = false;
        if (!this->template get_parameter<bool>(name, x)) return false;
        v = ParameterValue(x);
        break;
    }
    case PARAMETER_INTEGER: {
        int64_t x = 0;
        if (!this->template get_parameter<int64_t>(name, x)) return false;
        v = ParameterValue(static_cast<long long>(x));
        break;
    }
    case PARAMETER_DOUBLE: {
        double x = 0.0;
        if (!this->template get_parameter<double>(name, x)) return false;
        v = ParameterValue(x);
        break;
    }
    case PARAMETER_STRING: {
        ::std::string x;
        if (!this->template get_parameter<::std::string>(name, x)) return false;
        v = ParameterValue(x);
        break;
    }
    case PARAMETER_BOOL_ARRAY: {
        ::std::vector<bool> x;
        if (!this->template get_parameter<::std::vector<bool>>(name, x)) return false;
        v = ParameterValue(x);
        break;
    }
    case PARAMETER_INTEGER_ARRAY: {
        ::std::vector<int64_t> x;
        if (!this->template get_parameter<::std::vector<int64_t>>(name, x)) return false;
        v = ParameterValue(x);
        break;
    }
    case PARAMETER_DOUBLE_ARRAY: {
        ::std::vector<double> x;
        if (!this->template get_parameter<::std::vector<double>>(name, x)) return false;
        v = ParameterValue(x);
        break;
    }
    default:
        return false;
    }
    out = Parameter(::std::string(name), v);
    return true;
}

inline Parameter Node::get_parameter(const ::std::string& name) const {
    Parameter p(name);
    (void)this->get_parameter(name.c_str(), p);
    return p;
}

inline ::std::vector<Parameter>
Node::get_parameters(const ::std::vector<::std::string>& names) const {
    ::std::vector<Parameter> out;
    out.reserve(names.size());
    for (const auto& n : names) {
        out.push_back(this->get_parameter(n));
    }
    return out;
}

inline Node::SetParametersOutcome Node::set_parameter(const Parameter& p) {
    const char* name = p.get_name().c_str();
    Result r(NROS_CPP_RET_UNSUPPORTED);
    switch (p.get_type()) {
    case PARAMETER_BOOL:
        r = this->template set_parameter<bool>(name, p.as_bool());
        break;
    case PARAMETER_INTEGER:
        r = this->template set_parameter<int64_t>(name, p.as_int());
        break;
    case PARAMETER_DOUBLE:
        r = this->template set_parameter<double>(name, p.as_double());
        break;
    case PARAMETER_STRING:
        r = this->template set_parameter<::std::string>(name, p.as_string());
        break;
    case PARAMETER_BOOL_ARRAY:
        r = this->template set_parameter<::std::vector<bool>>(name, p.as_bool_array());
        break;
    case PARAMETER_INTEGER_ARRAY:
        r = this->template set_parameter<::std::vector<int64_t>>(name, p.as_integer_array());
        break;
    case PARAMETER_DOUBLE_ARRAY:
        r = this->template set_parameter<::std::vector<double>>(name, p.as_double_array());
        break;
    default:
        break;
    }
    return ::rclcpp::detail::node_set_outcome(r);
}

inline ::std::vector<Node::SetParametersOutcome>
Node::set_parameters(const ::std::vector<Parameter>& parameters) {
    ::std::vector<SetParametersOutcome> out;
    out.reserve(parameters.size());
    for (const auto& p : parameters) {
        out.push_back(this->set_parameter(p));
    }
    return out;
}

template <typename F>
inline node_interfaces::OnSetParametersCallbackHandle::SharedPtr
Node::add_on_set_parameters_callback(F callback) {
    using Handle = node_interfaces::OnSetParametersCallbackHandle;
    auto handle = ::std::make_shared<Handle>();
    handle->callback = [callback](const ::std::vector<Parameter>& params) mutable -> bool {
        return static_cast<bool>(callback(params).successful);
    };
    handle->node = this->ffi_handle();
    Result r = ::rclcpp::detail::node_param_add_on_set_callback(handle->node, &Handle::trampoline,
                                                                handle.get(), &handle->token);
    if (!r.ok()) {
        return nullptr;
    }
    handle->registered = true;
    return handle;
}

inline void Node::remove_on_set_parameters_callback(
    const node_interfaces::OnSetParametersCallbackHandle* handle) {
    if (handle != nullptr) {
        const_cast<node_interfaces::OnSetParametersCallbackHandle*>(handle)->unregister();
    }
}

namespace node_interfaces {
inline bool NodeParametersInterface::has_parameter(const ::std::string& name) const {
    return node_ != nullptr && node_->has_parameter(name);
}
inline ParameterValue NodeParametersInterface::declare_parameter(const ::std::string& name,
                                                                 const ParameterValue& v) {
    return node_ == nullptr ? ParameterValue() : node_->declare_parameter(name, v);
}
template <typename D>
inline ParameterValue NodeParametersInterface::declare_parameter(const ::std::string& name,
                                                                 const ParameterValue& v,
                                                                 const D& descriptor) {
    return node_ == nullptr ? ParameterValue() : node_->declare_parameter(name, v, descriptor);
}
inline bool NodeParametersInterface::get_parameter(const ::std::string& name,
                                                   Parameter& out) const {
    return node_ != nullptr && node_->get_parameter(name, out);
}
inline Parameter NodeParametersInterface::get_parameter(const ::std::string& name) const {
    return node_ == nullptr ? Parameter(name) : node_->get_parameter(name);
}
inline const ::std::map<::std::string, ParameterValue>&
NodeParametersInterface::get_parameter_overrides() const {
    using Map = ::std::map<::std::string, ParameterValue>;
    // One copy per node, keyed by its FFI handle, so every call (and every
    // view of the node) returns the same map. The executor dispatches on one
    // task, which is what makes a plain static safe here.
    static ::std::map<const void*, Map> copies;
    static Map none;
    if (node_ == nullptr || node_->ffi_handle() == nullptr) {
        return none;
    }
    struct Sink {
        static bool add(const ParameterWrite* write, void* context) {
            if (write != nullptr && write->name != nullptr) {
                (*static_cast<Map*>(context))[write->name] =
                    Parameter::from_write(*write).get_parameter_value();
            }
            return true;
        }
    };
    Map now;
    (void)::rclcpp::detail::node_param_list_overrides(node_->ffi_handle(), &Sink::add, &now,
                                                      nullptr);
    Map& kept = copies[node_->ffi_handle()];
    if (kept != now) {
        kept = static_cast<Map&&>(now);
    }
    return kept;
}
} // namespace node_interfaces

} // namespace rclcpp
#endif // NROS_CPP_STD

// --- Rate / WallRate (phase-417 W2.d) ----------------------------------------
//
// A FORWARDER onto `rclcpp::spin(remaining_ms, poll_ms)`, which budgets by wall
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
//   * **Your callbacks RUN during the sleep.** `rclcpp::spin` drives the
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
//   * Before `rclcpp::init()` there is no executor to spin, so `sleep()` returns
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
// RFC-0096 D1 states. `period()` returns `rclcpp::Duration`, which is ours and
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

    /// Construct from a period — `rclcpp::Rate(rclcpp::Duration::from_nanoseconds(n))`.
    /// The always-available spelling, on every target.
    explicit Rate(::rclcpp::Duration period) : period_ns_(period.nanoseconds()) {
        if (period_ns_ < 0) period_ns_ = 0;
        reset();
    }

#ifdef NROS_CPP_HAS_STD_CHRONO // hosted-family: chrono-interop
    /// Construct from a period — `rclcpp::Rate(std::chrono::milliseconds(100))`.
    ///
    /// A gated METHOD, not a gated type: `sizeof(Rate)` is the same in every
    /// configuration. Where `<chrono>` is absent, the `rclcpp::Duration` overload
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
        (void)::rclcpp::spin_in(remaining_ms, poll_ms);
        return true;
    }

    /// `rclcpp::Rate::reset()` — anchor the next tick one period from now.
    void reset() { next_tick_ns_ = nros_cpp_time_ns() + static_cast<uint64_t>(period_ns_); }

    /// The configured period.
    ///
    /// `rclcpp::Duration` rather than `std::chrono::nanoseconds` (phase-442 W5):
    /// a return type that exists only where `<chrono>` does would keep the whole
    /// class hostage to the toolchain, which is what this work item removed.
    /// `.nanoseconds()` is the same number upstream's `.count()` gives.
    ::rclcpp::Duration period() const { return ::rclcpp::Duration::from_nanoseconds(period_ns_); }

  private:
    int64_t period_ns_;
    uint64_t next_tick_ns_;
};

/// `rclcpp::WallRate` — the steady-clock rate. Identical to `Rate` here; see
/// the "same clock" bullet in the envelope above.
using WallRate = Rate;

} // namespace rclcpp

// issue 2015 — `rclcpp_action::create_client` / `create_server`, at upstream's
// paths. Included LAST because they build on everything above
// (`detail::require_created`); the umbrella reaches them as it reaches
// `rclcpp_action::Client` / `Server`, so `#include <rclcpp/rclcpp.hpp>` alone
// is enough for a ported action node, as upstream's transitive includes are.
#include "rclcpp_action/create_client.hpp"
#include "rclcpp_action/create_server.hpp"
// issue 2024 — `rclcpp::ParameterEventHandler`, at upstream's umbrella. LAST for
// the same reason: it builds on `Node::create_subscription`.
#include "rclcpp/parameter_event_handler.hpp"

#endif // NROS_RCLCPP_RCLCPP_HPP
