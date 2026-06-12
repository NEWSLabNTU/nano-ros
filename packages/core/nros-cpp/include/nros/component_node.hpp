/// @file component_node.hpp
/// @brief Phase 242.1 (RFC-0044) — `nros::ComponentNode`, the rclcpp-faithful
/// IS-A-node base.
///
/// RFC-0043 gave the C++ component a *default-construct + two-phase
/// `Result configure(Node&)`* shape (still available — the lower-level option in
/// `<nros/component.hpp>`). That shape cannot host a real `rclcpp`-style node: a
/// node that **IS-A `Node`**, takes its identity in the **constructor**, and
/// **creates publishers/subscriptions/timers (with typed member callbacks) in
/// the ctor body**. RFC-0044 adds that faithful shape here.
///
/// A user node **derives** `ComponentNode`; its ctor receives the executor-bound
/// node handle from the entry and wires entities as member calls:
///
/// ```cpp
/// class Talker : public nros::ComponentNode {
///     nros::Publisher<Int32> pub_;
///     int count_ = 0;
///   public:
///     explicit Talker(nros::NodeHandle h) : nros::ComponentNode(h, "talker") {
///         pub_ = create_publisher<Int32>("/chatter");           // aborts on fail
///         create_timer<Talker, &Talker::on_tick>(500);          // typed member timer
///     }
///     void on_tick() { Int32 m; m.data = count_++; pub_.publish(m); }
/// };
/// NROS_COMPONENT(Talker);   // factory + sizeof/alignof + class metadata
///
/// class Listener : public nros::ComponentNode {
///   public:
///     explicit Listener(nros::NodeHandle h) : nros::ComponentNode(h, "listener") {
///         create_subscription<Int32, Listener, &Listener::on_msg>("/chatter");
///         // or, ergonomically, inside the ctor:  NROS_SUBSCRIBE(Int32, on_msg, "/chatter");
///     }
///     void on_msg(const Int32& m) { /* real body */ }
/// };
/// NROS_COMPONENT(Listener);
/// ```
///
/// ## Design choices (RFC-0044 open questions)
/// - **Q1 wrap vs derive → WRAP.** `ComponentNode` *owns* a `nros::Node node_`
///   member (the user derives `ComponentNode`, `ComponentNode` wraps `Node`).
///   This keeps `Node`'s value-semantics + its FFI handle clean (vs deriving
///   `Node` directly, which would expose `Node`'s move/dtor surface on every
///   user node). `create_*` forward to the owned `node_`.
/// - **Q2 abort vs error-flag → ABORT.** Entity/param creation failure in the
///   ctor is boot-fatal on firmware (no graceful degradation — the image cannot
///   run), the same outcome a thrown `rclcpp` ctor exception has. The `create_*`
///   members therefore return the entity by value (or `void`) and **abort** on
///   failure via `detail::component_fatal` — no `Result` threading at the
///   authoring surface. The fallible `configure(Node&)` + `bind_*` path in
///   `<nros/component.hpp>` stays for callers that want an error return.
/// - **Ctor handle.** `ComponentNode(NodeHandle, name)` receives the
///   executor-bound handle (the entry constructs the component *after*
///   `nros::init`). `NodeHandle` carries the opaque executor handle; the ctor
///   creates the owned node against it.

#ifndef NROS_CPP_COMPONENT_NODE_BASE_HPP
#define NROS_CPP_COMPONENT_NODE_BASE_HPP

#include <cstddef>
#include <cstdint>
#include <new> // placement new for the NROS_COMPONENT factory

#include "nros/component.hpp" // bind_subscription / bind_timer (the no-alloc trampolines)
#include "nros/node.hpp"
#include "nros/publisher.hpp"
#include "nros/qos.hpp"
#include "nros/result.hpp"
#include "nros/timer.hpp"

#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
#include <cstdio>  // fprintf — boot-fatal diagnostic (hosted only)
#include <cstdlib> // abort — boot-fatal halt (hosted only)
#endif

