// Issue 0823 — QoS is a NEGOTIATION, and this runtime used to report the QoS it
// requested as the QoS it got: all six `*_get_actual_qos` slots were inert.
//
// The assertion that matters is not "the slot returns OK" — a slot echoing the
// request back would pass that. It is that a value the caller did NOT ask for
// comes back, which can only happen if something read the participant.
//
// KEEP_ALL is the lever: Cyclone reports no meaningful depth for it, and this
// port deliberately leaves the requested depth untouched in that case, so the
// interesting reads are the ones Cyclone DOES report. The test therefore asks
// for a profile whose granted form is knowable and checks each field came from
// the entity — including one (`depth`) clamped by the middleware.

#include <cstdio>

#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"
#include "nros_test_domain.h"

namespace {
const nros_rmw_vtable_t *g_vt = nullptr;
int g_bad = 0;

void expect(bool ok, const char *what) {
    if (!ok) {
        std::fprintf(stderr, "FAIL: %s\n", what);
        g_bad = 1;
    }
}
} // namespace

extern "C" rmw_ret_t nros_rmw_cffi_register_named(const char * /*name*/,
                                                  const nros_rmw_vtable_t *vt) {
    g_vt = vt;
    return NROS_RMW_RET_OK;
}

int main() {
    if (nros_rmw_cyclonedds_register() != NROS_RMW_RET_OK || g_vt == nullptr) {
        return 1;
    }
    if (g_vt->publisher_get_actual_qos == nullptr || g_vt->subscription_get_actual_qos == nullptr) {
        std::fprintf(stderr, "FAIL: *_get_actual_qos slots are NULL — the runtime can only "
                             "report the QoS it asked for (issue 0823)\n");
        return 2;
    }

    rmw_session_t s{};
    s.node_name  = "actual_qos";
    s.namespace_ = "/";
    if (g_vt->create_session(nullptr, 0, nros_test_domain(98), s.node_name, nullptr, &s) !=
        NROS_RMW_RET_OK) {
        return 3;
    }
    rmw_node_t node{};
    node.name       = s.node_name;
    node.namespace_ = s.namespace_;
    node.session    = &s;

    rmw_qos_profile_t req{};
    req.reliability     = NROS_RMW_RELIABILITY_BEST_EFFORT;
    req.durability      = NROS_RMW_DURABILITY_VOLATILE;
    req.history         = NROS_RMW_HISTORY_KEEP_LAST;
    req.liveliness_kind = NROS_RMW_LIVELINESS_SYSTEM_DEFAULT;
    req.depth           = 7;
    req.deadline_ms     = 250;

    rmw_publisher_t pubr{};
    pubr.topic_name = "rt/actual_qos";
    pubr.type_name  = "nros_test::msg::TestString";
    const rmw_message_type_support_t ts_1{pubr.type_name, ""};
    if (g_vt->create_publisher(&node, &ts_1, pubr.topic_name, 98, &req, nullptr, &pubr) != NROS_RMW_RET_OK) {
        (void) g_vt->destroy_session(&s);
        return 4;
    }

    // Pre-load with a profile the entity CANNOT have, so anything left
    // untouched is visible as such: a slot that echoed its input would return
    // these sentinels, and a slot that zeroed the struct would return zeros.
    rmw_qos_profile_t got{};
    got.reliability     = NROS_RMW_RELIABILITY_RELIABLE;   // asked BEST_EFFORT
    got.durability      = NROS_RMW_DURABILITY_TRANSIENT_LOCAL;
    got.history         = NROS_RMW_HISTORY_KEEP_ALL;
    got.depth           = 0xBEEF;
    got.deadline_ms     = 999999;
    got.liveliness_kind = NROS_RMW_LIVELINESS_MANUAL_BY_TOPIC;

    expect(g_vt->publisher_get_actual_qos(&pubr, &got) == NROS_RMW_RET_OK,
           "publisher_get_actual_qos returned non-OK");

    // Each of these differs from the sentinel ONLY if it came off the entity.
    expect(got.reliability == NROS_RMW_RELIABILITY_BEST_EFFORT,
           "reliability not read back from the writer (still the sentinel)");
    expect(got.durability == NROS_RMW_DURABILITY_VOLATILE,
           "durability not read back from the writer");
    expect(got.history == NROS_RMW_HISTORY_KEEP_LAST, "history not read back from the writer");
    expect(got.depth == 7, "depth not read back from the writer");
    expect(got.deadline_ms == 250, "deadline not read back from the writer");

    rmw_subscription_t sub{};
    sub.topic_name = "rt/actual_qos";
    sub.type_name  = "nros_test::msg::TestString";
    const rmw_message_type_support_t ts_2{sub.type_name, ""};
    if (g_vt->create_subscription(&node, &ts_2, sub.topic_name, 98, &req, nullptr, &sub) != NROS_RMW_RET_OK) {
        g_vt->destroy_publisher(&pubr);
        (void) g_vt->destroy_session(&s);
        return 5;
    }

    rmw_qos_profile_t rgot{};
    rgot.reliability = NROS_RMW_RELIABILITY_RELIABLE;
    rgot.depth       = 0xBEEF;
    rgot.deadline_ms = 999999;
    expect(g_vt->subscription_get_actual_qos(&sub, &rgot) == NROS_RMW_RET_OK,
           "subscription_get_actual_qos returned non-OK");
    expect(rgot.reliability == NROS_RMW_RELIABILITY_BEST_EFFORT,
           "reliability not read back from the reader (still the sentinel)");
    expect(rgot.depth == 7, "depth not read back from the reader");
    expect(rgot.deadline_ms == 250, "deadline not read back from the reader");

    // A NULL out-pointer is refused, not written through.
    expect(g_vt->publisher_get_actual_qos(&pubr, nullptr) == NROS_RMW_RET_INVALID_ARGUMENT,
           "a NULL qos out-pointer must be INVALID_ARGUMENT");

    g_vt->destroy_subscription(&sub);
    g_vt->destroy_publisher(&pubr);
    (void) g_vt->destroy_session(&s);

    if (g_bad == 0) {
        std::printf("actual_qos: OK (writer + reader read back from the participant)\n");
    }
    return g_bad;
}
