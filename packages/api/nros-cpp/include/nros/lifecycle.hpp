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

#include <cstddef>
#include <cstdint>

#include "nros/result.hpp"
// phase-417 stage 2b (RFC-0089) — `nros::TopicEndpointInfo` and the visitor
// typedef used by the graph forwarders below.
#include "nros/graph.hpp"
// phase-417 W4.f — `rclcpp_lifecycle::LifecycleNode` carries the same clock and
// the same parameter surface as `rclcpp::Node`, so this header reaches both.
#include "nros/clock.hpp"
#include "nros/node_parameters.hpp"
#include "nros/publisher.hpp"
// `node.hpp` for `rclcpp::Node` ITSELF, which the parameter forwarders call and
// `bind(Node&)` takes. A forward declaration would have been enough to DECLARE
// them and is not enough to write them: the hosted `std::string` overloads are
// guarded on `NROS_CPP_HAS_STD_STRING` (phase-476 W3), and `node.hpp`'s includes
// are what define it here exactly as they do there. A guard that answered
// differently in this header than in `node.hpp` would give a lifecycle node a
// different parameter surface per translation unit.
//
// No cycle: `node.hpp` does not include this file and never has. The include is
// one-way and `nros.hpp` already pulls both, in this order.
#include "nros/node.hpp"

#include "nros_cpp_ffi.h" // lifecycle FFI: register_lifecycle_services / get_current_state /
                          // change_state / autostart / register_on_* (+ the
                          // nros_cpp_lifecycle_callback_t typedef), all cbindgen-generated
                          // from nros-cpp/src/lifecycle_shim.rs.
// phase-417 W4.f — `nros_lifecycle_transition_{label,start_state,goal_state}`
// and `nros_lifecycle_transition_graph`, the in-process view of the REP-2002
// table. They are PURE functions over `nros_core::lifecycle`, so `Transition`
// below stores an id and nothing else: the labels and the two states have ONE
// home, in Rust, and a C++ table of them would be the fourth copy.
#include "nros/lifecycle.h"

namespace nros {

/// REP-2002 primary states. Values match `nros_cpp_lifecycle_get_current_state()` /
/// `nros_core::lifecycle::LifecycleState` (`Unknown` is the `0` sentinel returned for a
/// null executor or before services are registered).
///
/// The four primary states carry their `lifecycle_msgs/msg/State` ids.
/// `ErrorProcessing` deliberately does not: upstream spells it
/// `TRANSITION_STATE_ERRORPROCESSING = 15`, but `5` is UNASSIGNED upstream, so
/// unlike the transition ids (issue 1099) it can never silently name a
/// different state. The wire mapping in `nros-node` sends 15.
enum class LifecycleState : uint8_t {
    Unknown = 0,
    Unconfigured = 1,
    Inactive = 2,
    Active = 3,
    Finalized = 4,
    ErrorProcessing = 5,
};

/// REP-2002 transition ids — the values of `lifecycle_msgs/msg/Transition`.
///
/// Issue 1099: these ids used to be nano-ros's own numbering, which collided
/// with upstream's on four of eight values (ours had `Activate = 2`, upstream
/// has `CLEANUP = 2`). A ported rclcpp file calling
/// `trigger_transition(lifecycle_msgs::msg::Transition::TRANSITION_CLEANUP)`
/// — i.e. `2` — on an Inactive node was silently ACTIVATING it and getting
/// `Result::ok` back. The numbering is now upstream's everywhere: here, in
/// `NROS_LIFECYCLE_TRANSITION_*`, in `nros_core::lifecycle`, and on the wire.
///
/// Prefer these names over the raw byte; `trigger_transition` accepts either.
enum class LifecycleTransition : uint8_t {
    Configure = 1,
    Cleanup = 2,
    Activate = 3,
    Deactivate = 4,
    /// Shutdown from `Unconfigured`. `LifecycleNode::shutdown()` picks the
    /// right one of the three for you.
    ShutdownUnconfigured = 5,
    /// Shutdown from `Inactive`.
    ShutdownInactive = 6,
    /// Shutdown from `Active`.
    ShutdownActive = 7,
    /// `ErrorProcessing` -> `Unconfigured`. Upstream models error recovery as
    /// an implicit transition and gives its success edge id `60`; upstream `8`
    /// is `TRANSITION_DESTROY`, which nano-ros does not implement and which
    /// `trigger_transition` rejects.
    ErrorRecovery = 60,
};

/// Transition-callback outcome (rclcpp `CallbackReturn` shape). `Failure` rolls
/// the transition back; `Error` routes to the error-processing transition.
enum class CallbackReturn : uint8_t {
    Success = 0,
    Failure = 1,
    Error = 2,
};

/// The shutdown transition legal from `state`, or `ShutdownUnconfigured` when
/// none is (`Unknown`, `Finalized`, `ErrorProcessing`).
///
/// REP-2002 gives shutdown three ids, one per legal source state, so no fixed
/// id can shut down a node in an arbitrary state — which is why
/// `LifecycleNode::shutdown()` sending `5` unconditionally could not shut down
/// an Inactive or Active node (issue 1099).
///
/// The no-legal-shutdown states fall back to `ShutdownUnconfigured` on purpose:
/// the state machine already refuses it from those states, so the error comes
/// from the ONE place that owns the rule instead of being re-derived here.
///
/// `constexpr` so a test can pin every arm without building an executor.
constexpr LifecycleTransition shutdown_transition_for(LifecycleState state) {
    return state == LifecycleState::Inactive ? LifecycleTransition::ShutdownInactive
           : state == LifecycleState::Active ? LifecycleTransition::ShutdownActive
                                             : LifecycleTransition::ShutdownUnconfigured;
}

/// One row of the REP-2002 transition graph — rclcpp's
/// `rclcpp_lifecycle::Transition`.
///
/// phase-417 W4.f. Before this, C++ had `enum class LifecycleTransition` (issue
/// 1099) and nothing else: the LABEL and the two STATES that upstream's object
/// carries were served over `~/get_transition_graph` to remote peers and were
/// unreadable to the node's own code. An enum is not that object, which is why
/// the `cpp:Transition` ledger row stayed a gap after 1099 closed the id half.
///
/// **It stores an id and nothing else.** Every accessor calls the pure C
/// function over `nros_core::lifecycle`, so the eight labels and sixteen states
/// have exactly one home — the one `~/get_transition_graph` reads. A `constexpr`
/// table of them in this header would be a fourth copy of `lifecycle_msgs`, and
/// the three that already exist are the reason issue 1099 happened.
///
/// Two deliberate differences from rclcpp, both from the same constraint:
///
///  * `start_state()` / `goal_state()` return [`LifecycleState`], not a
///    `rclcpp_lifecycle::State` object. Upstream's `State` owns an `rcl_state_t`
///    and an allocator; ours is the enum plus [`state_label`], which is the
///    same two fields with no owner.
///  * `label()` returns a BORROWED `const char*` with static lifetime rather
///    than a `std::string`. Nothing is allocated and nothing is freed.
///
/// rclcpp spells the source state `start_state`; `nros_core`'s Rust spells it
/// `source_state`. This takes rclcpp's, because a ported file writes it — and
/// a third spelling is what the ledger row warned against.
class Transition {
  public:
    /// The transition with this `lifecycle_msgs/msg/Transition` id.
    constexpr explicit Transition(uint8_t transition_id) : id_(transition_id) {}
    /// The transition named by the enum — the spelling to prefer.
    constexpr explicit Transition(LifecycleTransition transition)
        : id_(static_cast<uint8_t>(transition)) {}

    /// The `lifecycle_msgs/msg/Transition` id — rclcpp's `Transition::id()`.
    constexpr uint8_t id() const { return id_; }

    /// The same id as the enum. Unimplemented ids (`0` = CREATE, `8` =
    /// DESTROY) have no enumerator, so read [`id`] and compare if you need to
    /// tell those apart; [`valid`] is the question that usually means.
    constexpr LifecycleTransition transition() const {
        return static_cast<LifecycleTransition>(id_);
    }

    /// Whether this id is a transition nano-ros implements. `false` for `0`,
    /// `8` and anything outside the table, for which the accessors below
    /// answer with `nullptr` / `LifecycleState::Unknown` rather than a
    /// neighbouring row.
    bool valid() const { return nros_lifecycle_transition_label(id_) != nullptr; }

    /// The wire label — `"configure"`, `"shutdown"`, … — or `nullptr` for an
    /// id we do not implement. Borrowed, static lifetime, never freed.
    ///
    /// The three shutdown ids all read `"shutdown"`: that is upstream's
    /// spelling, and which one a `"shutdown"` request means is resolved
    /// against the CURRENT state (see [`shutdown_transition_for`]).
    const char* label() const { return nros_lifecycle_transition_label(id_); }