/// Max timers a single `ComponentNode` may own via the storage-less
/// `create_timer` members. `nros::Timer`'s dtor cancels its timer, so a timer
/// created in the ctor must outlive the call — `ComponentNode` parks them in a
/// fixed-capacity inline pool (no heap). Bump if a node needs more; override by
/// defining `NROS_COMPONENT_MAX_TIMERS` before including this header.
#ifndef NROS_COMPONENT_MAX_TIMERS
#define NROS_COMPONENT_MAX_TIMERS 8
#endif

namespace nros {

/// Executor-bound node handle the entry hands to a `ComponentNode` ctor
/// (RFC-0044 §Design.1). Carries the opaque executor handle the owned node is
/// created against — the same pointer `nros::global_handle()` /
/// `Node::executor_handle()` expose. The entry obtains it post-`init` and
/// constructs the component with it (placement-new in 242.4).
struct NodeHandle {
    void* executor;
    constexpr NodeHandle() : executor(nullptr) {}
    explicit constexpr NodeHandle(void* exec) : executor(exec) {}
    constexpr bool valid() const { return executor != nullptr; }
};

namespace detail {

/// Boot-fatal halt (RFC-0044 Q2). A failed entity/param creation in a
/// `ComponentNode` ctor is unrecoverable on firmware (boot is all-or-nothing) —
/// the same outcome a thrown `rclcpp` ctor exception has. v1 is a lean abort:
/// hosted builds print a diagnostic then `std::abort()`; freestanding builds
/// `__builtin_trap()`. (Revisit for multi-node boot diagnostics — RFC-0044 Q2.)
[[noreturn]] inline void component_fatal(const char* what, int32_t code) {
#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
    ::std::fprintf(stderr, "[nros] FATAL: ComponentNode %s failed (code=%d) — halting boot\n",
                   (what != nullptr) ? what : "?", static_cast<int>(code));
    ::std::abort();
#else
    (void)what;
    (void)code;
    __builtin_trap();
#endif
    // Unreachable — both branches above are [[noreturn]] in practice; loop to
    // satisfy the [[noreturn]] contract on toolchains that don't see it.
    for (;;) {
    }
}

} // namespace detail

/// rclcpp-faithful node base (RFC-0044). The user **derives** this; it **wraps**
/// an owned `nros::Node`. The ctor receives the executor-bound `NodeHandle` and
/// creates the owned node; the `create_*` members forward to that node and
/// **abort on creation failure** (boot-fatal — no `Result` at the surface).
class ComponentNode {
  public:
    /// Construct the node identity against the executor-bound handle. Aborts if
    /// the handle is null or node creation fails (boot-fatal, RFC-0044 Q2).
    ///
    /// @param handle  Executor-bound handle from the entry (post-`nros::init`).
    /// @param name    Node name (null-terminated). The derived ctor supplies it,
    ///                rclcpp-style (`: ComponentNode(h, "controller")`).
    /// @param ns      Node namespace, or nullptr for "/".
    explicit ComponentNode(NodeHandle handle, const char* name, const char* ns = nullptr) {
        if (!handle.valid()) {
            detail::component_fatal("ctor (null executor handle)", -1);
        }
        // Friend access (declared in node.hpp): bind the owned node to the
        // executor handle, then create it — exactly what Executor::create_node /
        // create_node do, but against the entry-supplied handle.
        node_.executor_handle_ = handle.executor;
        Result r = Node::create(node_, name, ns);
        if (!r.ok()) {
            detail::component_fatal("node create", r.raw());
        }
    }

    virtual ~ComponentNode() = default;

    // -- Accessors -------------------------------------------------------

    /// The owned executor-bound node (for the lower-level `bind_*` /
    /// `create_*_raw` helpers in `<nros/component.hpp>`, or `nros-cpp` entities
    /// the convenience members don't cover — services, actions).
    Node& node() { return node_; }
    const Node& node() const { return node_; }

    /// Node name (rclcpp `get_name()` parity).
    const char* get_name() const { return node_.get_name(); }
    /// Node namespace.
    const char* get_namespace() const { return node_.get_namespace(); }
    /// `nros_log::Logger` keyed on this node (for the `NROS_LOG_*` macros).
    const void* get_logger() const { return node_.get_logger(); }

    // -- Entity creation (abort-on-fatal) --------------------------------

