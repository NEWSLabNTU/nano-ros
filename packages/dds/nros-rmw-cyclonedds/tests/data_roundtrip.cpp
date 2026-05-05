// Phase 117.6.B end-to-end raw-CDR data path test.
//
// Publishes a hand-crafted CDR-encoded `nros_test::msg::TestString`
// payload via the vtable's `publish_raw`, then `try_recv_raw` on a
// subscriber created on the same topic. Verifies the bytes
// round-trip through Cyclone's writer + reader.
//
// CDR-LE wire format for the test type
//   struct TestString { string data; };
// is:
//   00 01 00 00         // encapsulation: CDR_LE, options=0
//   <len>               // uint32_t length-including-NUL
//   <chars> 00          // payload + null terminator
// All multi-byte fields little-endian.
//
// We pre-build the CDR for the string "hello", publish it on
// "rt/data_roundtrip", spin until the reader has data (with a
// short timeout), then take it back and assert the same bytes.

#include <cstdio>
#include <cstring>
#include <thread>
#include <chrono>

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

    // Build the CDR for `TestString { data = "hello" }`. Length
    // includes the trailing NUL — that's the IDL `string` wire
    // format Cyclone emits.
    const char *msg = "hello";
    size_t mlen = std::strlen(msg) + 1;  // include NUL
    uint8_t cdr[64] = {
        0x00, 0x01, 0x00, 0x00,                    // encap: CDR_LE
        static_cast<uint8_t>(mlen & 0xff),
        static_cast<uint8_t>((mlen >> 8) & 0xff),
        static_cast<uint8_t>((mlen >> 16) & 0xff),
        static_cast<uint8_t>((mlen >> 24) & 0xff),
    };
    std::memcpy(cdr + 8, msg, mlen);
    size_t cdr_len = 8 + mlen;

    nros_rmw_session_t s{};
    s.node_name  = "data_roundtrip";
    s.namespace_ = "/";
    if (g_vt->open(nullptr, 0, 99, s.node_name, &s) != NROS_RMW_RET_OK) {
        return 2;
    }

    nros_rmw_qos_t qos = NROS_RMW_QOS_PROFILE_DEFAULT;

    nros_rmw_subscriber_t sub{};
    sub.topic_name = "rt/data_roundtrip";
    sub.type_name  = "nros_test::msg::TestString";
    sub.qos        = qos;
    if (g_vt->create_subscriber(&s, sub.topic_name, sub.type_name, "",
                                99, &qos, &sub) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_subscriber failed\n");
        return 3;
    }

    nros_rmw_publisher_t pub{};
    pub.topic_name = "rt/data_roundtrip";
    pub.type_name  = "nros_test::msg::TestString";
    pub.qos        = qos;
    if (g_vt->create_publisher(&s, pub.topic_name, pub.type_name, "",
                               99, &qos, &pub) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_publisher failed\n");
        return 4;
    }

    // Reliable QoS — Cyclone takes a moment to discover the local
    // peer, so spin briefly before publishing so the writer doesn't
    // pre-empt subscription matching.
    std::this_thread::sleep_for(std::chrono::milliseconds(200));

    nros_rmw_ret_t pr = g_vt->publish_raw(&pub, cdr, cdr_len);
    if (pr != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "publish_raw returned %d\n", static_cast<int>(pr));
        return 5;
    }

    // Poll for data (max ~1s).
    bool got = false;
    for (int i = 0; i < 100 && !got; ++i) {
        if (g_vt->has_data(&sub)) {
            got = true;
            break;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(10));
    }
    if (!got) {
        std::fprintf(stderr, "no data after publish\n");
        return 6;
    }

    uint8_t buf[256] = {};
    int32_t n = g_vt->try_recv_raw(&sub, buf, sizeof(buf));
    if (n <= 0) {
        std::fprintf(stderr, "try_recv_raw returned %d\n", n);
        return 7;
    }
    if (static_cast<size_t>(n) != cdr_len) {
        std::fprintf(stderr,
                     "round-trip size mismatch: pub=%zu sub=%d\n",
                     cdr_len, n);
        return 8;
    }
    if (std::memcmp(buf, cdr, cdr_len) != 0) {
        std::fprintf(stderr, "round-trip bytes mismatch\n");
        for (size_t i = 0; i < cdr_len; ++i) {
            std::fprintf(stderr, "  [%zu] sent=%02x got=%02x\n", i,
                         cdr[i], buf[i]);
        }
        return 9;
    }

    g_vt->destroy_publisher(&pub);
    g_vt->destroy_subscriber(&sub);
    (void) g_vt->close(&s);
    std::printf("OK %d bytes round-tripped\n", n);
    return 0;
}
