// phase-476 W0 — destroying a C++ node releases the arena entries it
// registered, and only those.
//
// A C++ callback's capture is copied into the executor arena and usually holds
// the node's `this`. Before W0, `nros_cpp_node_destroy` was a no-op and an arena
// entry did not record which node registered it, so a subscription outlived its
// node and went on dispatching into freed memory.
//
// Observed through the stub RMW backend in accept mode: every entity it creates
// counts as live until the runtime destroys it, so a released entry is visible
// as the live count falling. Node B's subscription is the control — a release
// keyed on anything wider than the node would take it too.

#include "nros/executor.hpp"
#include "nros/node.hpp"
#include "nros/subscription.hpp"

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

void check_int(long got, long expect, const char* what) {
    if (got != expect) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s — expected %ld, got %ld\n", what, expect, got);
    }
}

alignas(8) unsigned char g_storage[NROS_CPP_EXECUTOR_STORAGE_SIZE];

void on_message(const uint8_t*, size_t, void*) {}

/// A capturing registration, the shape every ported `create_subscription`
/// takes: the capture is a node pointer.
size_t subscribe(const nros_cpp_node_t* node, const char* topic) {
    nros_cpp_qos_t qos;
    std::memset(&qos, 0, sizeof(qos));
    qos.depth = 1;
    const void* capture = node;
    size_t handle = 0;
    nros_cpp_ret_t rc = nros_cpp_subscription_register_capturing(
        node, topic, "std_msgs::msg::dds_::Int32_", "", qos, on_message,
        reinterpret_cast<const uint8_t*>(&capture), sizeof(capture), nullptr, &handle, nullptr);
    check_int(rc, NROS_CPP_RET_OK, topic);
    return handle;
}

} // namespace

int main() {
    nros_stub_rmw_set_accept_entities(true);
    nros_cpp_ret_t rc = nros_cpp_init_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "w0", nullptr, g_storage);
    if (rc != NROS_CPP_RET_OK) {
        std::fprintf(stderr, "node_destroy_releases_entries_runtime: init returned %d\n", (int)rc);
        return 1;
    }
    void* h = g_storage;

    nros_cpp_node_t a;
    nros_cpp_node_t b;
    std::memset(&a, 0, sizeof(a));
    std::memset(&b, 0, sizeof(b));
    check_int(nros_cpp_node_create(h, "a", nullptr, &a), NROS_CPP_RET_OK, "create node a");
    check_int(nros_cpp_node_create(h, "b", nullptr, &b), NROS_CPP_RET_OK, "create node b");

    const int32_t before = nros_stub_rmw_live_entities();
    (void)subscribe(&a, "a_one");
    (void)subscribe(&a, "a_two");
    (void)subscribe(&b, "b_one");
    check_int(nros_stub_rmw_live_entities() - before, 3, "three subscriptions are live");

    check_int(nros_cpp_node_destroy(&a), NROS_CPP_RET_OK, "destroy node a");
    check_int(nros_stub_rmw_live_entities() - before, 1,
              "destroying node a released its two subscriptions and kept b's");

    // A second destroy finds nothing of a's left.
    check_int(nros_cpp_node_destroy(&a), NROS_CPP_RET_OK, "destroy node a again");
    check_int(nros_stub_rmw_live_entities() - before, 1, "a second destroy releases nothing");

    check_int(nros_cpp_node_destroy(&b), NROS_CPP_RET_OK, "destroy node b");
    check_int(nros_stub_rmw_live_entities() - before, 0, "destroying node b released its own");

    (void)nros_cpp_fini(h);
    if (g_failures != 0) {
        std::fprintf(stderr, "node_destroy_releases_entries_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("node_destroy_releases_entries_runtime: OK\n");
    return 0;
}