    /// The state this transition may be taken FROM — rclcpp's
    /// `Transition::start_state()`.
    LifecycleState start_state() const {
        return static_cast<LifecycleState>(nros_lifecycle_transition_start_state(id_));
    }

    /// The state this transition ADVERTISES as its destination — rclcpp's
    /// `Transition::goal_state()`.
    ///
    /// Where a SUCCEEDING callback lands. A failing one rolls back, and an
    /// erroring one routes to `ErrorProcessing`; both are runtime outcomes,
    /// not properties of the graph this describes.
    LifecycleState goal_state() const {
        return static_cast<LifecycleState>(nros_lifecycle_transition_goal_state(id_));
    }

  private:
    uint8_t id_;
};

/// One visit per row of the transition graph; return `false` to stop. Same
/// shape as `graph.hpp`'s visitors, because it answers the same kind of
/// question with the same constraint.
using TransitionVisitFn = bool (*)(void* ctx, const Transition& transition);

/// The `lifecycle_msgs/msg/State.label` for `state` — `"active"`,
/// `"unconfigured"`, … — or `nullptr` for `LifecycleState::Unknown`.
///
/// The free form, which [`State::label`] below calls. phase-417 W4.f wrote
/// this one and stopped here, on the argument that "the only thing upstream's
/// `State` adds to the enum is this string, so a whole type to carry it would
/// be a wrapper over one accessor". G6 disagrees with that argument for the
/// reason the sibling already demonstrates: [`Transition`] carries `label()`
/// as a METHOD fifty lines above, so a ported `state.label()` compiled and a
/// ported `transition.label()` did too — the pair was half applied, not
/// deliberately asymmetric. Both spellings stay; this is the implementation
/// and there is no second table.
inline const char* state_label(LifecycleState state) {
    return nros_lifecycle_state_label(static_cast<uint8_t>(state));
}

/// One REP-2002 primary state as a value — rclcpp's `rclcpp_lifecycle::State`.
///
/// phase-417 G6, and it is a SURFACING job rather than a feature: every byte
/// it returns already shipped (`nros_lifecycle_state_label`, W4.f), and what
/// was missing was the RECEIVER. Upstream's `State` is a value type with
/// `.id()` and `.label()`; ours was a bare enum plus a free function, so a
/// ported `state.label()` did not compile while the sibling
/// `transition.label()` did.
///
/// **It stores an id and nothing else**, exactly like [`Transition`]. The
/// accessor calls the pure C function over `nros_core::lifecycle`, so the five
/// labels have one home — the one `~/get_transition_graph` reads — and a
/// `constexpr` table of them in this header would be a further copy of
/// `lifecycle_msgs`, which is what issue 1099 was.
///
/// Two deliberate differences from rclcpp, both the ones the header already
/// records for `Transition`:
///
///  * upstream's `State` owns an `rcl_lifecycle_state_t` AND an
///    `rcl_allocator_t`, because its label is a heap string it must free.
///    There is no allocator here (RFC-0022), so there is nothing to own: this
///    is one `uint8_t`.
///  * [`label`] returns a BORROWED `const char*` with static lifetime rather
///    than a `std::string`. Nothing is allocated and nothing is freed.
class State {
  public:
    /// The state with this `lifecycle_msgs/msg/State` id.
    constexpr explicit State(uint8_t state_id) : id_(state_id) {}
    /// The state named by the enum — the spelling to prefer.
    constexpr explicit State(LifecycleState state) : id_(static_cast<uint8_t>(state)) {}

    /// The `lifecycle_msgs/msg/State` id — rclcpp's `State::id()`.
    constexpr uint8_t id() const { return id_; }

    /// The same id as the enum. `ErrorProcessing` is the one to read
    /// carefully: we carry `5`, upstream sends `15` on the wire, and the
    /// mapping lives in `nros-node` — see [`LifecycleState`].
    constexpr LifecycleState state() const { return static_cast<LifecycleState>(id_); }

    /// Whether this id is a state nano-ros implements. `false` for anything
    /// outside the table, for which [`label`] answers `nullptr` rather than a
    /// neighbouring row.
    bool valid() const { return nros_lifecycle_state_label(id_) != nullptr; }

    /// The wire label — `"unconfigured"`, `"active"`, … — or `nullptr` for an
    /// id we do not implement. Borrowed, static lifetime, never freed.
    const char* label() const { return nros_lifecycle_state_label(id_); }

  private:
    uint8_t id_;
};

/// An entity whose sends follow the node's REP-2002 state — rclcpp's
/// `rclcpp_lifecycle::node_interfaces::ManagedEntityInterface`.
///
/// phase-417 W4.f. We had lifecycle state and no entity that observed it, so a
/// deactivated node's publishers kept publishing — the half of REP-2002 that is
/// about what a node SENDS rather than what it IS.
///
/// **Registration is an intrusive list, which is the divergence.** rclcpp holds
/// `std::vector<std::weak_ptr<ManagedEntityInterface>>` on the node; we have no
/// allocator and no `shared_ptr` on a freestanding target, and a fixed array
/// would put a capacity into a class layout (`check-cpp-capability-layout`
/// forbids a build knob deciding a `sizeof`). So the link lives in the ENTITY:
/// one pointer each, no capacity, nothing to exhaust and nothing to size.
///
/// The cost of that choice, stated: an entity must outlive the node it is added
/// to, and must not be added to two nodes. Both hold for the storage this API
/// has — entities are members of the node's own class or file-scope statics.
namespace detail {
class LifecycleEngine;
} // namespace detail

class ManagedEntityInterface {
  public:
    ManagedEntityInterface() = default;
    /// An entity that is linked into a node's list unlinks itself when it
    /// dies, so destroying one before its node leaves no dangling pointer.
    virtual ~ManagedEntityInterface();

    /// phase-482 W4 — MOVABLE, and a move keeps the node's list correct.
    ///
    /// `rclcpp_lifecycle::LifecycleNode::create_publisher` returns its
    /// publisher in a move-only `nros::Owned<T>` (the freestanding `SharedPtr`),
    /// so the entity changes address at least once between registration and
    /// the member it lands in. The list is intrusive, so a move re-points the
    /// node's link at the new object and leaves the old one unlinked. Copying
    /// would put one entity in the list twice and stays deleted.
    ManagedEntityInterface(ManagedEntityInterface&& other) noexcept;
    ManagedEntityInterface& operator=(ManagedEntityInterface&& other) noexcept;
    ManagedEntityInterface(const ManagedEntityInterface&) = delete;
    ManagedEntityInterface& operator=(const ManagedEntityInterface&) = delete;

    /// The node reached `Active`. Non-pure with a default so a freestanding
    /// build needs no `__cxa_pure_virtual`.
    virtual void on_activate() {}
    /// The node left `Active`.
    virtual void on_deactivate() {}
    /// Whether this entity is currently allowed to send.
    virtual bool is_activated() const { return false; }

  private:
    friend class detail::LifecycleEngine;
    /// Intrusive link; owned by the node's list, never by the entity.
    ManagedEntityInterface* next_managed_ = nullptr;
    /// The node whose list this entity is in, or `nullptr`. What lets a move
    /// or a destructor find the link to repair.
    detail::LifecycleEngine* owner_ = nullptr;
};

/// The default `ManagedEntityInterface` — a flag, as upstream's
/// `SimpleManagedEntity` is.
class SimpleManagedEntity : public ManagedEntityInterface {
  public:
    SimpleManagedEntity() = default;
    SimpleManagedEntity(SimpleManagedEntity&&) noexcept = default;
    SimpleManagedEntity& operator=(SimpleManagedEntity&&) noexcept = default;

    void on_activate() override { activated_ = true; }
    void on_deactivate() override { activated_ = false; }
    bool is_activated() const override { return activated_; }

  private:
    bool activated_ = false;
};

namespace detail {

/// The REP-2002 engine behind `rclcpp_lifecycle::LifecycleNode` (phase-482 W4).
///
/// It was `nros::LifecycleNode`, a mixin a component bound to its node. That name
/// is now a deprecated forwarder; the type a user writes is
/// `rclcpp_lifecycle::LifecycleNode`, which IS a node and owns one of these.
///
/// (Original description:) rclcpp-shape managed node (REP-2002).
///
/// Inherit and override the `on_*` hooks, then call `register_services()` (or
/// `autostart()`). Transitions are driven either externally (`ros2 lifecycle set`
/// against the registered services) or programmatically (`configure()`,
/// `activate()`, …). The `previous` argument to each hook is the state being left.
///
/// Freestanding-safe: the virtuals are non-pure with defaults (no
/// `__cxa_pure_virtual`), and the class uses no exceptions / RTTI / heap.
class LifecycleEngine {
  public:
    /// @param executor_handle Raw executor handle from `Executor::handle()`.
    explicit LifecycleEngine(void* executor_handle) : exec_(executor_handle) {}

