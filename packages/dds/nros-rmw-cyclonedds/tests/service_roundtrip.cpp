// Phase 117.7 service request/reply data-plane round-trip.
//
// Drives a full call_raw → server.try_recv_request → server.send_reply
// → client receives reply chain on the AddTwoInts test type.
//
// Wire format (CDR-LE, XCDR1):
//   Request:  int64 a, int64 b
//   Response: int64 sum
//
// Tests on a single thread by polling try_recv_request between
// call_raw's internal poll loop. Cyclone services discovery happens
// in its background thread so the writer/reader pair will rendezvous
// after a short delay.

#include <chrono>
#include <cstdio>
#include <cstring>
#include <thread>

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"

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

extern "C" nros_rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vt) {
    g_vt = vt;
    return NROS_RMW_RET_OK;
}

int main() {
    if (nros_rmw_cyclonedds_register() != NROS_RMW_RET_OK || g_vt == nullptr) {
        return 1;
    }

    nros_rmw_session_t s{};
    s.node_name  = "service_roundtrip";
    s.namespace_ = "/";
    if (g_vt->open(nullptr, 0, 99, s.node_name, &s) != NROS_RMW_RET_OK) {
        return 2;
    }

    nros_rmw_service_server_t srv{};
    srv.service_name = "rq/svc_roundtrip";
    srv.type_name    = "nros_test::srv::AddTwoInts";
    if (g_vt->create_service_server(&s, srv.service_name, srv.type_name, "",
                                    99, &srv) != NROS_RMW_RET_OK) {
        return 3;
    }

    nros_rmw_service_client_t cli{};
    cli.service_name = "rq/svc_roundtrip";
    cli.type_name    = "nros_test::srv::AddTwoInts";
    if (g_vt->create_service_client(&s, cli.service_name, cli.type_name, "",
                                    99, &cli) != NROS_RMW_RET_OK) {
        g_vt->destroy_service_server(&srv);
        (void) g_vt->close(&s);
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

    // The client's call_raw blocks until reply arrives; service the
    // request from a worker thread.
    std::thread server([&]() {
        for (int i = 0; i < 200; ++i) {
            if (g_vt->has_request(&srv)) {
                uint8_t rbuf[64] = {};
                int64_t seq = -1;
                int32_t r = g_vt->try_recv_request(&srv, rbuf, sizeof(rbuf), &seq);
                if (r > 0) {
                    int64_t a = get_le64(rbuf + 4);
                    int64_t b = get_le64(rbuf + 12);
                    uint8_t reply[12] = {0x00, 0x01, 0x00, 0x00};
                    put_le64(reply + 4, a + b);
                    (void) g_vt->send_reply(&srv, seq, reply, sizeof(reply));
                    return;
                }
            }
            std::this_thread::sleep_for(std::chrono::milliseconds(10));
        }
    });

    uint8_t rep[64] = {};
    int32_t n = g_vt->call_raw(&cli, req, sizeof(req), rep, sizeof(rep));
    server.join();

    if (n <= 0) {
        std::fprintf(stderr, "call_raw returned %d\n", n);
        g_vt->destroy_service_client(&cli);
        g_vt->destroy_service_server(&srv);
        (void) g_vt->close(&s);
        return 5;
    }

    int64_t sum = get_le64(rep + 4);
    if (sum != 18) {
        std::fprintf(stderr, "expected sum=18, got %lld\n", static_cast<long long>(sum));
        return 6;
    }

    g_vt->destroy_service_client(&cli);
    g_vt->destroy_service_server(&srv);
    (void) g_vt->close(&s);
    std::printf("OK 7+11=18\n");
    return 0;
}