    /// Create a publisher. Returns it by value (move it into a member —
    /// `pub_ = create_publisher<M>("/topic")`). Aborts on failure.
    template <typename M>
    Publisher<M> create_publisher(const char* topic, const QoS& qos = QoS::default_profile()) {
        Publisher<M> pub;
        Result r = node_.create_publisher(pub, topic, qos);
        if (!r.ok()) {
            detail::component_fatal("create_publisher", r.raw());
        }
        return pub;
    }

    /// Create a **typed member-callback** subscription (RFC-0044 §Design.2(1)):
    /// `void C::Method(const M&)`. Registers a raw subscription on the executor
    /// keyed on `M::TYPE_NAME` (DDS-mangled) with a no-alloc deserialize-then-
    /// dispatch trampoline (see `nros::bind_subscription`); the executor arena
    /// owns the subscriber — no C++ `Subscription<M>` storage needed. Aborts on
    /// failure. `C` is the derived type; `self` is `this`.
    ///
    /// Call as `create_subscription<M, Self, &Self::on_msg>(topic)`, or use the
    /// `NROS_SUBSCRIBE(M, on_msg, topic)` macro inside the derived ctor to derive
    /// `Self` automatically.
    template <typename M, class C, void (C::*Method)(const M& msg)>
    void create_subscription(const char* topic, const QoS& qos = QoS::default_profile()) {
        Result r = bind_subscription<M, C, Method>(node_, topic, static_cast<C*>(this), qos);
        if (!r.ok()) {
            detail::component_fatal("create_subscription", r.raw());
        }
    }

    /// Create a **typed member** repeating timer: `void C::Method()` fires every
    /// `period_ms` during `spin_once`. The `nros::Timer` is parked in the
    /// component's inline pool (its dtor cancels the timer, so it must outlive
    /// the call). Aborts on failure or pool exhaustion. `C` is the derived type.
    ///
    /// Call as `create_timer<Self, &Self::tick>(period_ms)`, or use
    /// `NROS_CREATE_TIMER(period_ms, tick)` inside the derived ctor.
    template <class C, void (C::*Method)()>
    void create_timer(uint64_t period_ms) {
        if (timer_count_ >= NROS_COMPONENT_MAX_TIMERS) {
            detail::component_fatal("create_timer (timer pool exhausted)", -1);
        }
        Timer& slot = timers_[timer_count_];
        Result r = bind_timer<C, Method>(node_, slot, period_ms, static_cast<C*>(this));
        if (!r.ok()) {
            detail::component_fatal("create_timer", r.raw());
        }
        ++timer_count_;
    }

    /// Create a repeating timer from a plain C callback + ctx (the escape hatch
    /// for non-member tick handlers). Parked in the inline pool. Aborts on fail.
    void create_timer(uint64_t period_ms, nros_cpp_timer_callback_t callback,
                      void* context = nullptr) {
        if (timer_count_ >= NROS_COMPONENT_MAX_TIMERS) {
            detail::component_fatal("create_timer (timer pool exhausted)", -1);
        }
        Result r = node_.create_timer(timers_[timer_count_], period_ms, callback, context);
        if (!r.ok()) {
            detail::component_fatal("create_timer", r.raw());
        }
        ++timer_count_;
    }

  protected:
    ComponentNode(const ComponentNode&) = delete;
    ComponentNode& operator=(const ComponentNode&) = delete;

    Node node_;
    Timer timers_[NROS_COMPONENT_MAX_TIMERS];
    size_t timer_count_ = 0;
};

} // namespace nros

// -- Ergonomic macros (derive `Self` from the enclosing ctor's `this`) -------
//
// Used inside a derived `ComponentNode` ctor body, where `this` is the derived
// type. `decltype(*this)` is `Self&`; `_nros_self_t` strips the reference so
// `&Self::method` is well-formed. These give the RFC-0044 §Design.1 ergonomic
// (`create_subscription<M>(topic, &Self::on_msg)`) within C++14's no-`auto`-
// non-type-param limit (the underlying member takes `M`/`C`/`Method` as
// explicit template params; the macro fills `C`/`Method` from `this`).

namespace nros {
namespace detail {
template <class T> struct strip_ref {
    using type = T;
};
template <class T> struct strip_ref<T&> {
    using type = T;
};
} // namespace detail
} // namespace nros

