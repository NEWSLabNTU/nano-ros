// Issue 1088 — more requests outstanding than the server has correlation slots.
//
// `kRequestSlots` (service.cpp) bounds how many requests one server may hold
// between `take_request` and `send_response`. Two things used to go wrong past
// that bound, and this test pins both:
//
//   1. LOSS. `take_typed_wire` is a destructive `dds_takecdr`; it ran BEFORE the
//      slot search, so the 33rd request was consumed, found no slot, and was
//      destroyed. Fixed in bd9948e23d (slot reserved before the take) with no
//      regression test — this is it: every request must be answered.
//   2. OBSERVABILITY. The adapter then folded the resulting WOULD_BLOCK into
//      `taken = false` + OK, which upstream defines as "nothing was pending".
//      A saturated server read as an idle one. `take_request` must now say
//      WOULD_BLOCK while a request IS pending and no slot is free.
//
// Shape: one server, several clients on one participant (a client may hold only
// `kMaxOutstandingRequests` = 8 calls, so one client cannot saturate 32 slots).
// Requests are sent ONE AT A TIME and the server takes each before the next is
// sent, so the DDS reader never holds more than one unread sample — the request
// reader is KEEP_LAST(10), and a burst of 35 would test DDS history depth, not
// this backend. Replies are likewise released one at a time and collected
// before the next, for the same reason on the reply readers.
//
// Every request carries a distinct (a, b), so each reply is checked by VALUE as
// well as by sequence id: a test whose replies were interchangeable would pass
// with a request answered twice and another dropped.

#include <chrono>
#include <cstdio>
#include <cstring>
#include <thread>

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_cyclonedds.h"
#include "nros_test_domain.h"

namespace {
const nros_rmw_vtable_t* g_vt = nullptr;

// Mirrors `kRequestSlots` in src/service.cpp. If that constant moves, this test
// fails at step 2 with a message naming both numbers — change them together.
constexpr int kBackendRequestSlots = 32;
constexpr int kClients = 5;
constexpr int kPerClient = 7;                 // <= kMaxOutstandingRequests (8)
constexpr int kTotal = kClients * kPerClient; // 35 > 32

void put_le64(uint8_t* out, int64_t v) {
    for (int i = 0; i < 8; ++i)
        out[i] = static_cast<uint8_t>((v >> (i * 8)) & 0xff);
}
int64_t get_le64(const uint8_t* in) {
    int64_t v = 0;
    for (int i = 0; i < 8; ++i)
        v |= static_cast<int64_t>(in[i]) << (i * 8);
    return v;
}

int64_t arg_a(int i) {
    return 1000 + i;
}
int64_t arg_b(int i) {
    return 7 * i;
}
int64_t want_sum(int i) {
    return arg_a(i) + arg_b(i);
}

void sleep_ms(int ms) {
    std::this_thread::sleep_for(std::chrono::milliseconds(ms));
}

struct Held {
    int64_t seq{-1};
    int64_t a{0};
    int64_t b{0};
};
} // namespace

extern "C" rmw_ret_t nros_rmw_cffi_register_named(const char* /*name*/,
                                                  const nros_rmw_vtable_t* vt) {
    g_vt = vt;
    return NROS_RMW_RET_OK;
}

