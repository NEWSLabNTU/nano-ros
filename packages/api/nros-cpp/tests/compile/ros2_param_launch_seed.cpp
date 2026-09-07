// POSITIVE compile probe: the parameter surface a ported `rclcpp::Node` uses,
// compiled with the `param_services` capability macro on and at the standard
// the component lane really builds with.
//
// WHAT IT USED TO PIN, and why the file survives the change. Before phase-426
// W4, `rclcpp::Node::declare_parameter<T>` forwarded to a NODE-LOCAL
// `nros::ParameterServer` — a second store. Where the EXECUTOR's store also
// existed (`NROS_SYSTEM_PARAM_SERVICES`), the generated entry had already
// seeded launch parameters into THAT one, so `declare_parameter` had to reach
// across and adopt the seeded value or a launch parameter would be dead
// weight. Two helpers did that reaching — `rclcpp::detail::
// adopt_executor_param_seed` in C++14 and `ComponentNode::adopt_launch_seed_`
// in C++17 `if constexpr` — and this TU was the only thing that compiled
// either, because no other probe defines the macro.
//
// W4 deleted both C++ stores and both helpers. There is ONE store now, the
// executor's, and adoption is what it already answers: `declare` reports
// `ALREADY_EXISTS` for a name the seed put there and the facade reads it back.
// So the branch this file was written to reach no longer exists.
//
// It is still worth compiling. `NROS_SYSTEM_PARAM_SERVICES` is what a bringup
// declaring `param_services` defines, and it is the configuration in which the
// executor's parameter FFI is actually linked — so this is the TU that says
// "the shape a real parameter-using image compiles is the shape we ship". The
// define stays IN THE FILE rather than on the command line, so the lane needs
// no special flags for it, and `-std=c++17` stays because that is what the
// component lane compiles with.

#define NROS_SYSTEM_PARAM_SERVICES 1

#include <nros/nros.hpp>

#include <string>

namespace nros_cpp_ros2_param_launch_seed_test {

// Every parameter type this path can carry end-to-end. The list is short, and
// the reason is the forwarder overload set, not this probe: `node_param_declare`
// / `node_param_get` / `node_param_set` (`nros/node_parameters.hpp`, phase-426
// W4) cover `bool`, `int`, `int64_t`, `double` and (for declare/set only)
// `const char*`, plus `std::string` and `std::vector<T>` behind `NROS_CPP_STD`.
// W4 moved that set out of `nros::ParameterServer` and did not widen it, so both
// gaps below are unchanged and still measured:
//
//   * `declare_parameter<float>` does NOT compile -- there is no
//     `node_param_get(..., float&)`, so the read-back has nothing to bind.
//     `float` is the type a ported control node most often uses for a gain.
//   * `declare_parameter<std::string>` compiles only where `NROS_CPP_STD` is
//     defined. This probe is hosted-STL by construction and still does not
//     define that macro, because doing so would change what every other
//     nano-ros header does in the same TU -- the flag-gated-struct-field hazard
//     of issue 0135 -- which is not a decision this file can make on its own.
//
// Both are compile ERRORS, so they are loud rather than silent and the
// compile-or-conform rule is satisfied. They are still porting friction, and
// they now live in `node_parameters.hpp`.
inline void declare_every_seedable_type(rclcpp::Node& node) {
    const bool verbose = node.declare_parameter<bool>("verbose", false);
    const int64_t depth = node.declare_parameter<int64_t>("queue_depth", 10);
    const int narrow = node.declare_parameter<int>("narrow", 1);
    const double period = node.declare_parameter<double>("ctrl_period", 0.15);

    (void)verbose;
    (void)depth;
    (void)narrow;
    (void)period;
}

// A launch parameter reaches the node because `declare_parameter` declares into
// the store the seed was written to and reads back what is there --
// `ALREADY_EXISTS` then a get, no copying between stores. Reading them back
// through the two-argument form, plus a set and a `has`, pins the whole
// forwarder set in the configuration that actually links it.
inline void read_back(rclcpp::Node& node) {
    bool b = false;
    int64_t i = 0;
    double d = 0.0;
    (void)node.get_parameter<bool>("verbose", b);
    (void)node.get_parameter<int64_t>("queue_depth", i);
    (void)node.get_parameter<double>("ctrl_period", d);
    (void)node.set_parameter<double>("ctrl_period", 0.05).ok();
    (void)node.has_parameter("ctrl_period");
}

// `std::string`-KEYED (not std::string-valued) -- how rclcpp itself keys
// parameters, and the reason the shim carries a second set of overloads.
inline void declare_string_keyed(rclcpp::Node& node) {
    const std::string prefix("ctrl.");
    (void)node.declare_parameter<double>(prefix + "gain", 1.0);
    (void)node.get_parameter<double>(prefix + "gain");
    (void)node.has_parameter(prefix + "gain");
    (void)node.set_parameter<double>(prefix + "gain", 2.0).ok();
}

} // namespace nros_cpp_ros2_param_launch_seed_test
