// phase-417 W4.f — `nros::LifecycleNode` carries the surface
// `rclcpp_lifecycle::LifecycleNode` does.
//
// Fifteen ledger rows said it did not, and they split into four claims this TU
// pins, because each is the kind that compiles fine while being wrong:
//
//  1. TRANSITION CALLBACKS WITHOUT SUBCLASSING. C and Rust both let a user
//     register one; C++ exposed only the virtual overrides, even though
//     `register_services()` already called the FFI's `register_on_*` one layer
//     down. Six rows for one missing surface. Pinned as method POINTERS, so a
//     renamed or re-arited method is a hard error rather than an overload that
//     silently stops being found.
//
//  2. THE CLOCK. `rclcpp::Node` has had one since issue 0789; the lifecycle
//     mixin had no node to ask. It has its own now, so the accessor answers
//     with or without `bind(Node&)` — which is the difference from the
//     parameter surface below, and the reason the two were separate rows.
//
//  3. THE TRANSITION GRAPH AS AN OBJECT. `enum class LifecycleTransition`
//     (issue 1099) is the id and nothing else; rclcpp's `Transition` carries a
//     label, a start state and a goal state. Those were served over
//     `~/get_transition_graph` to remote peers and unreadable in process in all
//     three languages.
//
//  4. THE MANAGED-ENTITY PROTOCOL. We had lifecycle state and no entity that
//     observed it, so a deactivated node's publishers kept publishing.
//
// Freestanding on purpose: `-fno-exceptions -fno-rtti`, C++14, no STL. The
// whole point of the shapes chosen (function pointer + context, a visitor over
// the graph, an intrusive entity list) is that they cost nothing on a target
// with no allocator, and a TU that needed `<functional>` or `<vector>` to
// express them would not be evidence of that.

#include <nros/lifecycle.hpp>
#include <nros/nros.hpp>

