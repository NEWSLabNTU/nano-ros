// phase-397 W2.b — an arrival on a registered entity calls THAT entity's
// callback, and not the session wake.
//
// `subscription_set_on_new_message_callback` puts a data_available listener on
// the subscription's reader. Cyclone invokes a reader's own listener instead of
// the participant's, which is where the session wake lives, so:
//
//   1. a subscription WITH a callback: arrivals count on it, never on the
//      session wake;
//   2. a second subscription on the same session WITHOUT one: its arrivals
//      still reach the session wake (the fallback is intact);
//   3. clearing the callback returns the first subscription's arrivals to the
//      session wake.
//
// Negative control: before anything is published, both counters are zero.
//
// Run twice (issue 1963). With no argument the session wake is installed
// FIRST, as the executor does at session open. With `late` it is installed
// AFTER every entity exists; the three properties must hold either way. The
// late ordering is the one that failed property 2 while the participant's
// listener was attached at install time, because Cyclone only pushes a parent
// listener down into readers that inherited it when they were created.
//
// Stubs `nros_rmw_cffi_register_named` since the runtime isn't linked.

#include <atomic>
#include <chrono>
#include <cstdio>
#include <cstring>
#include <thread>

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"
#include "nros_test_domain.h"

namespace {
const nros_rmw_vtable_t* g_vt = nullptr;

std::atomic<int> g_session_wakes{0};
std::atomic<int> g_entity_wakes{0};
std::atomic<const void*> g_last_user_data{nullptr};

void on_session_wake(void* /*ctx*/) {
    g_session_wakes.fetch_add(1);
}

void on_entity_wake(const void* user_data, size_t n) {
    if (n >= 1) {
        g_entity_wakes.fetch_add(1);
        g_last_user_data.store(user_data);
    }
}

int g_marker = 0;

/// `TestString { data = "wake" }`, CDR_LE.
struct Sample {
    uint8_t cdr[16]{};
    size_t len{0};
};

Sample make_sample() {
    Sample s;
    const char* msg = "wake";
    const size_t mlen = std::strlen(msg) + 1;
    s.cdr[1] = 0x01;
    s.cdr[4] = static_cast<uint8_t>(mlen);
    std::memcpy(s.cdr + 8, msg, mlen);
    s.len = 8 + mlen;
    return s;
}

/// Publish until `counter` moves past `from`, or give up after ~5 s.
bool publish_until(rmw_publisher_t* pub, const Sample& s, std::atomic<int>& counter, int from) {
    for (int i = 0; i < 100; ++i) {
        if (g_vt->publish(pub, rmw_byte_span_t{s.cdr, s.len}) != NROS_RMW_RET_OK) {
            return false;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(50));
        if (counter.load() > from) {
            return true;
        }
    }
    return false;
}

void drain(rmw_subscription_t* sub) {
    uint8_t buf[64];
    size_t n = 0;
    bool took = true;
    while (took && nros_test_take(g_vt, sub, buf, sizeof(buf), &n, &took) == NROS_RMW_RET_OK) {
    }
}
} // namespace

extern "C" rmw_ret_t nros_rmw_cffi_register_named(const char* /*name*/,
                                                  const nros_rmw_vtable_t* vt) {
    g_vt = vt;
    return NROS_RMW_RET_OK;
}