    /// Two-phase construction for the component model: nano-ros components are
    /// constructed before the executor handle exists, so a component that inherits
    /// `LifecycleEngine` default-constructs here and calls `bind()` from its install
    /// hook (`configure(Node&)`, where `node.executor_handle()` is available) before
    /// `register_services()`. Until bound, `get_current_state()` reads `Unconfigured` and the
    /// register/transition calls return `InvalidArgument` rather than trapping.
    LifecycleEngine() : exec_(nullptr) {}

    /// Unlinks every managed entity still in the list, so an entity that
    /// outlives its node is left unregistered rather than holding a pointer
    /// into a destroyed list.
    virtual ~LifecycleEngine() {
        ManagedEntityInterface* e = managed_;
        while (e != nullptr) {
            ManagedEntityInterface* next = e->next_managed_;
            e->next_managed_ = nullptr;
            e->owner_ = nullptr;
            e = next;
        }
        managed_ = nullptr;
    }

    LifecycleEngine(const LifecycleEngine&) = delete;
    LifecycleEngine& operator=(const LifecycleEngine&) = delete;

    /// Bind the executor handle for a default-constructed node (two-phase init).
    /// Call once, before `register_services()` / `autostart()`.
    ///
    /// This overload binds the EXECUTOR only, so the parameter surface below
    /// has no node to key on and answers `InvalidArgument`. Prefer
    /// `bind(Node&)`, which binds both.
    void bind(void* executor_handle) { exec_ = executor_handle; }

    /// Bind the executor AND the node this managed node's parameters belong to
    /// — phase-417 W4.f.
    ///
    /// `rclcpp_lifecycle::LifecycleEngine` IS a node upstream and carries the
    /// whole `rclcpp::Node` parameter surface. Ours is a MIXIN over the
    /// executor's REP-2002 state machine and holds no node handle, which is
    /// exactly why the `cpp:LifecycleEngine::get_clock` and
    /// `cpp:LifecycleEngine::*parameter*` ledger rows stayed open: there was
    /// nothing for the accessor to reach.
    ///
    /// Binding rather than OWNING is the answer, and the reason is phase-426's:
    /// a node here is a separate object with its own entity storage and its own
    /// row in the executor's table, so a `LifecycleEngine` that constructed one
    /// would be a second node for one logical node. The parameter methods below
    /// forward to THIS node — the same `rclcpp::Node` methods, so the store, the
    /// declared-parameter contract check and the node keying are reached through
    /// one implementation rather than copied into a second.
    ///
    /// Call it from the component install hook, where `configure(Node&)` hands
    /// you the node, and before `register_services()` / `autostart()`.
    void bind(::rclcpp::Node& node) {
        exec_ = node.executor_handle();
        node_ = &node;
    }

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

    // ---- Transition callbacks WITHOUT subclassing (phase-417 W4.f) --------
    //
    // rclcpp lets a user register a transition callback instead of overriding a
    // virtual, and all three of our languages had that capability except this
    // one: C has `nros_lifecycle_register_on_*` and
    // `nros_executor_lifecycle_register_on_*`, Rust has
    // `LifecyclePollingNode::register_on_*` and the `LifecycleCallbacks` trait,
    // and C++ exposed only the overrides — even though `register_services()`
    // already calls `nros_cpp_lifecycle_register_on_*` one layer down. Six
    // ledger rows for one missing surface.
    //
    // WHAT IT IS NOT: a `std::function`. rclcpp takes one because it allocates;
    // storing six here would put six `std::function`s in this class, which a
    // freestanding target cannot have AND which would make the class layout
    // depend on a capability probe — the `check-cpp-capability-layout` rule.
    // So it is a function pointer plus a context, which is the shape every
    // other callback seam in this API already uses.
    //
    // WHERE IT IS STORED: in this class, not at the FFI. The trampolines below
    // dispatch to the registered callback when there is one and to the virtual
    // otherwise, so registration works BEFORE or AFTER `register_services()`
    // and there is still exactly one registrant at the C seam. Registering at
    // the FFI directly would make the two orders behave differently — register
    // first and `register_services()` would silently overwrite you.

    /// A transition callback: `CallbackReturn (*)(LifecycleState previous, void* context)`.
    typedef CallbackReturn (*TransitionCallbackType)(LifecycleState previous, void* context);

    /// Run `callback` instead of `on_configure` — rclcpp's
    /// `register_on_configure`. `nullptr` restores the virtual.
    void register_on_configure(TransitionCallbackType callback, void* context = nullptr) {
        cb_configure_ = callback;
        ctx_configure_ = context;
    }
    /// Run `callback` instead of `on_activate`. See [`register_on_configure`].
    void register_on_activate(TransitionCallbackType callback, void* context = nullptr) {
        cb_activate_ = callback;
        ctx_activate_ = context;
    }
    /// Run `callback` instead of `on_deactivate`. See [`register_on_configure`].
    void register_on_deactivate(TransitionCallbackType callback, void* context = nullptr) {
        cb_deactivate_ = callback;
        ctx_deactivate_ = context;
    }
    /// Run `callback` instead of `on_cleanup`. See [`register_on_configure`].
    void register_on_cleanup(TransitionCallbackType callback, void* context = nullptr) {
        cb_cleanup_ = callback;
        ctx_cleanup_ = context;
    }
    /// Run `callback` instead of `on_shutdown`. See [`register_on_configure`].
    void register_on_shutdown(TransitionCallbackType callback, void* context = nullptr) {
        cb_shutdown_ = callback;
        ctx_shutdown_ = context;
    }
    /// Run `callback` instead of `on_error`. See [`register_on_configure`].
    void register_on_error(TransitionCallbackType callback, void* context = nullptr) {
        cb_error_ = callback;
        ctx_error_ = context;
    }