/// Inside a `ComponentNode` ctor: subscribe `void Self::method(const Msg&)` to
/// `topic`. Derives `Self` from `this` so only the message type, method, and
/// topic are spelled.
#define NROS_SUBSCRIBE(Msg, method, topic)                                                         \
    this->template create_subscription<                                                            \
        Msg, ::nros::detail::strip_ref<decltype(*this)>::type,                                     \
        &::nros::detail::strip_ref<decltype(*this)>::type::method>((topic))

/// Inside a `ComponentNode` ctor: create a repeating timer calling
/// `void Self::method()` every `period_ms`. Derives `Self` from `this`.
#define NROS_CREATE_TIMER(period_ms, method)                                                       \
    this->template create_timer<::nros::detail::strip_ref<decltype(*this)>::type,                  \
                                &::nros::detail::strip_ref<decltype(*this)>::type::method>(         \
        (period_ms))

// -- NROS_COMPONENT(Class) ---------------------------------------------------
//
// Phase 242.1.2 — emits the per-pkg metadata the *typed* codegen entry (242.4)
// needs to placement-construct the component into an arena slot with the entry's
// node handle. Parallels `NROS_NODE_REGISTER` (node_pkg.hpp), but for the
// IS-A-node ctor shape: a factory (placement-new with the handle) + `sizeof` /
// `alignof` (arena sizing) + the qualified class string (lint / diagnostics).
//
// `NROS_PKG_NAME` is the cmake-injected (pre-sanitised) pkg token, same source
// as `NROS_NODE_REGISTER`. Hand-written pkgs `#define NROS_PKG_NAME my_pkg`
// before including this header.

#ifndef NROS_PKG_NAME
#define NROS_PKG_NAME unknown
#endif

#define _NROS_COMP_CAT_(a, b) a##b
#define _NROS_COMP_CAT(a, b) _NROS_COMP_CAT_(a, b)
#define _NROS_COMP_STR_(x) #x
#define _NROS_COMP_STR(x) _NROS_COMP_STR_(x)

#define _NROS_COMP_FACTORY_SYM(pkg) _NROS_COMP_CAT(__nros_component_factory_, pkg)
#define _NROS_COMP_SIZE_SYM(pkg) _NROS_COMP_CAT(__nros_component_size_, pkg)
#define _NROS_COMP_ALIGN_SYM(pkg) _NROS_COMP_CAT(__nros_component_align_, pkg)
#define _NROS_COMP_CLASS_SYM(pkg) _NROS_COMP_CAT(__nros_component_class_, pkg)

/// Register an `nros::ComponentNode`-derived class as the pkg's component.
/// Emits:
///  - `__nros_component_factory_<pkg>(void* storage, void* node_handle)` — a
///    C-ABI factory that placement-news `Class(nros::NodeHandle(node_handle))`
///    into the entry-owned arena slot and returns it as `nros::ComponentNode*`.
///  - `__nros_component_size_<pkg>` / `__nros_component_align_<pkg>` — the arena
///    slot size + alignment the entry must reserve.
///  - `__nros_component_class_<pkg>` — the `"<pkg>::<Class>"` string for lint.
///
/// The derived class MUST have an `explicit Class(nros::NodeHandle)` ctor (it
/// forwards the handle + the node name to the `ComponentNode` base).
#define NROS_COMPONENT(Class)                                                                      \
    extern "C" ::nros::ComponentNode* _NROS_COMP_FACTORY_SYM(NROS_PKG_NAME)(void* storage,         \
                                                                            void* node_handle) {   \
        return new (storage) Class(::nros::NodeHandle(node_handle));                               \
    }                                                                                              \
    extern "C" const ::std::size_t _NROS_COMP_SIZE_SYM(NROS_PKG_NAME) = sizeof(Class);             \
    extern "C" const ::std::size_t _NROS_COMP_ALIGN_SYM(NROS_PKG_NAME) = alignof(Class);           \
    extern "C" const char _NROS_COMP_CLASS_SYM(NROS_PKG_NAME)[] =                                  \
        _NROS_COMP_STR(NROS_PKG_NAME) "::" _NROS_COMP_STR(Class)

#endif // NROS_CPP_COMPONENT_NODE_BASE_HPP
