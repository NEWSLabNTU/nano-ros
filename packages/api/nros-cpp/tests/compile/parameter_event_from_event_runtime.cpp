// issue 2080 -- `rclcpp::ParameterEventHandler::get_parameter_from_event` /
// `get_parameters_from_event`, the static helpers that read a RECEIVED
// `rcl_interfaces::msg::ParameterEvent`, on a hosted build.
//
// The helpers are templates over the caller's generated event type, so this
// file declares `rcl_interfaces::msg::{ParameterValue, Parameter,
// ParameterEvent}` with the field types the nano-ros C++ codegen emits for
// them (`rclcpp::FixedString` / `rclcpp::FixedSequence` in inline mode, and a
// `rclcpp::HeapString` variant for heap mode), and calls the helpers with
// upstream's call syntax. Nothing here names a nano-ros-only spelling.

#include "rclcpp/rclcpp.hpp"

// What a generated message header includes for its field types.
#include "rclcpp/fixed_sequence.hpp"
#include "rclcpp/fixed_string.hpp"
#include "rclcpp/heap_string.hpp"

#include <cstdio>
#include <string>
#include <vector>

extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {}

// What `nros generate-cpp` emits for these messages (inline mode), trimmed to
// the fields: unbounded strings become `FixedString<N>`, unbounded sequences
// `FixedSequence<T, 64>`.
namespace builtin_interfaces {
namespace msg {
struct Time {
    int32_t sec = {};
    uint32_t nanosec = {};
};
} // namespace msg
} // namespace builtin_interfaces

namespace rcl_interfaces {
namespace msg {
struct ParameterValue {
    uint8_t type = {};
    bool bool_value = {};
    int64_t integer_value = {};
    double double_value = {};
    rclcpp::FixedString<256> string_value = {};
    rclcpp::FixedSequence<uint8_t, 64> byte_array_value = {};
    rclcpp::FixedSequence<bool, 64> bool_array_value = {};
    rclcpp::FixedSequence<int64_t, 64> integer_array_value = {};
    rclcpp::FixedSequence<double, 64> double_array_value = {};
    rclcpp::FixedSequence<rclcpp::FixedString<256>, 64> string_array_value = {};
};
struct Parameter {
    rclcpp::FixedString<256> name = {};
    rcl_interfaces::msg::ParameterValue value = {};
};
struct ParameterEvent {
    builtin_interfaces::msg::Time stamp = {};
    rclcpp::FixedString<256> node = {};
    rclcpp::FixedSequence<rcl_interfaces::msg::Parameter, 64> new_parameters = {};
    rclcpp::FixedSequence<rcl_interfaces::msg::Parameter, 64> changed_parameters = {};
    rclcpp::FixedSequence<rcl_interfaces::msg::Parameter, 64> deleted_parameters = {};
};
} // namespace msg
} // namespace rcl_interfaces

// The heap-mode spelling of the string fields (`rclcpp::HeapString`).
namespace heap_mode {
struct Parameter {
    rclcpp::HeapString name;
    rcl_interfaces::msg::ParameterValue value = {};
};
struct ParameterEvent {
    rclcpp::HeapString node;
    std::vector<Parameter> new_parameters;
    std::vector<Parameter> changed_parameters;
};
} // namespace heap_mode

namespace {

int g_failures = 0;

void check(bool cond, const char* what) {
    if (!cond) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s\n", what);
    }
}

rcl_interfaces::msg::Parameter integer(const char* name, int64_t v) {
    rcl_interfaces::msg::Parameter p;
    p.name = name;
    p.value.type = rclcpp::PARAMETER_INTEGER;
    p.value.integer_value = v;
    return p;
}

} // namespace

