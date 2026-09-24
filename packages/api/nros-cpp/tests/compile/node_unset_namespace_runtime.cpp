// Issue 1473 — "the caller named no namespace" has ONE meaning at this ABI,
// and it is not the same request as "put this node at the root".
//
// The two C++ FFI entry points reach one `NodeBuilder`, and they reached it
// with opposite answers for the same stated input: `nros_cpp_node_create`
// substituted the literal `"/"` for a NULL namespace and always called
// `.namespace("/")`, while `nros_cpp_node_create_ex` left `.namespace()`
// UNCALLED for `namespace_len == 0` and so inherited the executor's.
//
// A RUN, and on an executor opened at a NON-ROOT namespace, because on a root
// executor the two answers are the same value and every probe passes. MEASURED
// before the fix, this file's own executor (`/island`) on the stub backend:
//
//     4-arg,  ns = NULL                 handle_ns=/  fqn=/alpha  'chatter' -> /chatter
//     _ex,    namespace_len = 0         handle_ns=/  fqn=/beta   'chatter' -> /chatter
//     4-arg,  ns = "/"  (explicit root) handle_ns=/  fqn=/gamma  'chatter' -> /chatter
//     _ex,    namespace = "/"           handle_ns=/  fqn=/delta  'chatter' -> /chatter
//
// — which is the SECOND finding and the one the issue did not state. At the
// C++ surface the two entry points did not disagree with each other; `_ex`
// disagreed with ITSELF. Its inheritance landed in the executor's `NodeRecord`
// (measured in `nros-node`'s
// `an_unnamed_namespace_inherits_the_executors_and_an_explicit_root_does_not`:
// `/island`) and was then overwritten on the handle with `"/"` — and the handle
// is what `resolve_node_entity_name` reads for every publisher, subscription
// and service, while `nros_node::executor::action` reads the RECORD. One node,
// two namespaces, split by entity kind.
//
// So the fix is two things and this file asserts both: the entry points agree,
// AND each node's handle reports what the executor recorded for it.
//
// The assertions come in PAIRS. Making unset inherit is easy to "fix" by
// making everything inherit, which would leave no way to place a node at the
// root of a namespaced executor — so every positive case has an explicit-root
// twin measured beside it.
//
// Links `libnros_cpp.a` and the shared C stub RMW backend
// (`nros-c/tests/run/stub_rmw_backend.c`) for the reason every runtime probe
// here does: a node needs an executor and an executor needs a session. The
// resolved topic name is read back through the stub's
// `nros_stub_rmw_last_entity_name()`, which records the name the create slot
// was ASKED for — the slot then refuses, so nothing touches the wire.
//
// Negative controls, MEASURED against this file:
//   * `store_recorded_namespace` made to write `"/"` unconditionally (the
//     pre-fix handle write) -> 8 failures: every inheriting expectation on all
//     three unset nodes. The four explicit-root assertions stay GREEN, which is
//     exactly why a probe on a root executor would see nothing at all.
//   * the fix kept, but `nros_cpp_node_create` left substituting `"/"` for an
//     unset namespace -> 5 failures, all on the 4-argument form; `_ex`'s node
//     stays green. That is the divergence as filed, and this is the shape it
//     takes once the handle stops hiding it.

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

void check_str(const char* got, const char* expect, const char* what) {
    if (got == nullptr || std::strcmp(got, expect) != 0) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s — expected \"%s\", got \"%s\"\n", what, expect,
                     got == nullptr ? "(null)" : got);
    }
}

alignas(8) unsigned char g_storage[NROS_CPP_EXECUTOR_STORAGE_SIZE];
// `nros_cpp_publisher_create` writes a `CppPublisher` here on success. The stub
// refuses every entity slot, so nothing is ever written — the call is made for
// the NAME RESOLUTION that happens before the refusal, which is the only place
// a test can read the wire name a node computed (issue 1384's reason for
// `nros_stub_rmw_last_entity_name`). Sized well past the handle either way.
alignas(8) unsigned char g_pub[4096];

/// The fully-qualified name this node answers for itself.
void check_fqn(const nros_cpp_node_t* node, const char* expect, const char* what) {
    char buf[160];
    std::memset(buf, 0, sizeof(buf));
    nros_cpp_ret_t r = nros_cpp_node_get_fully_qualified_name(node, buf, sizeof(buf), nullptr);
    if (r != NROS_CPP_RET_OK) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s — get_fully_qualified_name returned %d\n", what, (int)r);
        return;
    }
    check_str(buf, expect, what);
}

/// Where a RELATIVE topic declared on this node lands — the half that decides
/// delivery rather than naming, and the one a caller cannot work around.
void check_relative_topic(const nros_cpp_node_t* node, const char* expect, const char* what) {
    nros_stub_rmw_clear_last_entity_name();
    nros_cpp_qos_t qos;
    std::memset(&qos, 0, sizeof(qos));
    qos.depth = 10;
    (void)nros_cpp_publisher_create(node, "chatter", "std_msgs::msg::dds_::String_", "", qos,
                                    g_pub);
    check_str(nros_stub_rmw_last_entity_name(), expect, what);
}

nros_cpp_node_options_t options_with_namespace(const char* ns) {
    nros_cpp_node_options_t o = nros_cpp_node_get_default_options();
    if (ns != nullptr) {
        const size_t n = std::strlen(ns);
        std::memcpy(o.namespace_, ns, n);
        o.namespace_len = n;
    }
    return o;
}

} // namespace