    /// Register the five REP-2002 services and bind the `on_*` trampolines. Call
    /// once during setup; afterwards `ros2 lifecycle set|get|list` drives this node.
    Result register_services() {
        Result r = Result(nros_cpp_register_lifecycle_services(exec_));
        if (!r) {
            return r;
        }
        nros_cpp_lifecycle_register_on_configure(exec_, &LifecycleEngine::tramp_configure, this);
        nros_cpp_lifecycle_register_on_activate(exec_, &LifecycleEngine::tramp_activate, this);
        nros_cpp_lifecycle_register_on_deactivate(exec_, &LifecycleEngine::tramp_deactivate, this);
        nros_cpp_lifecycle_register_on_cleanup(exec_, &LifecycleEngine::tramp_cleanup, this);
        nros_cpp_lifecycle_register_on_shutdown(exec_, &LifecycleEngine::tramp_shutdown, this);
        nros_cpp_lifecycle_register_on_error(exec_, &LifecycleEngine::tramp_error, this);
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
    LifecycleState get_current_state() const {
        return static_cast<LifecycleState>(nros_cpp_lifecycle_get_current_state(exec_));
    }

    // Programmatic transitions. Spelled with the enum, never a literal —
    // issue 1099 is what a literal costs.
    Result configure() { return trigger_transition(LifecycleTransition::Configure); }
    Result activate() { return trigger_transition(LifecycleTransition::Activate); }
    Result deactivate() { return trigger_transition(LifecycleTransition::Deactivate); }
    Result cleanup() { return trigger_transition(LifecycleTransition::Cleanup); }

    /// Shut the node down from WHEREVER it currently is.
    ///
    /// REP-2002 gives shutdown three ids, one per legal source state, so a
    /// fixed id can only ever shut down a node in one state. This used to send
    /// `5` (`ShutdownUnconfigured`) unconditionally, which meant `shutdown()`
    /// could not shut down an Inactive or Active node — the two states a node
    /// that has done any work is actually in — and returned an
    /// invalid-transition error instead (issue 1099).
    ///
    /// Resolution matches `nros_node::lifecycle`'s own `shutdown()` and the
    /// generic `"shutdown"` label the `ChangeState` service accepts, so the
    /// C++ API, the Rust API and `ros2 lifecycle set <node> shutdown` now agree
    /// about the same node.
    ///
    /// The mapping is [`shutdown_transition_for`]; see it for the
    /// no-legal-shutdown states.
    Result shutdown() { return trigger_transition(shutdown_transition_for(get_current_state())); }

    /// Drive an arbitrary REP-2002 transition — the type-safe spelling, and the
    /// one to prefer.
    Result trigger_transition(LifecycleTransition transition) {
        return trigger_transition(static_cast<uint8_t>(transition));
    }

    /// Drive an arbitrary REP-2002 transition by id — rclcpp's
    /// `LifecycleEngine::trigger_transition(uint8_t)`, and PUBLIC for the same
    /// reason: it is how a ported node reaches a transition that has no named
    /// helper above. The four helpers are this call with the id filled in.
    ///
    /// `transition_id` is a `lifecycle_msgs/msg/Transition` id — the SAME
    /// number rclcpp means (issue 1099; it was nano-ros's own numbering before,
    /// which disagreed with upstream on four of eight values and made this
    /// call silently do the wrong transition). `0` (`TRANSITION_CREATE`) and
    /// `8` (`TRANSITION_DESTROY`) are not implemented and return
    /// `InvalidArgument` rather than aliasing onto a transition we do have.
    ///
    /// Phase 379 W5: this was the `protected` `trigger(uint8_t)`, which a
    /// ported rclcpp node could not call at all.
    Result trigger_transition(uint8_t transition_id) {
        return Result(nros_cpp_lifecycle_change_state(exec_, transition_id));
    }

    // ---- Clock — phase-417 W4.f ------------------------------------------

    /// This node's clock — rclcpp's `LifecycleEngine::get_clock()`.
    ///
    /// ROS time, as `rclcpp::Node`'s is, and a member of this class rather than
    /// a forward to a bound node: a `Clock` owns no handle and no allocation
    /// (see `clock.hpp`), the ROS-time override is process-global, and an
    /// UNBOUND lifecycle node can still ask what time it is. So the accessor
    /// works with or without `bind(Node&)`, which the parameter surface below
    /// cannot say.
    Clock* get_clock() { return &clock_; }
    /// Const overload of [`get_clock`].
    const Clock* get_clock() const { return &clock_; }

    /// Shorthand for `get_clock()->now()` — rclcpp's `LifecycleEngine::now()`.
    Time now() const { return clock_.now(); }

    // ---- The transition graph, in process — phase-417 W4.f ---------------

    /// Every transition in this node's REP-2002 state machine — rclcpp's
    /// `LifecycleEngine::get_transition_graph()`. Return `false` from `visit` to
    /// stop.
    ///
    /// The SAME eight rows `~/get_transition_graph` serves a remote
    /// `ros2 lifecycle list`, read from the one table in `nros_core`, so a node
    /// and a peer cannot disagree about it. State-INDEPENDENT: it is the whole
    /// graph, not the transitions currently reachable.
    ///
    /// Upstream returns `std::vector<Transition>`; this VISITS, for the same
    /// reason every graph query on this class does — there is no allocator to
    /// hand a container back from, and the visitor is the vocabulary those
    /// forwarders already established.
    ///
    /// NOT an `nros::Span` over the static table, which is the shape that fits
    /// best and is nonetheless wrong here: `span.hpp` is reached by nothing
    /// else in these headers, so taking it would newly expose `Span` /
    /// `StringView` / `LeSpan` on the public C++ surface as a side effect of a
    /// lifecycle accessor. `graph.hpp` and `options.hpp` each declined it for
    /// exactly that reason (16 unledgered items, measured), and whoever
    /// classifies those types should do it deliberately rather than inherit it
    /// from this call.
    Result get_transition_graph(TransitionVisitFn visit, void* ctx) const {
        if (visit == nullptr) {
            return Result(::nros::ErrorCode::InvalidArgument);
        }
        const uint8_t* ids = nullptr;
        ::size_t count = 0;
        const nros_ret_t r = nros_lifecycle_transition_graph(&ids, &count);
        if (r != NROS_RET_OK) {
            return Result(r);
        }
        for (::size_t i = 0; i < count; ++i) {
            if (!visit(ctx, Transition(ids[i]))) {
                break;
            }
        }
        return Result();
    }

    // ---- Managed entities — phase-417 W4.f -------------------------------

    /// Have `entity` follow this node's activation — rclcpp's
    /// `LifecycleEngine::add_managed_entity()`.
    ///
    /// On a successful `activate` transition every registered entity's
    /// `on_activate()` runs, and on `deactivate` / `cleanup` / `shutdown` its
    /// `on_deactivate()` does. A [`LifecyclePublisher`] added this way stops
    /// sending while the node is not Active, which is the half of REP-2002 our
    /// lifecycle state had no entity observing.
    ///
    /// The list is INTRUSIVE (see [`ManagedEntityInterface`]): `entity` must
    /// outlive this node and must not already be in another node's list.
    /// Adding the same entity twice is refused rather than corrupting the list.
    Result add_managed_entity(ManagedEntityInterface* entity) {
        if (entity == nullptr) {
            return Result(::nros::ErrorCode::InvalidArgument);
        }
        for (ManagedEntityInterface* e = managed_; e != nullptr; e = e->next_managed_) {
            if (e == entity) {
                return Result(::nros::ErrorCode::AlreadyExists);
            }
        }
        if (entity->owner_ != nullptr) {
            // In ANOTHER node's list: one link per entity, so refuse.
            return Result(::nros::ErrorCode::AlreadyExists);
        }
        entity->next_managed_ = managed_;
        entity->owner_ = this;
        managed_ = entity;
        return Result();
    }

    /// phase-482 W4 — the moved-to entity takes the moved-from one's place in
    /// the list. Called by `ManagedEntityInterface`'s move operations only.
    void replace_managed_entity(ManagedEntityInterface* from, ManagedEntityInterface* to) {
        for (ManagedEntityInterface** link = &managed_; *link != nullptr;
             link = &(*link)->next_managed_) {
            if (*link == from) {
                to->next_managed_ = from->next_managed_;
                to->owner_ = this;
                *link = to;
                from->next_managed_ = nullptr;
                from->owner_ = nullptr;
                return;
            }
        }
    }

    /// Unlink `entity` (its destructor's call).
    void remove_managed_entity(ManagedEntityInterface* entity) {
        for (ManagedEntityInterface** link = &managed_; *link != nullptr;
             link = &(*link)->next_managed_) {
            if (*link == entity) {
                *link = entity->next_managed_;
                entity->next_managed_ = nullptr;
                entity->owner_ = nullptr;
                return;
            }
        }
    }

    // ---- Parameters — phase-417 W4.f -------------------------------------
    //
    // `rclcpp_lifecycle::LifecycleEngine` repeats `rclcpp::Node`'s parameter
    // surface verbatim, and a lifecycle node's parameters are not
    // lifecycle-gated upstream either. Ours repeats it by FORWARDING to the
    // node `bind(Node&)` supplied — each body is one call to the identically
    // named `rclcpp::Node` method, so the executor-owned `nros_params` store,
    // the per-node keying and the declared-parameter contract are reached
    // through one implementation.
    //
    // A table of any kind here would be the second store phase-426 spent six
    // work items removing, in a smaller spelling. There is no state below this
    // line: the only member the parameter surface adds is the `rclcpp::Node*`
    // itself.
    //
    // UNBOUND (no `bind(Node&)`) each call reports `InvalidArgument` and each
    // value-returning one hands back the caller's default — the same answer
    // the register/transition calls give an unbound node, never a silent write
    // into some other node's parameters.

    /// `rclcpp_lifecycle::LifecycleEngine::declare_parameter<T>(name, default)`.
    template <typename T> T declare_parameter(const char* name, T default_value = T()) {
        return node_ == nullptr ? default_value
                                : node_->template declare_parameter<T>(name, default_value);
    }
    /// `declare_parameter<T>(name, default, descriptor)`.
    template <typename T>
    T declare_parameter(const char* name, T default_value,
                        const ::rclcpp::ParameterDescriptor& descriptor) {
        return node_ == nullptr
                   ? default_value
                   : node_->template declare_parameter<T>(name, default_value, descriptor);
    }
    /// `get_parameter<T>(name, out)` — `false` when the name is not declared.
    template <typename T> bool get_parameter(const char* name, T& out) const {
        return node_ != nullptr && node_->template get_parameter<T>(name, out);
    }
    /// `get_parameter<T>(name)` — the value, or `T()` when undeclared.
    template <typename T> T get_parameter(const char* name) const {
        return node_ == nullptr ? T() : node_->template get_parameter<T>(name);
    }
    /// `get_parameter_or<T>(name, out, fallback)`.
    template <typename T> bool get_parameter_or(const char* name, T& out, T fallback) const {
        if (node_ == nullptr) {
            out = fallback;
            return false;
        }
        return node_->template get_parameter_or<T>(name, out, fallback);
    }
    /// Set a declared parameter, through the same `apply` a remote
    /// `ros2 param set` reaches.
    template <typename T> Result set_parameter(const char* name, T value) {
        return node_ == nullptr ? Result(::nros::ErrorCode::InvalidArgument)
                                : node_->template set_parameter<T>(name, value);
    }
    /// `has_parameter(name)`.
    bool has_parameter(const char* name) const {
        return node_ != nullptr && node_->has_parameter(name);
    }
    /// `undeclare_parameter(name)`. Returns `Result` where upstream returns `void`
    /// and throws; on an UNBOUND lifecycle node it answers `InvalidArgument`.
    Result undeclare_parameter(const char* name) {
        return node_ == nullptr ? Result(::nros::ErrorCode::InvalidArgument)
                                : node_->undeclare_parameter(name);
    }
    /// The declared TYPE of one parameter.
    ::rclcpp::ParameterType get_parameter_type(const char* name) const {
        return node_ == nullptr ? ::rclcpp::PARAMETER_NOT_SET : node_->get_parameter_type(name);
    }
    /// `get_parameter_types(names, count)`, into caller storage.
    Result get_parameter_types(const char* const* names, ::size_t count,
                               ::rclcpp::ParameterType* out) const {
        return node_ == nullptr ? Result(::nros::ErrorCode::InvalidArgument)
                                : node_->get_parameter_types(names, count, out);
    }
    /// `describe_parameter(name)`, into caller storage. See `rclcpp::Node`.
    Result describe_parameter(const char* name, ::rclcpp::ParameterDescriptor& out, char* text,
                              ::size_t text_len) const {
        return node_ == nullptr ? Result(::nros::ErrorCode::InvalidArgument)
                                : node_->describe_parameter(name, out, text, text_len);
    }
    /// `list_parameters(prefix, …)`, into a caller-owned rectangle.
    Result list_parameters(const char* prefix, char* out_names, ::size_t name_stride,
                           ::size_t max_names, ::size_t& count) const {
        return node_ == nullptr
                   ? Result(::nros::ErrorCode::InvalidArgument)
                   : node_->list_parameters(prefix, out_names, name_stride, max_names, count);
    }
    /// `set_parameters_atomically(writes, count)` — all or nothing.
    Result set_parameters_atomically(const ::rclcpp::ParameterWrite* writes, ::size_t count) {
        return node_ == nullptr ? Result(::nros::ErrorCode::InvalidArgument)
                                : node_->set_parameters_atomically(writes, count);
    }
    /// `add_on_set_parameters_callback(callback, context, out_handle)`.
    Result add_on_set_parameters_callback(::rclcpp::OnSetParametersCallbackType callback,
                                          void* context,
                                          ::rclcpp::ParameterCallbackHandle& out_handle) {
        return node_ == nullptr
                   ? Result(::nros::ErrorCode::InvalidArgument)
                   : node_->add_on_set_parameters_callback(callback, context, out_handle);
    }
    /// `remove_on_set_parameters_callback(handle)`.
    Result remove_on_set_parameters_callback(::rclcpp::ParameterCallbackHandle handle) {
        return node_ == nullptr ? Result(::nros::ErrorCode::InvalidArgument)
                                : node_->remove_on_set_parameters_callback(handle);
    }

#ifdef NROS_CPP_HAS_STD_STRING // hosted-family: string-interop
    /// `std::string`-keyed overloads, and the bulk `declare_parameters`. The
    /// SAME set `rclcpp::Node` carries behind the SAME guard, because a ported
    /// call site keys on `std::string` — which does not convert to
    /// `const char*`, so without these it does not bind at all. A gate on
    /// METHODS, never on a member: nothing below changes `sizeof`.
    template <typename T> T declare_parameter(const ::std::string& name, T default_value = T()) {
        return this->template declare_parameter<T>(name.c_str(), default_value);
    }
    template <typename T>
    T declare_parameter(const ::std::string& name, T default_value,
                        const ::rclcpp::ParameterDescriptor& descriptor) {
        return this->template declare_parameter<T>(name.c_str(), default_value, descriptor);
    }
    template <typename T> bool get_parameter(const ::std::string& name, T& out) const {
        return this->template get_parameter<T>(name.c_str(), out);
    }
    template <typename T> T get_parameter(const ::std::string& name) const {
        return this->template get_parameter<T>(name.c_str());
    }
    template <typename T>
    bool get_parameter_or(const ::std::string& name, T& out, T fallback) const {
        return this->template get_parameter_or<T>(name.c_str(), out, fallback);
    }
    template <typename T> Result set_parameter(const ::std::string& name, T value) {
        return this->template set_parameter<T>(name.c_str(), value);
    }
    bool has_parameter(const ::std::string& name) const {
        return this->has_parameter(name.c_str());
    }
    Result undeclare_parameter(const ::std::string& name) {
        return this->undeclare_parameter(name.c_str());
    }
    ::rclcpp::ParameterType get_parameter_type(const ::std::string& name) const {
        return this->get_parameter_type(name.c_str());
    }
    /// `declare_parameters<T>(prefix, map)` — the bulk form, hosted-only
    /// because upstream's argument IS a `std::map`. Returns `Result`, not
    /// upstream's `std::vector<T>` (see `rclcpp::Node::declare_parameters`); on an
    /// UNBOUND lifecycle node it answers `InvalidArgument`.
    template <typename T>
    Result declare_parameters(const ::std::string& prefix, const ::std::map<::std::string, T>& m) {
        return node_ == nullptr ? Result(::nros::ErrorCode::InvalidArgument)
                                : node_->template declare_parameters<T>(prefix, m);
    }
#endif // NROS_CPP_HAS_STD_STRING

    // ---- Graph queries — phase-417 stage 2b (RFC-0089) --------------------
    //
    // rclcpp_lifecycle's `LifecycleEngine` carries the same graph surface as
    // `rclcpp::Node`, so a ported managed node reaches these on `this`. Each
    // one FORWARDS to the executor this node is bound to — the graph's
    // receiver, because there is one session per image (RFC-0002) — and does
    // nothing else: no state, no loop, no caching, no name construction.
    // RFC-0019 keeps the behaviour in Rust. The bodies are the same one-line
    // forwards `rclcpp::Node` carries; see `node.hpp` for the per-call
    // documentation.
    //
    // On an UNBOUND node (default-constructed, `bind()` not yet called) they
    // return `InvalidArgument`, matching the register/transition calls above
    // rather than trapping.
    //
    // The envelope every one of them shares (ADOPT-BOUNDED, RFC-0089): they
    // report what has been DISCOVERED and never block, so an empty result
    // means "nobody seen yet" and never "nobody exists". `ErrorCode::
    // Unsupported`, which is what a backend with no graph at all returns, is a
    // DIFFERENT answer from an empty one and must not be collapsed into zero.

    /// Every node on the graph, with its namespace — rclcpp's
    /// `get_node_names()`. `enclave` is `nullptr` where the backend tracks
    /// none; strings are BORROWED for the call; return `false` to stop.
    Result get_node_names(nros_cpp_node_visit_fn visit, void* ctx) const {
        return Result(nros_cpp_executor_get_node_names(exec_, visit, ctx));
    }

    /// Every topic on the graph, with the types on it — rclcpp's
    /// `get_topic_names_and_types()`. One visit per distinct TOPIC.
    Result get_topic_names_and_types(nros_cpp_names_and_types_visit_fn visit, void* ctx) const {
        return Result(nros_cpp_executor_get_topic_names_and_types(exec_, visit, ctx));
    }

    /// Every service on the graph, with its types — rclcpp's
    /// `get_service_names_and_types()`, over servers and clients.
    Result get_service_names_and_types(nros_cpp_names_and_types_visit_fn visit, void* ctx) const {
        return Result(nros_cpp_executor_get_service_names_and_types(exec_, visit, ctx));
    }

    /// How many publishers are visible on `topic_name` — rclcpp's
    /// `count_publishers()`. The name is used as given: not remapped, not
    /// expanded. A zero is never a proof of absence.
    Result count_publishers(const char* topic_name, size_t* out_count) const {
        return Result(nros_cpp_executor_count_publishers(exec_, topic_name, out_count));
    }

    /// How many subscribers are visible on `topic_name` — rclcpp's
    /// `count_subscribers()`. See [`count_publishers`].
    Result count_subscribers(const char* topic_name, size_t* out_count) const {
        return Result(nros_cpp_executor_count_subscribers(exec_, topic_name, out_count));
    }

    /// What one named node PUBLISHES, with the types.
    Result get_publisher_names_and_types_by_node(const char* node_name, const char* node_namespace,
                                                 nros_cpp_names_and_types_visit_fn visit,
                                                 void* ctx) const {
        return Result(nros_cpp_executor_get_publisher_names_and_types_by_node(
            exec_, node_name, node_namespace, visit, ctx));
    }

    /// What one named node SUBSCRIBES to, with the types. `subscription`, not
    /// `subscriber` — the C++ surface takes rclcpp's vocabulary and this
    /// matches `rclcpp::Node` / `nros::Executor` rather than adding a third
    /// spelling.
    Result get_subscription_names_and_types_by_node(const char* node_name,
                                                    const char* node_namespace,
                                                    nros_cpp_names_and_types_visit_fn visit,
                                                    void* ctx) const {
        return Result(nros_cpp_executor_get_subscription_names_and_types_by_node(
            exec_, node_name, node_namespace, visit, ctx));
    }

    /// What services one named node SERVES, with the types — servers only,
    /// not clients, as upstream.
    Result get_service_names_and_types_by_node(const char* node_name, const char* node_namespace,
                                               nros_cpp_names_and_types_visit_fn visit,
                                               void* ctx) const {
        return Result(nros_cpp_executor_get_service_names_and_types_by_node(
            exec_, node_name, node_namespace, visit, ctx));
    }

    /// What services one named node CALLS, with the types.
    Result get_client_names_and_types_by_node(const char* node_name, const char* node_namespace,
                                              nros_cpp_names_and_types_visit_fn visit,
                                              void* ctx) const {
        return Result(nros_cpp_executor_get_client_names_and_types_by_node(
            exec_, node_name, node_namespace, visit, ctx));
    }

    /// The publishers discovered on `topic_name`, one visit each — rclcpp's
    /// `get_publishers_info_by_topic()`.
    ///
    /// The endpoint carries NO QoS profile: rclcpp's `qos_profile()` reports
    /// the GRANTED profile, no backend behind this API can read a remote's
    /// granted profile back, and reporting the remote's DECLARED one instead
    /// would be a confident wrong answer to the question ("why is nothing
    /// arriving?") the field exists to answer.
    ///
    /// rclcpp also takes `no_mangle`; there is no such parameter here, because
    /// accepting one and ignoring it would silently drop configuration.
    Result get_publishers_info_by_topic(const char* topic_name,
                                        nros_cpp_endpoint_info_visit_fn visit, void* ctx) const {
        return Result(
            nros_cpp_executor_get_publishers_info_by_topic(exec_, topic_name, visit, ctx));
    }

    /// The publishers on `topic_name`, visited as [`nros::TopicEndpointInfo`]
    /// — the rclcpp-shaped overload of the call above, a pure conversion over
    /// the same query.
    Result get_publishers_info_by_topic(const char* topic_name, TopicEndpointInfoVisitFn visit,
                                        void* ctx) const {
        detail::EndpointInfoTrampoline tramp{visit, ctx};
        return Result(nros_cpp_executor_get_publishers_info_by_topic(
            exec_, topic_name, &detail::EndpointInfoTrampoline::thunk, &tramp));
    }

    /// The subscriptions discovered on `topic_name`, one visit each —
    /// rclcpp's `get_subscriptions_info_by_topic()`. See
    /// [`get_publishers_info_by_topic`] for the QoS and `no_mangle` envelopes.
    Result get_subscriptions_info_by_topic(const char* topic_name,
                                           nros_cpp_endpoint_info_visit_fn visit, void* ctx) const {
        return Result(
            nros_cpp_executor_get_subscriptions_info_by_topic(exec_, topic_name, visit, ctx));
    }

    /// The subscriptions on `topic_name`, visited as
    /// [`nros::TopicEndpointInfo`] — the rclcpp-shaped overload.
    Result get_subscriptions_info_by_topic(const char* topic_name, TopicEndpointInfoVisitFn visit,
                                           void* ctx) const {
        detail::EndpointInfoTrampoline tramp{visit, ctx};
        return Result(nros_cpp_executor_get_subscriptions_info_by_topic(
            exec_, topic_name, &detail::EndpointInfoTrampoline::thunk, &tramp));
    }

  protected:
    void* exec_;

  private:
    // Trampolines: `previous` = get_current_state() at callback entry (the SM invokes the
    // callback before committing the new state), then dispatch to the REGISTERED
    // callback if there is one and to the virtual otherwise (phase-417 W4.f).
    //
    // A successful `activate` / `deactivate` also drives the managed entities,
    // AFTER the user's hook and only when it succeeded — a publisher must not
    // start sending because a callback that rolled the transition back happened
    // to run first.
    static CallbackReturn dispatch(LifecycleEngine* n, TransitionCallbackType cb, void* ctx,
                                   CallbackReturn (LifecycleEngine::*fallback)(LifecycleState)) {
        const LifecycleState previous = n->get_current_state();
        return cb != nullptr ? cb(previous, ctx) : (n->*fallback)(previous);
    }
    void activate_managed_entities() {
        for (ManagedEntityInterface* e = managed_; e != nullptr; e = e->next_managed_) {
            e->on_activate();
        }
    }
    void deactivate_managed_entities() {
        for (ManagedEntityInterface* e = managed_; e != nullptr; e = e->next_managed_) {
            e->on_deactivate();
        }
    }
    static uint8_t tramp_configure(void* self) {
        auto* n = static_cast<LifecycleEngine*>(self);
        return static_cast<uint8_t>(
            dispatch(n, n->cb_configure_, n->ctx_configure_, &LifecycleEngine::on_configure));
    }
    static uint8_t tramp_activate(void* self) {
        auto* n = static_cast<LifecycleEngine*>(self);
        CallbackReturn r =
            dispatch(n, n->cb_activate_, n->ctx_activate_, &LifecycleEngine::on_activate);
        if (r == CallbackReturn::Success) {
            n->activate_managed_entities();
        }
        return static_cast<uint8_t>(r);
    }
    static uint8_t tramp_deactivate(void* self) {
        auto* n = static_cast<LifecycleEngine*>(self);
        CallbackReturn r =
            dispatch(n, n->cb_deactivate_, n->ctx_deactivate_, &LifecycleEngine::on_deactivate);
        if (r == CallbackReturn::Success) {
            n->deactivate_managed_entities();
        }
        return static_cast<uint8_t>(r);
    }
    static uint8_t tramp_cleanup(void* self) {
        auto* n = static_cast<LifecycleEngine*>(self);
        CallbackReturn r =
            dispatch(n, n->cb_cleanup_, n->ctx_cleanup_, &LifecycleEngine::on_cleanup);
        if (r == CallbackReturn::Success) {
            n->deactivate_managed_entities();
        }
        return static_cast<uint8_t>(r);
    }
    static uint8_t tramp_shutdown(void* self) {
        auto* n = static_cast<LifecycleEngine*>(self);
        CallbackReturn r =
            dispatch(n, n->cb_shutdown_, n->ctx_shutdown_, &LifecycleEngine::on_shutdown);
        if (r == CallbackReturn::Success) {
            n->deactivate_managed_entities();
        }
        return static_cast<uint8_t>(r);
    }
    static uint8_t tramp_error(void* self) {
        auto* n = static_cast<LifecycleEngine*>(self);
        return static_cast<uint8_t>(
            dispatch(n, n->cb_error_, n->ctx_error_, &LifecycleEngine::on_error));
    }

    // phase-417 W4.f state. Every member here is UNCONDITIONAL — no `#if` on a
    // capability probe reaches a field, so `sizeof(LifecycleEngine)` is the same
    // in every translation unit (`check-cpp-capability-layout`).
    //
    // The node this managed node's parameters belong to, or `nullptr` when only
    // the executor was bound. A POINTER, never a node: see `bind(Node&)`.
    ::rclcpp::Node* node_ = nullptr;
    /// This node's clock (ROS time), as `rclcpp::Node` carries one.
    Clock clock_{NROS_CLOCK_ROS_TIME};
    /// Head of the intrusive managed-entity list; see `add_managed_entity`.
    ManagedEntityInterface* managed_ = nullptr;
    TransitionCallbackType cb_configure_ = nullptr;
    TransitionCallbackType cb_activate_ = nullptr;
    TransitionCallbackType cb_deactivate_ = nullptr;
    TransitionCallbackType cb_cleanup_ = nullptr;
    TransitionCallbackType cb_shutdown_ = nullptr;
    TransitionCallbackType cb_error_ = nullptr;
    void* ctx_configure_ = nullptr;
    void* ctx_activate_ = nullptr;
    void* ctx_deactivate_ = nullptr;
    void* ctx_cleanup_ = nullptr;
    void* ctx_shutdown_ = nullptr;
    void* ctx_error_ = nullptr;
};

} // namespace detail

// ManagedEntityInterface's link maintenance needs the complete engine type.
inline ManagedEntityInterface::~ManagedEntityInterface() {
    if (owner_ != nullptr) {
        owner_->remove_managed_entity(this);
    }
}

inline ManagedEntityInterface::ManagedEntityInterface(ManagedEntityInterface&& other) noexcept {
    if (other.owner_ != nullptr) {
        other.owner_->replace_managed_entity(&other, this);
    }
}

inline ManagedEntityInterface&
ManagedEntityInterface::operator=(ManagedEntityInterface&& other) noexcept {
    if (this != &other) {
        if (owner_ != nullptr) {
            owner_->remove_managed_entity(this);
        }
        if (other.owner_ != nullptr) {
            other.owner_->replace_managed_entity(&other, this);
        }
    }
    return *this;
}

/// A publisher whose sends are DROPPED while its node is not Active — rclcpp's
/// `rclcpp_lifecycle::LifecyclePublisher`.
///
/// phase-417 W4.f. Create the inner publisher the ordinary way and hand the
/// wrapper to the node:
///
/// ```cpp
/// nros::LifecyclePublisher<std_msgs::msg::Int32> pub_;
/// // in the install hook:
/// node.create_publisher(pub_.publisher(), "/chatter");
/// lifecycle_node.add_managed_entity(&pub_);
/// ```
///
/// Two lines rather than upstream's `create_publisher`, and the reason is the
/// one `bind(Node&)` states: a `LifecycleNode` here is a mixin over the state
/// machine, not a node, so it is the `rclcpp::Node` that creates entities. What
/// the wrapper adds is the GATE, which is the part REP-2002 is about.
///
/// `publish()` on an inactive node is a NO-OP returning ok, which is upstream's
/// behaviour: a managed node publishes nothing before `on_activate` and that is
/// not an error its callers should have to handle. Read [`is_activated`] when
/// you need to know.
template <typename M> class LifecyclePublisher : public SimpleManagedEntity {
  public:
    /// What `rclcpp_lifecycle::LifecycleNode::create_publisher<M>` returns:
    /// move-only, no allocator, every target (as `rclcpp::Publisher<M>::SharedPtr`
    /// is). The managed-entity link survives the moves (phase-482 W4).
    using SharedPtr = ::nros::Owned<LifecyclePublisher>;

    LifecyclePublisher() = default;
    LifecyclePublisher(LifecyclePublisher&&) noexcept = default;
    LifecyclePublisher& operator=(LifecyclePublisher&&) noexcept = default;

    /// The wrapped publisher, to hand to `Node::create_publisher`.
    ::rclcpp::Publisher<M>& publisher() { return pub_; }
    /// Const overload of [`publisher`].
    const ::rclcpp::Publisher<M>& publisher() const { return pub_; }

    /// Publish `msg` if the node is Active; drop it otherwise.
    Result publish(const M& msg) {
        if (!this->is_activated()) {
            return Result();
        }
        return pub_.publish(msg);
    }

  private:
    ::rclcpp::Publisher<M> pub_;
};

/// DEPRECATED (phase-482 W4) — derive from `rclcpp_lifecycle::LifecycleNode`.
///
/// This was the managed-node type: a mixin a component bound to its node with
/// `bind(Node&)`. `rclcpp_lifecycle::LifecycleNode` IS a node, takes upstream's
/// constructor and callback signatures, and owns the same engine, so a ported
/// lifecycle node's class body compiles against it unchanged. This forwarder
/// keeps the old spelling compiling for one release, and the attribute names
/// the replacement. A CLASS, not an alias: `[[deprecated]]` on an alias is
/// silent on clang (see `nros::Expected<T>`'s note before phase-482 W6).
/// Measured: clang warns on every use, including a base clause; GCC 12 warns
/// on a variable or member of this type but not on `: public LifecycleNode`.
class NROS_CPP_DEPRECATED_MSG(
    "nros::LifecycleNode is deprecated (phase-482 W4): derive from "
    "rclcpp_lifecycle::LifecycleNode, which IS a node and takes upstream's "
    "constructor and on_* signatures.") LifecycleNode : public detail::LifecycleEngine {
  public:
    using detail::LifecycleEngine::LifecycleEngine;
    LifecycleNode() = default;
};

} // namespace nros

// ============================================================================
// rclcpp_lifecycle:: — the ROS 2 spelling (phase-482 W4, RFC-0089)
// ============================================================================

namespace rclcpp_lifecycle {

/// `rclcpp_lifecycle::State` — one `lifecycle_msgs/msg/State` id, with `id()`
/// and a borrowed `label()`. See `nros::State` for the two differences.
using State = ::nros::State;
/// `rclcpp_lifecycle::Transition`. See `nros::Transition`.
using Transition = ::nros::Transition;
/// `rclcpp_lifecycle::LifecyclePublisher<M>` — drops sends while the node is
/// not Active. See `nros::LifecyclePublisher`.
template <typename M> using LifecyclePublisher = ::nros::LifecyclePublisher<M>;

namespace node_interfaces {

/// `rclcpp_lifecycle::node_interfaces::LifecycleNodeInterface` — the six
/// transition callbacks, with upstream's signatures and defaults.
///
/// Non-pure with defaults, so a freestanding build needs no
/// `__cxa_pure_virtual`; `on_error` defaults to FAILURE, the rest to SUCCESS,
/// as upstream's do.
class LifecycleNodeInterface {
  public:
    /// Upstream's values: `lifecycle_msgs/msg/Transition`'s
    /// `TRANSITION_CALLBACK_{SUCCESS,FAILURE,ERROR}` (97, 98, 99).
    enum class CallbackReturn : uint8_t {
        SUCCESS = 97,
        FAILURE = 98,
        ERROR = 99,
    };

