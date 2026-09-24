// Issue 1456 — an `rclcpp`-shape component takes its identity from the LAUNCH
// FILE when the launch file declares one, and from its own class literal when
// it does not.
//
// A RUN rather than a `static_assert`, because every shape of this defect
// COMPILES. Before the fix the generated entry built the component's handle
// with the executor alone; the component then named itself, and the name it
// chose was a perfectly valid C++ string. Measured on a real bus (a zenoh
// router, `ros2 node list --no-daemon`) for a plan declaring
// `name="alpha" namespace="/island"`:
//
//     rclcpp shape      -> /rclcpp_class_name     (topics at the ROOT)
//     configure shape   -> /island/beta           (topics under /island)
//
// two nodes in ONE image, one launch file, disagreeing about where they are.
// Only a node that was actually created can answer which of those two it is,
// which is why this links `libnros_cpp.a` and the shared C stub RMW backend
// (`nros-c/tests/run/stub_rmw_backend.c`) rather than asserting on a header: a
// node needs an executor and an executor needs a session. Nothing here touches
// the wire.
//
// The assertions come in pairs on purpose. The POSITIVE direction (the launch
// identity wins) and the NEGATIVE direction (an undeclared half leaves the
// class's literal standing, and `""` is "undeclared", never a name) are two
// different bugs, and a fix that satisfies one while breaking the other is the
// likely mistake — the entry's resolved `name` falls back to the node's `exec`,
// so passing it unconditionally would silently outrank every component class's
// literal in every launch file that omits `name=`.
//
// Negative controls, MEASURED against this file rather than predicted:
//   * `NodeHandle::resolve_name` made to ignore the handle and always answer
//     the fallback -> 3 failures, exactly the three cases that declare a name
//     (`/island/alpha`, `/beta`, `/island/gamma`), each reporting the class
//     literal in its place. That is the pre-fix behaviour.
//   * both `resolve_*` made to drop the empty-string guard (any non-null
//     pointer wins) -> 1 failure, and it is not a wrong NAME: `nros_cpp_node_create`
//     REFUSES an empty name (`NROS_CPP_RET_INVALID_ARGUMENT`), so the
//     component does not come up at all. An empty `name=` in a launch file
//     would take the image down.

#include "nros/executor.hpp"
#include "nros/node.hpp"

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

/// A user's `rclcpp`-shape component, verbatim in the shape the cmake verb
/// `nros_components_register_node(... SHAPE rclcpp)` requires and the generated
/// entry constructs: one `NodeHandle` parameter, the name a literal in the
/// member-init list. Nothing here knows about a launch file.
class Component : public ::rclcpp::Node {
  public:
    explicit Component(::nros::NodeHandle h) : ::rclcpp::Node(h, "class_literal") {}
};

/// Same, but the class also names a namespace — the case where BOTH halves of
/// the launch identity have something of the class's to outrank.
class ComponentWithNs : public ::rclcpp::Node {
  public:
    explicit ComponentWithNs(::nros::NodeHandle h)
        : ::rclcpp::Node(h, "class_literal", "/class_ns") {}
};

void check_fqn(const ::rclcpp::Node& node, const char* expect, const char* what) {
    char buf[128];
    std::memset(buf, 0, sizeof(buf));
    ::nros::Result r = node.get_fully_qualified_name(buf, sizeof(buf), nullptr);
    if (!r.ok()) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s — get_fully_qualified_name returned %d\n", what, r.raw());
        return;
    }
    if (std::strcmp(buf, expect) != 0) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s — expected \"%s\", got \"%s\"\n", what, expect, buf);
    }
}

} // namespace

int main() {
    // TWO executors, because the node table is bounded (`NROS_EXECUTOR_MAX_NODES`,
    // default 4) and `nros_cpp_node_destroy` is a no-op: a slot is never handed
    // back, so seven nodes do not fit in one. Sequential, each finalised by its
    // own destructor before the next opens — not a claim that two sessions can
    // coexist.
    {
        ::nros::Executor exec;
        // The guard compares against the count BEFORE this round, so an
        // assertion failure in an earlier round cannot read as a setup failure
        // here — the two are different diagnoses.
        const int before = g_failures;
        check(
            ::nros::Executor::create_with_rmw(exec, NROS_STUB_RMW_NAME, nullptr, 0, "i1456a").ok(),
            "executor create on the stub backend (round 1)");
        if (g_failures != before) {
            std::fprintf(stderr, "node_launch_identity_runtime: round 1 setup failed\n");
            return 1;
        }
        void* h = exec.handle();

        // --- 1. both halves declared: the launch file is authoritative ------
        ::nros::NodeHandle h_declared(h, "alpha", "/island");
        Component declared(h_declared);
        check(declared.ok(), "the declared-identity node must come up");
        check_fqn(declared, "/island/alpha",
                  "a launch-declared name and namespace must outrank the component class's own "
                  "literal — otherwise one component class cannot be instantiated twice in one "
                  "launch file, because both instances answer the same name");

        // --- 2. the class's literal is a DEFAULT, never dead ----------------
        ::nros::NodeHandle h_bare(h);
        Component undeclared(h_bare);
        check(undeclared.ok(), "the undeclared node must come up");
        check_fqn(undeclared, "/class_literal",
                  "a component the launch file does not name keeps its class's literal, at the "
                  "root — the 1-argument handle declares nothing, which is what a hand-written "
                  "main and every pre-1456 caller build");

        // --- 3. the halves are INDEPENDENT ----------------------------------
        ::nros::NodeHandle h_ns_only(h, nullptr, "/island");
        Component ns_only(h_ns_only);
        check_fqn(ns_only, "/island/class_literal",
                  "a launch file declaring only a namespace must move the node and leave its "
                  "name alone");

        ::nros::NodeHandle h_name_only(h, "beta", nullptr);
        Component name_only(h_name_only);
        check_fqn(name_only, "/beta",
                  "a launch file declaring only a name must rename the node and leave it where "
                  "its class put it");
    }

    {
        ::nros::Executor exec;
        const int before = g_failures;
        check(
            ::nros::Executor::create_with_rmw(exec, NROS_STUB_RMW_NAME, nullptr, 0, "i1456b").ok(),
            "executor create on the stub backend (round 2)");
        if (g_failures != before) {
            std::fprintf(stderr, "node_launch_identity_runtime: round 2 setup failed\n");
            return 1;
        }
        void* h = exec.handle();

        // --- 4. a class that names BOTH is outranked in BOTH ----------------
        ::nros::NodeHandle h_both(h, "gamma", "/island");
        ComponentWithNs both(h_both);
        check_fqn(both, "/island/gamma",
                  "the precedence is per HALF and the handle wins both: a class namespace is a "
                  "default like a class name");

        ::nros::NodeHandle h_bare(h);
        ComponentWithNs both_undeclared(h_bare);
        check_fqn(both_undeclared, "/class_ns/class_literal",
                  "…and with nothing declared, BOTH of the class's own choices stand");

        // --- 5. `""` is UNSET, never an identity ----------------------------
        ::nros::NodeHandle h_empty(h, "", "");
        ComponentWithNs empty(h_empty);
        check_fqn(empty, "/class_ns/class_literal",
                  "an empty name or namespace on the handle means the launch file declared "
                  "none — a node called \"\" is not a node, and the empty namespace is not the "
                  "root by accident");
    }

    if (g_failures != 0) {
        std::fprintf(stderr, "node_launch_identity_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("node_launch_identity_runtime: ok\n");
    return 0;
}
