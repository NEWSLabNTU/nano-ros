// Issue 1972 — matched and incompatible-type events, provoked for real.
//
// A subscription on `rt/matched_events` watches a matching publisher APPEAR
// (current_count 0 -> 1) and LEAVE (1 -> 0), and the publisher sees its own
// match. Then a publisher of a DIFFERENT type takes the same topic name and the
// subscription reports an incompatible type. A test that only checked "the
// slot answers taken=false" would pass against a backend that never fires.

#include <chrono>
#include <cstdio>
#include <cstring>
#include <thread>

#include "nros/rmw_event.h"
#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"
#include "nros_test_domain.h"

namespace {
const nros_rmw_vtable_t *g_vt = nullptr;

/// Poll `take` until it reports an event (or ~3 s pass). Returns true with
/// `*out` filled when one was taken.
template <typename Take>
bool wait_event(Take take, rmw_event_payload_t *out) {
    for (int i = 0; i < 150; ++i) {
        bool took = false;
        if (take(out, &took) != NROS_RMW_RET_OK) return false;
        if (took) return true;
        std::this_thread::sleep_for(std::chrono::milliseconds(20));
    }
    return false;
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
    rmw_session_t s{};
    s.node_name  = "matched_events";
    s.namespace_ = "/";
    const uint32_t domain = nros_test_domain(98);
    if (g_vt->create_session(nullptr, 0, domain, s.node_name, nullptr, &s) != NROS_RMW_RET_OK) {
        return 3;
    }
    rmw_node_t node{};
    node.name       = s.node_name;
    node.namespace_ = s.namespace_;
    node.session    = &s;

    rmw_qos_profile_t qos{};
    qos.reliability     = NROS_RMW_RELIABILITY_RELIABLE;
    qos.durability      = NROS_RMW_DURABILITY_VOLATILE;
    qos.history         = NROS_RMW_HISTORY_KEEP_LAST;
    qos.liveliness_kind = NROS_RMW_LIVELINESS_SYSTEM_DEFAULT;
    qos.depth           = 5;

    rmw_subscription_t sub{};
    sub.topic_name = "rt/matched_events";
    sub.type_name  = "nros_test::msg::TestString";
    const rmw_message_type_support_t sub_ts{sub.type_name, ""};
    if (g_vt->create_subscription(&node, &sub_ts, sub.topic_name, domain, &qos, nullptr, &sub) !=
        NROS_RMW_RET_OK) {
        (void) g_vt->destroy_session(&s);
        return 4;
    }
    auto sub_take = [&](rmw_event_type_t kind) {
        return [&, kind](rmw_event_payload_t *out, bool *took) {
            return g_vt->subscription_take_event(&sub, kind, out, took);
        };
    };

    int rc = 0;
    rmw_event_payload_t ev{};

    // Nobody publishes yet: nothing matched, nothing to report.
    {
        bool took = false;
        if (g_vt->subscription_take_event(&sub, NROS_RMW_EVENT_SUBSCRIPTION_MATCHED, &ev, &took) !=
                NROS_RMW_RET_OK ||
            took) {
            std::fprintf(stderr, "FAIL: a matched event with no publisher\n");
            rc = 5;
        }
    }

    // A matching publisher appears: 0 -> 1 on both sides.
    rmw_publisher_t pubr{};
    pubr.topic_name = "rt/matched_events";
    pubr.type_name  = "nros_test::msg::TestString";
    const rmw_message_type_support_t pub_ts{pubr.type_name, ""};
    if (rc == 0 && g_vt->create_publisher(&node, &pub_ts, pubr.topic_name, domain, &qos, nullptr,
                                          &pubr) != NROS_RMW_RET_OK) {
        rc = 6;
    }
    if (rc == 0) {
        if (!wait_event(sub_take(NROS_RMW_EVENT_SUBSCRIPTION_MATCHED), &ev)) {
            std::fprintf(stderr, "FAIL: no SUBSCRIPTION_MATCHED after a publisher appeared\n");
            rc = 7;
        } else if (ev.matched.current_count != 1 || ev.matched.current_count_change != 1 ||
                   ev.matched.total_count != 1 || ev.matched.total_count_change != 1) {
            std::fprintf(stderr, "FAIL: appear reported total=%u(+%u) current=%u(%+d)\n",
                         ev.matched.total_count, ev.matched.total_count_change,
                         ev.matched.current_count, ev.matched.current_count_change);
            rc = 8;
        }
    }
    if (rc == 0) {
        auto pub_take = [&](rmw_event_payload_t *out, bool *took) {
            return g_vt->publisher_take_event(&pubr, NROS_RMW_EVENT_PUBLICATION_MATCHED, out,
                                              took);
        };
        if (!wait_event(pub_take, &ev) || ev.matched.current_count != 1) {
            std::fprintf(stderr, "FAIL: the publisher did not see its match\n");
            rc = 9;
        }
    }
    // Consumed: the next poll reports nothing until the set moves again.
    if (rc == 0) {
        bool took = false;
        if (g_vt->subscription_take_event(&sub, NROS_RMW_EVENT_SUBSCRIPTION_MATCHED, &ev, &took) !=
                NROS_RMW_RET_OK ||
            took) {
            std::fprintf(stderr, "FAIL: the same match was reported twice\n");
            rc = 10;
        }
    }

    // It leaves: 1 -> 0, a negative change, total unchanged.
    if (rc == 0) {
        g_vt->destroy_publisher(&pubr);
        if (!wait_event(sub_take(NROS_RMW_EVENT_SUBSCRIPTION_MATCHED), &ev)) {
            std::fprintf(stderr, "FAIL: no SUBSCRIPTION_MATCHED after the publisher left\n");
            rc = 11;
        } else if (ev.matched.current_count != 0 || ev.matched.current_count_change != -1 ||
                   ev.matched.total_count != 1 || ev.matched.total_count_change != 0) {
            std::fprintf(stderr, "FAIL: leave reported total=%u(+%u) current=%u(%+d)\n",
                         ev.matched.total_count, ev.matched.total_count_change,
                         ev.matched.current_count, ev.matched.current_count_change);
            rc = 12;
        }
    }

    // A publisher of ANOTHER type on the same name: incompatible, not matched.
    rmw_publisher_t other{};
    other.topic_name = "rt/matched_events";
    other.type_name  = "nros_test::msg::TestInt32";
    const rmw_message_type_support_t other_ts{other.type_name, ""};
    bool other_up = false;
    if (rc == 0) {
        if (g_vt->create_publisher(&node, &other_ts, other.topic_name, domain, &qos, nullptr,
                                   &other) != NROS_RMW_RET_OK) {
            std::fprintf(stderr, "FAIL: create the second-type publisher\n");
            rc = 13;
        } else {
            other_up = true;
        }
    }
    if (rc == 0) {
        if (!wait_event(sub_take(NROS_RMW_EVENT_SUBSCRIPTION_INCOMPATIBLE_TYPE), &ev)) {
            std::fprintf(stderr, "FAIL: no SUBSCRIPTION_INCOMPATIBLE_TYPE for a second type\n");
            rc = 14;
        } else if (ev.incompatible_type.total_count != 1 ||
                   ev.incompatible_type.total_count_change != 1) {
            std::fprintf(stderr, "FAIL: incompatible reported %u(+%u)\n",
                         ev.incompatible_type.total_count,
                         ev.incompatible_type.total_count_change);
            rc = 15;
        }
    }
    if (rc == 0) {
        bool took = false;
        if (g_vt->subscription_take_event(&sub, NROS_RMW_EVENT_SUBSCRIPTION_MATCHED, &ev, &took) !=
                NROS_RMW_RET_OK ||
            took) {
            std::fprintf(stderr, "FAIL: a different-type publisher counted as matched\n");
            rc = 16;
        }
    }

    // Wrong-side kinds are a caller error, as for the Tier-1 kinds.
    if (rc == 0 && g_vt->subscription_take_event(&sub, NROS_RMW_EVENT_PUBLICATION_MATCHED, &ev,
                                                 nullptr) != NROS_RMW_RET_INVALID_ARGUMENT) {
        rc = 17;
    }
    if (rc == 0) {
        bool took = false;
        if (g_vt->subscription_take_event(&sub, NROS_RMW_EVENT_PUBLISHER_INCOMPATIBLE_TYPE, &ev,
                                          &took) != NROS_RMW_RET_INVALID_ARGUMENT) {
            std::fprintf(stderr, "FAIL: a publisher kind on a subscription was accepted\n");
            rc = 18;
        }
    }

    if (other_up) g_vt->destroy_publisher(&other);
    g_vt->destroy_subscription(&sub);
    (void) g_vt->destroy_session(&s);
    if (rc == 0) {
        std::printf("MATCHED_EVENTS_OK\n");
    }
    return rc;
}
