// Issue 0971 — a batch drain that stops because a message did not fit reports
// the count it earned, and the NEXT take reports why it stopped.
//
// The `take_sequence` slot is contractually a count: "partial drains MUST use
// the count form, not error-out" (`rmw_vtable.h`). So the status has nowhere to
// go at the moment it happens, and before this test existed it went nowhere at
// all — a partial drain and a drained reader returned the same answer, and the
// oversized message was consumed with no signal.
//
// This is also the first coverage `take_sequence` has in this backend. It had
// none, which is how the unreachable `if (err < 0)` branch survived from the day
// it was written.
//
// Shape: publish a short message and a long one, then drain with a
// `per_msg_cap` that fits the short one only. Expected sequence of answers:
//
//   1. OK, count == 1        (the short message; the long one stopped the batch)
//   2. BUFFER_TOO_SMALL      (why it stopped, taking nothing else)
//   3. OK, count == 0        (the reader is empty — the long one was consumed)
//
// The loop below tolerates the two messages arriving in separate batches, since
// nothing here can wait for "both delivered": this backend's `has_data` is
// deliberately a conservative `true`. What it does NOT tolerate is the long
// message being delivered, or the status never arriving.

#include <cstdio>
#include <cstring>
#include <thread>
#include <chrono>

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"
#include "nros_test_domain.h"

namespace {
const nros_rmw_vtable_t* g_vt = nullptr;

// `TestString { string data; }` — 4-byte encapsulation, u32 length including
// the NUL, then the bytes.
size_t build_cdr(uint8_t* out, size_t out_cap, const char* msg) {
    const size_t mlen = std::strlen(msg) + 1;
    const size_t total = 8 + mlen;
    if (out_cap < total) {
        return 0;
    }
    out[0] = 0x00;
    out[1] = 0x01;
    out[2] = 0x00;
    out[3] = 0x00;
    out[4] = static_cast<uint8_t>(mlen & 0xff);
    out[5] = static_cast<uint8_t>((mlen >> 8) & 0xff);
    out[6] = static_cast<uint8_t>((mlen >> 16) & 0xff);
    out[7] = static_cast<uint8_t>((mlen >> 24) & 0xff);
    std::memcpy(out + 8, msg, mlen);
    return total;
}
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
    if (g_vt->take_sequence == nullptr) {
        std::fprintf(stderr, "backend has no native take_sequence\n");
        return 1;
    }

    rmw_session_t s{};
    s.node_name = "take_sequence_pending_status";
    s.namespace_ = "/";
    if (g_vt->create_session(nullptr, 0, nros_test_domain(99), s.node_name, nullptr, &s) !=
        NROS_RMW_RET_OK) {
        return 2;
    }

    rmw_node_t node{};
    node.name = s.node_name;
    node.namespace_ = s.namespace_;
    node.session = &s;

    rmw_qos_profile_t qos = NROS_RMW_QOS_PROFILE_DEFAULT;

    rmw_subscription_t sub{};
    sub.topic_name = "rt/take_seq_pending";
    sub.type_name = "nros_test::msg::TestString";
    sub.qos = qos;
    const rmw_message_type_support_t ts_1{sub.type_name, ""};
    if (g_vt->create_subscription(&node, &ts_1, sub.topic_name, 99, &qos, nullptr, &sub) !=
        NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_subscription failed\n");
        return 3;
    }

    rmw_publisher_t pub{};
    pub.topic_name = "rt/take_seq_pending";
    pub.type_name = "nros_test::msg::TestString";
    pub.qos = qos;
    const rmw_message_type_support_t ts_2{pub.type_name, ""};
    if (g_vt->create_publisher(&node, &ts_2, pub.topic_name, 99, &qos, nullptr, &pub) !=
        NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_publisher failed\n");
        return 4;
    }

    std::this_thread::sleep_for(std::chrono::milliseconds(200));

    uint8_t small_cdr[64];
    uint8_t big_cdr[512];
    const size_t small_len = build_cdr(small_cdr, sizeof(small_cdr), "hi");
    const size_t big_len = build_cdr(
        big_cdr, sizeof(big_cdr),
        "this string is comfortably longer than the slot the drain will be given");
    if (small_len == 0 || big_len == 0 || big_len <= small_len) {
        std::fprintf(stderr, "test payloads are wrong: small=%zu big=%zu\n", small_len, big_len);
        return 5;
    }

    if (g_vt->publish(&pub, rmw_byte_span_t{small_cdr, small_len}) != NROS_RMW_RET_OK ||
        g_vt->publish(&pub, rmw_byte_span_t{big_cdr, big_len}) != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "publish failed\n");
        return 6;
    }

    std::this_thread::sleep_for(std::chrono::milliseconds(300));

    // Fits the short message, and cannot fit the long one.
    const size_t per_msg_cap = small_len + 4;
    constexpr size_t kMaxMsgs = 4;
    uint8_t buf[kMaxMsgs * 128];
    size_t out_lens[kMaxMsgs];

    size_t delivered = 0;
    bool saw_too_small = false;
    for (int attempt = 0; attempt < 40 && !saw_too_small; ++attempt) {
        size_t taken = 0;
        rmw_ret_t rc =
            g_vt->take_sequence(&sub, buf, per_msg_cap, kMaxMsgs, out_lens, &taken);
        if (rc == NROS_RMW_RET_BUFFER_TOO_SMALL) {
            saw_too_small = true;
            // Contract: the status call reports nothing else. `taken` is only
            // meaningful on OK, so what is checked is that no NEW message was
            // counted as delivered — that is covered by `delivered` below.
            break;
        }
        if (rc != NROS_RMW_RET_OK) {
            std::fprintf(stderr, "take_sequence returned %d\n", static_cast<int>(rc));
            return 7;
        }
        for (size_t i = 0; i < taken; ++i) {
            if (out_lens[i] > per_msg_cap) {
                std::fprintf(stderr, "delivered %zu bytes into a %zu-byte slot\n", out_lens[i],
                             per_msg_cap);
                return 8;
            }
            if (out_lens[i] == big_len) {
                std::fprintf(stderr, "the oversized message was delivered\n");
                return 9;
            }
            delivered++;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(25));
    }

    if (!saw_too_small) {
        std::fprintf(stderr,
                     "drain never reported why it stopped: %zu message(s) delivered, no "
                     "BUFFER_TOO_SMALL in 40 attempts\n",
                     delivered);
        return 10;
    }
    if (delivered != 1) {
        std::fprintf(stderr, "expected exactly the short message, got %zu\n", delivered);
        return 11;
    }

    // The status is consumed by the call that reported it, so the next call is
    // a plain empty drain rather than the same error forever.
    size_t taken_after = 0;
    rmw_ret_t rc_after =
        g_vt->take_sequence(&sub, buf, per_msg_cap, kMaxMsgs, out_lens, &taken_after);
    if (rc_after != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "status was not cleared: next take_sequence returned %d\n",
                     static_cast<int>(rc_after));
        return 12;
    }
    if (taken_after != 0) {
        std::fprintf(stderr, "expected an empty reader after the drain, got %zu\n", taken_after);
        return 13;
    }

    g_vt->destroy_publisher(&pub);
    g_vt->destroy_subscription(&sub);
    (void)g_vt->destroy_session(&s);
    std::printf("OK take_sequence_pending_status — 1 delivered, BUFFER_TOO_SMALL reported once, "
                "then cleared\n");
    return 0;
}
