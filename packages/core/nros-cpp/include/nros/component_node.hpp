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
///         pub_ = create_publisher<Int32>("/chatter");           // sets ok()=false on fail
///         create_timer<Talker, &Talker::on_tick>(500);          // typed member timer
///     }
///     void on_tick() { Int32 m; m.data = count_++; pub_.publish(m); }
/// };
/// NROS_COMPONENT(Talker);   // factory + shape:"rclcpp" + class metadata
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
/// - **Q2 abort vs error-flag → ok()-FLAG (242.4).** Entity/param creation
///   failure in the ctor is boot-fatal on firmware (no graceful degradation —
///   the image cannot run), the same outcome a thrown `rclcpp` ctor exception
///   has. But the ctor does **not** abort: a failure records an internal error
///   flag surfaced via `bool ok() const` (+ `error_what()` / `error_code()`).
///   The codegen Entry / single-node carrier checks `ok()` **post-construct**
///   and halts boot **naming the failing node** (RFC-0044 Q2 — multi-node boot
///   diagnostics: *which* node failed). The `create_*` members therefore return
///   the entity by value (or `void`) and set the flag on failure — no `Result`
///   threading at the authoring surface. The fallible `configure(Node&)` +
///   `bind_*` path in `<nros/component.hpp>` stays for callers that want an
///   error return.
/// - **Ctor handle.** `ComponentNode(NodeHandle, name)` receives the
///   executor-bound handle (the entry constructs the component *after*
///   `nros::init`). `NodeHandle` carries the opaque executor handle; the ctor
///   creates the owned node against it.

#ifndef NROS_CPP_COMPONENT_NODE_BASE_HPP
#define NROS_CPP_COMPONENT_NODE_BASE_HPP

#include <cstddef>
#include <cstdint>
#include <new>         // placement new for the NROS_COMPONENT factory
#include <type_traits> // enable_if / is_std_vector split for the param facade (C++14 freestanding subset)

#include "nros/component.hpp" // bind_subscription / bind_timer (the no-alloc trampolines)
#include "nros/node.hpp"
#include "nros/parameter.hpp" // ParameterServer backing the value-returning facade (242.7)
#include "nros/publisher.hpp"
#include "nros/qos.hpp"
#include "nros/result.hpp"
#include "nros/timer.hpp"

// Phase 242.7 (RFC-0044) — sizing for the per-node `ParameterServer` backing the
// rclcpp-faithful value-returning parameter facade. Overridable per build with a
// `#define` before including this header. Defaults cover a real controller node
// (ASI's MPC/PID declares ~150 scalar params + a few `std::vector<double>`
// weight matrices); the storage is fixed-capacity (no heap).
#ifndef NROS_COMPONENT_MAX_PARAMS
#define NROS_COMPONENT_MAX_PARAMS 256 // scalar parameter slots
#endif
#ifndef NROS_COMPONENT_MAX_SEQ_PARAMS
#define NROS_COMPONENT_MAX_SEQ_PARAMS 8 // sequence (vector) parameter slots
#endif
#ifndef NROS_COMPONENT_SEQ_POOL_BYTES
#define NROS_COMPONENT_SEQ_POOL_BYTES 4096 // inline byte pool for all sequence elements
#endif
#ifndef NROS_PARAM_SEQ_DEFAULT_CAP
#define NROS_PARAM_SEQ_DEFAULT_CAP 64 // per-vector element capacity when the caller gives no N
#endif

#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
#include <cstdio> // fprintf — boot-failure diagnostic (hosted only)
#endif
// Issue 0112 — `<string>` is needed ONLY by the `NROS_CPP_STD` `std::string`-keyed
// parameter overloads below. A hosted *compiler* (`__STDC_HOSTED__ == 1`) can still
// be invoked with `-nostdinc++` against a minimal C++ library that lacks `<string>`
// (Zephyr's minimal libcpp), so gate the include on its actual consumer, not on
// compiler hostedness — else every Zephyr C++ entry fails with "string: No such file".
#ifdef NROS_CPP_STD
#include <string> // std::string-keyed parameter overloads (242.7 — rclcpp keys on std::string)
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

