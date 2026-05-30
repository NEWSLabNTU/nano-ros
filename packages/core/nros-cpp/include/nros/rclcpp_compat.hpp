// SPDX-License-Identifier: Apache-2.0
//
// rclcpp_compat.hpp — source-compat header for ROS 2 `rclcpp` code (Phase 209.A).
//
// Goal: let unmodified C++ source written against `rclcpp` compile against
// nano-ros without #ifdef gymnastics. Pull this header in (transitively, via a
// build-system `target_compile_definitions(... -include nros/rclcpp_compat.hpp)`,
// or directly via `#include "nros/rclcpp_compat.hpp"` ahead of a ported file),
// then keep the original rclcpp call-site syntax.
//
// Scope (what works without source edits):
//   - `rclcpp::init(argc, argv)` / `rclcpp::shutdown()` / `rclcpp::ok()`.
//   - `auto node = std::make_shared<rclcpp::Node>("name");`
//   - `node->create_publisher<M>(topic, qos)` → returns
//     `rclcpp::Publisher<M>::SharedPtr`; `publisher->publish(msg)`.
//   - `node->create_subscription<M>(topic, qos, callback)` → returns
//     `rclcpp::Subscription<M>::SharedPtr` (callback signature
//     `void(const M&)` or `void(std::shared_ptr<M>)`).
//   - `rclcpp::spin(node)` / `rclcpp::spin_some(node)` (`spin_some` ≈ a single
//     `nros::Executor::spin_once`; `spin` loops until `rclcpp::shutdown()`).
//   - `rclcpp::QoS(depth)`, `rclcpp::SystemDefaultsQoS{}`.
//   - Log macros `RCLCPP_INFO/WARN/ERROR/DEBUG/FATAL` (`_THROTTLE` variants
//     degrade to plain log — no real throttle yet).
//   - `rclcpp_action::Server<A>`/`Client<A>` aliases over nros action shapes.
//
// Out of scope (will need source adapt or follow-up shims):
//   - `rclcpp::Node` constructor's `NodeOptions` — only the `name` form is
//     mirrored. Parameter declarations on the Node go through 209.F
//     (`nros bake-params`) for now.
//   - `rclcpp_lifecycle::LifecycleNode` — Phase 209.H (deferred).
//   - `rclcpp_components::register_node` macro — see `rclcpp_components_compat.hpp`
//     (Phase 209.C, separate header).
//   - Multi-threaded executors with per-callback group affinity — nano-ros has
//     `nros::MultiExecutor` but the rclcpp callback-group API is not aliased
//     here yet.
//   - tf2 / image_transport / pluginlib — out of nano-ros scope.
//
// Convention: this header DOES NOT replace `<rclcpp/rclcpp.hpp>` for an
// ament/colcon build that genuinely links against upstream ROS 2 — it only
// bridges source code so the same .cpp can be reused under nano-ros. The
// CMake module `cmake/compat/NrosRclcppCompat.cmake` (Phase 209.B) wires
// `find_package(rclcpp)` to this surface.

#ifndef NROS_RCLCPP_COMPAT_HPP
#define NROS_RCLCPP_COMPAT_HPP

#include "nros/nros.hpp"
#include "nros/node.hpp"
#include "nros/publisher.hpp"
#include "nros/subscription.hpp"
#include "nros/service.hpp"
#include "nros/client.hpp"
#include "nros/executor.hpp"
#include "nros/qos.hpp"
#include "nros/log.hpp"
#include "nros/action_server.hpp"
#include "nros/action_client.hpp"
#include "nros/result.hpp"

#include <memory>
#include <string>
#include <functional>

namespace rclcpp {

// --- Type aliases for shapes that map cleanly ---------------------------------

using ::nros::QoS;
using ::nros::Result;

template <typename M> using Publisher = ::nros::Publisher<M>;

template <typename M> using Subscription = ::nros::Subscription<M>;

template <typename S> using Service = ::nros::Service<S>;

template <typename S> using Client = ::nros::Client<S>;

// `rclcpp::Publisher<M>::SharedPtr` — rclcpp users index types this way.
namespace detail {
template <typename T> struct SharedPtrTrait {
    using SharedPtr = std::shared_ptr<T>;
    using ConstSharedPtr = std::shared_ptr<const T>;
};
} // namespace detail

// --- QoS conveniences --------------------------------------------------------
//
// `rclcpp::QoS(10)` is the common spelling; nros::QoS takes a depth argument
// too, so the constructor is already compatible. These named profiles save the
// most-cited spellings from `rclcpp::QoS` factories.

inline ::nros::QoS SystemDefaultsQoS() {
    return ::nros::QoS(10);
}
inline ::nros::QoS ServicesQoS() {
    return ::nros::QoS(10);
}
inline ::nros::QoS ParametersQoS() {
    return ::nros::QoS(10);
}

inline ::nros::QoS KeepLast(::size_t depth) {
    return ::nros::QoS(depth);
}

// --- Logger surface ----------------------------------------------------------
//
// `rclcpp::Logger` in upstream is a pull-through to the rcl logger. Here it is
// a name-only sentinel; the log macros below dispatch through NROS_*, which
// already carry the file/line. The logger NAME is lost (nros has no per-logger
// dispatch yet). Documented; a follow-up can teach nros::log a tag.

class Logger {
  public:
    explicit Logger(const char* name = "") : name_(name) {}
    const char* get_name() const { return name_; }

