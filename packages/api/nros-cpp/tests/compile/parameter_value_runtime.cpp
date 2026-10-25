// issue 2056 — upstream's value-object parameter surface, on the one store.
//
// Ported code declares through `rclcpp::ParameterValue`, reads back through
// `rclcpp::Parameter`, and vetoes writes with a callback over
// `std::vector<rclcpp::Parameter>` that returns a `SetParametersResult`. Nav2's
// `nav2_velocity_smoother` does all three. This RUN checks each against the
// executor's store, including a `~/set_parameters` request staged on the stub
// backend, so the callback is shown voting on a REMOTE write and not only on
// one this file made.
//
// Built against an `nros-cpp` WITH `param-services` (`just check cpp`).

#define NROS_CPP_STD 1
#define NROS_SYSTEM_PARAM_SERVICES 1

#include "rclcpp/rclcpp.hpp"
#include "rclcpp/fixed_sequence.hpp"
#include "rclcpp/fixed_string.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

// The two `rcl_interfaces` messages the surface binds to, in the shape the
// codegen emits them (`FixedString` text, `FixedSequence` ranges). In a real
// build they come from the user's generated `rcl_interfaces` package.
namespace rcl_interfaces {
namespace msg {
struct FloatingPointRange {
    double from_value = 0.0;
    double to_value = 0.0;
    double step = 0.0;
};
struct IntegerRange {
    int64_t from_value = 0;
    int64_t to_value = 0;
    uint64_t step = 0;
};
struct ParameterDescriptor {
    rclcpp::FixedString<256> name = {};
    uint8_t type_ = {};
    rclcpp::FixedString<256> description = {};
    rclcpp::FixedString<256> additional_constraints = {};
    bool read_only = {};
    bool dynamic_typing = {};
    rclcpp::FixedSequence<FloatingPointRange, 1> floating_point_range = {};
    rclcpp::FixedSequence<IntegerRange, 1> integer_range = {};
};
struct SetParametersResult {
    bool successful = {};
    rclcpp::FixedString<256> reason = {};
};
} // namespace msg
} // namespace rcl_interfaces

namespace {

int g_failures = 0;

void check(bool cond, const char* what) {
    if (!cond) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s\n", what);
    }
}

void spin(rclcpp::Executor& exec) {
    for (int i = 0; i < 5; ++i)
        (void)exec.spin_once(1);
}

/// A little-endian CDR writer, enough for one `SetParameters` request.
struct Writer {
    uint8_t buf[256] = {0x00, 0x01, 0x00, 0x00};
    size_t pos = 4;
    void align(size_t n) {
        while ((pos - 4) % n != 0)
            buf[pos++] = 0;
    }
    void u8(uint8_t v) { buf[pos++] = v; }
    void u32(uint32_t v) {
        align(4);
        for (int i = 0; i < 4; ++i)
            buf[pos++] = uint8_t(v >> (8 * i));
    }
    void u64(uint64_t v) {
        align(8);
        for (int i = 0; i < 8; ++i)
            buf[pos++] = uint8_t(v >> (8 * i));
    }
    void f64(double d) {
        uint64_t v;
        std::memcpy(&v, &d, sizeof v);
        u64(v);
    }
    void str(const char* s) {
        const uint32_t n = uint32_t(std::strlen(s) + 1);
        u32(n);
        std::memcpy(buf + pos, s, n);
        pos += n;
    }
};

/// `SetParameters_Request` holding ONE double-array parameter.
size_t set_double_array_request(Writer& w, const char* name, const std::vector<double>& v) {
    w.u32(1);                             // parameters: 1
    w.str(name);                          // Parameter.name
    w.u8(rclcpp::PARAMETER_DOUBLE_ARRAY); // ParameterValue.type
    w.u8(0);                              // bool_value
    w.u64(0);                             // integer_value
    w.f64(0.0);                           // double_value
    w.str("");                            // string_value
    w.u32(0);                             // byte_array_value
    w.u32(0);                             // bool_array_value
    w.u32(0);                             // integer_array_value
    w.u32(uint32_t(v.size()));            // double_array_value
    for (double d : v)
        w.f64(d);
    w.u32(0); // string_array_value
    return w.pos;
}