/// Boot-failure diagnostic (RFC-0044 Q2, refined in 242.4). A failed entity/
/// param creation in a `ComponentNode` ctor is unrecoverable on firmware (boot
/// is all-or-nothing) — but the ctor no longer aborts. Instead it records an
/// `ok()` flag and the codegen Entry / single-node carrier checks it
/// post-construct, then halts boot **naming the failing node** via this helper.
/// Hosted builds print the node name + failure site to `stderr`; freestanding
/// builds are a no-op (the caller still returns the error code to halt boot).
/// NOT `[[noreturn]]` — the caller (entry/carrier) decides how to halt.
inline void report_component_failure(const char* node_name, const char* what, int32_t code) {
#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
    ::std::fprintf(stderr,
                   "[nros] FATAL: ComponentNode \"%s\" failed at %s (code=%d) — halting boot\n",
                   (node_name != nullptr) ? node_name : "?", (what != nullptr) ? what : "?",
                   static_cast<int>(code));
#else
    (void)node_name;
    (void)what;
    (void)code;
#endif
}

// Phase 242.7 — split the value-returning `declare_parameter`/`get_parameter`
// facade between scalar `T` (→ ParameterServer scalar store) and a
// `std::vector<T>` (→ a default-capacity `Seq`). `<type_traits>` is in the C++14
// freestanding subset; the `std::vector` specialization is hosted-only.
template <typename T> struct cn_is_std_vector : ::std::false_type {};
#ifdef NROS_CPP_STD
template <typename T, typename A>
struct cn_is_std_vector<::std::vector<T, A>> : ::std::true_type {};
#endif

} // namespace detail

/// rclcpp-faithful node base (RFC-0044). The user **derives** this; it **wraps**
/// an owned `nros::Node`. The ctor receives the executor-bound `NodeHandle` and
/// creates the owned node; the `create_*` members forward to that node and
/// **record an `ok()` flag on creation failure** (boot-fatal, but checked
/// post-construct by the entry/carrier — no `Result` at the surface, no abort).
class ComponentNode {
  public:
    /// Construct the node identity against the executor-bound handle. On a null
    /// handle or node-creation failure it sets `ok() == false` (the entry checks
    /// `ok()` post-construct + halts naming the node — RFC-0044 Q2). The ctor
    /// does NOT abort.
    ///
    /// @param handle  Executor-bound handle from the entry (post-`nros::init`).
    /// @param name    Node name (null-terminated). The derived ctor supplies it,
    ///                rclcpp-style (`: ComponentNode(h, "controller")`).
    /// @param ns      Node namespace, or nullptr for "/".
    explicit ComponentNode(NodeHandle handle, const char* name, const char* ns = nullptr) {
        if (!handle.valid()) {
            set_error("ctor (null executor handle)", -1);
            return;
        }
        // Friend access (declared in node.hpp): bind the owned node to the
        // executor handle, then create it — exactly what Executor::create_node /
        // create_node do, but against the entry-supplied handle.
        node_.executor_handle_ = handle.executor;
        Result r = Node::create(node_, name, ns);
        if (!r.ok()) {
            set_error("node create", r.raw());
        }
    }

    virtual ~ComponentNode() = default;

    // -- Boot status (RFC-0044 Q2 — checked post-construct by the entry) --

