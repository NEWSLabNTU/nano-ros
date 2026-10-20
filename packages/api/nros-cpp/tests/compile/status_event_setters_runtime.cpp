// Issue 1993 — the five Tier-1 C++ status-event setters reach the backend.
//
// `nros_cpp_{subscription,publisher}_set_*` for liveliness, deadline and
// message-lost (behind `PollSubscription::on_liveliness_changed` /
// `on_requested_deadline_missed` / `on_message_lost` and
// `Publisher::on_liveliness_lost` / `on_offered_deadline_missed`) returned
// `NROS_CPP_RET_UNSUPPORTED` unconditionally. They now register through
// nros-c's `status_event_slots` table, like the issue-1972 matched setters.
// Against the stub backend in accept mode this probe checks, per setter:
//
//   1. `NROS_CPP_RET_OK`, and the stub holds a registration carrying the
//      caller's `deadline_ms` (0 for the kinds that take none);
//   2. a fire calls the user callback with the entity's STORAGE, the status
//      and the context — and after a C++ move (`*_relocate`) the NEW storage;
//   3. with the backend denying status events (the XRCE / uORB answer) every
//      setter returns `NROS_CPP_RET_UNSUPPORTED`;
//   4. after `*_destroy` the backend still holds the registration and a fire
//      is a no-op.
//
// Negative control for (2): before any registration a fire returns false.

#include "rclcpp/executor.hpp"
#include "rclcpp/node.hpp"

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

void check(bool ok, const char* what) {
    if (!ok) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s\n", what);
    }
}

void check_int(long got, long expect, const char* what) {
    if (got != expect) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s — expected %ld, got %ld\n", what, expect, got);
    }
}

// rmw_event_type_t values.
enum : int {
    K_LIVELINESS_CHANGED = 0,
    K_REQUESTED_DEADLINE_MISSED = 1,
    K_MESSAGE_LOST = 2,
    K_LIVELINESS_LOST = 3,
    K_OFFERED_DEADLINE_MISSED = 4,
    K_COUNT = 5,
};

struct Seen {
    int calls;
    void* storage;
    void* ctx;
    uint32_t total;
    uint32_t change;
    nros_cpp_liveliness_changed_status_t live;
};

Seen g_seen[K_COUNT];
int g_ctx_ids[K_COUNT] = {K_LIVELINESS_CHANGED, K_REQUESTED_DEADLINE_MISSED, K_MESSAGE_LOST,
                          K_LIVELINESS_LOST, K_OFFERED_DEADLINE_MISSED};

extern "C" void on_liveliness_changed(void* storage, nros_cpp_liveliness_changed_status_t s,
                                      void* ctx) {
    Seen& v = g_seen[K_LIVELINESS_CHANGED];
    ++v.calls;
    v.storage = storage;
    v.ctx = ctx;
    v.live = s;
}

extern "C" void on_sub_count(void* storage, nros_cpp_count_status_t s, void* ctx) {
    Seen& v = g_seen[*static_cast<const int*>(ctx)];
    ++v.calls;
    v.storage = storage;
    v.ctx = ctx;
    v.total = s.total_count;
    v.change = s.total_count_change;
}

extern "C" void on_pub_count(void* storage, nros_cpp_pub_count_status_t s, void* ctx) {
    Seen& v = g_seen[*static_cast<const int*>(ctx)];
    ++v.calls;
    v.storage = storage;
    v.ctx = ctx;
    v.total = s.total_count;
    v.change = s.total_count_change;
}

int total_calls() {
    int n = 0;
    for (const Seen& s : g_seen) n += s.calls;
    return n;
}

alignas(8) unsigned char g_storage[NROS_CPP_EXECUTOR_STORAGE_SIZE];
alignas(8) unsigned char g_pub[4096];
alignas(8) unsigned char g_pub_moved[4096];
alignas(8) unsigned char g_sub[4096];