/// `successful` of the first result in a `SetParameters_Response`, or -1.
int first_result(const uint8_t* p, size_t len) {
    // [encap 4][count u32][successful u8]...
    if (p == nullptr || len < 9) return -1;
    const uint32_t n =
        uint32_t(p[4]) | uint32_t(p[5]) << 8 | uint32_t(p[6]) << 16 | uint32_t(p[7]) << 24;
    if (n != 1) return -1;
    return p[8] != 0 ? 1 : 0;
}

bool remote_set(rclcpp::Executor& exec, int64_t seq, const char* name, const std::vector<double>& v,
                int* successful) {
    Writer w;
    const size_t len = set_double_array_request(w, name, v);
    if (!nros_stub_rmw_stage_request("/pv_node/set_parameters", seq, w.buf, len)) return false;
    for (int i = 0; i < 50; ++i) {
        (void)exec.spin_once(1);
        int64_t got_seq = -1;
        size_t got_len = 0;
        const uint8_t* got =
            nros_stub_rmw_last_response("/pv_node/set_parameters", &got_seq, &got_len);
        if (got != nullptr && got_seq == seq) {
            *successful = first_result(got, got_len);
            return true;
        }
    }
    return false;
}

/// What the callback was shown, and its veto: no negative element.
struct Smoother {
    std::vector<std::string> seen_names;
    std::vector<rclcpp::ParameterType> seen_types;
    std::vector<double> last_array;

    rcl_interfaces::msg::SetParametersResult on_set(std::vector<rclcpp::Parameter> parameters) {
        rcl_interfaces::msg::SetParametersResult result;
        result.successful = true;
        for (auto parameter : parameters) {
            seen_names.push_back(parameter.get_name());
            seen_types.push_back(parameter.get_type());
            if (parameter.get_type() == rclcpp::ParameterType::PARAMETER_DOUBLE_ARRAY) {
                last_array = parameter.as_double_array();
                for (double d : parameter.as_double_array()) {
                    if (d < 0.0) {
                        result.successful = false;
                        result.reason = "negative";
                    }
                }
            }
        }
        return result;
    }
};

} // namespace

