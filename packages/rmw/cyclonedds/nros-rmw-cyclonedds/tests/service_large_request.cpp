// Issue 2819 — a client's request is not bounded by a fixed staging buffer.
//
// A Cyclone service client staged its request in a 64 KiB member, so every
// client paid that much whatever its type, and no client could send a request
// larger than it: `send_request` returned BUFFER_TOO_SMALL. The staging slot is
// now allocated for the request it holds, and only before the request writer
// has matched a server reader; after the match a request goes straight out.
//
// One server and one client on one participant. Each request is 70,000 user
// bytes, over the old 65,536-byte bound:
//   1. sent at once, before discovery, so it is STAGED; the client's poll
//      flushes it once the writer matches, and the server takes it whole;
//   2. sent after the match, so it is written directly; taken whole too.
// Both payloads are compared byte for byte, so a truncated or reordered copy
// fails as surely as a refusal.

#include <chrono>
#include <cstdio>
#include <cstring>
#include <thread>
#include <vector>

#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"
#include "nros_test_domain.h"

namespace {
const nros_rmw_vtable_t* g_vt = nullptr;
int g_failures = 0;

constexpr std::size_t kRequestBytes = 70000;

void check(bool ok, const char* what) {
    std::printf("  %s %s\n", ok ? "ok   " : "FAIL:", what);
    if (!ok) ++g_failures;
}

void sleep_ms(int ms) {
    std::this_thread::sleep_for(std::chrono::milliseconds(ms));
}

std::vector<uint8_t> make_request(uint8_t salt) {
    std::vector<uint8_t> req(kRequestBytes);
    req[0] = 0x00;
    req[1] = 0x01;
    req[2] = 0x00;
    req[3] = 0x00;
    for (std::size_t i = 4; i < req.size(); ++i) {
        req[i] = static_cast<uint8_t>((i * 31u + salt) & 0xff);
    }
    return req;
}

// Poll the server until a request arrives. Each round also polls the client,
// which is what flushes a request staged before the match.
rmw_ret_t take_request_until(rmw_client_t* cli, rmw_service_t* srv, rmw_mut_byte_span_t* span,
                             int64_t* seq, bool* taken) {
    uint8_t reply[64] = {};
    for (int i = 0; i < 500; ++i) {
        rmw_mut_byte_span_t rspan{reply, sizeof(reply), NROS_RMW_TAKE_LEN_UNKNOWN};
        int64_t rseq = -1;
        bool rtaken = false;
        (void)g_vt->take_response(cli, &rspan, &rseq, &rtaken);
        span->len = NROS_RMW_TAKE_LEN_UNKNOWN;
        *taken = false;
        rmw_ret_t rc = g_vt->take_request(srv, span, seq, taken);
        if (rc != NROS_RMW_RET_OK || *taken) return rc;
        sleep_ms(10);
    }
    return NROS_RMW_RET_TIMEOUT;
}
} // namespace

extern "C" rmw_ret_t nros_rmw_cffi_register_named(const char* /*name*/,
                                                  const nros_rmw_vtable_t* vt) {
    g_vt = vt;
    return NROS_RMW_RET_OK;
}

int main() {
    std::printf("issue 2819: a Cyclone client sends a request larger than 64 KiB\n");
    if (nros_rmw_cyclonedds_register() != NROS_RMW_RET_OK || g_vt == nullptr) return 1;

    rmw_session_t s{};
    s.node_name = "service_large_request";
    s.namespace_ = "/";
    if (g_vt->create_session(nullptr, 0, nros_test_domain(97), s.node_name, nullptr, &s) !=
        NROS_RMW_RET_OK) {
        return 2;
    }
    rmw_node_t node{};
    node.name = s.node_name;
    node.namespace_ = s.namespace_;
    node.session = &s;

    rmw_service_t srv{};
    srv.service_name = "svc_large_request";
    srv.type_name = "nros_test::srv::dds_::AddTwoInts";
    const rmw_service_type_support_t ts_srv{srv.type_name, ""};
    if (g_vt->create_service(&node, &ts_srv, srv.service_name, 97, nullptr, &srv) !=
        NROS_RMW_RET_OK) {
        return 3;
    }
    rmw_client_t cli{};
    cli.service_name = "svc_large_request";
    cli.type_name = "nros_test::srv::dds_::AddTwoInts";
    const rmw_service_type_support_t ts_cli{cli.type_name, ""};
    if (g_vt->create_client(&node, &ts_cli, cli.service_name, 97, nullptr, &cli) !=
        NROS_RMW_RET_OK) {
        return 4;
    }

    std::vector<uint8_t> got(kRequestBytes + 64);
    int64_t seq = -1;
    bool taken = false;

    // 1 — before discovery: the request is staged, then flushed.
    const std::vector<uint8_t> staged = make_request(1);
    rmw_ret_t rc = g_vt->send_request(&cli, rmw_byte_span_t{staged.data(), staged.size()}, nullptr);
    check(rc == NROS_RMW_RET_OK, "a 70,000-byte request sent before the match is accepted");
    std::printf("        send_request returned %d\n", static_cast<int>(rc));
    if (rc == NROS_RMW_RET_OK) {
        rmw_mut_byte_span_t span{got.data(), got.size(), NROS_RMW_TAKE_LEN_UNKNOWN};
        rc = take_request_until(&cli, &srv, &span, &seq, &taken);
        check(rc == NROS_RMW_RET_OK && taken && span.len == staged.size() &&
                  std::memcmp(got.data(), staged.data(), staged.size()) == 0,
              "the server takes the staged request whole and unchanged");
        std::printf("        take_request returned %d, taken %d, len %zu\n", static_cast<int>(rc),
                    taken ? 1 : 0, span.len);
        uint8_t reply[12] = {0x00, 0x01, 0x00, 0x00};
        (void)g_vt->send_response(&srv, seq, rmw_byte_span_t{reply, sizeof(reply)});
    }

    // 2 — after the match: written directly.
    sleep_ms(300);
    const std::vector<uint8_t> direct = make_request(2);
    rc = g_vt->send_request(&cli, rmw_byte_span_t{direct.data(), direct.size()}, nullptr);
    check(rc == NROS_RMW_RET_OK, "a 70,000-byte request sent after the match is accepted");
    if (rc == NROS_RMW_RET_OK) {
        rmw_mut_byte_span_t span{got.data(), got.size(), NROS_RMW_TAKE_LEN_UNKNOWN};
        rc = take_request_until(&cli, &srv, &span, &seq, &taken);
        check(rc == NROS_RMW_RET_OK && taken && span.len == direct.size() &&
                  std::memcmp(got.data(), direct.data(), direct.size()) == 0,
              "the server takes the direct request whole and unchanged");
    }

    g_vt->destroy_client(&cli);
    g_vt->destroy_service(&srv);
    (void)g_vt->destroy_session(&s);
    if (g_failures != 0) {
        std::printf("%d check(s) failed\n", g_failures);
        return 1;
    }
    std::printf("all checks passed\n");
    return 0;
}
