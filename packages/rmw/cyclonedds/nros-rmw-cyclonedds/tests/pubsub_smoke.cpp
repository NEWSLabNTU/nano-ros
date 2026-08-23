// Phase 117.6 entity-plumbing smoke test.
//
// Drives publisher_create / subscriber_create / *_destroy through
// the registered vtable. Verifies the topic + writer + reader are
// real Cyclone entities and that try_recv_raw yields no bytes before
// any publish (has_data is a poll-only conservative always-1).
//
// Stubs `nros_rmw_cffi_register` since the runtime isn't linked.

#include <cstdio>
#include <cstring>

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"
#include "nros_test_domain.h"

namespace {
const nros_rmw_vtable_t* g_vt = nullptr;
} // namespace

extern "C" rmw_ret_t nros_rmw_cffi_register_named(const char* /*name*/,
                                                       const nros_rmw_vtable_t* vt) {
    g_vt = vt;
    return NROS_RMW_RET_OK;
}

int main() {
    if (nros_rmw_cyclonedds_register() != NROS_RMW_RET_OK || g_vt == nullptr) {
        std::fprintf(stderr, "register failed\n");
        return 1;
    }

    rmw_session_t s{};
    s.node_name = "nros_rmw_cyclonedds_pubsub_smoke";
    s.namespace_ = "/";

    if (g_vt->create_session(nullptr, 0, nros_test_domain(99), s.node_name, &s) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "open failed\n");
        return 2;
    }

    // Default-ish QoS — reliability=reliable, history=keep_last(10).
    rmw_qos_profile_t qos = NROS_RMW_QOS_PROFILE_DEFAULT;

    // Publisher round-trip: create + destroy on the registered test
    // type.
    rmw_publisher_t pub{};
    pub.topic_name = "rt/pubsub_smoke";
    pub.type_name = "nros_test::msg::TestString";
    pub.qos = qos;
    if (g_vt->create_publisher(&s, pub.topic_name, pub.type_name, "", 99, &qos, nullptr, &pub) !=
        NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_publisher failed\n");
        (void)g_vt->destroy_session(&s);
        return 3;
    }
    if (pub.backend_data == nullptr) {
        std::fprintf(stderr, "publisher backend_data is NULL\n");
        return 4;
    }

    rmw_subscription_t sub{};
    sub.topic_name = "rt/pubsub_smoke";
    sub.type_name = "nros_test::msg::TestString";
    sub.qos = qos;
    if (g_vt->create_subscription(&s, sub.topic_name, sub.type_name, "", 99, &qos, nullptr, &sub) !=
        NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_subscription failed\n");
        g_vt->destroy_publisher(&pub);
        (void)g_vt->destroy_session(&s);
        return 5;
    }
    if (sub.backend_data == nullptr) {
        std::fprintf(stderr, "subscriber backend_data is NULL\n");
        return 6;
    }

    // No publish has occurred. This backend is poll-only: has_data() is a
    // conservative "maybe" that always returns 1 (Cyclone's DATA_AVAILABLE is
    // edge-like, so querying it as a pre-filter would suppress the take path).
    // `take` is the authoritative non-blocking check — with nothing published
    // it must report `taken = false` with an OK status. Phase 376 W3.d step A
    // retired NROS_RMW_RET_NO_DATA here: an empty subscription is no longer a
    // negative sentinel, so this assertion now distinguishes "nothing to take"
    // from "the take FAILED", which the old `> 0` test could not.
    uint8_t rxbuf[64];
    size_t rxn = 0;
    bool rxtook = true;
    if (g_vt->take(&sub, rxbuf, sizeof(rxbuf), &rxn, &rxtook) != NROS_RMW_RET_OK || rxtook) {
        std::fprintf(stderr, "take should yield nothing with no published data\n");
        g_vt->destroy_subscription(&sub);
        g_vt->destroy_publisher(&pub);
        (void)g_vt->destroy_session(&s);
        return 7;
    }

    // publish_raw with too-short input (< 4-byte CDR header) → invalid arg.
    if (g_vt->publish_raw(&pub, reinterpret_cast<const uint8_t*>("x"), 1) !=
        NROS_RMW_RET_INVALID_ARGUMENT) {
        std::fprintf(stderr, "publish_raw too-short should report INVALID_ARGUMENT\n");
        return 8;
    }

    // Unknown type: create_publisher must report UNSUPPORTED, not
    // ERROR.
    rmw_publisher_t bad{};
    if (g_vt->create_publisher(&s, "rt/unknown", "no::such::Type", "", 99, &qos, nullptr, &bad) !=
        NROS_RMW_RET_UNSUPPORTED) {
        std::fprintf(stderr, "create_publisher unknown-type should be UNSUPPORTED\n");
        return 9;
    }

    g_vt->destroy_subscription(&sub);
    g_vt->destroy_publisher(&pub);
    if (g_vt->destroy_session(&s) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "close failed\n");
        return 10;
    }

    std::printf("OK\n");
    return 0;
}
