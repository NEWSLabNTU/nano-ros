// EXPECTED-FAILURE probe — phase-430 W7.
//
// `rclcpp::TimerBase` is RETIRED. The HIERARCHY is deleted outright — there is
// no polymorphic base, `detail::WallTimer` derives from nothing, and
// `create_wall_timer` hands back `std::shared_ptr<nros::Timer>` — but the NAME
// survives one release as a deprecated alias for `rclcpp::Timer`, because three
// in-tree source files use it and one of them
// (`examples/templates/cpp-port-minimal-publisher`) is vendored deliberately
// UNMODIFIED to demonstrate that upstream source compiles here.
//
// A retirement nobody is told about is just a rename that has not happened yet,
// so this asserts the attribute FIRES: under `-Werror=deprecated-declarations`
// the retired spelling must fail, and the lane greps the diagnostic for the
// replacement. "It failed" is also what a typo produces.
//
// The other half — that the hierarchy is really gone — is
// `ros2_one_dispatch_path.cpp`'s `!is_polymorphic<detail::WallTimer>` assertion
// and its pin on `create_wall_timer`'s return type.
//
// Same shape as `qos_deprecation_probe.cpp`, `receive_deprecation_probe.cpp`
// and `bind_timer_deprecation_probe.cpp`.

#include <nros/nros.hpp>

namespace nros_cpp_timer_base_deprecation_probe {

struct Holder {
    rclcpp::TimerBase::SharedPtr the_retired_spelling;
};

} // namespace nros_cpp_timer_base_deprecation_probe
