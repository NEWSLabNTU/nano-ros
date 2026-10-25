// issue 2035 -- `rclcpp::NodeOptions().start_parameter_event_publisher(false)`
// turns ONE node's `/parameter_events` off, as upstream's does. It used to be
// refused at compile time; the per-node opt-out existed only for C
// (`notify_changed_over_dds = false` -> `Executor::disable_parameter_events_on`).
//
// A RUN on the stub backend, built against an `nros-cpp` WITH `param-services`
// (the events exist only there). Two nodes on one executor, so the topic is
// live: the default node's set is published under its name, the opted-out
// node's set is not published at all, before or after the services come up.

#define NROS_CPP_STD 1
#define NROS_SYSTEM_PARAM_SERVICES 1

#include "rclcpp/rclcpp.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <cstdio>
#include <cstring>

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

/// What a `ParameterEvent` frame says, up to its first non-empty list: which
/// list (0 new, 1 changed, 2 deleted, -1 none) and the parameter's name.
struct Frame {
    bool ok = false;
    char node[64] = {0};
    int slot = -1;
    char name[64] = {0};
};

/// A little-endian CDR reader over one frame, enough for the fields above.
struct Reader {
    const uint8_t* p;
    size_t len;
    size_t pos = 4; // the 4-byte encapsulation header
    bool u32(uint32_t& out) {
        pos = 4 + ((pos - 4 + 3) & ~size_t(3));
        if (pos + 4 > len) return false;
        out = uint32_t(p[pos]) | uint32_t(p[pos + 1]) << 8 | uint32_t(p[pos + 2]) << 16 |
              uint32_t(p[pos + 3]) << 24;
        pos += 4;
        return true;
    }
    bool str(char* out, size_t cap) {
        uint32_t n = 0;
        if (!u32(n) || pos + n > len || n == 0 || n > cap) return false;
        std::memcpy(out, p + pos, n); // n includes the NUL
        out[n - 1] = '\0';
        pos += n;
        return true;
    }
};

Frame last_frame() {
    Frame f;
    size_t len = 0;
    const uint8_t* bytes = nros_stub_rmw_last_payload(&len);
    Reader r{bytes, len};
    uint32_t sec = 0, nanosec = 0;
    if (!r.u32(sec) || !r.u32(nanosec) || !r.str(f.node, sizeof f.node)) return f;
    for (int slot = 0; slot < 3; ++slot) {
        uint32_t n = 0;
        if (!r.u32(n)) return f;
        if (n == 0) continue;
        if (n != 1 || !r.str(f.name, sizeof f.name)) return f;
        f.slot = slot;
        break;
    }
    f.ok = true;
    return f;
}

void pump(const rclcpp::Node::SharedPtr& node) {
    for (int i = 0; i < 5; ++i)
        rclcpp::spin_some(node);
}

} // namespace

int main() {
    nros_stub_rmw_set_accept_entities(true);
    if (!rclcpp::init_with_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "i2035").ok()) {
        std::fprintf(stderr, "node_options_param_events_runtime: init on the stub failed\n");
        return 1;
    }

    // The option: a real setter / getter now, default on as upstream's.
    check(rclcpp::NodeOptions().start_parameter_event_publisher(),
          "start_parameter_event_publisher() defaults to true");
    const rclcpp::NodeOptions off = rclcpp::NodeOptions().start_parameter_event_publisher(false);
    check(!off.start_parameter_event_publisher(), "the setter is read back by the getter");

    auto on_node = rclcpp::Node::make_shared("pe_on");
    auto off_node = rclcpp::Node::make_shared("pe_off", off);
    check(on_node && on_node->initialized() && off_node && off_node->initialized(),
          "both nodes come up");
    check(!off_node->get_node_options().start_parameter_event_publisher(),
          "the node keeps the options it was given");
    (void)on_node->declare_parameter<int64_t>("gain", 1);
    (void)off_node->declare_parameter<int64_t>("gain", 1);

    check(on_node->start_parameter_services().ok(), "start the parameter services");
    pump(on_node);
    const uint32_t started = nros_stub_rmw_publish_count();
    Frame f = last_frame();
    check(f.ok && std::strcmp(f.node, "/pe_off") != 0,
          "the start-up events never name the opted-out node");

    // The opted-out node: a set publishes nothing.
    check(off_node->set_parameter<int64_t>("gain", 7).ok(), "set gain on the opted-out node");
    pump(on_node);
    check(nros_stub_rmw_publish_count() == started,
          "the opted-out node's set publishes no /parameter_events");

    // The default node: a set is still published, under its own name.
    check(on_node->set_parameter<int64_t>("gain", 5).ok(), "set gain on the default node");
    pump(on_node);
    check(nros_stub_rmw_publish_count() == started + 1, "the default node's set is published");
    f = last_frame();
    check(f.ok && std::strcmp(f.node, "/pe_on") == 0 && f.slot == 1 &&
              std::strcmp(f.name, "gain") == 0,
          "...as a changed_parameters event naming /pe_on");

    rclcpp::shutdown();
    if (g_failures) {
        std::fprintf(stderr, "node_options_param_events_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("node_options_param_events_runtime: OK\n");
    return 0;
}
