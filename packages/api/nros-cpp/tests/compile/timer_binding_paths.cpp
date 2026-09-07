// phase-427 W3 — INSTANTIATION probe for the timer-binding paths.
//
// `create_wall_timer<C, &C::method>(out, ms, self)` is the overload that
// RETIRED the free `nros::bind_timer`, and every path that used to reach
// `bind_timer` now reaches it instead. Each of those is a TEMPLATE BODY, which
// the header parse loop never type-checks — `-include <nros/node.hpp>`
// instantiates nothing. That is exactly how a retirement rots: the free
// function keeps compiling because it is still parsed, and the member the call
// sites were moved to is never built.
//
// So this file instantiates all five:
//
//   1. the member overload, called directly (what the 22 migrated example call
//      sites now write);
//   2. `NROS_BIND_TIMER`, which must expand to the MEMBER — a deprecation a
//      macro hides is not a deprecation;
//   3. `create_timer<C, &C::m>(out, clock, ms, self)`, the clock-taking sibling
//      (phase-430 W6);
//   4. `create_timer_in<C, &C::m>(group, out, ms, self)`, the RFC-0047 sibling;
//   5. `ComponentNode::create_wall_timer<C, &C::m>(ms)`, whose body was the one
//      remaining in-header caller of the free function.
//
// `nros::bind_timer` itself is NOT called here: its deprecation is asserted by
// the expected-failure probe `bind_timer_deprecation_probe.cpp`, which is where
// a `-Werror=deprecated-declarations` build belongs.
//
// Compiled c++17 rather than c++14 because `ComponentNode` is the tree's one
// `if constexpr` user (`adopt_launch_seed_`), which is the C++17 floor
// `check-cxx-standard-floor` records.

#include <nros/nros.hpp>

namespace nros_cpp_timer_binding_paths_test {

/// The firmware component shape: no allocator, no derivation, no vtable. The
/// member pointer is a template parameter, so the trampoline is a capture-less
/// lambda converting to the executor's raw `void(*)(void*)`.
struct Component {
    nros::Timer timer;
    int ticks = 0;

    void on_tick() { ++ticks; }

    nros::Result bound_directly(rclcpp::Node& node) {
        return node.create_wall_timer<Component, &Component::on_tick>(timer, 100, this);
    }

    nros::Result bound_through_the_macro(rclcpp::Node& node) {
        return NROS_BIND_TIMER(node, Component, on_tick, timer, 100, this);
    }

    nros::Result bound_on_a_clock(rclcpp::Node& node) {
        return node.create_timer<Component, &Component::on_tick>(timer, *node.get_clock(), 100,
                                                                 this);
    }

    nros::Result bound_in_a_group(rclcpp::Node& node) {
        nros::CallbackGroup group = node.create_callback_group("ctrl");
        return node.create_timer_in<Component, &Component::on_tick>(group, timer, 100, this);
    }
};

/// The RFC-0044 derivable shape. Its `create_wall_timer` body used to call the
/// free `bind_timer`; it calls the member now, and this is the only place that
/// body is instantiated.
class DerivedComponent : public nros::ComponentNode {
  public:
    explicit DerivedComponent(nros::NodeHandle handle)
        : nros::ComponentNode(handle, "derived_component") {
        create_wall_timer<DerivedComponent, &DerivedComponent::on_tick>(100);
    }

    void on_tick() {}
};

inline void probe_entry_points(rclcpp::Node& node) {
    Component c;
    (void)c.bound_directly(node);
    (void)c.bound_through_the_macro(node);
    (void)c.bound_on_a_clock(node);
    (void)c.bound_in_a_group(node);

    DerivedComponent d{nros::NodeHandle(node.executor_handle())};
    (void)d.ok();
}

} // namespace nros_cpp_timer_binding_paths_test