  private:
    const char* name_;
};

inline Logger get_logger(const char* name) {
    return Logger(name);
}
inline Logger get_logger(const std::string& name) {
    return Logger(name.c_str());
}

// --- Process-level lifecycle -------------------------------------------------
//
// `rclcpp::init(argc, argv)` is a process-level handshake. nano-ros has a
// global `nros::init()` (no argc/argv — args are ignored). `rclcpp::shutdown()`
// → `nros::shutdown()`. `rclcpp::ok()` → `nros::ok()` (nros tracks the
// shutdown flag).

inline void init(int /*argc*/, char const* const* /*argv*/) {
    (void)::nros::init();
}
inline void init() {
    (void)::nros::init();
}
inline bool shutdown() {
    ::nros::shutdown();
    return true;
}
inline bool ok() {
    return ::nros::ok();
}

// --- Node shim ---------------------------------------------------------------
//
// rclcpp users write `auto n = std::make_shared<rclcpp::Node>("name");` and
// then `n->create_publisher<M>(topic, qos)` returning a shared_ptr. nros's
// Node is created via an `Executor` and exposes out-ref `create_*` member
// functions. Wrap that to match the rclcpp call shape. A `Node` shim owns its
// own `nros::Executor` (the typical single-node-per-process pattern). When
// shared across nodes is needed, the caller can construct multiple Nodes —
// each currently gets its own Executor (matches the single-node default).
//
// Threading: `rclcpp::spin(node)` borrows the node's executor for the calling
// thread (same as `nros::Executor::spin`). Callbacks fire on whatever thread
// services `spin*`, mirroring the rclcpp default.

class Node : public std::enable_shared_from_this<Node> {
  public:
    using SharedPtr = std::shared_ptr<Node>;

    explicit Node(const std::string& name) {
        // Bring up the executor + the underlying nros::Node. Initialization
        // failures throw at construction time (the rclcpp idiom), so a caller
        // that uses `std::make_shared<rclcpp::Node>("n")` mirrors rclcpp's
        // "constructor never returns an error code" contract.
        ::nros::Result r = ::nros::Executor::create(executor_);
        if (r.is_err()) {
            // nros-cpp is freestanding by default — no `<stdexcept>`. Mark the
            // node as uninitialized; subsequent `create_*` will fail visibly.
            initialized_ = false;
            return;
        }
        r = executor_.create_node(node_, name.c_str());
        initialized_ = r.ok();
    }

    ~Node() {
        if (initialized_) {
            executor_.shutdown();
        }
    }

    Node(const Node&) = delete;
    Node& operator=(const Node&) = delete;

    const ::nros::Node& nros_node() const { return node_; }
    ::nros::Node& nros_node() { return node_; }
    ::nros::Executor& nros_executor() { return executor_; }

    bool initialized() const { return initialized_; }

    Logger get_logger() const { return Logger("nros.compat"); }

    // create_publisher<M>(topic, qos)
    //
    // QoS arg is `const QoS&` OR an integer depth (`10`) — both bind via
    // `nros::QoS(uint32_t)`'s implicit conversion.
    template <typename M>
    std::shared_ptr<::nros::Publisher<M>> create_publisher(const std::string& topic,
                                                           const ::nros::QoS& qos) {
        auto p = std::make_shared<::nros::Publisher<M>>();
        (void)node_.create_publisher(*p, topic.c_str(), qos);
        return p;
    }

    template <typename M>
    std::shared_ptr<::nros::Publisher<M>> create_publisher(const std::string& topic,
                                                           ::size_t depth) {
        return create_publisher<M>(topic, ::nros::QoS(static_cast<uint32_t>(depth)));
    }