    virtual ~LifecycleNodeInterface() = default;

    virtual CallbackReturn on_configure(const State& previous_state) {
        (void)previous_state;
        return CallbackReturn::SUCCESS;
    }
    virtual CallbackReturn on_cleanup(const State& previous_state) {
        (void)previous_state;
        return CallbackReturn::SUCCESS;
    }
    virtual CallbackReturn on_shutdown(const State& previous_state) {
        (void)previous_state;
        return CallbackReturn::SUCCESS;
    }
    virtual CallbackReturn on_activate(const State& previous_state) {
        (void)previous_state;
        return CallbackReturn::SUCCESS;
    }
    virtual CallbackReturn on_deactivate(const State& previous_state) {
        (void)previous_state;
        return CallbackReturn::SUCCESS;
    }
    virtual CallbackReturn on_error(const State& previous_state) {
        (void)previous_state;
        return CallbackReturn::FAILURE;
    }
};

} // namespace node_interfaces

/// `rclcpp_lifecycle::LifecycleNode` — a REP-2002 managed node that IS an
/// `rclcpp::Node`.
///
/// Upstream's constructor and callback signatures, so a ported lifecycle
/// node's class body compiles unchanged: override `on_configure(const State&)`
/// and friends, return `CallbackReturn::SUCCESS`, create publishers with
/// `create_publisher<M>(topic, qos)`, and drive transitions with
/// `configure()` / `activate()` / … or `ros2 lifecycle set`.
///
/// The five REP-2002 services are registered by the constructor, as upstream
/// registers them. Publishers made by `create_publisher` are managed entities:
/// they send only while the node is Active, and a successful `activate`
/// activates them whether or not the override calls the base (upstream Iron and
/// later; Humble code that calls `pub->on_activate()` itself still works).
///
/// Differences, each bounded and stated:
///  * Construction cannot throw (RFC-0018). A failed construction leaves
///    `ok()` false, as `rclcpp::Node`'s does.
///  * One managed node per executor: the REP-2002 state machine lives on the
///    executor (`nros_executor_lifecycle_*`).
///  * Not movable: the engine's managed-entity list points into this object.
///  * `register_on_*` takes a function pointer and a context, not a
///    `std::function`.
class LifecycleNode : public ::rclcpp::Node, public node_interfaces::LifecycleNodeInterface {
  public:
    using SharedPtr = ::nros::Handle<LifecycleNode>;
    using ConstSharedPtr = ::nros::Handle<const LifecycleNode>;
    using CallbackReturn = node_interfaces::LifecycleNodeInterface::CallbackReturn;