int main() {
    using rclcpp::ParameterEventHandler;

    rcl_interfaces::msg::ParameterEvent event;
    event.node = "/ns/talker";
    event.new_parameters.push_back(integer("rate", 3));
    {
        rcl_interfaces::msg::Parameter s;
        s.name = "label";
        s.value.type = rclcpp::PARAMETER_STRING;
        s.value.string_value = "hello";
        event.new_parameters.push_back(s);
    }
    event.changed_parameters.push_back(integer("gain", 42));
    {
        rcl_interfaces::msg::Parameter a;
        a.name = "weights";
        a.value.type = rclcpp::PARAMETER_DOUBLE_ARRAY;
        a.value.double_array_value.push_back(1.5);
        a.value.double_array_value.push_back(2.5);
        event.changed_parameters.push_back(a);
    }
    {
        rcl_interfaces::msg::Parameter a;
        a.name = "names";
        a.value.type = rclcpp::PARAMETER_STRING_ARRAY;
        rclcpp::FixedString<256> x;
        x = "a";
        a.value.string_array_value.push_back(x);
        x = "bc";
        a.value.string_array_value.push_back(x);
        event.changed_parameters.push_back(a);
    }
    {
        rcl_interfaces::msg::Parameter b;
        b.name = "flags";
        b.value.type = rclcpp::PARAMETER_BOOL_ARRAY;
        b.value.bool_array_value.push_back(true);
        b.value.bool_array_value.push_back(false);
        event.changed_parameters.push_back(b);
    }
    event.deleted_parameters.push_back(integer("old", 0));

    // bool overload: a new parameter, a changed one, an array.
    rclcpp::Parameter p;
    check(ParameterEventHandler::get_parameter_from_event(event, p, "label", "/ns/talker"),
          "a new parameter is found");
    check(p.get_name() == "label" && p.get_type() == rclcpp::PARAMETER_STRING &&
              p.as_string() == "hello",
          "...decoded as a STRING with its value");
    check(ParameterEventHandler::get_parameter_from_event(event, p, "gain", "/ns/talker"),
          "a changed parameter is found");
    check(p.get_type() == rclcpp::PARAMETER_INTEGER && p.as_int() == 42,
          "...decoded as an INTEGER with its value");
    check(ParameterEventHandler::get_parameter_from_event(event, p, "weights", "/ns/talker") &&
              p.as_double_array() == std::vector<double>({1.5, 2.5}),
          "a double array decodes whole");
    check(ParameterEventHandler::get_parameter_from_event(event, p, "names", "/ns/talker") &&
              p.as_string_array() == std::vector<std::string>({"a", "bc"}),
          "a string array decodes whole");
    check(ParameterEventHandler::get_parameter_from_event(event, p, "flags", "/ns/talker") &&
              p.as_bool_array() == std::vector<bool>({true, false}),
          "a bool array decodes whole");

    // Not found: another node, a deleted parameter, an absent name.
    rclcpp::Parameter untouched("keep", rclcpp::ParameterValue(7));
    check(!ParameterEventHandler::get_parameter_from_event(event, untouched, "gain", "/other"),
          "another node's event does not match");
    check(!ParameterEventHandler::get_parameter_from_event(event, untouched, "old", "/ns/talker"),
          "a DELETED parameter is not reported");
    check(!ParameterEventHandler::get_parameter_from_event(event, untouched, "nope", "/ns/talker"),
          "an absent name is not reported");
    check(!ParameterEventHandler::get_parameter_from_event(event, untouched, "gain"),
          "node_name defaults to \"\", compared verbatim as upstream");
    check(untouched.get_name() == "keep" && untouched.as_int() == 7,
          "a miss leaves the out-parameter alone");

    // Value overload.
    rclcpp::Parameter got =
        ParameterEventHandler::get_parameter_from_event(event, "rate", "/ns/talker");
    check(got.get_name() == "rate" && got.as_int() == 3,
          "the value overload returns the parameter");
    rclcpp::Parameter missing =
        ParameterEventHandler::get_parameter_from_event(event, "nope", "/ns/talker");
    check(missing.get_name() == "nope" && missing.get_type() == rclcpp::PARAMETER_NOT_SET,
          "an absent name returns a NOT_SET parameter of that name, as upstream");
    rclcpp::Parameter wrong_node =
        ParameterEventHandler::get_parameter_from_event(event, "gain", "/other");
    check(wrong_node.get_type() == rclcpp::PARAMETER_NOT_SET,
          "a node mismatch returns NOT_SET (upstream throws; nothing here does)");

    // Every new + changed parameter, in order; deletions excluded.
    std::vector<rclcpp::Parameter> all = ParameterEventHandler::get_parameters_from_event(event);
    check(all.size() == 6, "get_parameters_from_event returns new + changed, not deleted");
    if (all.size() == 6) {
        check(all[0].get_name() == "rate" && all[1].get_name() == "label" &&
                  all[2].get_name() == "gain" && all[5].get_name() == "flags",
              "...new first, then changed, each in event order");
    }

    // Heap-mode string fields go through the same template.
    heap_mode::ParameterEvent h;
    h.node = std::string("/mpc");
    heap_mode::Parameter hp;
    hp.name = std::string("horizon");
    hp.value.type = rclcpp::PARAMETER_DOUBLE;
    hp.value.double_value = 0.25;
    h.changed_parameters.push_back(static_cast<heap_mode::Parameter&&>(hp));
    rclcpp::Parameter hv;
    check(ParameterEventHandler::get_parameter_from_event(h, hv, "horizon", "/mpc") &&
              hv.as_double() == 0.25,
          "HeapString fields read the same way");
    check(ParameterEventHandler::get_parameters_from_event(h).size() == 1,
          "get_parameters_from_event over the heap-mode event");

    if (g_failures != 0) {
        std::fprintf(stderr, "parameter_event_from_event_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("parameter_event_from_event_runtime: OK\n");
    return 0;
}