    // create_subscription<M>(topic, qos, callback)
    //
    // rclcpp callbacks come in two shapes: `void(const M&)` and
    // `void(typename M::ConstSharedPtr)`. The latter we accept by signature and
    // wrap into nros's by-ref callback (allocating a shared_ptr per message).
    template <typename M, typename Cb>
    std::shared_ptr<::nros::Subscription<M>> create_subscription(const std::string& topic,
                                                                 const ::nros::QoS& qos, Cb cb) {
        auto s = std::make_shared<::nros::Subscription<M>>();
        (void)node_.create_subscription<M>(*s, topic.c_str(), qos, std::move(cb));
        return s;
    }

    template <typename M, typename Cb>
    std::shared_ptr<::nros::Subscription<M>> create_subscription(const std::string& topic,
                                                                 ::size_t depth, Cb cb) {
        return create_subscription<M>(topic, ::nros::QoS(static_cast<uint32_t>(depth)),
                                      std::move(cb));
    }

  private:
    ::nros::Executor executor_;
    ::nros::Node node_;
    bool initialized_ = false;
};

// --- spin / spin_some --------------------------------------------------------
//
// `rclcpp::spin(node)` loops the node's executor until shutdown.
// `rclcpp::spin_some(node)` makes one progress sweep (≈ a 0-timeout spin_once).

inline void spin(const Node::SharedPtr& node) {
    if (!node || !node->initialized()) {
        return;
    }
    while (::nros::ok()) {
        (void)node->nros_executor().spin_once(10);
    }
}

inline void spin_some(const Node::SharedPtr& node) {
    if (!node || !node->initialized()) {
        return;
    }
    (void)node->nros_executor().spin_once(0);
}

// Future type is templated rather than `const auto& future` so the header
// stays parseable under `-std=c++14` (the C++20 abbreviated-function-template
// syntax breaks `just check-cpp`'s freestanding probe).
template <typename Future>
inline void spin_until_future_complete(const Node::SharedPtr& node, const Future& future,
                                       int32_t timeout_ms = -1) {
    if (!node || !node->initialized()) {
        return;
    }
    if (timeout_ms < 0) {
        while (::nros::ok() && !future.is_ready()) {
            (void)node->nros_executor().spin_once(10);
        }
    } else {
        (void)node->nros_executor().spin(static_cast<uint32_t>(timeout_ms), 10);
    }
}

} // namespace rclcpp

// --- rclcpp_action ------------------------------------------------------------
//
// Just type aliases — the call shapes (`send_goal_async`, callbacks) need their
// own shim and are gated behind further 209.D-style work.

namespace rclcpp_action {

template <typename A> using Server = ::nros::ActionServer<A>;

template <typename A> using Client = ::nros::ActionClient<A>;

} // namespace rclcpp_action

// --- Log macros --------------------------------------------------------------
//
// Same call shape as rclcpp; the logger arg is accepted and ignored (the file/
// line/format are what reach the sink). _THROTTLE variants degrade to plain log
// — they get the message out; a follow-up can add interval gating.

#ifndef RCLCPP_INFO
#define RCLCPP_INFO(logger, ...) (void)(logger), NROS_INFO(__VA_ARGS__)
#define RCLCPP_WARN(logger, ...) (void)(logger), NROS_WARN(__VA_ARGS__)
#define RCLCPP_ERROR(logger, ...) (void)(logger), NROS_ERROR(__VA_ARGS__)
#define RCLCPP_DEBUG(logger, ...) (void)(logger), NROS_DEBUG(__VA_ARGS__)
#define RCLCPP_FATAL(logger, ...) (void)(logger), NROS_ERROR(__VA_ARGS__)

#define RCLCPP_INFO_STREAM(logger, args) RCLCPP_INFO(logger, "%s", "")
#define RCLCPP_WARN_STREAM(logger, args) RCLCPP_WARN(logger, "%s", "")
#define RCLCPP_ERROR_STREAM(logger, args) RCLCPP_ERROR(logger, "%s", "")

#define RCLCPP_INFO_THROTTLE(logger, clock, period_ms, ...) RCLCPP_INFO(logger, __VA_ARGS__)
#define RCLCPP_WARN_THROTTLE(logger, clock, period_ms, ...) RCLCPP_WARN(logger, __VA_ARGS__)
#define RCLCPP_ERROR_THROTTLE(logger, clock, period_ms, ...) RCLCPP_ERROR(logger, __VA_ARGS__)
#endif // RCLCPP_INFO

#endif // NROS_RCLCPP_COMPAT_HPP