int main() {
    // The executor's namespace is the whole point: `nros::init(locator, domain,
    // session, node_namespace)` (issue 1434) is how a launch-declared namespace
    // reaches the session, and before this fix nothing a C++ entry created
    // afterwards could inherit it. `Executor::create*` passes `nullptr` for the
    // namespace, so the probe reaches `nros_cpp_init_rmw` directly — which is
    // what that overload of `nros::init` does.
    nros_cpp_ret_t rc =
        nros_cpp_init_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "i1473", "/island", g_storage);
    if (rc != NROS_CPP_RET_OK) {
        std::fprintf(stderr, "node_unset_namespace_runtime: init returned %d\n", (int)rc);
        return 1;
    }
    void* h = g_storage;

    // --- 1. unset, through the 4-argument form --------------------------
    nros_cpp_node_t alpha;
    std::memset(&alpha, 0, sizeof(alpha));
    if (nros_cpp_node_create(h, "alpha", nullptr, &alpha) != NROS_CPP_RET_OK) {
        std::fprintf(stderr, "node_unset_namespace_runtime: creating alpha failed\n");
        return 1;
    }
    check_str(nros_cpp_node_get_namespace(&alpha), "/island",
              "nros_cpp_node_create with a NULL namespace must INHERIT the executor's — NULL is "
              "\"the caller named none\", which is what nros_cpp_node_create_ex has always "
              "answered for namespace_len == 0");
    check_fqn(&alpha, "/island/alpha", "…and the node must say so when asked its own name");
    check_relative_topic(&alpha, "/island/chatter",
                         "…and its relative topics must resolve there, which is the half that "
                         "decides whether anything is delivered");

    // --- 2. unset, through the options form -----------------------------
    nros_cpp_node_t beta;
    std::memset(&beta, 0, sizeof(beta));
    nros_cpp_node_options_t bare = nros_cpp_node_get_default_options();
    if (nros_cpp_node_create_ex(h, "beta", &bare, &beta) != NROS_CPP_RET_OK) {
        std::fprintf(stderr, "node_unset_namespace_runtime: creating beta failed\n");
        return 1;
    }
    check_str(nros_cpp_node_get_namespace(&beta), "/island",
              "nros_cpp_node_create_ex already INHERITED for namespace_len == 0 — but it then "
              "wrote \"/\" onto the handle, and the handle is what every C++ publisher, "
              "subscription and service reads. The two must be the same answer");
    check_fqn(&beta, "/island/beta",
              "the two entry points must name the same namespace for the same stated input");
    check_relative_topic(&beta, "/island/chatter",
                         "…including on the wire: an unset namespace through EITHER entry point "
                         "must put a relative topic in the same place");

    // --- 3. the explicit root, through the 4-argument form ---------------
    nros_cpp_node_t gamma;
    std::memset(&gamma, 0, sizeof(gamma));
    if (nros_cpp_node_create(h, "gamma", "/", &gamma) != NROS_CPP_RET_OK) {
        std::fprintf(stderr, "node_unset_namespace_runtime: creating gamma failed\n");
        return 1;
    }
    check_str(nros_cpp_node_get_namespace(&gamma), "/",
              "a caller that ASKED for the root must still get the root — \"/\" is one character "
              "and it is the whole reason NULL is free to mean something else");
    check_fqn(&gamma, "/gamma",
              "…and must be distinguishable from the node that asked for nothing");
    check_relative_topic(&gamma, "/chatter",
                         "…on the wire too: a root node on a namespaced executor is a legitimate "
                         "request and it must reach the root");

    // --- 4. the explicit root, through the options form ------------------
    nros_cpp_node_t delta;
    std::memset(&delta, 0, sizeof(delta));
    nros_cpp_node_options_t rooted = options_with_namespace("/");
    if (nros_cpp_node_create_ex(h, "delta", &rooted, &delta) != NROS_CPP_RET_OK) {
        std::fprintf(stderr, "node_unset_namespace_runtime: creating delta failed\n");
        return 1;
    }
    check_str(nros_cpp_node_get_namespace(&delta), "/",
              "the options form must carry the same two meanings: namespace_len == 0 is unset, a "
              "written \"/\" is the root");
    check_fqn(&delta, "/delta", "…and answer identically to the 4-argument form");
    check_relative_topic(&delta, "/chatter", "…including where its relative topics land");

    (void)nros_cpp_fini(h);

    // --- 5. `""` is UNSET, not a namespace, and not the root -------------
    //
    // Its own executor: `NROS_EXECUTOR_MAX_NODES` is 4 by default and
    // `nros_cpp_node_destroy` is a no-op, so a fifth node does not fit above.
    // Sequential — not a claim that two sessions coexist.
    rc = nros_cpp_init_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "i1473b", "/island", g_storage);
    if (rc != NROS_CPP_RET_OK) {
        std::fprintf(stderr, "node_unset_namespace_runtime: second init returned %d\n", (int)rc);
        return 1;
    }
    h = g_storage;

    nros_cpp_node_t epsilon;
    std::memset(&epsilon, 0, sizeof(epsilon));
    if (nros_cpp_node_create(h, "epsilon", "", &epsilon) != NROS_CPP_RET_OK) {
        std::fprintf(stderr, "node_unset_namespace_runtime: creating epsilon failed\n");
        return 1;
    }
    check_str(nros_cpp_node_get_namespace(&epsilon), "/island",
              "an EMPTY namespace string is \"the caller filled nothing in\", never the empty "
              "namespace — an unresolved bake macro expands to \"\", and this is the rule "
              "nros::init's node_namespace and NodeHandle::resolve_namespace already state");
    check_fqn(&epsilon, "/island/epsilon", "…so it must answer exactly as NULL does");

    (void)nros_cpp_fini(h);

    if (g_failures != 0) {
        std::fprintf(stderr, "node_unset_namespace_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("node_unset_namespace_runtime: ok\n");
    return 0;
}
