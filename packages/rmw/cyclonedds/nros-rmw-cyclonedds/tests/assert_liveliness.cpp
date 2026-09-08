// Issue 1231 — manual liveliness assertion, provoked rather than asserted.
//
// `publisher_assert_liveliness` was NULL on this backend with no reason
// recorded at the slot. It is wired now, and the thing worth testing is not
// that it returns OK — a stub does that — but that the call REACHES Cyclone's
// writer lease. So this runs the lease twice over:
//
//   Phase A: a MANUAL_BY_TOPIC publisher with a 500 ms lease, asserted every
//            100 ms for 2 s. No LIVELINESS_LOST may appear. A stub that
//            returned OK without touching the writer fails here, because the
//            lease would expire four times over.
//   Phase B: the same publisher, left alone. LIVELINESS_LOST MUST appear.
//            That is phase A's negative control: without it, phase A would
//            also pass on a backend where the lease never runs at all.
//
// No reader is involved on purpose. Measured in the pinned 0.10.5: the writer's
// lease is registered at creation for any non-AUTOMATIC kind with a finite
// duration (`ddsi_endpoint.c:1006`), with no dependency on a matched reader,
// and the housekeeping thread's expiry path raises LIVELINESS_LOST on the
// writer itself (`q_lease.c:297` -> `ddsi_endpoint.c:728`).
//
// The AUTOMATIC case is checked too: the ABI documents this slot as a no-op
// returning OK for a kind that has no lease to renew, so a publisher that
// never asked for manual liveliness must still get OK.

#include <chrono>
#include <cstdio>
#include <thread>

#include "nros/rmw_event.h"
#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"
#include "nros_test_domain.h"

namespace {

const nros_rmw_vtable_t* g_vt = nullptr;

constexpr uint32_t kLeaseMs = 500;

void sleep_ms(int ms) {
    std::this_thread::sleep_for(std::chrono::milliseconds(ms));
}

/// Drain a LIVELINESS_LOST from the publisher. `*out_taken` says whether one
/// was there; a non-OK return is a hard failure of the poll slot itself.
bool poll_lost(const rmw_publisher_t* pubr, bool* out_taken) {
    rmw_event_payload_t ev{};
    *out_taken = false;
    const rmw_ret_t r =
        g_vt->publisher_take_event(pubr, NROS_RMW_EVENT_LIVELINESS_LOST, &ev, out_taken);
    if (r != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "FAIL: publisher_take_event(LIVELINESS_LOST) rc=%d\n", (int)r);
        return false;
    }
    if (*out_taken && ev.count.total_count_change == 0) {
        std::fprintf(stderr, "FAIL: LIVELINESS_LOST taken with a zero change count\n");
        return false;
    }
    return true;
}

rmw_ret_t make_publisher(rmw_node_t* node, rmw_publisher_t* out, const char* topic,
                         uint8_t liveliness_kind, uint32_t lease_ms) {
    rmw_qos_profile_t qos{};
    qos.reliability = NROS_RMW_RELIABILITY_RELIABLE;
    qos.durability = NROS_RMW_DURABILITY_VOLATILE;
    qos.history = NROS_RMW_HISTORY_KEEP_LAST;
    qos.depth = 5;
    qos.liveliness_kind = liveliness_kind;
    qos.liveliness_lease_ms = lease_ms;

    out->topic_name = topic;
    out->type_name = "nros_test::msg::TestString";
    const rmw_message_type_support_t ts{out->type_name, ""};
    return g_vt->create_publisher(node, &ts, out->topic_name, 0, &qos, nullptr, out);
}

} // namespace

extern "C" rmw_ret_t nros_rmw_cffi_register_named(const char* /*name*/,
                                                  const nros_rmw_vtable_t* vt) {
    g_vt = vt;
    return NROS_RMW_RET_OK;
}

