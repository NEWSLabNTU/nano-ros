// issue 2024 -- `rclcpp::ParameterEventHandler`, the SUBSCRIBING half of
// `/parameter_events`, on a hosted build.
//
//   * `add_parameter_callback(name, cb[, node])` gets a `rclcpp::Parameter` for
//     each new or changed value of that parameter of that node -- this node by
//     default, a relative node name resolved against this node's namespace --
//     and nothing for another node, another name, or a deletion;
//   * `add_parameter_event_callback(cb)` gets every event, decoded into the
//     type the callback takes (here a test-local stand-in for the generated
//     `rcl_interfaces::msg::ParameterEvent`);
//   * dropping a handle unregisters it, and so does destroying the handler.
//
// A RUN on the stub backend, which delivers the frames this test stages on the
// `/parameter_events` subscription the handler created
// (`nros_stub_rmw_stage_sample`). Every frame is encoded here as a ROS 2 peer
// writes it: little-endian CDR of `rcl_interfaces/msg/ParameterEvent`.

#include "rclcpp/rclcpp.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <cstdio>
#include <cstring>
#include <memory>
#include <string>
#include <vector>

extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

namespace {

int g_failures = 0;

void check(bool cond, const char* what) {
    if (!cond) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s\n", what);
    }
}

/// A little-endian CDR writer for one ParameterEvent frame.
struct Writer {
    std::vector<uint8_t> b{0x00, 0x01, 0x00, 0x00};
    void align(size_t n) {
        while ((b.size() - 4) % n != 0)
            b.push_back(0);
    }
    template <typename T> void put(T v, size_t a) {
        align(a);
        const uint8_t* p = reinterpret_cast<const uint8_t*>(&v);
        b.insert(b.end(), p, p + sizeof(T));
    }
    void u32(uint32_t v) { put(v, 4); }
    void str(const std::string& s) {
        u32(static_cast<uint32_t>(s.size() + 1));
        b.insert(b.end(), s.begin(), s.end());
        b.push_back(0);
    }
};

struct Value {
    uint8_t type = 0;
    bool b = false;
    int64_t i = 0;
    double d = 0.0;
    std::string s;
    std::vector<double> doubles;
};

void param(Writer& w, const std::string& name, const Value& v) {
    w.str(name);
    w.put<uint8_t>(v.type, 1);
    w.put<uint8_t>(v.b ? 1 : 0, 1);
    w.put<int64_t>(v.i, 8);
    w.put<double>(v.d, 8);
    w.str(v.s);
    w.u32(0); // byte_array
    w.u32(0); // bool_array
    w.u32(0); // integer_array
    w.u32(static_cast<uint32_t>(v.doubles.size()));
    for (double x : v.doubles)
        w.put<double>(x, 8);
    w.u32(0); // string_array
}

struct Entry {
    std::string name;
    Value v;
};

void stage(const std::string& node, const std::vector<Entry>& fresh,
           const std::vector<Entry>& changed, const std::vector<Entry>& deleted) {
    Writer w;
    w.put<int32_t>(1, 4); // stamp.sec
    w.u32(2);             // stamp.nanosec
    w.str(node);
    for (const std::vector<Entry>* list : {&fresh, &changed, &deleted}) {
        w.u32(static_cast<uint32_t>(list->size()));
        for (const Entry& e : *list)
            param(w, e.name, e.v);
    }
    check(nros_stub_rmw_stage_sample("parameter_events", w.b.data(), w.b.size()),
          "stage a ParameterEvent frame");
}

Value integer(int64_t i) {
    Value v;
    v.type = rclcpp::PARAMETER_INTEGER;
    v.i = i;
    return v;
}

/// Stands in for the generated `rcl_interfaces::msg::ParameterEvent`: what an
/// event callback takes is whatever its own `ffi_deserialize` reads.
struct FakeParameterEvent {
    std::string node;
    static int ffi_deserialize(const uint8_t* data, size_t len, void* out) {
        // stamp (8) at 4, then the node string.
        if (len < 16) return -1;
        uint32_t n = 0;
        std::memcpy(&n, data + 12, 4);
        if (n == 0 || 16 + n > len) return -1;
        static_cast<FakeParameterEvent*>(out)->node.assign(
            reinterpret_cast<const char*>(data + 16), n - 1);
        return 0;
    }
};

void pump(const rclcpp::Node::SharedPtr& node) {
    for (int i = 0; i < 4; ++i)
        rclcpp::spin_some(node);
}

} // namespace