int main() {
    if (nros_rmw_cyclonedds_register() != NROS_RMW_RET_OK || g_vt == nullptr) return 1;

    rmw_session_t s{};
    s.node_name = "slots_exhausted";
    s.namespace_ = "/";
    if (g_vt->create_session(nullptr, 0, nros_test_domain(99), s.node_name, nullptr, &s) !=
        NROS_RMW_RET_OK) {
        return 2;
    }
    rmw_node_t node{};
    node.name = s.node_name;
    node.namespace_ = s.namespace_;
    node.session = &s;

    const char* kType = "nros_test::srv::dds_::AddTwoInts";
    const char* kName = "svc_slots_exhausted";

    rmw_service_t srv{};
    srv.service_name = kName;
    srv.type_name = kType;
    const rmw_service_type_support_t srv_ts{kType, ""};
    if (g_vt->create_service(&node, &srv_ts, kName, 99, nullptr, &srv) != NROS_RMW_RET_OK) {
        (void)g_vt->destroy_session(&s);
        return 3;
    }

    rmw_client_t cli[kClients]{};
    rmw_service_type_support_t cli_ts[kClients];
    for (int c = 0; c < kClients; ++c) {
        cli[c].service_name = kName;
        cli[c].type_name = kType;
        cli_ts[c] = rmw_service_type_support_t{kType, ""};
        if (g_vt->create_client(&node, &cli_ts[c], kName, 99, nullptr, &cli[c]) !=
            NROS_RMW_RET_OK) {
            std::fprintf(stderr, "create_client %d failed\n", c);
            return 4;
        }
    }
    sleep_ms(300);

    int rc = 0;
    int64_t sent_seq[kTotal];
    Held held[kTotal];
    int n_held = 0;
    bool saw_exhausted = false;

    // ---- 1. Send every request; take each while a slot is free. ----------
    for (int i = 0; i < kTotal && rc == 0; ++i) {
        const int c = i / kPerClient;
        uint8_t req[24] = {0x00, 0x01, 0x00, 0x00};
        put_le64(req + 4, arg_a(i));
        put_le64(req + 12, arg_b(i));
        rmw_ret_t sr = NROS_RMW_RET_WOULD_BLOCK;
        for (int k = 0; k < 400 && sr == NROS_RMW_RET_WOULD_BLOCK; ++k) {
            sr = g_vt->send_request(&cli[c], rmw_byte_span_t{req, sizeof(req)}, &sent_seq[i]);
            if (sr == NROS_RMW_RET_WOULD_BLOCK) sleep_ms(5);
        }
        if (sr != NROS_RMW_RET_OK) {
            std::fprintf(stderr, "send_request %d (client %d) rc=%d\n", i, c, (int)sr);
            rc = 5;
            break;
        }

        // Wait until the request is visible on the server's reader.
        bool pending = false;
        for (int k = 0; k < 1000 && !pending; ++k) {
            if (g_vt->has_request(&srv, &pending) != NROS_RMW_RET_OK) {
                rc = 6;
                break;
            }
            if (!pending) sleep_ms(2);
        }
        if (rc != 0) break;
        if (!pending) {
            std::fprintf(stderr, "request %d never became visible to the server\n", i);
            rc = 7;
            break;
        }

        uint8_t buf[64] = {};
        int64_t seq = -1;
        size_t len = 0;
        bool taken = false;
        const rmw_ret_t tr =
            nros_test_take_request(g_vt, &srv, buf, sizeof(buf), &seq, &len, &taken);

        if (n_held < kBackendRequestSlots) {
            if (tr != NROS_RMW_RET_OK || !taken) {
                std::fprintf(stderr, "take %d with %d of %d slots held: rc=%d taken=%d\n", i,
                             n_held, kBackendRequestSlots, (int)tr, (int)taken);
                rc = 8;
                break;
            }
            held[n_held++] = Held{seq, get_le64(buf + 4), get_le64(buf + 12)};
            continue;
        }

        // ---- 2. Every slot held and a request pending: must be OBSERVABLE.
        if (tr == NROS_RMW_RET_OK && !taken) {
            // The pre-fix adapter lands here: "nothing pending", which is false —
            // `has_request` said true a moment ago and the sample is still there.
            std::fprintf(stderr,
                         "EXHAUSTION NOT OBSERVABLE: %d slots held, has_request=true, and "
                         "take_request reported an EMPTY queue (taken=false, OK) — a saturated "
                         "server reads as an idle one (issue 1088)\n",
                         n_held);
            rc = 9;
            break;
        }
        if (tr == NROS_RMW_RET_OK && taken) {
            std::fprintf(stderr,
                         "took request %d with %d slots already held: the backend has more than "
                         "kBackendRequestSlots=%d — update this test with service.cpp\n",
                         i, n_held, kBackendRequestSlots);
            rc = 10;
            break;
        }
        if (tr != NROS_RMW_RET_WOULD_BLOCK) {
            std::fprintf(stderr, "take with every slot held: rc=%d, want WOULD_BLOCK (%d)\n",
                         (int)tr, (int)NROS_RMW_RET_WOULD_BLOCK);
            rc = 11;
            break;
        }
        saw_exhausted = true;
        // Nothing consumed: the request must still be pending.
        bool still = false;
        if (g_vt->has_request(&srv, &still) != NROS_RMW_RET_OK || !still) {
            std::fprintf(stderr, "WOULD_BLOCK consumed the request (has_request=false after)\n");
            rc = 12;
            break;
        }
    }
    if (rc == 0 && !saw_exhausted) {
        std::fprintf(stderr, "never reached exhaustion (%d held)\n", n_held);
        rc = 13;
    }

    // ---- 3. Answer one, take one: every request is eventually answered. -----
    int answered = 0;
    bool got[kTotal] = {};
    auto answer_and_collect = [&](const Held& h) -> int {
        uint8_t reply[12] = {0x00, 0x01, 0x00, 0x00};
        put_le64(reply + 4, h.a + h.b);
        const rmw_ret_t sr =
            g_vt->send_response(&srv, h.seq, rmw_byte_span_t{reply, sizeof(reply)});
        if (sr != NROS_RMW_RET_OK) {
            std::fprintf(stderr, "send_response seq=%lld rc=%d\n", (long long)h.seq, (int)sr);
            return 14;
        }
        // The request index is recoverable from its (distinct) a-argument.
        const int idx = static_cast<int>(h.a - 1000);
        if (idx < 0 || idx >= kTotal) {
            std::fprintf(stderr, "held request carries a=%lld, not one we sent\n", (long long)h.a);
            return 15;
        }
        const int c = idx / kPerClient;
        for (int k = 0; k < 1000; ++k) {
            // Drain every client: a reply reader sees the others' replies too and
            // discards them; only the addressed client must report it.
            for (int d = 0; d < kClients; ++d) {
                uint8_t rep[64] = {};
                int64_t seq = -1;
                size_t n = 0;
                bool took = false;
                const rmw_ret_t tr =
                    nros_test_take_response(g_vt, &cli[d], rep, sizeof(rep), &seq, &n, &took);
                if (tr != NROS_RMW_RET_OK) {
                    std::fprintf(stderr, "take_response client %d rc=%d\n", d, (int)tr);
                    return 16;
                }
                if (!took) continue;
                if (d != c || seq != sent_seq[idx]) {
                    std::fprintf(stderr, "client %d got seq %lld; expected client %d seq %lld\n", d,
                                 (long long)seq, c, (long long)sent_seq[idx]);
                    return 17;
                }
                const int64_t sum = get_le64(rep + 4);
                if (sum != want_sum(idx)) {
                    std::fprintf(stderr, "reply %d sum %lld, want %lld\n", idx, (long long)sum,
                                 (long long)want_sum(idx));
                    return 18;
                }
                if (got[idx]) {
                    std::fprintf(stderr, "request %d answered twice\n", idx);
                    return 19;
                }
                got[idx] = true;
                ++answered;
                return 0;
            }
            sleep_ms(2);
        }
        std::fprintf(stderr, "no reply for request %d (client %d)\n", idx, c);
        return 20;
    };

    int next_release = 0;
    while (rc == 0 && next_release < n_held) {
        rc = answer_and_collect(held[next_release++]);
        if (rc != 0) break;
        // A slot is free again: the request left pending must now be takeable.
        bool pending = false;
        if (g_vt->has_request(&srv, &pending) != NROS_RMW_RET_OK) {
            rc = 21;
            break;
        }
        while (pending && rc == 0) {
            uint8_t buf[64] = {};
            int64_t seq = -1;
            size_t len = 0;
            bool taken = false;
            const rmw_ret_t tr =
                nros_test_take_request(g_vt, &srv, buf, sizeof(buf), &seq, &len, &taken);
            if (tr == NROS_RMW_RET_WOULD_BLOCK) break; // still full; release another
            if (tr != NROS_RMW_RET_OK || !taken) {
                std::fprintf(stderr, "take after a release: rc=%d taken=%d\n", (int)tr, (int)taken);
                rc = 22;
                break;
            }
            if (n_held >= kTotal) {
                rc = 23;
                break;
            }
            held[n_held++] = Held{seq, get_le64(buf + 4), get_le64(buf + 12)};
            if (g_vt->has_request(&srv, &pending) != NROS_RMW_RET_OK) rc = 21;
        }
    }

    if (rc == 0 && answered != kTotal) {
        std::fprintf(stderr, "LOST REQUESTS: answered %d of %d\n", answered, kTotal);
        for (int i = 0; i < kTotal; ++i) {
            if (!got[i])
                std::fprintf(stderr, "  request %d (client %d) never answered\n", i,
                             i / kPerClient);
        }
        rc = 24;
    }

    for (int c = 0; c < kClients; ++c)
        g_vt->destroy_client(&cli[c]);
    g_vt->destroy_service(&srv);
    (void)g_vt->destroy_session(&s);
    if (rc == 0) {
        std::printf("SLOTS_EXHAUSTED_OK answered=%d exhausted_at=%d\n", answered,
                    kBackendRequestSlots);
    }
    return rc;
}
