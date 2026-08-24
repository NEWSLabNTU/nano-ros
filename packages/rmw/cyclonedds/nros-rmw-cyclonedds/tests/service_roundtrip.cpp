// Phase 117.7 service request/reply data-plane round-trip.
//
// Drives a full send_request_raw → server.take_request →
// server.send_reply → client take_response chain on the
// AddTwoInts test type.
//
// Wire format (CDR-LE, XCDR1):
//   Request:  int64 a, int64 b
//   Response: int64 sum
//
// Tests on a single thread by polling take_request between
// the client's reply poll loop. Cyclone services discovery happens
// in its background thread so the writer/reader pair will rendezvous
// after a short delay.

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

// Phase-301: the blocking `call_raw` vtable slot was deleted; emulate
// the old blocking call with the non-blocking send + poll pair.
// Issue 0773 — status returned, length out. This returned either a byte count
// or an `NROS_RMW_RET_*` code through one `int32_t`, the same shape that turned
// `BUFFER_TOO_SMALL` into a slice bound in the backend once W3.d step B made
// the codes positive. A test helper carrying the bug it is meant to catch is
// worse than no helper.
rmw_ret_t call_blocking(rmw_client_t *cli, const uint8_t *req, size_t req_len, uint8_t *rep,
                        size_t rep_cap, size_t *out_len) {
    if (out_len == nullptr) return NROS_RMW_RET_INVALID_ARGUMENT;
    *out_len = 0;
    rmw_ret_t sr = g_vt->send_request(cli, req, req_len);
    if (sr != NROS_RMW_RET_OK) {
        return sr;
    }
    const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(5);
    while (std::chrono::steady_clock::now() < deadline) {
        size_t n = 0;
        bool took = false;
        rmw_ret_t rc = g_vt->take_response(cli, rep, rep_cap, &n, &took);
        if (rc != NROS_RMW_RET_OK) {
            return rc;
        }
        if (took) {
            *out_len = n;
            return NROS_RMW_RET_OK;
        }
        std::this_thread::sleep_for(std::chrono::milliseconds(5));
    }
    return NROS_RMW_RET_TIMEOUT;
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
    s.node_name  = "service_roundtrip";
    s.namespace_ = "/";
    if (g_vt->create_session(nullptr, 0, nros_test_domain(99), s.node_name, &s) != NROS_RMW_RET_OK) {
        return 2;
    }

    // Phase 376 W5/B1 — entities are created ON A NODE now. The node
    // carries its own identity plus the route to its session.
    rmw_node_t node{};
    node.name       = s.node_name;
    node.namespace_ = s.namespace_;
    node.session    = &s;

    // Phase 193.5 — exercise the non-default QoS path (not the `nullptr` ⇒
    // SERVICES_DEFAULT branch). A RELIABLE + VOLATILE + KEEP_LAST(5) profile is a
    // valid non-default for request/reply (RELIABLE is effectively required); the
    // same profile on both endpoints keeps the reader/writer matched. This drives
    // Cyclone's `qos != nullptr ? *qos : SERVICES_DEFAULT` branch end-to-end.
    rmw_qos_profile_t qos{};
    qos.reliability     = NROS_RMW_RELIABILITY_RELIABLE;
    qos.durability      = NROS_RMW_DURABILITY_VOLATILE;
    qos.history         = NROS_RMW_HISTORY_KEEP_LAST;
    qos.liveliness_kind = NROS_RMW_LIVELINESS_SYSTEM_DEFAULT;
    qos.depth           = 5;

    rmw_service_t srv{};
    srv.service_name = "svc_roundtrip";
    srv.type_name    = "nros_test::srv::dds_::AddTwoInts";
    if (g_vt->create_service(&node, srv.service_name, srv.type_name, "",
                                    99, &qos, &srv) != NROS_RMW_RET_OK) {
        return 3;
    }

    rmw_client_t cli{};
    cli.service_name = "svc_roundtrip";
    cli.type_name    = "nros_test::srv::dds_::AddTwoInts";
    if (g_vt->create_client(&node, cli.service_name, cli.type_name, "",
                                    99, &qos, &cli) != NROS_RMW_RET_OK) {
        g_vt->destroy_service(&srv);
        (void) g_vt->destroy_session(&s);
        return 4;
    }

    // Discovery delay.
    std::this_thread::sleep_for(std::chrono::milliseconds(300));

    // Build CDR-LE request: a=7, b=11.
    uint8_t req[24] = {
        0x00, 0x01, 0x00, 0x00,  // encap CDR_LE
    };
    put_le64(req + 4,  7);
    put_le64(req + 12, 11);

    // The client's call_blocking poll loop runs until the reply
    // arrives; service the request from a worker thread.
    std::thread server([&]() {
        for (int i = 0; i < 200; ++i) {
            bool has_r = false;
            /* Phase 376 W3.d step A — flag out, status returned. */
            if (g_vt->has_request(&srv, &has_r) == NROS_RMW_RET_OK && has_r) {
                uint8_t rbuf[64] = {};
                int64_t seq = -1;
                size_t r = 0;
                bool rtook = false;
                /* Phase 376 W3.b/W3.d step A. */
                if (g_vt->take_request(&srv, rbuf, sizeof(rbuf), &seq, &r, &rtook) ==
                        NROS_RMW_RET_OK &&
                    rtook && r > 0) {
                    int64_t a = get_le64(rbuf + 4);
                    int64_t b = get_le64(rbuf + 12);
                    uint8_t reply[12] = {0x00, 0x01, 0x00, 0x00};
                    put_le64(reply + 4, a + b);
                    (void) g_vt->send_response(&srv, seq, reply, sizeof(reply));
                    return;
                }
            }
            std::this_thread::sleep_for(std::chrono::milliseconds(10));
        }
    });

    uint8_t rep[64] = {};
    size_t n = 0;
    rmw_ret_t call_rc = call_blocking(&cli, req, sizeof(req), rep, sizeof(rep), &n);
    server.join();

    if (call_rc != NROS_RMW_RET_OK || n == 0) {
        std::fprintf(stderr, "call_blocking failed rc=%d len=%zu\n", (int) call_rc, n);
        g_vt->destroy_client(&cli);
        g_vt->destroy_service(&srv);
        (void) g_vt->destroy_session(&s);
        return 5;
    }

    int64_t sum = get_le64(rep + 4);
    if (sum != 18) {
        std::fprintf(stderr, "expected sum=18, got %lld\n", static_cast<long long>(sum));
        return 6;
    }

    g_vt->destroy_client(&cli);
    g_vt->destroy_service(&srv);
    (void) g_vt->destroy_session(&s);
    std::printf("OK 7+11=18\n");
    return 0;
}