    /// `LifecycleNode(node_name, options)` — upstream's shape, on the global
    /// executor `rclcpp::init()` created.
    explicit LifecycleNode(const char* node_name,
                           const ::rclcpp::NodeOptions& options = ::rclcpp::NodeOptions())
        : ::rclcpp::Node(node_name, options) {
        start();
    }

    /// `LifecycleNode(node_name, namespace, options)`.
    LifecycleNode(const char* node_name, const char* namespace_,
                  const ::rclcpp::NodeOptions& options = ::rclcpp::NodeOptions())
        : ::rclcpp::Node(node_name, namespace_, options) {
        start();
    }

    /// The component-model constructor (`SHAPE rclcpp`): construct against the
    /// executor-bound handle the generated entry hands in. The launch file's
    /// name and namespace win over `node_name` / `namespace_`, as for
    /// `rclcpp::Node(NodeHandle, …)`.
    LifecycleNode(::nros::NodeHandle handle, const char* node_name,
                  const char* namespace_ = nullptr)
        : ::rclcpp::Node(handle, node_name, namespace_) {
        start();
    }

#if defined(NROS_CPP_HAS_STD_STRING) // hosted-family: string-interop
    explicit LifecycleNode(const ::std::string& node_name,
                           const ::rclcpp::NodeOptions& options = ::rclcpp::NodeOptions())
        : LifecycleNode(node_name.c_str(), options) {}
    LifecycleNode(const ::std::string& node_name, const ::std::string& namespace_,
                  const ::rclcpp::NodeOptions& options = ::rclcpp::NodeOptions())
        : LifecycleNode(node_name.c_str(), namespace_.c_str(), options) {}
#endif

