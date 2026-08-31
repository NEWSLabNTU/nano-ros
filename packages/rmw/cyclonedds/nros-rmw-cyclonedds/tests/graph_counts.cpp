// phase-393 W2 — matched counts and the publisher GID.
//
// "Is anyone subscribed to this topic" is what an operator asks when nothing
// arrives, and until these slots were filled the runtime could not answer it in
// any language. The assertion that matters is that the count MOVES when a peer
// appears: a stub returning 0, or returning the number of local entities, would
// satisfy "the slot returns OK".

#include <chrono>
#include <cstdio>
#include <cstring>
#include <thread>

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
    if (g_vt->publisher_count_matched_subscriptions == nullptr ||
        g_vt->subscription_count_matched_publishers == nullptr ||
        g_vt->get_gid_for_publisher == nullptr) {
        std::fprintf(stderr, "FAIL: matched-count / gid slots are NULL\n");
        return 2;
    }

    rmw_session_t s{};
    s.node_name  = "graph_counts";
    s.namespace_ = "/";
    if (g_vt->create_session(nullptr, 0, nros_test_domain(97), s.node_name, nullptr, &s) !=
        NROS_RMW_RET_OK) {
        return 3;
    }
    rmw_node_t node{};
    node.name       = s.node_name;
    node.namespace_ = s.namespace_;
    node.session    = &s;

    rmw_qos_profile_t qos{};
    qos.reliability = NROS_RMW_RELIABILITY_RELIABLE;
    qos.durability  = NROS_RMW_DURABILITY_VOLATILE;
    qos.history     = NROS_RMW_HISTORY_KEEP_LAST;
    qos.depth       = 5;

    rmw_publisher_t pubr{};
    pubr.topic_name = "rt/graph_counts";
    pubr.type_name  = "nros_test::msg::TestString";
    const rmw_message_type_support_t ts_1{pubr.type_name, ""};
    if (g_vt->create_publisher(&node, &ts_1, pubr.topic_name, 97, &qos, nullptr, &pubr) != NROS_RMW_RET_OK) {
        (void) g_vt->destroy_session(&s);
        return 4;
    }

    // Before any reader exists the writer matches nobody. A stub that returned
    // a local entity count would already be wrong here.
    size_t n = 12345;
    expect(g_vt->publisher_count_matched_subscriptions(&pubr, &n) == NROS_RMW_RET_OK,
           "publisher_count_matched_subscriptions returned non-OK");
    expect(n == 0, "a writer with no reader must match 0");

    rmw_subscription_t sub{};
    sub.topic_name = "rt/graph_counts";
    sub.type_name  = "nros_test::msg::TestString";
    const rmw_message_type_support_t ts_2{sub.type_name, ""};
    if (g_vt->create_subscription(&node, &ts_2, sub.topic_name, 97, &qos, nullptr, &sub) != NROS_RMW_RET_OK) {
        g_vt->destroy_publisher(&pubr);
        (void) g_vt->destroy_session(&s);
        return 5;
    }

    // SEDP within one participant still takes a moment.
    for (int i = 0; i < 50; ++i) {
        n = 0;
        if (g_vt->publisher_count_matched_subscriptions(&pubr, &n) == NROS_RMW_RET_OK && n > 0) {
            break;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(20));
    }
    expect(n == 1, "the writer must see the reader that matched it");

    size_t m = 0;
    expect(g_vt->subscription_count_matched_publishers(&sub, &m) == NROS_RMW_RET_OK,
           "subscription_count_matched_publishers returned non-OK");
    expect(m == 1, "the reader must see the writer that matched it");

    // GID: 16 Cyclone bytes zero-padded into 24, non-zero, stable across reads,
    // and carrying the identifier that makes it comparable at all.
    rmw_gid_t g1{};
    rmw_gid_t g2{};
    std::memset(g1.data, 0xAA, RMW_GID_STORAGE_SIZE);
    expect(g_vt->get_gid_for_publisher(&pubr, &g1) == NROS_RMW_RET_OK, "get_gid_for_publisher");
    expect(g_vt->get_gid_for_publisher(&pubr, &g2) == NROS_RMW_RET_OK, "get_gid_for_publisher #2");
    bool nonzero = false;
    for (unsigned i = 0; i < RMW_GID_STORAGE_SIZE; ++i) {
        if (g1.data[i] != 0) {
            nonzero = true;
        }
    }
    expect(nonzero, "a gid of all zeroes is not an identifier");
    expect(std::memcmp(g1.data, g2.data, RMW_GID_STORAGE_SIZE) == 0,
           "two reads of one publisher must give the same gid (uninitialised tail?)");
    bool tail_zero = true;
    for (unsigned i = 16; i < RMW_GID_STORAGE_SIZE; ++i) {
        if (g1.data[i] != 0) {
            tail_zero = false;
        }
    }
    expect(tail_zero, "Cyclone's GUID is 16 bytes; the 24-byte tail must be zero-padded");
    expect(g1.implementation_identifier != nullptr &&
               std::strcmp(g1.implementation_identifier, "cyclonedds") == 0,
           "a gid without its identifier can be compared against a foreign backend's");

    g_vt->destroy_subscription(&sub);
    g_vt->destroy_publisher(&pubr);
    (void) g_vt->destroy_session(&s);

    if (g_bad == 0) {
        std::printf("graph_counts: OK (matched counts move, gid stable and padded)\n");
    }
    return g_bad;
}