    /// `true` while no entity/param creation has failed. The codegen Entry /
    /// single-node carrier checks this immediately after construction and halts
    /// boot (naming this node) when it is `false`.
    bool ok() const { return ok_; }
    /// The site of the first failure (`"create_publisher"`, `"node create"`, …),
    /// or `nullptr` when `ok()`. For the boot diagnostic.
    const char* error_what() const { return error_what_; }
    /// The raw error code of the first failure, or `0` when `ok()`.
    int32_t error_code() const { return error_code_; }

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
    /// `pub_ = create_publisher<M>("/topic")`). Sets `ok()=false` on failure.
    template <typename M>
    Publisher<M> create_publisher(const char* topic, const QoS& qos = QoS::default_profile()) {
        Publisher<M> pub;
        Result r = node_.create_publisher(pub, topic, qos);
        if (!r.ok()) {
            set_error("create_publisher", r.raw());
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
            set_error("create_subscription", r.raw());
        }
    }

    /// Create a **typed member** repeating timer: `void C::Method()` fires every
    /// `period_ms` during `spin_once`. The `nros::Timer` is parked in the
    /// component's inline pool (its dtor cancels the timer, so it must outlive
    /// the call). Aborts on failure or pool exhaustion. `C` is the derived type.
    ///
    /// Call as `create_timer<Self, &Self::tick>(period_ms)`, or use
    /// `NROS_CREATE_TIMER(period_ms, tick)` inside the derived ctor.
    template <class C, void (C::*Method)()> void create_timer(uint64_t period_ms) {
        if (timer_count_ >= NROS_COMPONENT_MAX_TIMERS) {
            set_error("create_timer (timer pool exhausted)", -1);
            return;
        }
        Timer& slot = timers_[timer_count_];
        Result r = bind_timer<C, Method>(node_, slot, period_ms, static_cast<C*>(this));
        if (!r.ok()) {
            set_error("create_timer", r.raw());
            return;
        }
        ++timer_count_;
    }

    /// Create a repeating timer from a plain C callback + ctx (the escape hatch
    /// for non-member tick handlers). Parked in the inline pool. Sets `ok()=false`
    /// on failure.
    void create_timer(uint64_t period_ms, nros_cpp_timer_callback_t callback,
                      void* context = nullptr) {
        if (timer_count_ >= NROS_COMPONENT_MAX_TIMERS) {
            set_error("create_timer (timer pool exhausted)", -1);
            return;
        }
        Result r = node_.create_timer(timers_[timer_count_], period_ms, callback, context);
        if (!r.ok()) {
            set_error("create_timer", r.raw());
            return;
        }
        ++timer_count_;
    }

    // -- Phase 273 (RFC-0047) — Callback-group API -------------------------

    /// Create a named callback-group token (RFC-0047).
    ///
    /// The returned `CallbackGroup` may be passed to `create_timer_in`,
    /// `create_subscription_in`, or `create_publisher_in` to associate entities
    /// with a group's SchedContext (resolved via `group_sched_table`).
    ///
    /// @param name  Group name — must be a string literal or static-lifetime string.
    CallbackGroup create_callback_group(const char* name) {
        return node_.create_callback_group(name);
    }

    /// Create a **typed member** repeating timer **in** a callback group (RFC-0047).
    ///
    /// Like `create_timer<C, Method>` but associates the timer with `group` so
    /// the executor binds it to the group's SchedContext.
    ///
    /// Call as `create_timer_in<Self, &Self::tick>(group, period_ms)`.
    template <class C, void (C::*Method)()>
    void create_timer_in(const CallbackGroup& group, uint64_t period_ms) {
        if (timer_count_ >= NROS_COMPONENT_MAX_TIMERS) {
            set_error("create_timer_in (timer pool exhausted)", -1);
            return;
        }
        Timer& slot = timers_[timer_count_];
        C* self = static_cast<C*>(this);
        Result r = node_.create_timer_in(
            group, slot, period_ms, [](void* ctx) { (static_cast<C*>(ctx)->*Method)(); }, self);
        if (!r.ok()) {
            set_error("create_timer_in", r.raw());
            return;
        }
        ++timer_count_;
    }