int main(int argc, char** argv) {
    const bool late = argc > 1 && std::strcmp(argv[1], "late") == 0;
    if (nros_rmw_cyclonedds_register() != NROS_RMW_RET_OK || g_vt == nullptr) {
        std::fprintf(stderr, "register failed\n");
        return 1;
    }
    if (g_vt->subscription_set_on_new_message_callback == nullptr ||
        g_vt->service_set_on_new_request_callback == nullptr ||
        g_vt->client_set_on_new_response_callback == nullptr) {
        std::fprintf(stderr, "an entity wake slot is NULL\n");
        return 2;
    }

    rmw_session_t s{};
    s.node_name = "nros_rmw_cyclonedds_entity_wake";
    s.namespace_ = "/";
    if (g_vt->create_session(nullptr, 0, nros_test_domain(99), s.node_name, nullptr, &s) !=
        NROS_RMW_RET_OK) {
        std::fprintf(stderr, "open failed\n");
        return 3;
    }
    rmw_node_t node{};
    node.name = s.node_name;
    node.namespace_ = s.namespace_;
    node.session = &s;
    rmw_qos_profile_t qos = NROS_RMW_QOS_PROFILE_DEFAULT;
    const rmw_message_type_support_t ts{"nros_test::msg::TestString", ""};

    // Installed first by default, as the executor does at session open; with
    // `late`, after the entities below.
    if (!late && g_vt->set_wake_callback(&s, on_session_wake, nullptr) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "set_wake_callback failed\n");
        return 5;
    }
    rmw_publisher_t pub_a{};
    rmw_publisher_t pub_b{};
    rmw_subscription_t sub_a{};
    rmw_subscription_t sub_b{};
    if (g_vt->create_publisher(&node, &ts, "rt/entity_wake_a", 99, &qos, nullptr, &pub_a) !=
            NROS_RMW_RET_OK ||
        g_vt->create_publisher(&node, &ts, "rt/entity_wake_b", 99, &qos, nullptr, &pub_b) !=
            NROS_RMW_RET_OK ||
        g_vt->create_subscription(&node, &ts, "rt/entity_wake_a", 99, &qos, nullptr, &sub_a) !=
            NROS_RMW_RET_OK ||
        g_vt->create_subscription(&node, &ts, "rt/entity_wake_b", 99, &qos, nullptr, &sub_b) !=
            NROS_RMW_RET_OK) {
        std::fprintf(stderr, "entity creation failed\n");
        return 4;
    }
    if (late && g_vt->set_wake_callback(&s, on_session_wake, nullptr) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "late set_wake_callback failed\n");
        return 5;
    }

    if (g_vt->subscription_set_on_new_message_callback(&sub_a, on_entity_wake, &g_marker) !=
        NROS_RMW_RET_OK) {
        std::fprintf(stderr, "subscription_set_on_new_message_callback failed\n");
        return 6;
    }

    // Discovery inside one participant is immediate, but let the matching settle.
    std::this_thread::sleep_for(std::chrono::milliseconds(200));
    if (g_session_wakes.load() != 0 || g_entity_wakes.load() != 0) {
        std::fprintf(stderr, "negative control: wakes before any publish (session %d, entity %d)\n",
                     g_session_wakes.load(), g_entity_wakes.load());
        return 7;
    }

    const Sample sample = make_sample();
    int rc = 0;

    // 1. Attributed: the entity's callback, with its own user_data, and no
    //    session wake.
    if (!publish_until(&pub_a, sample, g_entity_wakes, 0)) {
        std::fprintf(stderr, "1: the registered subscription's callback never fired\n");
        rc = 8;
    } else if (g_last_user_data.load() != &g_marker) {
        std::fprintf(stderr, "1: the callback got the wrong user_data\n");
        rc = 9;
    } else if (g_session_wakes.load() != 0) {
        std::fprintf(stderr, "1: %d session wake(s) for an attributed reader's arrivals\n",
                     g_session_wakes.load());
        rc = 10;
    }
    drain(&sub_a);

    // 2. Unattributed reader on the same session: the session wake is intact.
    const int entity_before = g_entity_wakes.load();
    if (rc == 0 && !publish_until(&pub_b, sample, g_session_wakes, 0)) {
        std::fprintf(stderr,
                     "2: an unregistered reader's arrival did not reach the session wake\n");
        rc = 11;
    } else if (rc == 0 && g_entity_wakes.load() != entity_before) {
        std::fprintf(stderr, "2: the other subscription's arrival called sub_a's callback\n");
        rc = 12;
    }
    drain(&sub_b);

    // 3. Cleared: sub_a's arrivals go back to the session wake.
    if (rc == 0) {
        if (g_vt->subscription_set_on_new_message_callback(&sub_a, nullptr, nullptr) !=
            NROS_RMW_RET_OK) {
            std::fprintf(stderr, "3: clearing failed\n");
            rc = 13;
        } else {
            const int session_before = g_session_wakes.load();
            const int entity_after_clear = g_entity_wakes.load();
            if (!publish_until(&pub_a, sample, g_session_wakes, session_before)) {
                std::fprintf(stderr,
                             "3: after clearing, sub_a's arrival reached no session wake\n");
                rc = 14;
            } else if (g_entity_wakes.load() != entity_after_clear) {
                std::fprintf(stderr, "3: a cleared callback still fired\n");
                rc = 15;
            }
        }
    }
    drain(&sub_a);

    (void)g_vt->set_wake_callback(&s, nullptr, nullptr);
    g_vt->destroy_subscription(&sub_b);
    g_vt->destroy_subscription(&sub_a);
    g_vt->destroy_publisher(&pub_b);
    g_vt->destroy_publisher(&pub_a);
    if (g_vt->destroy_session(&s) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "close failed\n");
        return 16;
    }
    if (rc == 0) {
        std::printf("OK %s: attributed %d, session %d\n", late ? "late" : "early",
                    g_entity_wakes.load(), g_session_wakes.load());
    }
    return rc;
}