    LifecycleNode(const LifecycleNode&) = delete;
    LifecycleNode& operator=(const LifecycleNode&) = delete;
    LifecycleNode(LifecycleNode&&) = delete;
    LifecycleNode& operator=(LifecycleNode&&) = delete;

    /// The current REP-2002 state.
    State get_current_state() const { return State(engine_.get_current_state()); }

    /// The transitions, by name. Each returns the state the node is in
    /// afterwards, as upstream's do; a refused transition leaves it unchanged.
    const State& configure() { return settle(engine_.configure()); }
    const State& cleanup() { return settle(engine_.cleanup()); }
    const State& activate() { return settle(engine_.activate()); }
    const State& deactivate() { return settle(engine_.deactivate()); }
    const State& shutdown() { return settle(engine_.shutdown()); }

    /// Drive a transition by its `lifecycle_msgs/msg/Transition` id.
    const State& trigger_transition(uint8_t transition_id) {
        return settle(engine_.trigger_transition(transition_id));
    }
    /// Drive the transition `transition` names.
    const State& trigger_transition(const Transition& transition) {
        return trigger_transition(transition.id());
    }

    /// A transition callback that replaces the matching `on_*` override:
    /// `CallbackReturn (*)(const State& previous, void* context)`.
    ///
    /// rclcpp takes a `std::function`; this is a function pointer plus a
    /// context, the shape every callback seam here uses, because a
    /// `std::function` member would need an allocator and would make the class
    /// layout depend on a capability probe. `nullptr` restores the override.
    typedef CallbackReturn (*TransitionCallbackType)(const State& previous, void* context);