    /// Create a repeating timer **in** a callback group from a plain C callback
    /// (escape hatch for non-member tick handlers). Parked in the inline pool.
    /// Sets `ok()=false` on failure.
    void create_timer_in(const CallbackGroup& group, uint64_t period_ms,
                         nros_cpp_timer_callback_t callback, void* context = nullptr) {
        if (timer_count_ >= NROS_COMPONENT_MAX_TIMERS) {
            set_error("create_timer_in (timer pool exhausted)", -1);
            return;
        }
        Result r =
            node_.create_timer_in(group, timers_[timer_count_], period_ms, callback, context);
        if (!r.ok()) {
            set_error("create_timer_in", r.raw());
            return;
        }
        ++timer_count_;
    }

    /// Create a **typed member-callback** subscription **in** a callback group
    /// (RFC-0047). Like `create_subscription<M, C, Method>` but associates the
    /// subscription with `group` so the executor binds it to the group's
    /// SchedContext via `group_sched_table`. The executor arena owns the subscriber.
    ///
    /// Call as `create_subscription_in<M, Self, &Self::on_msg>(group, topic)`.
    template <typename M, class C, void (C::*Method)(const M& msg)>
    void create_subscription_in(const CallbackGroup& group, const char* topic,
                                const QoS& qos = QoS::default_profile()) {
        const nros_cpp_node_t* h = node_.ffi_handle();
        if (h == nullptr) {
            set_error("create_subscription_in", -3);
            return;
        }
        nros_cpp_qos_t ffi_qos = detail::component_qos_to_ffi(qos);
        C* self = static_cast<C*>(this);
        size_t handle = static_cast<size_t>(-1);
        nros_cpp_ret_t ret = nros_cpp_subscription_register(
            h, topic, M::TYPE_NAME, "", ffi_qos,
            [](const uint8_t* data, size_t len, void* ctx) {
                M msg;
                if (M::ffi_deserialize(data, len, &msg) != 0) return;
                (static_cast<C*>(ctx)->*Method)(msg);
            },
            self, /*sched_context=*/0, &handle, group.get_name());
        if (ret != 0) {
            set_error("create_subscription_in", ret);
        }
    }

    // -- Parameters (RFC-0044 / 242.7 — value-returning rclcpp facade) -----
    //
    // rclcpp shape: `T declare_parameter<T>(name, default)` / `T
    // get_parameter<T>(name)` / `bool has_parameter(name)`, backed by the owned
    // `params_` ParameterServer. No-exceptions reconciliation: a failed
    // declare/get sets the `ok()`-flag (boot-fatal, checked post-construct) and
    // returns the default. Scalars route to the scalar store; `std::vector<T>`
    // (hosted) routes to a default-capacity `Seq` so the vendored
    // `declare_parameter<std::vector<double>>(name, {…})` compiles unchanged.

    /// Declare + read back a **scalar** parameter, rclcpp value-returning shape.
    template <typename T,
              typename = typename ::std::enable_if<!detail::cn_is_std_vector<T>::value>::type>
    T declare_parameter(const char* name, T default_value = T{}) {
        Result r = params_.template declare_parameter<T>(name, default_value);
        // Launch-seeded params (phase-269 entry post-configure) are DECLARED
        // before the component ctor runs; a component re-declare is the rclcpp
        // "declare adopts the override" case, not an error — fall through to
        // the read-back so the seeded value wins. (NROS_RET_ALREADY_EXISTS is
        // the C-ABI code; do not confuse with the C++ ErrorCode at -5.)
        if (!r.ok() && r.raw() != NROS_RET_ALREADY_EXISTS) {
            set_error("declare_parameter", r.raw());
            return default_value;
        }
        T out{};
        r = params_.template get_parameter<T>(name, out);
        if (!r.ok()) {
            set_error("declare_parameter(read-back)", r.raw());
            return default_value;
        }
        return out;
    }

    /// Read a **scalar** parameter by value (rclcpp shape). Returns `T{}` if
    /// absent — the vendored callers always `declare` before `get`.
    template <typename T,
              typename = typename ::std::enable_if<!detail::cn_is_std_vector<T>::value>::type>
    T get_parameter(const char* name) const {
        T out{};
        (void)params_.template get_parameter<T>(name, out);
        return out;
    }

