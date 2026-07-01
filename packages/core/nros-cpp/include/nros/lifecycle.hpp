// nros-cpp: LifecycleNode API (REP-2002 managed nodes)
// Freestanding C++ — no exceptions, no RTTI, no STL required.
//
// Phase 270 (#103) — an rclcpp-shape managed-node wrapper over the executor's
// REP-2002 lifecycle state machine. Inherit `nros::LifecycleNode` and override
// the `on_*` transition hooks (matching
// `rclcpp_lifecycle::node_interfaces::LifecycleNodeInterface`); the base binds
// the REP-2002 services and bridges each transition to your override. The C
// state machine (`nros_executor_lifecycle_*`) does all the work — this class is
// a thin, allocation-free wrapper (RFC-0019).

/**
 * @file lifecycle.hpp
 * @ingroup grp_lifecycle
 * @brief Phase 270 — `nros::LifecycleNode` (REP-2002 managed node).
 */

#ifndef NROS_CPP_LIFECYCLE_HPP
#define NROS_CPP_LIFECYCLE_HPP

#include <cstdint>

#include "nros/result.hpp"

#include "nros_cpp_ffi.h" // lifecycle FFI: register_lifecycle_services / get_state /
                          // change_state / autostart / register_on_* (+ the
                          // nros_cpp_lifecycle_callback_t typedef), all cbindgen-generated
                          // from nros-cpp/src/lifecycle_shim.rs.

namespace nros {

/// REP-2002 primary states. Mirrors `nros_cpp_lifecycle_get_state()`.
enum class LifecycleState : uint8_t {
    Unconfigured = 0,
    Inactive = 1,
    Active = 2,
    Finalized = 3,
};

/// Transition-callback outcome (rclcpp `CallbackReturn` shape). `Failure` rolls
/// the transition back; `Error` routes to the error-processing transition.
enum class CallbackReturn : uint8_t {
    Success = 0,
    Failure = 1,
    Error = 2,
};

/// rclcpp-shape managed node (REP-2002).
///
/// Inherit and override the `on_*` hooks, then call `register_services()` (or
/// `autostart()`). Transitions are driven either externally (`ros2 lifecycle set`
/// against the registered services) or programmatically (`configure()`,
/// `activate()`, …). The `previous` argument to each hook is the state being left.
///
/// Freestanding-safe: the virtuals are non-pure with defaults (no
/// `__cxa_pure_virtual`), and the class uses no exceptions / RTTI / heap.
class LifecycleNode {
  public:
    /// @param executor_handle Raw executor handle from `Executor::handle()`.
    explicit LifecycleNode(void* executor_handle) : exec_(executor_handle) {}
    virtual ~LifecycleNode() = default;

    LifecycleNode(const LifecycleNode&) = delete;
    LifecycleNode& operator=(const LifecycleNode&) = delete;

    // Transition hooks — override the ones you need. Defaults: Success
    // (on_error: Failure), matching rclcpp.
    virtual CallbackReturn on_configure(LifecycleState previous) {
        (void)previous;
        return CallbackReturn::Success;
    }
    virtual CallbackReturn on_activate(LifecycleState previous) {
        (void)previous;
        return CallbackReturn::Success;
    }
    virtual CallbackReturn on_deactivate(LifecycleState previous) {
        (void)previous;
        return CallbackReturn::Success;
    }
    virtual CallbackReturn on_cleanup(LifecycleState previous) {
        (void)previous;
        return CallbackReturn::Success;
    }
    virtual CallbackReturn on_shutdown(LifecycleState previous) {
        (void)previous;
        return CallbackReturn::Success;
    }
    virtual CallbackReturn on_error(LifecycleState previous) {
        (void)previous;
        return CallbackReturn::Failure;
    }

    /// Register the five REP-2002 services and bind the `on_*` trampolines. Call
    /// once during setup; afterwards `ros2 lifecycle set|get|list` drives this node.
    Result register_services() {
        Result r = Result(nros_cpp_register_lifecycle_services(exec_));
        if (!r) {
            return r;
        }
        nros_cpp_lifecycle_register_on_configure(exec_, &LifecycleNode::tramp_configure, this);
        nros_cpp_lifecycle_register_on_activate(exec_, &LifecycleNode::tramp_activate, this);
        nros_cpp_lifecycle_register_on_deactivate(exec_, &LifecycleNode::tramp_deactivate, this);
        nros_cpp_lifecycle_register_on_cleanup(exec_, &LifecycleNode::tramp_cleanup, this);
        nros_cpp_lifecycle_register_on_shutdown(exec_, &LifecycleNode::tramp_shutdown, this);
        nros_cpp_lifecycle_register_on_error(exec_, &LifecycleNode::tramp_error, this);
        return Result();
    }

    /// Register services (binding the `on_*` trampolines) then drive the node to
    /// `target` at boot: `Inactive` = configure; `Active` = configure + activate.
    /// Unlike the raw `nros_cpp_lifecycle_autostart` FFI, this binds the callbacks
    /// first, so your overrides fire during the autostart transitions.
    Result autostart(LifecycleState target) {
        Result r = register_services();
        if (!r) {
            return r;
        }
        if (target == LifecycleState::Inactive || target == LifecycleState::Active) {
            r = configure();
            if (!r) {
                return r;
            }
        }
        if (target == LifecycleState::Active) {
            r = activate();
            if (!r) {
                return r;
            }
        }
        return Result();
    }

    /// Current REP-2002 state.
    LifecycleState get_state() const {
        return static_cast<LifecycleState>(nros_cpp_lifecycle_get_state(exec_));
    }

    // Programmatic transitions (REP-2002 transition ids).
    Result configure() { return trigger(1); }
    Result activate() { return trigger(2); }
    Result deactivate() { return trigger(3); }
    Result cleanup() { return trigger(4); }
    Result shutdown() { return trigger(5); }

  protected:
    Result trigger(uint8_t transition_id) {
        return Result(nros_cpp_lifecycle_change_state(exec_, transition_id));
    }
    void* exec_;

  private:
    // Trampolines: `previous` = get_state() at callback entry (the SM invokes the
    // callback before committing the new state), then dispatch to the virtual.
    static uint8_t tramp_configure(void* self) {
        auto* n = static_cast<LifecycleNode*>(self);
        return static_cast<uint8_t>(n->on_configure(n->get_state()));
    }
    static uint8_t tramp_activate(void* self) {
        auto* n = static_cast<LifecycleNode*>(self);
        return static_cast<uint8_t>(n->on_activate(n->get_state()));
    }
    static uint8_t tramp_deactivate(void* self) {
        auto* n = static_cast<LifecycleNode*>(self);
        return static_cast<uint8_t>(n->on_deactivate(n->get_state()));
    }
    static uint8_t tramp_cleanup(void* self) {
        auto* n = static_cast<LifecycleNode*>(self);
        return static_cast<uint8_t>(n->on_cleanup(n->get_state()));
    }
    static uint8_t tramp_shutdown(void* self) {
        auto* n = static_cast<LifecycleNode*>(self);
        return static_cast<uint8_t>(n->on_shutdown(n->get_state()));
    }
    static uint8_t tramp_error(void* self) {
        auto* n = static_cast<LifecycleNode*>(self);
        return static_cast<uint8_t>(n->on_error(n->get_state()));
    }
};

} // namespace nros

#endif // NROS_CPP_LIFECYCLE_HPP