    void register_on_configure(TransitionCallbackType cb, void* ctx = nullptr) { set(0, cb, ctx); }
    void register_on_activate(TransitionCallbackType cb, void* ctx = nullptr) { set(1, cb, ctx); }
    void register_on_deactivate(TransitionCallbackType cb, void* ctx = nullptr) { set(2, cb, ctx); }
    void register_on_cleanup(TransitionCallbackType cb, void* ctx = nullptr) { set(3, cb, ctx); }
    void register_on_shutdown(TransitionCallbackType cb, void* ctx = nullptr) { set(4, cb, ctx); }
    void register_on_error(TransitionCallbackType cb, void* ctx = nullptr) { set(5, cb, ctx); }

    /// The REP-2002 transition graph, one row per call to `visit`. See
    /// `nros::Transition`.
    ::nros::Result get_transition_graph(::nros::TransitionVisitFn visit, void* ctx) const {
        return engine_.get_transition_graph(visit, ctx);
    }

    /// Add an entity whose sends follow this node's state. Publishers made by
    /// `create_publisher` are added already. Upstream's is protected and takes
    /// a `std::weak_ptr`; this takes a pointer the entity's own move and
    /// destructor keep correct (`nros::ManagedEntityInterface`).
    ::nros::Result add_managed_entity(::nros::ManagedEntityInterface* entity) {
        return engine_.add_managed_entity(entity);
    }

    /// `create_publisher<M>(topic, qos)` — a managed publisher: it sends only
    /// while this node is Active. On failure the handle is empty and `ok()` is
    /// false, as for `rclcpp::Node::create_publisher`.
    template <typename M>
    typename LifecyclePublisher<M>::SharedPtr
    create_publisher(const char* topic, const ::nros::QoS& qos = ::nros::QoS::default_profile()) {
        typename LifecyclePublisher<M>::SharedPtr out{LifecyclePublisher<M>()};
        if (!this->::rclcpp::Node::create_publisher(out->publisher(), topic, qos).ok()) {
            return typename LifecyclePublisher<M>::SharedPtr();
        }
        (void)engine_.add_managed_entity(out.get());
        if (engine_.get_current_state() == ::nros::LifecycleState::Active) {
            out->on_activate();
        }
        return out;
    }

    /// `create_publisher<M>(topic, depth)`.
    template <typename M>
    typename LifecyclePublisher<M>::SharedPtr create_publisher(const char* topic, ::size_t depth) {
        return create_publisher<M>(topic, ::nros::QoS(static_cast<uint32_t>(depth)));
    }

#if defined(NROS_CPP_HAS_STD_STRING) // hosted-family: string-interop
    template <typename M>
    typename LifecyclePublisher<M>::SharedPtr
    create_publisher(const ::std::string& topic,
                     const ::nros::QoS& qos = ::nros::QoS::default_profile()) {
        return create_publisher<M>(topic.c_str(), qos);
    }
    template <typename M>
    typename LifecyclePublisher<M>::SharedPtr create_publisher(const ::std::string& topic,
                                                               ::size_t depth) {
        return create_publisher<M>(topic.c_str(), depth);
    }
#endif

  private:
    void start() {
        if (!this->ok()) {
            return;
        }
        engine_.bind(*this);
        engine_.register_on_configure(&LifecycleNode::cb_configure, this);
        engine_.register_on_activate(&LifecycleNode::cb_activate, this);
        engine_.register_on_deactivate(&LifecycleNode::cb_deactivate, this);
        engine_.register_on_cleanup(&LifecycleNode::cb_cleanup, this);
        engine_.register_on_shutdown(&LifecycleNode::cb_shutdown, this);
        engine_.register_on_error(&LifecycleNode::cb_error, this);
        (void)engine_.register_services();
        state_ = State(engine_.get_current_state());
    }

    const State& settle(::nros::Result) {
        state_ = State(engine_.get_current_state());
        return state_;
    }

    void set(int i, TransitionCallbackType cb, void* ctx) {
        user_cb_[i] = cb;
        user_ctx_[i] = ctx;
    }
    CallbackReturn
    run(int i, ::nros::LifecycleState previous,
        CallbackReturn (node_interfaces::LifecycleNodeInterface::*hook)(const State&)) {
        const State s(previous);
        return user_cb_[i] != nullptr ? user_cb_[i](s, user_ctx_[i]) : (this->*hook)(s);
    }

    static ::nros::CallbackReturn to_engine(CallbackReturn r) {
        return r == CallbackReturn::SUCCESS   ? ::nros::CallbackReturn::Success
               : r == CallbackReturn::FAILURE ? ::nros::CallbackReturn::Failure
                                              : ::nros::CallbackReturn::Error;
    }
    static ::nros::CallbackReturn cb_configure(::nros::LifecycleState previous, void* self) {
        return to_engine(static_cast<LifecycleNode*>(self)->run(
            0, previous, &node_interfaces::LifecycleNodeInterface::on_configure));
    }
    static ::nros::CallbackReturn cb_activate(::nros::LifecycleState previous, void* self) {
        return to_engine(static_cast<LifecycleNode*>(self)->run(
            1, previous, &node_interfaces::LifecycleNodeInterface::on_activate));
    }
    static ::nros::CallbackReturn cb_deactivate(::nros::LifecycleState previous, void* self) {
        return to_engine(static_cast<LifecycleNode*>(self)->run(
            2, previous, &node_interfaces::LifecycleNodeInterface::on_deactivate));
    }
    static ::nros::CallbackReturn cb_cleanup(::nros::LifecycleState previous, void* self) {
        return to_engine(static_cast<LifecycleNode*>(self)->run(
            3, previous, &node_interfaces::LifecycleNodeInterface::on_cleanup));
    }
    static ::nros::CallbackReturn cb_shutdown(::nros::LifecycleState previous, void* self) {
        return to_engine(static_cast<LifecycleNode*>(self)->run(
            4, previous, &node_interfaces::LifecycleNodeInterface::on_shutdown));
    }
    static ::nros::CallbackReturn cb_error(::nros::LifecycleState previous, void* self) {
        return to_engine(static_cast<LifecycleNode*>(self)->run(
            5, previous, &node_interfaces::LifecycleNodeInterface::on_error));
    }

    ::nros::detail::LifecycleEngine engine_;
    State state_{::nros::LifecycleState::Unconfigured};
    TransitionCallbackType user_cb_[6] = {nullptr, nullptr, nullptr, nullptr, nullptr, nullptr};
    void* user_ctx_[6] = {nullptr, nullptr, nullptr, nullptr, nullptr, nullptr};
};

} // namespace rclcpp_lifecycle

#endif // NROS_CPP_LIFECYCLE_HPP