    bool has_parameter(const char* name) const { return params_.has_parameter(name); }

#ifdef NROS_CPP_STD
    /// Declare + read back a **`std::vector<T>`** parameter (hosted). Backed by a
    /// default-capacity `Seq<T, NROS_PARAM_SEQ_DEFAULT_CAP>` — the caller supplies
    /// no `N`, matching the vendored `declare_parameter<std::vector<double>>`.
    template <typename V,
              typename = typename ::std::enable_if<detail::cn_is_std_vector<V>::value>::type,
              typename = void>
    V declare_parameter(const char* name, const V& default_value = V{}) {
        using Elem = typename V::value_type;
        Result r = params_.template declare_parameter<Elem, NROS_PARAM_SEQ_DEFAULT_CAP>(
            name, default_value);
        if (!r.ok() && r.raw() == NROS_RET_ALREADY_EXISTS) {
            r = Result(0); // launch-seeded — adopt the existing value below
        }
        if (!r.ok()) {
            set_error("declare_parameter(vector)", r.raw());
            return default_value;
        }
        V out;
        r = params_.template get_parameter<Elem>(name, out);
        if (!r.ok()) {
            set_error("declare_parameter(vector read-back)", r.raw());
            return default_value;
        }
        return out;
    }

    /// Read a **`std::vector<T>`** parameter by value (hosted).
    template <typename V,
              typename = typename ::std::enable_if<detail::cn_is_std_vector<V>::value>::type,
              typename = void>
    V get_parameter(const char* name) const {
        using Elem = typename V::value_type;
        V out;
        (void)params_.template get_parameter<Elem>(name, out);
        return out;
    }

    // 242.7 — `std::string`-keyed overloads (hosted). Real rclcpp nodes (ASI's
    // vendored MPC: `node.declare_parameter<int>(s)`, `declare_parameter<double>(ns
    // + "…")`) pass `std::string` names; `std::string` does not implicitly convert
    // to `const char*`, so forward via `.c_str()`. Covers scalar T + std::vector<T>;
    // these compile the vendored call sites unchanged. (rclcpp keys on std::string.)
    // Single std::string declare overload: forwards to the const-char* layer,
    // which SFINAE-splits scalar vs std::vector — so this one signature covers
    // both `declare_parameter<int>(s)` and `declare_parameter<std::vector<double>>(s, {…})`.
    template <typename T> T declare_parameter(const ::std::string& name, T default_value = T{}) {
        return this->template declare_parameter<T>(name.c_str(), default_value);
    }
    template <typename T> T get_parameter(const ::std::string& name) const {
        return this->template get_parameter<T>(name.c_str());
    }
    bool has_parameter(const ::std::string& name) const { return has_parameter(name.c_str()); }
#endif // NROS_CPP_STD

  protected:
    ComponentNode(const ComponentNode&) = delete;
    ComponentNode& operator=(const ComponentNode&) = delete;

    /// Record the first creation failure (RFC-0044 Q2). Idempotent on the *first*
    /// failure — later failures don't clobber the original diagnostic. Also emits
    /// the hosted `stderr` line so a failure is visible even before the entry's
    /// post-construct `ok()` check halts boot.
    void set_error(const char* what, int32_t code) {
        if (ok_) {
            ok_ = false;
            error_what_ = what;
            error_code_ = code;
            detail::report_component_failure(node_.get_name(), what, code);
        }
    }