void register_all(void* pub, void* sub, nros_cpp_ret_t expect, const char* phase) {
    char what[160];
    std::snprintf(what, sizeof what, "%s: liveliness changed", phase);
    check_int(nros_cpp_subscription_set_liveliness_changed(sub, on_liveliness_changed,
                                                           &g_ctx_ids[K_LIVELINESS_CHANGED]),
              expect, what);
    std::snprintf(what, sizeof what, "%s: requested deadline missed", phase);
    check_int(nros_cpp_subscription_set_requested_deadline_missed(
                  sub, 250, on_sub_count, &g_ctx_ids[K_REQUESTED_DEADLINE_MISSED]),
              expect, what);
    std::snprintf(what, sizeof what, "%s: message lost", phase);
    check_int(
        nros_cpp_subscription_set_message_lost(sub, on_sub_count, &g_ctx_ids[K_MESSAGE_LOST]),
        expect, what);
    std::snprintf(what, sizeof what, "%s: liveliness lost", phase);
    check_int(
        nros_cpp_publisher_set_liveliness_lost(pub, on_pub_count, &g_ctx_ids[K_LIVELINESS_LOST]),
        expect, what);
    std::snprintf(what, sizeof what, "%s: offered deadline missed", phase);
    check_int(nros_cpp_publisher_set_offered_deadline_missed(
                  pub, 500, on_pub_count, &g_ctx_ids[K_OFFERED_DEADLINE_MISSED]),
              expect, what);
}

} // namespace