namespace {

// ---- 1. register_on_* ------------------------------------------------------

using RegisterFn = void (nros::LifecycleNode::*)(nros::LifecycleNode::TransitionCallbackType,
                                                 void*);

constexpr RegisterFn reg_configure_ = &nros::LifecycleNode::register_on_configure;
constexpr RegisterFn reg_activate_ = &nros::LifecycleNode::register_on_activate;
constexpr RegisterFn reg_deactivate_ = &nros::LifecycleNode::register_on_deactivate;
constexpr RegisterFn reg_cleanup_ = &nros::LifecycleNode::register_on_cleanup;
constexpr RegisterFn reg_shutdown_ = &nros::LifecycleNode::register_on_shutdown;
constexpr RegisterFn reg_error_ = &nros::LifecycleNode::register_on_error;

/// A registered callback is a plain function, not a closure object: this one
/// would not compile if the seam took a `std::function`, which is the
/// divergence the ledger row records.
nros::CallbackReturn on_configure_fn(nros::LifecycleState previous, void* context) {
    (void)previous;
    (void)context;
    return nros::CallbackReturn::Success;
}

void registers_without_subclassing() {
    nros::LifecycleNode node;
    int context = 0;
    node.register_on_configure(&on_configure_fn, &context);
    node.register_on_activate(&on_configure_fn, &context);
    node.register_on_deactivate(&on_configure_fn, &context);
    node.register_on_cleanup(&on_configure_fn, &context);
    node.register_on_shutdown(&on_configure_fn, &context);
    node.register_on_error(&on_configure_fn, &context);
    // `nullptr` restores the virtual, and the context defaults away.
    node.register_on_configure(nullptr);
}

// ---- 2. clock --------------------------------------------------------------

using GetClockFn = nros::Clock* (nros::LifecycleNode::*)();
using NowFn = nros::Time (nros::LifecycleNode::*)() const;
constexpr GetClockFn get_clock_ = &nros::LifecycleNode::get_clock;
constexpr NowFn now_ = &nros::LifecycleNode::now;

// ---- 3. the transition graph ----------------------------------------------

/// `Transition` carries an id and NOTHING else — the labels and the two states
/// come from `nros_core` through the C seam on every call. A field added here
/// would be the fourth copy of `lifecycle_msgs` in this tree, and the three
/// that already exist are the reason issue 1099 happened.
static_assert(sizeof(nros::Transition) == sizeof(uint8_t), "nros::Transition must stay a bare id");

/// The id half is `constexpr`, so it needs no runtime at all.
constexpr nros::Transition activate_{nros::LifecycleTransition::Activate};
static_assert(activate_.id() == 3, "Activate is lifecycle_msgs TRANSITION_ACTIVATE");
static_assert(nros::Transition(static_cast<uint8_t>(60)).id() == 60,
              "an id with no enumerator is still addressable");

/// The four accessors rclcpp's `Transition` carries, plus the two ours adds.
bool visit_transition(void* ctx, const nros::Transition& t) {
    (void)ctx;
    (void)t.id();
    (void)t.label();
    (void)t.start_state();
    (void)t.goal_state();
    (void)t.valid();
    (void)t.transition();
    return true;
}

void reads_its_own_transition_graph() {
    nros::LifecycleNode node;
    (void)node.get_transition_graph(&visit_transition, nullptr);
    // A null visitor is refused rather than silently doing nothing.
    (void)node.get_transition_graph(nullptr, nullptr);
    (void)nros::state_label(nros::LifecycleState::Active);
}

// ---- 4. managed entities ---------------------------------------------------

/// A publisher whose sends follow the node's state. Declared, not created: the
/// creation half belongs to `rclcpp::Node` (see `bind(Node&)`), and this TU is
/// about the surface existing with the right shapes.
struct Gated : nros::SimpleManagedEntity {};

void managed_entities_follow_activation() {
    nros::LifecycleNode node;
    Gated entity;
    nros::Result r = node.add_managed_entity(&entity);
    (void)r;
    // Adding the same entity twice is REFUSED rather than corrupting the
    // intrusive list — the one failure mode the list shape introduces.
    r = node.add_managed_entity(&entity);
    (void)r;
    r = node.add_managed_entity(nullptr);
    (void)r;
    (void)entity.is_activated();
}

/// `ManagedEntityInterface`'s three verbs are non-pure with defaults, so a
/// freestanding image needs no `__cxa_pure_virtual`. A type that overrides
/// NONE of them must still be instantiable.
struct BareEntity : nros::ManagedEntityInterface {};
BareEntity bare_;

// A codegen'd message, in shape only (cf. `serialization_format.cpp`), so the
// `LifecyclePublisher<M>` template can be INSTANTIATED rather than merely
// parsed — a wrapper whose `publish` never compiles would look identical here
// otherwise.
struct Int32 {
    int32_t data{0};
    static const ::size_t SERIALIZED_SIZE_MAX = 16;
    static constexpr const char* TYPE_NAME = "std_msgs::msg::dds_::Int32_";
    static constexpr const char* TYPE_HASH = "RIHS01_int32_stub";
    static int ffi_publish(void*, const void*) { return 0; }
    static int ffi_serialize(const void*, uint8_t*, ::size_t, ::size_t* out) {
        if (out) *out = 0;
        return 0;
    }
    static int ffi_deserialize(const uint8_t*, ::size_t, void*) { return 0; }
};

} // namespace

namespace nros {
template <> struct format_of<::Int32> {
    static constexpr SerializationFormat value = SerializationFormat::Cdr;
};
} // namespace nros