int main() {
    nros_stub_rmw_set_accept_entities(true);
    // The stub stages frames without an arrival callback, so the executor must
    // POLL the subscription rather than wait to be woken for it.
    nros_stub_rmw_set_entity_wake_supported(false);
    if (!rclcpp::init_with_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "i2024").ok()) {
        std::fprintf(stderr, "parameter_event_handler_runtime: init on the stub failed\n");
        return 1;
    }
    auto node = rclcpp::Node::make_shared("watcher", "/ns");
    check(node && node->initialized(), "a node comes up on the stub backend");

    auto handler = std::make_shared<rclcpp::ParameterEventHandler>(node);
    check(std::strcmp(nros_stub_rmw_last_entity_name(), "/parameter_events") == 0,
          "the handler subscribed to /parameter_events");

    std::vector<std::string> seen;
    int64_t gain = 0;
    auto own = handler->add_parameter_callback("gain", [&](const rclcpp::Parameter& p) {
        seen.push_back("own:" + p.get_name());
        gain = p.as_int();
        check(p.get_type() == rclcpp::PARAMETER_INTEGER, "the value arrives typed INTEGER");
    });
    std::string label;
    auto other = handler->add_parameter_callback(
        "label",
        [&](const rclcpp::Parameter& p) {
            seen.push_back("other:" + p.get_name());
            label = p.as_string();
        },
        "talker"); // relative: resolves to /ns/talker
    std::vector<double> weights;
    auto abs = handler->add_parameter_callback(
        "weights", [&](const rclcpp::Parameter& p) { weights = p.as_double_array(); }, "/mpc");
    std::vector<std::string> events;
    auto every = handler->add_parameter_event_callback(
        [&](const FakeParameterEvent& ev) { events.push_back(ev.node); });

    // This node's own `gain`, changed.
    stage("/ns/watcher", {}, {{"gain", integer(42)}}, {});
    pump(node);
    check(gain == 42, "the per-parameter callback gets this node's changed value");
    check(events.size() == 1 && events[0] == "/ns/watcher",
          "the event callback gets the event, decoded into its own type");

    // A relative node name, a string value, among other parameters.
    Value s;
    s.type = rclcpp::PARAMETER_STRING;
    s.s = "hello";
    stage("/ns/talker", {{"rate", integer(3)}, {"label", s}}, {}, {});
    pump(node);
    check(label == "hello", "a relative node_name resolves against this node's namespace");

    // An absolute node name and an array value.
    Value arr;
    arr.type = rclcpp::PARAMETER_DOUBLE_ARRAY;
    arr.doubles = {1.5, 2.5};
    stage("/mpc", {}, {{"weights", arr}}, {});
    pump(node);
    check(weights.size() == 2 && weights[0] == 1.5 && weights[1] == 2.5,
          "an absolute node_name, and a double array decoded whole");

    // Another node's `gain`, and a DELETED `gain` here: neither calls `own`.
    seen.clear();
    stage("/ns/elsewhere", {}, {{"gain", integer(7)}}, {});
    stage("/ns/watcher", {}, {}, {{"gain", integer(0)}});
    pump(node);
    check(gain == 42 && seen.empty(),
          "no callback for another node's parameter, nor for a deletion");
    check(events.size() == 5, "...while the event callback sees every event");

    // Dropping a handle unregisters it.
    own.reset();
    stage("/ns/watcher", {}, {{"gain", integer(9)}}, {});
    pump(node);
    check(gain == 42, "a dropped parameter-callback handle is unregistered");
    handler->remove_parameter_event_callback(every);
    const size_t before = events.size();
    stage("/ns/watcher", {}, {{"gain", integer(10)}}, {});
    pump(node);
    check(events.size() == before, "remove_parameter_event_callback unregisters");

    // A truncated frame is dropped, not misread.
    {
        Writer w;
        w.put<int32_t>(1, 4);
        w.u32(2);
        w.str("/ns/talker");
        w.u32(1); // one new parameter ... and no bytes for it
        check(nros_stub_rmw_stage_sample("parameter_events", w.b.data(), w.b.size()),
              "stage a truncated frame");
        label.clear();
        pump(node);
        check(label.empty(), "a truncated frame reaches no parameter callback");
    }

    // Destroying the handler stops every callback, even a live handle's.
    handler.reset();
    stage("/ns/talker", {}, {{"label", s}}, {});
    pump(node);
    check(label.empty(), "a destroyed handler calls nothing");
    (void)other;
    (void)abs;

    rclcpp::shutdown();
    if (g_failures != 0) {
        std::fprintf(stderr, "parameter_event_handler_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("parameter_event_handler_runtime: OK\n");
    return 0;
}