int main() {
    nros_stub_rmw_set_accept_entities(true);
    nros_cpp_ret_t rc =
        nros_cpp_init_rmw(NROS_STUB_RMW_NAME, nullptr, 0, "status_events", nullptr, g_storage);
    if (rc != NROS_CPP_RET_OK) {
        std::fprintf(stderr, "status_event_setters_runtime: init returned %d\n", (int)rc);
        return 1;
    }
    void* h = g_storage;
    nros_cpp_node_t node;
    std::memset(&node, 0, sizeof(node));
    check_int(nros_cpp_node_create(h, "status_events", nullptr, &node), NROS_CPP_RET_OK,
              "create node");

    nros_cpp_qos_t qos;
    std::memset(&qos, 0, sizeof(qos));
    qos.depth = 1;
    check_int(nros_cpp_publisher_create(&node, "/status_events", "std_msgs::msg::dds_::Int32_",
                                        "", qos, g_pub),
              NROS_CPP_RET_OK, "create publisher");
    check_int(nros_cpp_subscription_create(&node, "/status_events",
                                           "std_msgs::msg::dds_::Int32_", "", qos, g_sub),
              NROS_CPP_RET_OK, "create poll subscription");
    if (g_failures != 0) return 1;

    // Caller error.
    check_int(nros_cpp_publisher_set_liveliness_lost(g_pub, nullptr, nullptr),
              NROS_CPP_RET_INVALID_ARGUMENT, "NULL callback is refused");

    // Negative control.
    check(!nros_stub_rmw_fire_liveliness_changed(1, 0, 1, 0),
          "negative control: no liveliness registration before the setter");
    for (int k = K_REQUESTED_DEADLINE_MISSED; k < K_COUNT; ++k) {
        check(!nros_stub_rmw_fire_count_event(k, 1, 1),
              "negative control: no count registration before the setter");
    }

    // ---- 3. a backend without status events is reported ----
    nros_stub_rmw_set_status_events_supported(false);
    register_all(g_pub, g_sub, NROS_CPP_RET_UNSUPPORTED, "no event support");
    nros_stub_rmw_set_status_events_supported(true);
    for (int k = 0; k < K_COUNT; ++k) {
        check(nros_stub_rmw_status_event_deadline_ms(k) == -1,
              "an UNSUPPORTED answer registered nothing in the backend");
    }

    // ---- 1. every setter registers, with its deadline ----
    register_all(g_pub, g_sub, NROS_CPP_RET_OK, "supported");
    check_int(nros_stub_rmw_status_event_deadline_ms(K_REQUESTED_DEADLINE_MISSED), 250,
              "requested-deadline registration carries the caller's deadline_ms");
    check_int(nros_stub_rmw_status_event_deadline_ms(K_OFFERED_DEADLINE_MISSED), 500,
              "offered-deadline registration carries the caller's deadline_ms");
    check_int(nros_stub_rmw_status_event_deadline_ms(K_LIVELINESS_CHANGED), 0,
              "liveliness changed carries deadline 0");
    check_int(nros_stub_rmw_status_event_deadline_ms(K_MESSAGE_LOST), 0,
              "message lost carries deadline 0");
    check_int(nros_stub_rmw_status_event_deadline_ms(K_LIVELINESS_LOST), 0,
              "liveliness lost carries deadline 0");

    // ---- 2. a fire reaches the user callback ----
    check(nros_stub_rmw_fire_liveliness_changed(4, 0, 1, 0), "liveliness registration held");
    {
        const Seen& s = g_seen[K_LIVELINESS_CHANGED];
        check_int(s.calls, 1, "liveliness changed: fired once");
        check(s.storage == g_sub, "liveliness changed: storage is the subscription's");
        check(s.ctx == &g_ctx_ids[K_LIVELINESS_CHANGED], "liveliness changed: context");
        check(s.live.alive_count == 4 && s.live.not_alive_count == 0 &&
                  s.live.alive_count_change == 1 && s.live.not_alive_count_change == 0,
              "liveliness changed: status fields arrive intact");
    }
    for (int k = K_REQUESTED_DEADLINE_MISSED; k < K_COUNT; ++k) {
        const uint32_t total = 200u + static_cast<uint32_t>(k);
        check(nros_stub_rmw_fire_count_event(k, total, 1), "count registration held");
        const Seen& s = g_seen[k];
        void* want = (k == K_LIVELINESS_LOST || k == K_OFFERED_DEADLINE_MISSED)
                         ? static_cast<void*>(g_pub)
                         : static_cast<void*>(g_sub);
        if (s.calls != 1 || s.storage != want || s.ctx != &g_ctx_ids[k] || s.total != total ||
            s.change != 1) {
            ++g_failures;
            std::fprintf(stderr,
                         "FAIL: kind %d: calls=%d storage_ok=%d ctx_ok=%d total=%u change=%u\n",
                         k, s.calls, s.storage == want, s.ctx == &g_ctx_ids[k], (unsigned)s.total,
                         (unsigned)s.change);
        }
    }

    // A C++ move relocates the storage; the backend's registration follows.
    check_int(nros_cpp_publisher_relocate(g_pub, g_pub_moved), NROS_CPP_RET_OK,
              "publisher relocated");
    check(nros_stub_rmw_fire_count_event(K_LIVELINESS_LOST, 300, 1), "liveliness lost re-fired");
    check(g_seen[K_LIVELINESS_LOST].storage == g_pub_moved,
          "after a move the callback receives the NEW storage");

    // ---- 4. a late fire after destroy is a no-op ----
    const int before = total_calls();
    check_int(nros_cpp_publisher_destroy(g_pub_moved), NROS_CPP_RET_OK, "publisher destroyed");
    check_int(nros_cpp_subscription_destroy(g_sub), NROS_CPP_RET_OK, "subscription destroyed");
    check(nros_stub_rmw_fire_count_event(K_OFFERED_DEADLINE_MISSED, 9, 1),
          "the backend still holds the publisher's registration");
    check(nros_stub_rmw_fire_count_event(K_REQUESTED_DEADLINE_MISSED, 9, 1),
          "the backend still holds the subscription's registration");
    check(nros_stub_rmw_fire_liveliness_changed(0, 1, -1, 1),
          "the backend still holds the subscription's registration");
    check_int(total_calls(), before, "no callback runs after destroy");

    (void)nros_cpp_node_destroy(&node);
    (void)nros_cpp_fini(h);
    if (g_failures != 0) {
        std::fprintf(stderr, "status_event_setters_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("status_event_setters_runtime: OK\n");
    return 0;
}
