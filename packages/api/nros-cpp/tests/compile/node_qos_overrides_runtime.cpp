// Issue 1854 -- an `rclcpp`-shape component gets its baked QoS override table
// from the `NodeHandle`, and has it BEFORE its own constructor body runs.
//
// The shape constructs its node and creates its entities in the component's
// constructor, so the generated entry has no point between "node created" and
// "entities created" at which to call `set_qos_overrides` the way it does for
// a `configure`-shape node. Before the fix the entry did not try: the table was
// lowered, carried on the plan and rendered nowhere, and an rclcpp-shape image
// ran the code's QoS whatever its launch or contract stated. Now the table
// rides on the handle and `Node`'s handle constructor installs it as soon as
// the node opens. A base is fully constructed before a derived constructor's
// body runs, so the place to observe "installed in time" is that body: every
// `create_publisher` / `create_subscription` a component writes is there, and
// each one folds whatever table the node holds AT THAT MOMENT (`nros-cpp`'s
// create path, issue 0298).
//
// A RUN for the reason node_launch_identity_runtime.cpp gives: every shape of
// this defect compiles, and only a created node can say what it holds. Same
// stub backend, no wire.
//
// Negative control, measured against this file: the install in `Node`'s handle
// constructor removed -> 2 failures (the table and its length, both observed
// from the constructor body as null / 0).

#include "nros/executor.hpp"
#include "nros/node.hpp"

extern "C" {
#include "stub_rmw_backend.h"
}

#include <cstdio>

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

/// A user's `rclcpp`-shape component. Its constructor body is where its
/// entities would be created, so that is where it records what table its node
/// holds.
class Component : public ::rclcpp::Node {
  public:
    explicit Component(::rclcpp::NodeHandle h) : ::rclcpp::Node(h, "handler") {
        const nros_cpp_node_t* n = this->ffi_handle();
        seen_table = n != nullptr ? n->qos_overrides : nullptr;
        seen_len = n != nullptr ? n->qos_overrides_len : 0;
    }
    const nros_cpp_qos_override_t* seen_table = nullptr;
    size_t seen_len = 0;
};

// What the generated entry bakes for a node whose contract requested
// `manual_by_topic` liveliness and a 500 ms lease on one subscription (the
// codes are `nros_rmw::qos_override_{role,policy}`; the entry's own copy is a
// `static const` array in the same way).
const nros_cpp_qos_override_t kTable[] = {
    {"/system/availability", 1, 6, 3},
    {"/system/availability", 1, 7, 500},
};

} // namespace

int main() {
    ::rclcpp::Executor exec;
    check(::rclcpp::Executor::create_with_rmw(exec, NROS_STUB_RMW_NAME, nullptr, 0, "i1854").ok(),
          "executor create on the stub backend");
    if (g_failures != 0) {
        std::fprintf(stderr, "node_qos_overrides_runtime: setup failed\n");
        return 1;
    }
    void* h = exec.handle();

    // --- 1. a handle carrying a table installs it before the body runs ------
    ::rclcpp::NodeHandle with_table(h, nullptr, nullptr, kTable, 2);
    Component baked(with_table);
    check(baked.ok(), "the node with a QoS override table must come up");
    check(baked.seen_table == kTable,
          "the handle's QoS override table must be on the node by the time the component's "
          "constructor body runs -- that is where its entities are created");
    check(baked.seen_len == 2, "...with its full length");

    // --- 2. the 3-argument handle installs nothing --------------------------
    ::rclcpp::NodeHandle bare(h, nullptr, nullptr);
    Component plain(bare);
    check(plain.ok(), "the node without a table must come up");
    check(plain.seen_table == nullptr && plain.seen_len == 0,
          "a handle that carries no table must leave the node with none -- the entry writes "
          "the 3-argument form for every node whose plan has no override");

    if (g_failures != 0) {
        std::fprintf(stderr, "node_qos_overrides_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("node_qos_overrides_runtime: ok\n");
    return 0;
}
