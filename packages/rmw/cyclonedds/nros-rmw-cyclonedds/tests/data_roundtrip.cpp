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

    rmw_session_t s{};
    s.node_name  = "data_roundtrip";
    s.namespace_ = "/";
    if (g_vt->create_session(nullptr, 0, nros_test_domain(99), s.node_name, nullptr, &s) != NROS_RMW_RET_OK) {
        return 2;
    }

    // Phase 376 W5/B1 — entities are created ON A NODE now. The node
    // carries its own identity plus the route to its session.
    rmw_node_t node{};
    node.name       = s.node_name;
    node.namespace_ = s.namespace_;
    node.session    = &s;

    rmw_qos_profile_t qos = NROS_RMW_QOS_PROFILE_DEFAULT;

    rmw_subscription_t sub{};
    sub.topic_name = "rt/data_roundtrip";
    sub.type_name  = "nros_test::msg::TestString";
    sub.qos        = qos;
    const rmw_message_type_support_t ts_1{sub.type_name, ""};
    if (g_vt->create_subscription(&node, &ts_1, sub.topic_name, 99, &qos, nullptr, &sub) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_subscription failed\n");
        return 3;
    }

    rmw_publisher_t pub{};
    pub.topic_name = "rt/data_roundtrip";
    pub.type_name  = "nros_test::msg::TestString";
    pub.qos        = qos;
    const rmw_message_type_support_t ts_2{pub.type_name, ""};
    if (g_vt->create_publisher(&node, &ts_2, pub.topic_name, 99, &qos, nullptr, &pub) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_publisher failed\n");
        return 4;
    }

    // Reliable QoS — Cyclone takes a moment to discover the local
    // peer, so spin briefly before publishing so the writer doesn't
    // pre-empt subscription matching.
    std::this_thread::sleep_for(std::chrono::milliseconds(200));

    rmw_ret_t pr = g_vt->publish(&pub, rmw_byte_span_t{cdr, cdr_len});
    if (pr != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "publish_raw returned %d\n", static_cast<int>(pr));
        return 5;
    }

    // Poll for data (max ~1s).
    bool got = false;
    for (int i = 0; i < 100 && !got; ++i) {
        bool has_d = false;
        /* Phase 376 W3.d step A — flag out, status returned. */
        if (g_vt->has_data(&sub, &has_d) == NROS_RMW_RET_OK && has_d) {
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
    size_t n = 0;
    bool took = false;
    /* Phase 376 W3.b/W3.d step A — status returned, bytes + taken out. */
    if (nros_test_take(g_vt, &sub, buf, sizeof(buf), &n, &took) != NROS_RMW_RET_OK || !took || n == 0) {
        std::fprintf(stderr, "take returned %zu bytes, taken=%d\n", n, (int)took);
        return 7;
    }
    // Issue 0970 — the bytes published are the bytes taken, exactly.
    //
    // Worth recording what this assertion has been through, because it is the
    // clearest measure of what the two issues did. Originally it passed for the
    // wrong reason: both directions round-tripped through a typed C struct, and
    // the 14 bytes out were a RE-SERIALISATION that happened to match the 14
    // bytes in. Issue 0969 made the take path return the serdata's own bytes,
    // and this went to 16 with `options = 2` — Cyclone's OWN sertype had padded
    // the payload to a 4-byte multiple on the way in and recorded the pad count
    // in the encapsulation options. That 16 was the honest wire answer for a
    // Cyclone-serialised sample, and an `rmw_cyclonedds_cpp` subscriber sees the
    // same, since `rmw_take_ser_int` returns `ddsi_serdata_size` verbatim.
    //
    // Issue 0970 removed the other half: our sertype stores what the publisher
    // handed it, so nothing pads and nothing rewrites the header. 14 in, 14 out,
    // identical bytes. Assert that exactly — the point of the change is that
    // this backend is now transparent to the CDR, and a length that drifted by
    // even the alignment pad would mean something started re-encoding again.
    //
    // TRANSPARENT IS NOT THE SAME AS UNPADDED, and this test is the loopback
    // case where the two coincide. Measured against a real ROS 2 publisher
    // (`ros2_pubsub_e2e`, which prints a `WIRE=` line for exactly this reason):
    // a 25-byte CDR message arrives as `len:28 hdr:00010000 cdr:25`. The three
    // extra bytes are the RTPS submessage's 4-byte alignment, applied by the
    // SENDER, and `from_ser` is handed that padded length — so `get_size`
    // returns it, having added nothing. The encapsulation options say `0000`,
    // not `0003`, so the pad length is not recoverable from the header either.
    //
    // Two consequences, neither removed by issue 0970: a deserialiser must
    // tolerate trailing bytes (nros-serdes reads by position, so it does), and
    // a receive buffer sized from a type's exact `MAX_SERIALIZED_SIZE` can be
    // up to 3 bytes short of what a remote peer delivers. See issue 0964.
    if (n != cdr_len) {
        std::fprintf(stderr,
                     "round-trip size mismatch: pub=%zu sub=%zu\n",
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
    g_vt->destroy_subscription(&sub);
    (void) g_vt->destroy_session(&s);
    std::printf("OK %zu bytes round-tripped (%zu sent + %zu pad)\n", n, cdr_len,
                n - cdr_len);
    return 0;
}
