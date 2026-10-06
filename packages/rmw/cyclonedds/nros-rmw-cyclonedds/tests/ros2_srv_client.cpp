// Phase 117.12.B — nano-ros service client binary for stock-RMW
// interop E2E. Calls `/add_two_ints` (example_interfaces/srv/
// AddTwoInts) once with `(a=11, b=31)`, prints `SUM=<value>` on
// stdout, then exits. Companion `ros2_srv_e2e.sh` runs
// `ros2 run demo_nodes_cpp add_two_ints_server` on the same domain
// and asserts the printed value is `42`.
//
// With `NROS_SRV_CLIENT_PAIR=1` it instead runs TWO clients of the same
// service in this one process against that stock server (issue 1291), and
// prints `PAIR_OK` only if each took exactly its own replies.

#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <thread>

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"
#include "nros_test_domain.h"

namespace {
const nros_rmw_vtable_t* g_vt = nullptr;

void put_le64(uint8_t* out, int64_t v) {
    for (int i = 0; i < 8; ++i) {
        out[i] = static_cast<uint8_t>((v >> (i * 8)) & 0xff);
    }
}
int64_t get_le64(const uint8_t* in) {
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
rmw_ret_t call_blocking(rmw_client_t* cli, const uint8_t* req, size_t req_len, uint8_t* rep,
                        size_t rep_cap, size_t* out_len) {
    if (out_len == nullptr) return NROS_RMW_RET_INVALID_ARGUMENT;
    *out_len = 0;
    rmw_ret_t sr = g_vt->send_request(cli, rmw_byte_span_t{req, req_len}, nullptr);
    if (sr != NROS_RMW_RET_OK) {
        return sr;
    }
    const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(5);
    while (std::chrono::steady_clock::now() < deadline) {
        /* Phase 376 W3.b/W3.d step A — a poll that takes nothing is OK with
         * `taken = false`; anything else (a real reply, or an error) ends the
         * loop, which is what `!= NO_DATA` used to mean. */
        size_t n = 0;
        bool took = false;
        rmw_ret_t rc = nros_test_take_response(g_vt, cli, rep, rep_cap, nullptr, &n, &took);
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

namespace {
void encode_req(uint8_t (&req)[20], int64_t a, int64_t b) {
    req[0] = 0x00;
    req[1] = 0x01;
    req[2] = 0x00;
    req[3] = 0x00;
    put_le64(req + 4, a);
    put_le64(req + 12, b);
}

// Issue 1291 — TWO clients of one service in ONE process, against a STOCK
// server. Both send a request every round before either takes, so they are
// outstanding at once and, numbering from the same start, carry the SAME
// sequence number. The stock server answers both on the one reply topic, and
// each client's reader sees both replies. Only the per-client request id the
// server echoes keeps them apart: with the old GUID-prefix id (identical for
// every writer in a process) client 0 accepts client 1's reply as its own.
//
// The payloads differ per client (b = 100 vs 1000), so a crossed reply is a
// WRONG SUM, not a lucky match. Prints `PAIR_OK rounds=<n>` on success and
// `PAIR_MISMATCH ...` naming the crossed reply otherwise.
int run_pair(rmw_node_t* node, uint32_t domain) {
    constexpr int kClients = 2;
    constexpr int64_t kOffset[kClients] = {100, 1000};
    rmw_client_t cli[kClients] = {};
    for (int c = 0; c < kClients; ++c) {
        cli[c].service_name = "add_two_ints";
        cli[c].type_name = "example_interfaces::srv::dds_::AddTwoInts";
        const rmw_service_type_support_t ts{cli[c].type_name, ""};
        if (g_vt->create_client(node, &ts, cli[c].service_name, domain, nullptr, &cli[c]) !=
            NROS_RMW_RET_OK) {
            std::fprintf(stderr, "create_client %d failed\n", c);
            return 3;
        }
    }
    std::this_thread::sleep_for(std::chrono::seconds(2));

    int rounds = 5;
    if (const char* e = std::getenv("NROS_SRV_PAIR_ROUNDS")) {
        const long parsed = std::strtol(e, nullptr, 10);
        if (parsed > 0) rounds = static_cast<int>(parsed);
    }
    int rc = 0;
    for (int r = 0; r < rounds && rc == 0; ++r) {
        for (int c = 0; c < kClients; ++c) {
            uint8_t req[20];
            encode_req(req, r, kOffset[c]);
            if (g_vt->send_request(&cli[c], rmw_byte_span_t{req, sizeof(req)}, nullptr) !=
                NROS_RMW_RET_OK) {
                std::fprintf(stderr, "send_request client=%d round=%d failed\n", c, r);
                rc = 4;
                break;
            }
        }
        bool done[kClients] = {false, false};
        const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(5);
        while (rc == 0 && !(done[0] && done[1]) && std::chrono::steady_clock::now() < deadline) {
            for (int c = 0; c < kClients; ++c) {
                if (done[c]) continue;
                uint8_t rep[64] = {};
                size_t n = 0;
                bool took = false;
                if (nros_test_take_response(g_vt, &cli[c], rep, sizeof(rep), nullptr, &n, &took) !=
                    NROS_RMW_RET_OK) {
                    rc = 5;
                    break;
                }
                if (!took) continue;
                const int64_t sum = get_le64(rep + 4);
                const int64_t want = r + kOffset[c];
                if (sum != want) {
                    std::printf("PAIR_MISMATCH client=%d round=%d sum=%lld want=%lld\n", c, r,
                                static_cast<long long>(sum), static_cast<long long>(want));
                    rc = 6;
                    break;
                }
                done[c] = true;
            }
            std::this_thread::sleep_for(std::chrono::milliseconds(5));
        }
        if (rc == 0 && !(done[0] && done[1])) {
            std::printf("PAIR_TIMEOUT round=%d client0=%d client1=%d\n", r, done[0], done[1]);
            rc = 7;
        }
    }
    if (rc == 0) std::printf("PAIR_OK rounds=%d\n", rounds);
    std::fflush(stdout);
    for (int c = 0; c < kClients; ++c)
        g_vt->destroy_client(&cli[c]);
    return rc;
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

    rmw_session_t s{};
    s.node_name = "ros2_srv_client";
    s.namespace_ = "/";
    uint32_t domain = 0;
    if (const char* e = std::getenv("ROS_DOMAIN_ID")) {
        domain = static_cast<uint32_t>(std::atoi(e));
    }
    if (g_vt->create_session(nullptr, 0, domain, s.node_name, nullptr, &s) != NROS_RMW_RET_OK) {
        return 2;
    }

    // Phase 376 W5/B1 — entities are created ON A NODE now. The node
    // carries its own identity plus the route to its session.
    rmw_node_t node{};
    node.name = s.node_name;
    node.namespace_ = s.namespace_;
    node.session = &s;

    if (const char* e = std::getenv("NROS_SRV_CLIENT_PAIR"); e != nullptr && e[0] == '1') {
        const int prc = run_pair(&node, domain);
        (void)g_vt->destroy_session(&s);
        return prc;
    }

    rmw_client_t cli{};
    cli.service_name = "add_two_ints";
    cli.type_name = "example_interfaces::srv::dds_::AddTwoInts";
    const rmw_service_type_support_t ts_1{cli.type_name, ""};
    if (g_vt->create_client(&node, &ts_1, cli.service_name, domain, nullptr, &cli) !=
        NROS_RMW_RET_OK) {
        std::fprintf(stderr, "create_client failed\n");
        return 3;
    }

    // Discovery delay so the stock server's reader matches our
    // request writer + response reader before the call fires.
    std::this_thread::sleep_for(std::chrono::seconds(2));

    uint8_t req[20] = {0x00, 0x01, 0x00, 0x00};
    put_le64(req + 4, 11);
    put_le64(req + 12, 31);
    uint8_t rep[64] = {};
    size_t n = 0;

    // Issue 0969 — `NROS_ROUNDTRIP_ITERS` repeats the CALL so the client half of
    // the service data path can be priced by slope, the same method and the same
    // variable name `data_roundtrip.cpp` uses.
    //
    // Why here and not only in `service_roundtrip.cpp`: that harness puts client
    // and server in ONE process, so Cyclone can hand the sample to the local
    // reader by reference and the write path may never serialise. A number
    // measured there can therefore under-report what a real deployment pays.
    // This binary talks to a SEPARATE server process, which is the case a
    // control loop actually runs.
    //
    // Default 1, so the interop test this binary exists for is unchanged.
    long iters = 1;
    if (const char* e = std::getenv("NROS_ROUNDTRIP_ITERS")) {
        const long parsed = std::strtol(e, nullptr, 10);
        if (parsed > 0) iters = parsed;
    }
    rmw_ret_t call_rc = NROS_RMW_RET_OK;
    for (long it = 0; it < iters; ++it) {
        n = 0;
        call_rc = call_blocking(&cli, req, sizeof(req), rep, sizeof(rep), &n);
        if (call_rc != NROS_RMW_RET_OK || n == 0) break;
    }
    if (call_rc != NROS_RMW_RET_OK || n == 0) {
        std::fprintf(stderr, "call_blocking failed rc=%d len=%zu\n", (int)call_rc, n);
        g_vt->destroy_client(&cli);
        (void)g_vt->destroy_session(&s);
        return 4;
    }
    int64_t sum = get_le64(rep + 4);
    std::printf("SUM=%lld\n", static_cast<long long>(sum));
    std::fflush(stdout);

    g_vt->destroy_client(&cli);
    (void)g_vt->destroy_session(&s);
    return 0;
}
