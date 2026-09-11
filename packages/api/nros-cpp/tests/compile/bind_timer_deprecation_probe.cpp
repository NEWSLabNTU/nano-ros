// EXPECTED-FAILURE probe — phase-427 W3.
//
// `nros::bind_timer` is RETIRED in favour of
// `node.create_wall_timer<C, &C::method>(out, period_ms, self)`, and it survives
// one release as a deprecated forwarder so an out-of-tree consumer gets the
// migration in the compiler's own words rather than as a missing symbol.
//
// A deprecation nobody is told about is just an alias, so this probe asserts
// the attribute FIRES: under `-Werror=deprecated-declarations` the old spelling
// must fail to compile, and the lane greps the diagnostic for the replacement,
// because "it failed" is also what a typo produces.
//
// Same shape as `qos_deprecation_probe.cpp` and `receive_deprecation_probe.cpp`.
// The positive twin — every path that used to reach `bind_timer` reaching the
// member instead — is `timer_binding_paths.cpp`.

#include <nros/nros.hpp>

namespace nros_cpp_bind_timer_deprecation_probe {

struct Component {
    nros::Timer timer;
    void on_tick() {}

    nros::Result the_retired_spelling(rclcpp::Node& node) {
        return nros::bind_timer<Component, &Component::on_tick>(node, timer, 100, this);
    }
};

} // namespace nros_cpp_bind_timer_deprecation_probe
