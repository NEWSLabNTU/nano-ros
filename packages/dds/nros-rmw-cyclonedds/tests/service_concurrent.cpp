// Phase 117.7.B concurrent-call correlation test.
//
// Two clients fire 5 calls each at the same service in tight
// succession. Server replies with `request_payload[0] + 100`
// (so each reply is uniquely tied to its request payload). Each
// client must receive only its own replies, in order — without
// the (client_id, seq) envelope correlation, replies would
// interleave and the per-client expected sums break.

#include <atomic>
#include <chrono>
#include <cstdio>
#include <cstring>
#include <thread>
#include <vector>

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"

namespace {
const nros_rmw_vtable_t *g_vt = nullptr;
constexpr int kCallsPerClient = 5;
} // namespace

extern "C" nros_rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vt) {
    g_vt = vt;
    return NROS_RMW_RET_OK;
}

static int run_client(nros_rmw_session_t *s, int client_idx,
                      std::atomic<int> *failures) {
    nros_rmw_service_client_t cli{};
    cli.service_name = "rq/concurrent_test";
    cli.type_name    = "anything";
    if (g_vt->create_service_client(s, cli.service_name, cli.type_name, "",
                                    99, &cli) != NROS_RMW_RET_OK) {
        failures->fetch_add(1);
        return 1;
    }
    // Discovery delay: Cyclone needs to sync the reader/writer
    // matching across 4 endpoints on one participant.
    std::this_thread::sleep_for(std::chrono::milliseconds(500));

    for (int i = 0; i < kCallsPerClient; ++i) {
        // Request payload: 4-byte CDR encap + 1 marker byte
        // identifying (client_idx, call_idx).
        uint8_t req[5] = {
            0x00, 0x01, 0x00, 0x00,
            static_cast<uint8_t>(client_idx * 10 + i),
        };
        uint8_t rep[16] = {};
        int32_t n = g_vt->call_raw(&cli, req, sizeof(req), rep, sizeof(rep));
        if (n < 0) {
            std::fprintf(stderr,
                "client %d call %d: call_raw returned %d\n",
                client_idx, i, n);
            failures->fetch_add(1);
            break;
        }
        if (n < 5 || rep[4] != static_cast<uint8_t>(req[4] + 100)) {
            std::fprintf(stderr,
                "client %d call %d: bad reply marker (got %u, want %u)\n",
                client_idx, i,
                n >= 5 ? rep[4] : 0,
                static_cast<unsigned>(req[4] + 100));
            failures->fetch_add(1);
        }
    }
    g_vt->destroy_service_client(&cli);
    return 0;
}

int main() {
    if (nros_rmw_cyclonedds_register() != NROS_RMW_RET_OK || g_vt == nullptr) {
        return 1;
    }

    nros_rmw_session_t s{};
    s.node_name  = "service_concurrent";
    s.namespace_ = "/";
    if (g_vt->open(nullptr, 0, 99, s.node_name, &s) != NROS_RMW_RET_OK) {
        return 2;
    }

    nros_rmw_service_server_t srv{};
    srv.service_name = "rq/concurrent_test";
    srv.type_name    = "anything";
    if (g_vt->create_service_server(&s, srv.service_name, srv.type_name, "",
                                    99, &srv) != NROS_RMW_RET_OK) {
        return 3;
    }

    std::atomic<bool> stop{false};
    std::atomic<int> failures{0};

    // Server task — drains requests until both clients finish.
    std::thread server([&]() {
        int handled = 0;
        const int total = 2 * kCallsPerClient;
        const auto deadline = std::chrono::steady_clock::now() +
                              std::chrono::seconds(10);
        while (handled < total &&
               std::chrono::steady_clock::now() < deadline) {
            if (g_vt->has_request(&srv)) {
                uint8_t rbuf[16] = {};
                int64_t seq = -1;
                int32_t r = g_vt->try_recv_request(&srv, rbuf, sizeof(rbuf), &seq);
                if (r > 0) {
                    uint8_t reply[5] = {0x00, 0x01, 0x00, 0x00,
                                        static_cast<uint8_t>(rbuf[4] + 100)};
                    (void) g_vt->send_reply(&srv, seq, reply, sizeof(reply));
                    ++handled;
                }
            } else {
                std::this_thread::sleep_for(std::chrono::milliseconds(5));
            }
        }
        if (handled < total) {
            std::fprintf(stderr, "server timed out: %d/%d\n", handled, total);
            failures.fetch_add(1);
        }
        stop.store(true);
    });

    std::thread c0([&]() { run_client(&s, 0, &failures); });
    // Stagger second client's startup so the writer-creation sequence
    // doesn't race with c0's first dds_write. Cyclone 0.10.5's local-
    // delivery fast path occasionally misses the second writer when
    // two writers on the same topic + same participant are created
    // back-to-back. 100 ms is enough to let c0's writer reach
    // matched-state before c1's appears.
    std::this_thread::sleep_for(std::chrono::milliseconds(100));
    std::thread c1([&]() { run_client(&s, 1, &failures); });
    c0.join();
    c1.join();
    server.join();

    g_vt->destroy_service_server(&srv);
    (void) g_vt->close(&s);

    int f = failures.load();
    if (f != 0) {
        std::fprintf(stderr, "%d failure(s)\n", f);
        return 4;
    }
    std::printf("OK %d concurrent calls correlated\n",
                2 * kCallsPerClient);
    return 0;
}
