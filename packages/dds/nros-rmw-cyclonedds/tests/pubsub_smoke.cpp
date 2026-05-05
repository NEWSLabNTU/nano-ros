// Phase 117.6 entity-plumbing smoke test.
//
// Drives publisher_create / subscriber_create / *_destroy through
// the registered vtable. Verifies the topic + writer + reader are
// real Cyclone entities and that has_data starts at zero (no data
// available before publish_raw lands in 117.6.B).
//
// Stubs `nros_rmw_cffi_register` since the runtime isn't linked.

#include <cstdio>
#include <cstring>

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"

namespace {
const nros_rmw_vtable_t *g_vt = nullptr;
} // namespace

extern "C" nros_rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vt) {
    g_vt = vt;
    return NROS_RMW_RET_OK;
}

int main() {
    if (nros_rmw_cyclonedds_register() != NROS_RMW_RET_OK || g_vt == nullptr) {
        std::fprintf(stderr, "register failed\n");
        return 1;
    }

    nros_rmw_session_t s{};
    s.node_name  = "nros_rmw_cyclonedds_pubsub_smoke";
    s.namespace_ = "/";

    if (g_vt->open(nullptr, 0, 99, s.node_name, &s) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "open failed\n");
        return 2;
    }

    // Default-ish QoS — reliability=reliable, history=keep_last(10).
    nros_rmw_qos_t qos = NROS_RMW_QOS_PROFILE_DEFAULT;

    // Publisher round-trip: create + destroy on the registered test
    // type.
    nros_rmw_publisher_t pub{};
    pub.topic_name = "rt/pubsub_smoke";
    pub.type_name  = "nros_test::msg::TestString";
    pub.qos        = qos;
    if (g_vt->create_publisher(&s, pub.topic_name, pub.type_name, "",
                               99, &qos, &pub) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_publisher failed\n");
        (void) g_vt->close(&s);
        return 3;
    }
    if (pub.backend_data == nullptr) {
        std::fprintf(stderr, "publisher backend_data is NULL\n");
        return 4;
    }

    nros_rmw_subscriber_t sub{};
    sub.topic_name = "rt/pubsub_smoke";
    sub.type_name  = "nros_test::msg::TestString";
    sub.qos        = qos;
    if (g_vt->create_subscriber(&s, sub.topic_name, sub.type_name, "",
                                99, &qos, &sub) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_subscriber failed\n");
        g_vt->destroy_publisher(&pub);
        (void) g_vt->close(&s);
        return 5;
    }
    if (sub.backend_data == nullptr) {
        std::fprintf(stderr, "subscriber backend_data is NULL\n");
        return 6;
    }

    // No publish has occurred — has_data must be 0.
    if (g_vt->has_data(&sub) != 0) {
        std::fprintf(stderr, "has_data should be 0 with no published data\n");
        return 7;
    }

    // publish_raw with too-short input (< 4-byte CDR header) → invalid arg.
    if (g_vt->publish_raw(&pub, reinterpret_cast<const uint8_t *>("x"), 1)
        != NROS_RMW_RET_INVALID_ARGUMENT) {
        std::fprintf(stderr, "publish_raw too-short should report INVALID_ARGUMENT\n");
        return 8;
    }

    // Unknown type: create_publisher must report UNSUPPORTED, not
    // ERROR.
    nros_rmw_publisher_t bad{};
    if (g_vt->create_publisher(&s, "rt/unknown", "no::such::Type", "",
                               99, &qos, &bad) != NROS_RMW_RET_UNSUPPORTED) {
        std::fprintf(stderr, "create_publisher unknown-type should be UNSUPPORTED\n");
        return 9;
    }

    g_vt->destroy_subscriber(&sub);
    g_vt->destroy_publisher(&pub);
    if (g_vt->close(&s) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "close failed\n");
        return 10;
    }

    std::printf("OK\n");
    return 0;
}
