// issue 2023 — a C++ node whose parameters are served PUBLISHES
// `/parameter_events` (`rcl_interfaces/msg/ParameterEvent`), as rclcpp's does:
// a start-up event naming what the node already holds, then one event per
// declare, set and undeclare, each carrying the node's fully qualified name.
//
// A RUN, because the publisher is created in the Rust core when the services
// come up (`Executor::register_parameter_services` -> the reconcile -> the ONE
// `create_param_event_pub`), and nothing in a header can say whether it was.
// The bytes asserted are the ones the stub backend was handed to publish
// (`nros_stub_rmw_last_payload`), so this is about what a subscriber receives.
//
// Built against an `nros-cpp` WITH `param-services` (`just check cpp`): the
// shipped feature set has no parameter store at all.

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

void spin(rclcpp::Executor& exec) {
    for (int i = 0; i < 5; ++i)
        (void)exec.spin_once(1);
}

} // namespace

int main() {
    // Six services and the publisher need the backend to accept entities.
    nros_stub_rmw_set_accept_entities(true);
    rclcpp::Executor exec;
    check(rclcpp::Executor::create_with_rmw(exec, NROS_STUB_RMW_NAME, nullptr, 0, "i2023").ok(),
          "executor create on the stub backend");

    rclcpp::Node node;
    check(exec.create_node(node, "pe_node").ok(), "create the node");
    (void)node.declare_parameter<int64_t>("gain", 3);

    // Not served: nothing is published.
    const uint32_t before = nros_stub_rmw_publish_count();
    spin(exec);
    check(nros_stub_rmw_publish_count() == before,
          "a node whose parameters are not served publishes no /parameter_events");

    // Served: the start-up event names what the node holds.
    check(node.start_parameter_services().ok(), "start the parameter services");
    spin(exec);
    const uint32_t started = nros_stub_rmw_publish_count();
    check(started > before, "the start-up event is published once the services are up");
    check(std::strcmp(nros_stub_rmw_last_entity_name(), "/parameter_events") == 0,
          "the last entity the services created is the /parameter_events publisher");
    Frame f = last_frame();
    check(f.ok, "the start-up frame decodes as a ParameterEvent");
    check(std::strcmp(f.node, "/pe_node") == 0, "the event names the node's FQN");
    check(f.slot == 0, "a start-up entry is in new_parameters");

    // set -> changed_parameters
    check(node.set_parameter<int64_t>("gain", 4).ok(), "set gain");
    spin(exec);
    check(nros_stub_rmw_publish_count() == started + 1, "one event per set");
    f = last_frame();
    check(f.ok && f.slot == 1 && std::strcmp(f.name, "gain") == 0,
          "a set is reported in changed_parameters, by name");
    check(std::strcmp(f.node, "/pe_node") == 0, "...under the node's FQN");

    // declare -> new_parameters
    (void)node.declare_parameter<double>("rate", 1.5);
    spin(exec);
    check(nros_stub_rmw_publish_count() == started + 2, "one event per declare");
    f = last_frame();
    check(f.ok && f.slot == 0 && std::strcmp(f.name, "rate") == 0,
          "a declare is reported in new_parameters, by name");

    // undeclare -> deleted_parameters
    check(node.undeclare_parameter("rate").ok(), "undeclare rate");
    spin(exec);
    check(nros_stub_rmw_publish_count() == started + 3, "one event per undeclare");
    f = last_frame();
    check(f.ok && f.slot == 2 && std::strcmp(f.name, "rate") == 0,
          "an undeclare is reported in deleted_parameters, by name");

    // Nothing changed: nothing published.
    spin(exec);
    check(nros_stub_rmw_publish_count() == started + 3, "no change, no event");

    if (g_failures) {
        std::fprintf(stderr, "parameter_events_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("parameter_events_runtime: ok (%u event(s))\n",
                nros_stub_rmw_publish_count() - before);
    return 0;
}