int main() {
    nros_stub_rmw_set_accept_entities(true);
    rclcpp::Executor exec;
    check(rclcpp::Executor::create_with_rmw(exec, NROS_STUB_RMW_NAME, nullptr, 0, "i2056").ok(),
          "executor create on the stub backend");
    rclcpp::Node node;
    check(exec.create_node(node, "pv_node").ok(), "create the node");
    check(node.start_parameter_services().ok(), "start the parameter services");

    // --- ParameterValue: value semantics, no node ----------------------------
    {
        rclcpp::ParameterValue a(std::vector<double>{0.5, 0.0, 2.5});
        rclcpp::ParameterValue b = a;
        check(a == b && b.get_type() == rclcpp::PARAMETER_DOUBLE_ARRAY,
              "a copied double-array value equals its source");
        check(b.as_double_array().size() == 3 && b.as_double_array()[2] == 2.5,
              "the copy owns the elements");
        rclcpp::ParameterValue c(std::move(a));
        check(c == b, "a moved value keeps its elements");
        check(rclcpp::ParameterValue(20).get<int>() == 20, "get<int>() on an integer value");
        check(rclcpp::ParameterValue(1.5).as_int() == 0,
              "a wrong-type read answers the zero value");
        check(rclcpp::ParameterValue("odom").as_string() == "odom",
              "a string literal builds a STRING value, not a bool");
        check(rclcpp::ParameterValue().get_type() == rclcpp::PARAMETER_NOT_SET,
              "a default value is NOT_SET");
    }

    // --- declare through ParameterValue -------------------------------------
    {
        rclcpp::ParameterValue v =
            node.declare_parameter("smoothing_frequency", rclcpp::ParameterValue(20.0));
        check(v.get_type() == rclcpp::PARAMETER_DOUBLE && v.as_double() == 20.0,
              "declare_parameter(name, ParameterValue(double)) answers the value in effect");
        double f = 0.0;
        check(node.get_parameter("smoothing_frequency", f) && f == 20.0,
              "...and the typed read sees the same store");

        (void)node.declare_parameter(std::string("feedback"),
                                     rclcpp::ParameterValue(std::string("OPEN_LOOP")));
        std::string fb;
        check(node.get_parameter("feedback", fb) && fb == "OPEN_LOOP",
              "a std::string-keyed STRING declare");

        (void)node.declare_parameter("scale_velocities", rclcpp::ParameterValue(false));
        (void)node.declare_parameter("max_velocity",
                                     rclcpp::ParameterValue(std::vector<double>{0.5, 0.0, 2.5}));
        std::vector<double> mv;
        check(node.get_parameter("max_velocity", mv) && mv.size() == 3 && mv[2] == 2.5,
              "a DOUBLE_ARRAY declare reaches the store");

        (void)node.declare_parameter("flags",
                                     rclcpp::ParameterValue(std::vector<bool>{true, false}));
        std::vector<bool> fl;
        check(node.get_parameter("flags", fl) && fl.size() == 2 && fl[0] && !fl[1],
              "a BOOL_ARRAY declare and read (std::vector<bool>)");

        check(node.declare_parameter("names", rclcpp::ParameterValue(std::vector<std::string>{"a"}))
                      .get_type() == rclcpp::PARAMETER_NOT_SET,
              "a STRING_ARRAY has no store form and answers NOT_SET");
        check(!node.has_parameter("names"), "...and declares nothing");
    }

    // --- declare with upstream's descriptor MESSAGE --------------------------
    {
        rcl_interfaces::msg::ParameterDescriptor d;
        d.description = "read only";
        d.read_only = true;
        (void)node.declare_parameter("frozen", rclcpp::ParameterValue(int64_t(7)), d);
        check(node.get_parameter<int64_t>("frozen") == 7, "declare with a descriptor message");
        check(!node.set_parameter(rclcpp::Parameter("frozen", int64_t(8))).successful,
              "the message's read_only reached the store");

        rcl_interfaces::msg::ParameterDescriptor r;
        rcl_interfaces::msg::FloatingPointRange range;
        range.from_value = 0.0;
        range.to_value = 10.0;
        (void)r.floating_point_range.push_back(range);
        (void)node.declare_parameter("bounded", 5.0, r);
        check(!node.set_parameter(rclcpp::Parameter("bounded", 11.0)).successful,
              "the message's floating_point_range reached the store (typed declare)");
        check(node.set_parameter(rclcpp::Parameter("bounded", 9.0)).successful,
              "...and admits a value inside it");
    }

    // --- get_parameter as a Parameter ---------------------------------------
    {
        rclcpp::Parameter p = node.get_parameter("max_velocity");
        check(p.get_name() == "max_velocity", "get_parameter(name) names the parameter");
        check(p.get_type() == rclcpp::PARAMETER_DOUBLE_ARRAY && p.as_double_array().size() == 3,
              "...and carries its value");
        rclcpp::Parameter missing;
        check(!node.get_parameter("no_such", missing),
              "get_parameter(name, out) on an undeclared name");
        check(node.get_parameter("no_such").get_type() == rclcpp::PARAMETER_NOT_SET,
              "get_parameter(name) on an undeclared name is NOT_SET");
    }

    // --- the vector on-set callback ------------------------------------------
    Smoother sm;
    using std::placeholders::_1;
    rclcpp::node_interfaces::OnSetParametersCallbackHandle::SharedPtr handle =
        node.add_on_set_parameters_callback(std::bind(&Smoother::on_set, &sm, _1));
    check(static_cast<bool>(handle), "add_on_set_parameters_callback(std::bind) registers");

    check(node.set_parameter(rclcpp::Parameter("max_velocity", std::vector<double>{1.0, 2.0, 3.0}))
              .successful,
          "a local set the callback accepts succeeds");
    check(sm.seen_names.size() == 1 && sm.seen_names[0] == "max_velocity",
          "the callback saw the write, by name");
    check(sm.seen_types.size() == 1 && sm.seen_types[0] == rclcpp::PARAMETER_DOUBLE_ARRAY,
          "...as a DOUBLE_ARRAY");
    check(sm.last_array.size() == 3 && sm.last_array[1] == 2.0,
          "...with the proposed ELEMENTS (the write's array fields)");

    check(
        !node.set_parameter(rclcpp::Parameter("max_velocity", std::vector<double>{-1.0, 0.0, 1.0}))
             .successful,
        "a local set the callback refuses fails");
    std::vector<double> mv;
    check(node.get_parameter("max_velocity", mv) && mv[0] == 1.0,
          "...and leaves the stored value unchanged");

    // A remote `ros2 param set`, through the served `~/set_parameters`.
    int ok = -1;
    const size_t seen_before = sm.seen_names.size();
    check(remote_set(exec, 31, "max_velocity", {-0.5, 0.0, -2.5}, &ok),
          "the set_parameters service answers");
    check(ok == 0, "the callback's refusal is the remote caller's answer");
    check(sm.seen_names.size() == seen_before + 1, "the callback saw the remote write");
    check(node.get_parameter("max_velocity", mv) && mv[0] == 1.0,
          "a remotely refused write leaves the store unchanged");
    check(remote_set(exec, 32, "max_velocity", {0.25, 0.0, 2.0}, &ok) && ok == 1,
          "a remote write the callback accepts succeeds");
    check(node.get_parameter("max_velocity", mv) && mv[0] == 0.25, "...and is stored");

    // Unregister: early, then by destruction.
    node.remove_on_set_parameters_callback(handle.get());
    check(
        node.set_parameter(rclcpp::Parameter("max_velocity", std::vector<double>{-9.0})).successful,
        "remove_on_set_parameters_callback(handle.get()) stops the veto");
    {
        auto h2 = node.add_on_set_parameters_callback(std::bind(&Smoother::on_set, &sm, _1));
        check(!node.set_parameter(rclcpp::Parameter("max_velocity", std::vector<double>{-1.0}))
                   .successful,
              "a second registration vetoes");
    }
    check(
        node.set_parameter(rclcpp::Parameter("max_velocity", std::vector<double>{-2.0})).successful,
        "destroying the handle unregisters the callback");
    handle.reset();

    // --- set_parameters_atomically carries an array ---------------------------
    {
        const double elems[3] = {4.0, 5.0, 6.0};
        rclcpp::ParameterWrite w;
        std::memset(&w, 0, sizeof w);
        w.name = "max_velocity";
        w.param_type = rclcpp::PARAMETER_DOUBLE_ARRAY;
        w.double_array = elems;
        w.array_len = 3;
        check(node.set_parameters_atomically(&w, 1).ok(),
              "set_parameters_atomically accepts a double array");
        check(node.get_parameter("max_velocity", mv) && mv.size() == 3 && mv[2] == 6.0,
              "...and stores its elements");
        w.double_array = nullptr;
        check(!node.set_parameters_atomically(&w, 1).ok(),
              "an array write with a NULL pointer and a length is refused");
    }

    // --- NodeParametersInterface ---------------------------------------------
    {
        rclcpp::node_interfaces::NodeParametersInterface::SharedPtr pi =
            node.get_node_parameters_interface();
        check(pi->has_parameter("max_velocity"), "the interface sees the node's parameters");
        rcl_interfaces::msg::ParameterDescriptor d;
        check(
            pi->declare_parameter("via_interface", rclcpp::ParameterValue(3.0), d).get<double>() ==
                3.0,
            "declare through the interface, with a descriptor message");
        rclcpp::Parameter p("via_interface", 0.0);
        check(pi->get_parameter("via_interface", p) && p.get_value<double>() == 3.0,
              "get_parameter(name, Parameter&) through the interface");
    }

    spin(exec);
    if (g_failures) {
        std::fprintf(stderr, "parameter_value_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("parameter_value_runtime: ok\n");
    return 0;
}
