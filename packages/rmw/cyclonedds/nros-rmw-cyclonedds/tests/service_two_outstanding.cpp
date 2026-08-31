// Issue 0778 — TWO requests in flight at once, from one client.
//
// This is the case the backend could not express before 2026-08-25. A single
// `pending_seq` meant the second `send_request` overwrote the first, and
// `take_response` then dropped the older reply as "for a different in-flight
// call". Both halves of the abandon were silent: the caller saw one reply and
// had no way to know the other request had been discarded rather than lost in
// the network.
//
// The server here answers `a + b`, so the two replies are DISTINGUISHABLE by
// value as well as by sequence id. A test where both calls compute the same
// answer would pass under the old code too.

#include <chrono>
#include <cstdio>
#include <cstring>
#include <thread>

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"
#include "nros_test_domain.h"

namespace {
const nros_rmw_vtable_t *g_vt = nullptr;

void put_le64(uint8_t *out, int64_t v) {
    for (int i = 0; i < 8; ++i) {
        out[i] = static_cast<uint8_t>((v >> (i * 8)) & 0xff);
    }
}
int64_t get_le64(const uint8_t *in) {
    int64_t v = 0;
    for (int i = 0; i < 8; ++i) {
        v |= static_cast<int64_t>(in[i]) << (i * 8);
    }
    return v;
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
    s.node_name  = "two_outstanding";
    s.namespace_ = "/";
    if (g_vt->create_session(nullptr, 0, nros_test_domain(99), s.node_name, nullptr, &s) !=
        NROS_RMW_RET_OK) {
        return 2;
    }

    rmw_node_t node{};
    node.name       = s.node_name;
    node.namespace_ = s.namespace_;
    node.session    = &s;

    rmw_service_t srv{};
    srv.service_name = "svc_two_outstanding";
    srv.type_name    = "nros_test::srv::dds_::AddTwoInts";
    const rmw_service_type_support_t ts_1{srv.type_name, ""};
    if (g_vt->create_service(&node, &ts_1, srv.service_name, 99, nullptr, &srv) !=
        NROS_RMW_RET_OK) {
        (void) g_vt->destroy_session(&s);
        return 3;
    }

    rmw_client_t cli{};
    cli.service_name = "svc_two_outstanding";
    cli.type_name    = "nros_test::srv::dds_::AddTwoInts";
    const rmw_service_type_support_t ts_2{cli.type_name, ""};
    if (g_vt->create_client(&node, &ts_2, cli.service_name, 99, nullptr, &cli) !=
        NROS_RMW_RET_OK) {
        g_vt->destroy_service(&srv);
        (void) g_vt->destroy_session(&s);
        return 4;
    }

    std::this_thread::sleep_for(std::chrono::milliseconds(300));

    // Answer every request that arrives, for as long as the client is asking.
    std::thread server([&]() {
        int answered = 0;
        for (int i = 0; i < 400 && answered < 2; ++i) {
            bool has_r = false;
            if (g_vt->has_request(&srv, &has_r) == NROS_RMW_RET_OK && has_r) {
                uint8_t rbuf[64] = {};
                int64_t seq = -1;
                size_t r    = 0;
                bool rtook  = false;
                if (nros_test_take_request(g_vt, &srv, rbuf, sizeof(rbuf), &seq, &r, &rtook) ==
                        NROS_RMW_RET_OK &&
                    rtook && r > 0) {
                    int64_t a = get_le64(rbuf + 4);
                    int64_t b = get_le64(rbuf + 12);
                    uint8_t reply[12] = {0x00, 0x01, 0x00, 0x00};
                    put_le64(reply + 4, a + b);
                    (void) g_vt->send_response(&srv, seq, rmw_byte_span_t{reply, sizeof(reply)});
                    ++answered;
                    continue;
                }
            }
            std::this_thread::sleep_for(std::chrono::milliseconds(5));
        }
    });

    // Two sends BEFORE any take. Distinct sums: 3 and 700.
    uint8_t req_a[24] = {0x00, 0x01, 0x00, 0x00};
    put_le64(req_a + 4, 1);
    put_le64(req_a + 12, 2);
    uint8_t req_b[24] = {0x00, 0x01, 0x00, 0x00};
    put_le64(req_b + 4, 300);
    put_le64(req_b + 12, 400);

    int64_t seq_a = -1;
    int64_t seq_b = -1;
    rmw_ret_t sa  = g_vt->send_request(&cli, rmw_byte_span_t{req_a, sizeof(req_a)}, &seq_a);
    rmw_ret_t sb  = NROS_RMW_RET_OK;
    // The pre-match staging window can legitimately refuse the second send with
    // WOULD_BLOCK; retry briefly rather than treating that as a failure.
    for (int i = 0; i < 200; ++i) {
        sb = g_vt->send_request(&cli, rmw_byte_span_t{req_b, sizeof(req_b)}, &seq_b);
        if (sb != NROS_RMW_RET_WOULD_BLOCK) break;
        std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }

    int rc = 0;
    if (sa != NROS_RMW_RET_OK || sb != NROS_RMW_RET_OK) {
        std::fprintf(stderr, "send failed: a=%d b=%d\n", (int) sa, (int) sb);
        rc = 5;
    } else if (seq_a == seq_b) {
        std::fprintf(stderr, "two sends reported the same sequence id %lld\n",
                     static_cast<long long>(seq_a));
        rc = 6;
    }

    // Collect BOTH replies. Neither may be lost, and each must name its own
    // request.
    bool got_a = false;
    bool got_b = false;
    for (int i = 0; i < 400 && rc == 0 && !(got_a && got_b); ++i) {
        uint8_t rep[64] = {};
        int64_t seq     = -1;
        size_t n        = 0;
        bool took       = false;
        rmw_ret_t tr    = nros_test_take_response(g_vt, &cli, rep, sizeof(rep), &seq, &n, &took);
        if (tr != NROS_RMW_RET_OK) {
            std::fprintf(stderr, "take_response rc=%d\n", (int) tr);
            rc = 7;
            break;
        }
        if (took) {
            int64_t sum = get_le64(rep + 4);
            if (seq == seq_a) {
                if (sum != 3) {
                    std::fprintf(stderr, "reply for seq_a has sum %lld, want 3\n",
                                 static_cast<long long>(sum));
                    rc = 8;
                    break;
                }
                got_a = true;
            } else if (seq == seq_b) {
                if (sum != 700) {
                    std::fprintf(stderr, "reply for seq_b has sum %lld, want 700\n",
                                 static_cast<long long>(sum));
                    rc = 9;
                    break;
                }
                got_b = true;
            } else {
                std::fprintf(stderr, "reply names sequence %lld, neither %lld nor %lld\n",
                             static_cast<long long>(seq), static_cast<long long>(seq_a),
                             static_cast<long long>(seq_b));
                rc = 10;
                break;
            }
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }

    server.join();

    if (rc == 0 && !(got_a && got_b)) {
        // The old backend failed exactly here: one reply arrived, the other's
        // request had been abandoned by the second send.
        std::fprintf(stderr, "lost a reply: got_a=%d got_b=%d\n", (int) got_a, (int) got_b);
        rc = 11;
    }

    g_vt->destroy_client(&cli);
    g_vt->destroy_service(&srv);
    (void) g_vt->destroy_session(&s);
    if (rc == 0) {
        std::printf("TWO_OUTSTANDING_OK\n");
    }
    return rc;
}
