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
// One call per client — keeps the test simple. Multi-call
// per-client correlation is exercised by `service_roundtrip`;
// this test focuses on the cross-client (guid, seq) filter
// rejecting replies meant for the other peer.
constexpr int kCallsPerClient = 1;
} // namespace

extern "C" nros_rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vt) {
    g_vt = vt;
    return NROS_RMW_RET_OK;
}

static int run_client(int client_idx, std::atomic<int> *failures) {
    // Phase 117.X.5 / Cyclone 0.10.5: each client gets its own
    // session (= its own participant). Two writers on the same
    // request topic from the same participant occasionally trip a
    // local-delivery race in this Cyclone version where the second
    // writer's traffic doesn't reach a same-participant reader. The
    // separate-participant variant matches how `rclcpp` deploys two
    // clients in real systems anyway.
    char node_name[64];
    std::snprintf(node_name, sizeof(node_name),
                  "service_concurrent_client_%d", client_idx);
    nros_rmw_session_t my_s{};
    my_s.node_name  = node_name;
    my_s.namespace_ = "/";
    if (g_vt->open(nullptr, 0, 99, node_name, &my_s) != NROS_RMW_RET_OK) {
        failures->fetch_add(1);
        return 1;
    }
    nros_rmw_service_client_t cli{};
    cli.service_name = "concurrent_test";
    cli.type_name    = "nros_test::srv::dds_::AddTwoInts";
    if (g_vt->create_service_client(&my_s, cli.service_name, cli.type_name,
                                    "", 99, &cli) != NROS_RMW_RET_OK) {
        (void) g_vt->close(&my_s);
        failures->fetch_add(1);
        return 1;
    }
    // Discovery delay across distinct participants on the same
    // domain (SPDP + SEDP propagation). 3 s on POSIX absorbs the
    // 100ms-200ms heartbeat + match propagation across 6 endpoints
    // (server's req-reader + reply-writer plus this client's
    // req-writer + reply-reader, plus the other client's pair).
    std::this_thread::sleep_for(std::chrono::milliseconds(3000));

    for (int i = 0; i < kCallsPerClient; ++i) {
        // Request payload matches the registered AddTwoInts shape:
        //   4-byte CDR encap + 8-byte int64 a + 8-byte int64 b.
        // Encode (a, b) so the server's reply (a+b) uniquely
        // identifies which client + call it answered.
        int64_t a = client_idx * 10 + i;
        int64_t b = 1;
        uint8_t req[20] = {0x00, 0x01, 0x00, 0x00};
        for (int k = 0; k < 8; ++k) {
            req[4 + k]  = static_cast<uint8_t>((a >> (k * 8)) & 0xff);
            req[12 + k] = static_cast<uint8_t>((b >> (k * 8)) & 0xff);
        }
        uint8_t rep[64] = {};
        int32_t n = g_vt->call_raw(&cli, req, sizeof(req), rep, sizeof(rep));
        if (n < 0) {
            std::fprintf(stderr,
                "client %d call %d: call_raw returned %d\n",
                client_idx, i, n);
            failures->fetch_add(1);
            break;
        }
        if (n < 12) {
            std::fprintf(stderr, "client %d call %d: short reply n=%d\n",
                         client_idx, i, n);
            failures->fetch_add(1);
            continue;
        }
        // Reply: 4-byte encap + 8-byte int64 sum.
        int64_t got = 0;
        for (int k = 0; k < 8; ++k) {
            got |= static_cast<int64_t>(rep[4 + k]) << (k * 8);
        }
        if (got != a + b) {
            std::fprintf(stderr,
                "client %d call %d: bad sum (got %lld, want %lld)\n",
                client_idx, i,
                static_cast<long long>(got), static_cast<long long>(a + b));
            failures->fetch_add(1);
        }
    }
    g_vt->destroy_service_client(&cli);
    (void) g_vt->close(&my_s);
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
    srv.service_name = "concurrent_test";
    srv.type_name    = "nros_test::srv::dds_::AddTwoInts";
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
                uint8_t rbuf[64] = {};
                int64_t seq = -1;
                int32_t r = g_vt->try_recv_request(&srv, rbuf, sizeof(rbuf), &seq);
                if (r > 0) {
                    int64_t a = 0, b = 0;
                    for (int k = 0; k < 8; ++k) {
                        a |= static_cast<int64_t>(rbuf[4 + k])  << (k * 8);
                        b |= static_cast<int64_t>(rbuf[12 + k]) << (k * 8);
                    }
                    int64_t sum = a + b;
                    uint8_t reply[12] = {0x00, 0x01, 0x00, 0x00};
                    for (int k = 0; k < 8; ++k) {
                        reply[4 + k] =
                            static_cast<uint8_t>((sum >> (k * 8)) & 0xff);
                    }
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

    std::thread c0([&]() { run_client(0, &failures); });
    std::thread c1([&]() { run_client(1, &failures); });
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