int main() {
    if (nros_rmw_cyclonedds_register() != NROS_RMW_RET_OK || g_vt == nullptr) {
        return 1;
    }
    // The regression guard for the slot itself: issue 1231 is exactly this
    // pointer being NULL, which the runtime reports as UNSUPPORTED.
    if (g_vt->publisher_assert_liveliness == nullptr) {
        std::fprintf(stderr, "FAIL: publisher_assert_liveliness is NULL\n");
        return 2;
    }
    if (g_vt->publisher_take_event == nullptr) {
        std::fprintf(stderr, "FAIL: publisher_take_event is NULL — no way to observe the lease\n");
        return 3;
    }

    rmw_session_t s{};
    s.node_name = "assert_liveliness";
    s.namespace_ = "/";
    if (g_vt->create_session(nullptr, 0, nros_test_domain(99), s.node_name, nullptr, &s) !=
        NROS_RMW_RET_OK) {
        std::fprintf(stderr, "FAIL: create_session\n");
        return 4;
    }
    rmw_node_t node{};
    node.name = s.node_name;
    node.namespace_ = s.namespace_;
    node.session = &s;

    rmw_publisher_t manual{};
    if (make_publisher(&node, &manual, "rt/assert_liveliness", NROS_RMW_LIVELINESS_MANUAL_BY_TOPIC,
                       kLeaseMs) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "FAIL: create_publisher (manual)\n");
        (void)g_vt->destroy_session(&s);
        return 5;
    }

    int rc = 0;
    bool took = false;

    // ---- Phase A — asserted every 100 ms against a 500 ms lease ----
    for (int i = 0; i < 20 && rc == 0; ++i) {
        const rmw_ret_t r = g_vt->publisher_assert_liveliness(&manual);
        if (r != NROS_RMW_RET_OK) {
            std::fprintf(stderr, "FAIL: assert_liveliness rc=%d\n", (int)r);
            rc = 6;
            break;
        }
        sleep_ms(100);
        if (!poll_lost(&manual, &took)) {
            rc = 7;
            break;
        }
        if (took) {
            std::fprintf(stderr,
                         "FAIL: LIVELINESS_LOST after %d ms of assertions at 100 ms against a "
                         "%u ms lease — the assertion is not reaching the writer\n",
                         (i + 1) * 100, kLeaseMs);
            rc = 8;
            break;
        }
    }

    // ---- Phase B — the negative control: stop, and the lease must lapse ----
    bool saw_lost = false;
    for (int i = 0; i < 100 && rc == 0 && !saw_lost; ++i) {
        sleep_ms(50);
        if (!poll_lost(&manual, &took)) {
            rc = 9;
            break;
        }
        saw_lost = took;
    }
    if (rc == 0 && !saw_lost) {
        std::fprintf(stderr,
                     "FAIL: no LIVELINESS_LOST after 5 s of silence on a %u ms lease — phase A "
                     "proved nothing, because the lease never runs here\n",
                     kLeaseMs);
        rc = 10;
    }

    // ---- The documented no-op: AUTOMATIC has no lease to renew ----
    if (rc == 0) {
        rmw_publisher_t automatic{};
        if (make_publisher(&node, &automatic, "rt/assert_liveliness_auto",
                           NROS_RMW_LIVELINESS_AUTOMATIC, kLeaseMs) != NROS_RMW_RET_OK) {
            std::fprintf(stderr, "FAIL: create_publisher (automatic)\n");
            rc = 11;
        } else {
            const rmw_ret_t r = g_vt->publisher_assert_liveliness(&automatic);
            if (r != NROS_RMW_RET_OK) {
                std::fprintf(stderr,
                             "FAIL: assert_liveliness on an AUTOMATIC publisher returned %d; the "
                             "ABI documents it as a no-op returning OK\n",
                             (int)r);
                rc = 12;
            }
            g_vt->destroy_publisher(&automatic);
        }
    }

    // A destroyed / never-created publisher is a caller error, not a silent OK.
    if (rc == 0) {
        rmw_publisher_t empty{};
        const rmw_ret_t r = g_vt->publisher_assert_liveliness(&empty);
        if (r != NROS_RMW_RET_INVALID_ARGUMENT) {
            std::fprintf(stderr,
                         "FAIL: assert_liveliness on an uninitialised publisher "
                         "returned %d\n",
                         (int)r);
            rc = 13;
        }
    }

    g_vt->destroy_publisher(&manual);
    (void)g_vt->destroy_session(&s);
    if (rc == 0) {
        std::printf("ASSERT_LIVELINESS_OK\n");
    }
    return rc;
}
