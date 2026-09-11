// POSITIVE compile probe — the HOSTED parameter overloads, on the node type.
//
// phase-426 W4. `NROS_CPP_STD` gates the `std::string`-VALUED and
// `std::vector<T>` parameter overloads, and until this file NOTHING IN THE TREE
// COMPILED THEM:
// `just check cpp` parses every header at `-std=c++14 -ffreestanding` without
// the macro, the `-nostdinc++` probes are the opposite configuration, and the
// only in-tree definer is `examples/px4/cpp/bridge/.../CMakeLists.txt:123`, on
// one module of a build no gate runs. So `declare_parameter<std::vector<double>>`
// — the ASI weight-matrix path, and the reason the array half of the parameter
// FFI exists — shipped unparsed, both before this wave and after it.
//
// That is the `check-required-features-reachable` class one language over: code
// behind a switch no lane flips reads as coverage and asserts nothing. W4
// rewrote exactly this code (from a per-node `Seq<T, N>` in an inline pool to
// the executor store's own array value), so the wave that changed it is the
// wave that owes it a compile.
//
// TWO macros, both in-file rather than on the command line so the lane needs no
// special flags:
//
//   * `NROS_CPP_STD` — the hosted opt-in. Defining it changes what EVERY
//     nano-ros header does in this TU (the flag-gated-field hazard of issue
//     0135), which is why it belongs in a TU of its own and not on the header
//     sweep. px4 does the same thing deliberately, one module at a time.
//   * `NROS_SYSTEM_PARAM_SERVICES` — what a bringup declaring `param_services`
//     defines. Nothing in the headers branches on it since W4, but this is the
//     configuration in which the parameter FFI is actually linked.
//
// `-std=c++17`, matching the component lane.

#define NROS_CPP_STD 1
#define NROS_SYSTEM_PARAM_SERVICES 1

#include <nros/nros.hpp>

#include <string>
#include <vector>

namespace nros_cpp_param_hosted_overloads_test {

// --- rclcpp::Node ------------------------------------------------------------
//
// `std::string`-VALUED (not merely string-keyed): the controller-mode /
// solver-type knobs a real ported node declares.
inline void rclcpp_node_string_values(rclcpp::Node& node) {
    const std::string mode = node.declare_parameter<std::string>("controller_mode", "mpc");
    (void)mode;

    std::string read_back;
    (void)node.get_parameter<std::string>("controller_mode", read_back);
    (void)node.get_parameter<std::string>("controller_mode");
    (void)node.set_parameter<std::string>("controller_mode", std::string("pid")).ok();

    // ...and the same through a `std::string` KEY, which is how rclcpp keys.
    const std::string key("controller_mode");
    (void)node.declare_parameter<std::string>(key, "mpc");
    (void)node.get_parameter<std::string>(key);
    (void)node.has_parameter(key);
}

// --- rclcpp::Node, subclassed --------------------------------------------------
//
// The value-returning facade, including the `std::vector<T>` overloads that had
// never been compiled. Written as a derived ctor because that is where a
// component declares its parameters, and because a member template is only
// instantiated when something calls it.
//
// phase-427 W4 — this said `nros::ComponentNode`, the type that WRAPPED a node.
// It is deleted; a component IS-A `rclcpp::Node` now, and the facade this probe
// compiles is the same one `rclcpp::Node` above wears, because they are one
// type. So the two halves of this file are no longer two facades that could
// disagree — they are two call SHAPES (a free function taking a `Node&`, and a
// ctor on a subclass of it) over one.
class HostedParamNode : public ::rclcpp::Node {
  public:
    explicit HostedParamNode(::nros::NodeHandle h) : ::rclcpp::Node(h, "hosted_params") {
        // Scalars, `const char*` keyed and `std::string` keyed.
        const double period = this->declare_parameter<double>("ctrl_period", 0.15);
        const int64_t depth = this->declare_parameter<int64_t>("queue_depth", 10);
        const int narrow = this->declare_parameter<int>("narrow", 1);
        const bool verbose = this->declare_parameter<bool>("verbose", false);
        (void)period;
        (void)depth;
        (void)narrow;
        (void)verbose;

        // `std::string` VALUES.
        const std::string mode = this->declare_parameter<std::string>("mode", "mpc");
        (void)mode;
        (void)this->get_parameter<std::string>("mode");

        // `std::vector<T>` VALUES — the weight-matrix path. Element types are
        // the three the store carries; `std::vector<bool>` is deliberately not
        // among them (it has no `data()`, and did not compile before W4 either).
        const std::vector<double> weights =
            this->declare_parameter<std::vector<double>>("mpc_weights", {1.5, 2.5, 3.5});
        const std::vector<int64_t> horizons =
            this->declare_parameter<std::vector<int64_t>>("horizons", {10, 20});
        (void)weights;
        (void)horizons;

        (void)this->get_parameter<std::vector<double>>("mpc_weights");
        (void)this->get_parameter<std::vector<int64_t>>("horizons");

        // `std::string`-KEYED, covering scalar and vector through one overload.
        const std::string prefix("ctrl.");
        (void)this->declare_parameter<double>(prefix + "gain", 1.0);
        (void)this->declare_parameter<std::vector<double>>(prefix + "weights", {1.0});
        (void)this->get_parameter<double>(prefix + "gain");
        (void)this->has_parameter(prefix + "gain");
    }
};

} // namespace nros_cpp_param_hosted_overloads_test
