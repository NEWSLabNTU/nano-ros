// Issue 0780 — polled status events, provoked for real.
//
// `take_event` was DECLINED on the grounds that our callback "already runs on
// the safe context, from inside drive_io". No backend did that, and this one
// least of all: its `drive_io` is a sleep. So there was no way for a
// cyclonedds status event to reach a caller at any price.
//
// This drives an actual REQUESTED_DEADLINE_MISSED rather than asserting a stub
// returns OK: a subscription with a 50 ms deadline and a publisher that sends
// once and then stops. A test that only checked "the slot exists and answers
// taken=false" would have passed against the NULL slot it replaced.

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
    if (g_vt->subscription_take_event == nullptr || g_vt->publisher_take_event == nullptr) {
        std::fprintf(stderr, "FAIL: take_event slots are NULL\n");
        return 2;
    }

    rmw_session_t s{};
    s.node_name  = "status_events";
    s.namespace_ = "/";
    if (g_vt->create_session(nullptr, 0, nros_test_domain(99), s.node_name, nullptr, &s) !=
        NROS_RMW_RET_OK) {
        return 3;
    }
    rmw_node_t node{};
    node.name       = s.node_name;
    node.namespace_ = s.namespace_;
    node.session    = &s;

    // A 50 ms deadline on both ends. RELIABLE + VOLATILE so the pair matches.
    rmw_qos_profile_t qos{};
    qos.reliability     = NROS_RMW_RELIABILITY_RELIABLE;
    qos.durability      = NROS_RMW_DURABILITY_VOLATILE;
    qos.history         = NROS_RMW_HISTORY_KEEP_LAST;
    qos.liveliness_kind = NROS_RMW_LIVELINESS_SYSTEM_DEFAULT;
    qos.depth           = 5;
    qos.deadline_ms     = 50;

    rmw_publisher_t pubr{};
    pubr.topic_name = "rt/status_events";
    pubr.type_name  = "nros_test::msg::TestString";
    if (g_vt->create_publisher(&node, pubr.topic_name, pubr.type_name, "", 99, &qos, nullptr,
                               &pubr) != NROS_RMW_RET_OK) {
        (void) g_vt->destroy_session(&s);
        return 4;
    }

    rmw_subscription_t sub{};
    sub.topic_name = "rt/status_events";
    sub.type_name  = "nros_test::msg::TestString";
    if (g_vt->create_subscription(&node, sub.topic_name, sub.type_name, "", 99, &qos, nullptr,
                                  &sub) != NROS_RMW_RET_OK) {
        g_vt->destroy_publisher(&pubr);
        (void) g_vt->destroy_session(&s);
        return 5;
    }

    std::this_thread::sleep_for(std::chrono::milliseconds(300));

    int rc = 0;
    rmw_event_payload_t ev{};
    bool took = false;

    // A quiet reader has missed nothing yet.
    if (g_vt->subscription_take_event(&sub, NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED, &ev,
                                      &took) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "FAIL: take_event errored on a quiet reader\n");
        rc = 6;
    } else if (took) {
        // Discovery-time misses are possible before the writer matches; drain
        // and continue rather than failing on a legitimate one.
        took = false;
    }

    // One sample, then silence — the deadline must lapse.
    // CDR-LE TestString: encap + u32 length(3) + "hi\0".
    uint8_t msg[12] = {0x00, 0x01, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 'h', 'i', 0x00, 0x00};
    if (rc == 0 && g_vt->publish(&pubr, msg, sizeof(msg)) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "FAIL: publish\n");
        rc = 7;
    }

    bool saw_missed = false;
    for (int i = 0; i < 100 && rc == 0 && !saw_missed; ++i) {
        std::this_thread::sleep_for(std::chrono::milliseconds(20));
        rmw_ret_t r = g_vt->subscription_take_event(
            &sub, NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED, &ev, &took);
        if (r != NROS_RMW_RET_OK) {
            std::fprintf(stderr, "FAIL: take_event rc=%d\n", (int) r);
            rc = 8;
            break;
        }
        if (took) {
            if (ev.count.total_count_change == 0) {
                std::fprintf(stderr, "FAIL: taken with a zero change count\n");
                rc = 9;
                break;
            }
            saw_missed = true;
        }
    }
    if (rc == 0 && !saw_missed) {
        std::fprintf(stderr, "FAIL: no REQUESTED_DEADLINE_MISSED after 2 s of silence\n");
        rc = 10;
    }

    // Consumed by the take: the very next poll must report nothing.
    if (rc == 0) {
        rmw_ret_t r = g_vt->subscription_take_event(
            &sub, NROS_RMW_EVENT_REQUESTED_DEADLINE_MISSED, &ev, &took);
        if (r != NROS_RMW_RET_OK) {
            rc = 11;
        } else if (took) {
            std::fprintf(stderr,
                         "FAIL: the same event was taken twice — the read did not consume it\n");
            rc = 12;
        }
    }

    // A publisher kind on a subscription is a caller error, not an empty poll.
    if (rc == 0) {
        rmw_ret_t r =
            g_vt->subscription_take_event(&sub, NROS_RMW_EVENT_LIVELINESS_LOST, &ev, &took);
        if (r != NROS_RMW_RET_INVALID_ARGUMENT) {
            std::fprintf(stderr, "FAIL: publisher kind on a subscription returned %d\n", (int) r);
            rc = 13;
        }
    }
    if (rc == 0) {
        rmw_ret_t r =
            g_vt->publisher_take_event(&pubr, NROS_RMW_EVENT_MESSAGE_LOST, &ev, &took);
        if (r != NROS_RMW_RET_INVALID_ARGUMENT) {
            std::fprintf(stderr, "FAIL: subscription kind on a publisher returned %d\n", (int) r);
            rc = 14;
        }
    }

    g_vt->destroy_subscription(&sub);
    g_vt->destroy_publisher(&pubr);
    (void) g_vt->destroy_session(&s);
    if (rc == 0) {
        std::printf("STATUS_EVENTS_OK\n");
    }
    return rc;
}