namespace {

/// A gated publisher IS a managed entity, and its inner publisher is reachable
/// for `Node::create_publisher` to fill.
void a_lifecycle_publisher_is_gated_on_activation() {
    nros::LifecycleNode node;
    nros::LifecyclePublisher<::Int32> pub;
    (void)node.add_managed_entity(&pub);
    ::rclcpp::Publisher<::Int32>& inner = pub.publisher();
    (void)inner;
    ::Int32 msg;
    // Inactive: a no-op that reports ok, as upstream's does. Nothing about
    // this call site says whether the node is active, which is the point.
    (void)pub.publish(msg);
    (void)pub.is_activated();
}

/// `bind(Node&)` and the parameter forwarders against a REAL `rclcpp::Node`.
/// The unbound probes above exercise the null arm; this one instantiates the
/// forward itself, which is the half that would not compile if `Node`'s
/// signature moved.
void binds_a_node(::rclcpp::Node& node) {
    nros::LifecycleNode lc;
    lc.bind(node);
    (void)lc.declare_parameter<int64_t>("depth", 10);
    (void)lc.get_parameter<int64_t>("depth");
    (void)lc.has_parameter("depth");
}

// ---- 5. the parameter surface ----------------------------------------------
//
// The `cpp:LifecycleNode::*parameter*` glob row: upstream repeats `Node`'s
// whole parameter surface on the lifecycle node. Method pointers again, because
// a forwarder that stopped forwarding would still compile at a call site that
// happened to match another overload.

using HasParamFn = bool (nros::LifecycleNode::*)(const char*) const;
using UndeclareFn = nros::Result (nros::LifecycleNode::*)(const char*);
using ParamTypeFn = rclcpp::ParameterType (nros::LifecycleNode::*)(const char*) const;
using SetAtomicFn = nros::Result (nros::LifecycleNode::*)(const rclcpp::ParameterWrite*, ::size_t);
using RemoveCbFn = nros::Result (nros::LifecycleNode::*)(rclcpp::ParameterCallbackHandle);

constexpr HasParamFn has_parameter_ = &nros::LifecycleNode::has_parameter;
constexpr UndeclareFn undeclare_parameter_ = &nros::LifecycleNode::undeclare_parameter;
constexpr ParamTypeFn get_parameter_type_ = &nros::LifecycleNode::get_parameter_type;
constexpr SetAtomicFn set_parameters_atomically_ = &nros::LifecycleNode::set_parameters_atomically;
constexpr RemoveCbFn remove_on_set_ = &nros::LifecycleNode::remove_on_set_parameters_callback;

void declares_and_reads_parameters() {
    nros::LifecycleNode node;
    // UNBOUND: every one of these answers without a node, and none of them
    // writes anywhere. That is the property, not a convenience — a lifecycle
    // node that silently wrote into some other node's parameters is the defect
    // phase-426 spent six work items removing.
    int64_t depth = node.declare_parameter<int64_t>("depth", 10);
    (void)depth;
    bool enabled = false;
    (void)node.get_parameter<bool>("enabled", enabled);
    (void)node.get_parameter<int64_t>("depth");
    (void)node.get_parameter_or<int64_t>("depth", depth, 7);
    (void)node.set_parameter<int64_t>("depth", 11);
    (void)node.has_parameter("depth");
    (void)node.undeclare_parameter("depth");
    (void)node.get_parameter_type("depth");

    rclcpp::ParameterDescriptor descriptor = rclcpp::parameter_descriptor();
    descriptor.description = "how deep";
    (void)node.declare_parameter<int64_t>("depth", 10, descriptor);
    char text[64] = {0};
    (void)node.describe_parameter("depth", descriptor, text, sizeof(text));

    char names[4][32] = {{0}};
    ::size_t count = 0;
    (void)node.list_parameters("", &names[0][0], 32, 4, count);

    const char* wanted[1] = {"depth"};
    rclcpp::ParameterType types[1] = {rclcpp::PARAMETER_NOT_SET};
    (void)node.get_parameter_types(wanted, 1, types);
}

} // namespace

int main() {
    registers_without_subclassing();
    reads_its_own_transition_graph();
    managed_entities_follow_activation();
    a_lifecycle_publisher_is_gated_on_activation();
    declares_and_reads_parameters();
    (void)&binds_a_node;
    (void)reg_configure_;
    (void)reg_activate_;
    (void)reg_deactivate_;
    (void)reg_cleanup_;
    (void)reg_shutdown_;
    (void)reg_error_;
    (void)get_clock_;
    (void)now_;
    (void)has_parameter_;
    (void)undeclare_parameter_;
    (void)get_parameter_type_;
    (void)set_parameters_atomically_;
    (void)remove_on_set_;
    (void)bare_;
    return 0;
}
