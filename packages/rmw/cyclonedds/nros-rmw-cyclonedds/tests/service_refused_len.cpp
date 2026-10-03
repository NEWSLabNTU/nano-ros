// Issue 1632 — a too-small take_request / take_response names the size the
// request or reply needed.
//
// `take_sequence_pending_status.cpp` proves the subscription half (issue
// 1612). The rule is the same on the two service slots: on
// `NROS_RMW_RET_BUFFER_TOO_SMALL` the span's `len` is the size the CALLER's
// buffer needed — the user payload, encapsulation included, NOT the wire
// sample, which also carries the 16-byte request header the caller never
// sees.
//
// One server and one client on one participant, AddTwoInts:
//   1. request (24 user bytes) into a 10-byte buffer -> BUFFER_TOO_SMALL, len 24;
//   2. a second request taken whole, answered with a 12-byte reply;
//   3. that reply into a 4-byte buffer -> BUFFER_TOO_SMALL, len 12.

#include <chrono>
#include <cstdio>
#include <cstring>
#include <thread>

#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"
#include "nros_test_domain.h"

namespace {
const nros_rmw_vtable_t* g_vt = nullptr;
int g_failures = 0;

void check(bool ok, const char* what) {
    std::printf("  %s %s\n", ok ? "ok   " : "FAIL:", what);
    if (!ok) ++g_failures;
}

void put_le64(uint8_t* out, int64_t v) {
    for (int i = 0; i < 8; ++i)
        out[i] = static_cast<uint8_t>((v >> (i * 8)) & 0xff);
}

void sleep_ms(int ms) {
    std::this_thread::sleep_for(std::chrono::milliseconds(ms));
}

// Poll `take_request` until something other than "nothing pending" comes back.
rmw_ret_t take_request_until(rmw_service_t* srv, rmw_mut_byte_span_t* span, int64_t* seq,
                             bool* taken) {
    for (int i = 0; i < 300; ++i) {
        span->len = NROS_RMW_TAKE_LEN_UNKNOWN;
        *taken = false;
        rmw_ret_t rc = g_vt->take_request(srv, span, seq, taken);
        if (rc != NROS_RMW_RET_OK || *taken) return rc;
        sleep_ms(10);
    }
    return NROS_RMW_RET_TIMEOUT;
}

rmw_ret_t take_response_until(rmw_client_t* cli, rmw_mut_byte_span_t* span, int64_t* seq,
                              bool* taken) {
    for (int i = 0; i < 300; ++i) {
        span->len = NROS_RMW_TAKE_LEN_UNKNOWN;
        *taken = false;
        rmw_ret_t rc = g_vt->take_response(cli, span, seq, taken);
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
    std::printf("issue 1632: a too-small Cyclone service take reports the payload's size\n");
    if (nros_rmw_cyclonedds_register() != NROS_RMW_RET_OK || g_vt == nullptr) return 1;

    rmw_session_t s{};
    s.node_name = "service_refused_len";
    s.namespace_ = "/";
    if (g_vt->create_session(nullptr, 0, nros_test_domain(98), s.node_name, nullptr, &s) !=
        NROS_RMW_RET_OK) {
        return 2;
    }
    rmw_node_t node{};
    node.name = s.node_name;
    node.namespace_ = s.namespace_;
    node.session = &s;

    rmw_service_t srv{};
    srv.service_name = "svc_refused_len";
    srv.type_name = "nros_test::srv::dds_::AddTwoInts";
    const rmw_service_type_support_t ts_srv{srv.type_name, ""};
    if (g_vt->create_service(&node, &ts_srv, srv.service_name, 98, nullptr, &srv) !=
        NROS_RMW_RET_OK) {
        return 3;
    }
    rmw_client_t cli{};
    cli.service_name = "svc_refused_len";
    cli.type_name = "nros_test::srv::dds_::AddTwoInts";
    const rmw_service_type_support_t ts_cli{cli.type_name, ""};
    if (g_vt->create_client(&node, &ts_cli, cli.service_name, 98, nullptr, &cli) !=
        NROS_RMW_RET_OK) {
        return 4;
    }
    sleep_ms(300); // discovery

    uint8_t req[24] = {0x00, 0x01, 0x00, 0x00};
    put_le64(req + 4, 7);
    put_le64(req + 12, 11);

    // 1 — the request refused, with its size.
    if (g_vt->send_request(&cli, rmw_byte_span_t{req, sizeof(req)}, nullptr) != NROS_RMW_RET_OK)
        return 5;
    uint8_t small[10] = {};
    rmw_mut_byte_span_t span{small, sizeof(small), NROS_RMW_TAKE_LEN_UNKNOWN};
    int64_t seq = -1;
    bool taken = false;
    rmw_ret_t rc = take_request_until(&srv, &span, &seq, &taken);
    check(rc == NROS_RMW_RET_BUFFER_TOO_SMALL, "a request over the buffer is BUFFER_TOO_SMALL");
    check(span.len == sizeof(req), "and `len` names the size the request needed");
    std::printf("        request %zu bytes, buffer %zu, reported len %zu\n", sizeof(req),
                sizeof(small), span.len);

    // 2 — a second request, taken whole and answered.
    if (g_vt->send_request(&cli, rmw_byte_span_t{req, sizeof(req)}, nullptr) != NROS_RMW_RET_OK)
        return 6;
    uint8_t big[64] = {};
    rmw_mut_byte_span_t whole{big, sizeof(big), NROS_RMW_TAKE_LEN_UNKNOWN};
    rc = take_request_until(&srv, &whole, &seq, &taken);
    check(rc == NROS_RMW_RET_OK && taken && whole.len == sizeof(req),
          "a buffer that fits takes the request, same length");
    uint8_t reply[12] = {0x00, 0x01, 0x00, 0x00};
    put_le64(reply + 4, 18);
    check(g_vt->send_response(&srv, seq, rmw_byte_span_t{reply, sizeof(reply)}) == NROS_RMW_RET_OK,
          "the reply is sent");

    // 3 — the reply refused, with its size.
    uint8_t tiny[4] = {};
    rmw_mut_byte_span_t rspan{tiny, sizeof(tiny), NROS_RMW_TAKE_LEN_UNKNOWN};
    int64_t rseq = -1;
    rc = take_response_until(&cli, &rspan, &rseq, &taken);
    check(rc == NROS_RMW_RET_BUFFER_TOO_SMALL, "a reply over the buffer is BUFFER_TOO_SMALL");
    check(rspan.len == sizeof(reply), "and `len` names the size the reply needed");
    std::printf("        reply %zu bytes, buffer %zu, reported len %zu\n", sizeof(reply),
                sizeof(tiny), rspan.len);

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