    Node node_;
    Timer timers_[NROS_COMPONENT_MAX_TIMERS];
    size_t timer_count_ = 0;
    // 242.7 — backs the value-returning parameter facade. Fixed-capacity, no heap;
    // sized by the NROS_COMPONENT_MAX_{PARAMS,SEQ_PARAMS} / SEQ_POOL_BYTES knobs.
    ParameterServer<NROS_COMPONENT_MAX_PARAMS, NROS_COMPONENT_MAX_SEQ_PARAMS,
                    NROS_COMPONENT_SEQ_POOL_BYTES>
        params_;
    bool ok_ = true;
    const char* error_what_ = nullptr;
    int32_t error_code_ = 0;
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
    this->template create_subscription<Msg, ::nros::detail::strip_ref<decltype(*this)>::type,      \
                                       &::nros::detail::strip_ref<decltype(*this)>::type::method>( \
        (topic))

/// Inside a `ComponentNode` ctor: create a repeating timer calling
/// `void Self::method()` every `period_ms`. Derives `Self` from `this`.
#define NROS_CREATE_TIMER(period_ms, method)                                                       \
    this->template create_timer<::nros::detail::strip_ref<decltype(*this)>::type,                  \
                                &::nros::detail::strip_ref<decltype(*this)>::type::method>(        \
        (period_ms))

// -- NROS_COMPONENT(Class) ---------------------------------------------------
//
// Phase 242.1.2 / 242.4 — marks an `nros::ComponentNode`-derived class as the
// pkg's rclcpp-faithful (IS-A-node) component. Parallels `NROS_NODE_REGISTER`
// (node_pkg.hpp), but for the construct-with-handle ctor shape.
//
// Emits the **factory** (placement-new with the entry's executor node handle) +
// the qualified **class** string + a **shape:"rclcpp"** marker. There is NO
// `sizeof`/`alignof` metadata: the typed codegen entry (242.4) `#include`s the
// component header, so `sizeof(Class)` / a `Storage<Class>` is a compile-time
// fact there — not a codegen input. The codegen-reachable `shape` marker travels
// via the cmake metadata (`nano_ros_node_register(SHAPE rclcpp …)` →
// `nros-metadata.json components[].shape`); the `__nros_component_shape_<pkg>`
// symbol below is the matching C++-side assertion of intent (so the macro that
// declares "I am an rclcpp component" carries the marker alongside the factory).
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
#define _NROS_COMP_CLASS_SYM(pkg) _NROS_COMP_CAT(__nros_component_class_, pkg)
#define _NROS_COMP_SHAPE_SYM(pkg) _NROS_COMP_CAT(__nros_component_shape_, pkg)

/// Register an `nros::ComponentNode`-derived class as the pkg's component.
/// Emits:
///  - `__nros_component_factory_<pkg>(void* storage, void* node_handle)` — a
///    C-ABI factory that placement-news `Class(nros::NodeHandle(node_handle))`
///    into the entry-owned arena slot and returns it as `nros::ComponentNode*`.
///  - `__nros_component_class_<pkg>` — the `"<pkg>::<Class>"` string for lint.
///  - `__nros_component_shape_<pkg>` — the `"rclcpp"` shape marker (RFC-0044
///    §impl): this is the construct-with-handle IS-A-node shape, not the legacy
///    `configure(Node&)` shape. The codegen reads the same marker from the cmake
///    metadata; this symbol is its C++-side counterpart.
///
/// The derived class MUST have an `explicit Class(nros::NodeHandle)` ctor (it
/// forwards the handle + the node name to the `ComponentNode` base).
#define NROS_COMPONENT(Class)                                                                      \
    extern "C" ::nros::ComponentNode* _NROS_COMP_FACTORY_SYM(NROS_PKG_NAME)(void* storage,         \
                                                                            void* node_handle) {   \
        return new (storage) Class(::nros::NodeHandle(node_handle));                               \
    }                                                                                              \
    extern "C" const char _NROS_COMP_CLASS_SYM(NROS_PKG_NAME)[] =                                  \
        _NROS_COMP_STR(NROS_PKG_NAME) "::" _NROS_COMP_STR(Class);                                  \
    extern "C" const char _NROS_COMP_SHAPE_SYM(NROS_PKG_NAME)[] = "rclcpp"

#endif // NROS_CPP_COMPONENT_NODE_BASE_HPP
