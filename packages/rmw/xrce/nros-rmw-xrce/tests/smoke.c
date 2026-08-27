/* Phase 115.K.2 smoke test.
 *
 * Confirms:
 *   1. The static library compiles + links.
 *   2. `nros_rmw_xrce_register()` reaches its `nros_rmw_cffi_register`
 *      hand-off and propagates the return code unchanged.
 *
 * The real `nros_rmw_cffi_register` symbol lives in the
 * `nros-rmw-cffi` Rust crate; this test stubs it with a local
 * implementation that records the vtable pointer it received and
 * returns OK. Validating wire-up at the link layer + sanity-checking
 * that the vtable is non-NULL on the way through.
 */

#include "nros/rmw_ret.h"
#include "nros/rmw_vtable.h"
#include "nros_rmw_xrce.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const nros_rmw_vtable_t *g_received_vtable = NULL;

rmw_ret_t nros_rmw_cffi_register(const nros_rmw_vtable_t *vtable) {
    g_received_vtable = vtable;
    return NROS_RMW_RET_OK;
}

/* Issue 0787 — the NAMED registry entry, which is what `vtable.c` calls
 * (phase 104.B.2). Only the legacy single-argument form was stubbed, so this
 * test had not LINKED since the named registry landed. */
rmw_ret_t nros_rmw_cffi_register_named(const char *name, const nros_rmw_vtable_t *vtable) {
    (void) name;
    g_received_vtable = vtable;
    return NROS_RMW_RET_OK;
}

/* The platform clock + sleep this backend's session drive loop calls. Same
 * reason as the UDP stubs below. */
uint64_t nros_platform_clock_ns(void) { return 0; }
void nros_platform_sleep_ms(size_t ms) { (void) ms; }
void nros_platform_udp_set_recv_timeout(const void *sock, uint32_t timeout_ms) {
    (void) sock;
    (void) timeout_ms;
}

/* Issue 0787 — the platform UDP primitives this backend's `nros_udp`
 * transport calls. They live in the Rust platform layer, which a standalone C
 * build of this backend does not link, so the smoke test stubs them exactly as
 * it already stubs the registry entry point. Every one FAILS: the test never
 * opens a transport, and a stub that pretended to succeed would make the test
 * assert against a socket that does not exist.
 *
 * Without these the test did not LINK, which is why nothing built this backend
 * on a host and why five phase-376 signature changes crossed it unchecked. */
int8_t nros_platform_udp_create_endpoint(void *ep, const uint8_t *address,
                                         const uint8_t *port) {
    (void) ep;
    (void) address;
    (void) port;
    return -1;
}
void nros_platform_udp_free_endpoint(void *ep) { (void) ep; }
int8_t nros_platform_udp_open(void *sock, const void *endpoint, uint32_t timeout_ms) {
    (void) sock;
    (void) endpoint;
    (void) timeout_ms;
    return -1;
}
void nros_platform_udp_close(void *sock) { (void) sock; }
size_t nros_platform_udp_read(const void *sock, uint8_t *buf, size_t len) {
    (void) sock;
    (void) buf;
    (void) len;
    return 0;
}
size_t nros_platform_udp_send(const void *sock, const uint8_t *buf, size_t len,
                              const void *endpoint) {
    (void) sock;
    (void) buf;
    (void) len;
    (void) endpoint;
    return 0;
}

/* Issue 0782 — the streamed-publish chunk loop.
 *
 * `xrce_publisher_publish_streamed` used to `malloc(total)` and memcpy the
 * header-stripped body into the reserved slot; it now writes straight into the
 * stream. The index arithmetic that replaced the memcpy spans two destinations
 * — a 4-byte encapsulation-header scratch, then the caller's slot — and is the
 * only part of that path a host with no XRCE agent can exercise. Left untested
 * it would have been verified by reading, which is how issue 0787's four
 * defects reached main.
 */
/* Issue 0782 — declared locally rather than by including `internal.h`, which
 * pulls in the whole micro-XRCE SDK include path this test deliberately does
 * not carry. A signature that drifts from the definition is a link error, not
 * silent breakage. */
size_t xrce_drive_streamed_body(uint8_t *body, size_t body_len, size_t total,
                                void (*chunk_cb)(uint8_t *out_buf, size_t cap,
                                                 size_t *out_written, void *user_ctx),
                                void *user_ctx);

static const uint8_t *g_src;
static size_t g_src_len;
static size_t g_src_pos;
static size_t g_max_chunk;

static void feed_chunk(uint8_t *out, size_t cap, size_t *written, void *ctx) {
    (void) ctx;
    size_t n = g_src_len - g_src_pos;
    if (n > cap) n = cap;
    if (n > g_max_chunk) n = g_max_chunk;
    memcpy(out, g_src + g_src_pos, n);
    g_src_pos += n;
    *written = n;
}

static int check_stream_loop(void) {
    /* 4-byte encap header + 10 body bytes. */
    static const uint8_t payload[14] = {0xAA, 0xBB, 0xCC, 0xDD, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9};
    /* Drive it at several chunk sizes: 1 forces the header to be absorbed over
     * four separate callbacks, 3 straddles the header/body boundary, 64 takes
     * everything the cap allows. */
    const size_t sizes[] = {1, 2, 3, 4, 5, 64};
    for (size_t i = 0; i < sizeof(sizes) / sizeof(sizes[0]); ++i) {
        uint8_t body[10];
        memset(body, 0xEE, sizeof(body));
        g_src = payload;
        g_src_len = sizeof(payload);
        g_src_pos = 0;
        g_max_chunk = sizes[i];

        size_t consumed =
            xrce_drive_streamed_body(body, sizeof(body), sizeof(payload), feed_chunk, NULL);
        if (consumed != sizeof(payload)) {
            fprintf(stderr, "FAIL: chunk=%zu consumed %zu, want %zu\n", sizes[i], consumed,
                    sizeof(payload));
            return 1;
        }
        if (memcmp(body, payload + 4, sizeof(body)) != 0) {
            fprintf(stderr, "FAIL: chunk=%zu body mismatch — the header was not stripped\n",
                    sizes[i]);
            return 1;
        }
    }

    /* EOF before `total`: report what was consumed so the caller can zero the
     * rest and error, rather than publishing uninitialised stream memory. */
    uint8_t body[10];
    memset(body, 0xEE, sizeof(body));
    g_src = payload;
    g_src_len = 9; /* header + 5 body bytes, then EOF */
    g_src_pos = 0;
    g_max_chunk = 64;
    size_t consumed =
        xrce_drive_streamed_body(body, sizeof(body), sizeof(payload), feed_chunk, NULL);
    if (consumed != 9) {
        fprintf(stderr, "FAIL: short delivery consumed %zu, want 9\n", consumed);
        return 1;
    }
    if (memcmp(body, payload + 4, 5) != 0) {
        fprintf(stderr, "FAIL: short delivery lost the bytes it DID receive\n");
        return 1;
    }
    return 0;
}

int main(void) {
    g_received_vtable = NULL;

    rmw_ret_t r = nros_rmw_xrce_register();
    if (r != NROS_RMW_RET_OK) {
        fprintf(stderr, "FAIL: nros_rmw_xrce_register returned %d, expected NROS_RMW_RET_OK\n",
                (int)r);
        return EXIT_FAILURE;
    }
    if (g_received_vtable == NULL) {
        fprintf(stderr, "FAIL: nros_rmw_cffi_register received NULL vtable\n");
        return EXIT_FAILURE;
    }
    if (g_received_vtable->create_session == NULL) {
        fprintf(stderr, "FAIL: vtable->create_session is NULL\n");
        return EXIT_FAILURE;
    }
    if (g_received_vtable->create_publisher == NULL) {
        fprintf(stderr, "FAIL: vtable->create_publisher is NULL\n");
        return EXIT_FAILURE;
    }
    if (g_received_vtable->create_subscription == NULL) {
        fprintf(stderr, "FAIL: vtable->create_subscription is NULL\n");
        return EXIT_FAILURE;
    }

    /* Phase 115.K.2.1 — open() now actually attempts UDP transport
     * + uxr_create_session against the configured agent. Without an
     * agent listening, the call fails with NROS_RMW_RET_ERROR; with
     * an agent it returns OK. Either is fine for this smoke; what
     * we care about is that the call REACHES the backend instead
     * of hitting the K.2.0 UNSUPPORTED stub.
     *
     * Use port 1 to make the "no agent" case deterministic — it's
     * reserved + nothing is listening. */
    rmw_session_t session = {0};
    r = g_received_vtable->create_session("127.0.0.1:1", 0, 0, "smoke", NULL, &session);
    if (r == NROS_RMW_RET_UNSUPPORTED) {
        fprintf(stderr,
                "FAIL: open returned UNSUPPORTED — K.2.1 should have replaced the stub\n");
        return EXIT_FAILURE;
    }
    if (r == NROS_RMW_RET_OK) {
        /* Surprise — agent on port 1. Close cleanly. */
        g_received_vtable->destroy_session(&session);
    }

    /* Phase 115.K.2.2 — publish_raw on a NULL backend_data publisher
     * must reach the backend (no longer the K.2.0 UNSUPPORTED stub)
     * and return INVALID_ARGUMENT. */
    rmw_publisher_t pub = {0};
    r = g_received_vtable->publish(&pub, NULL, 0);
    if (r != NROS_RMW_RET_INVALID_ARGUMENT) {
        fprintf(stderr,
                "FAIL: publish_raw on NULL backend_data returned %d, expected INVALID_ARGUMENT\n",
                (int)r);
        return EXIT_FAILURE;
    }

    /* Phase 115.K.2.2 — take / has_data on a fresh subscriber
     * shell with NULL backend_data must reach the backend. */
    rmw_subscription_t sub = {0};
    size_t rr_len = 0;
    bool rr_took = false;
    rmw_ret_t rr = g_received_vtable->take(&sub, NULL, 0, &rr_len, &rr_took);
    if (rr != NROS_RMW_RET_INVALID_ARGUMENT) {
        fprintf(stderr, "FAIL: take on NULL backend_data returned %d, expected INVALID_ARGUMENT\n",
                (int)rr);
        return EXIT_FAILURE;
    }
    /* Phase 376 W3.d step A — a NULL `backend_data` is now INVALID_ARGUMENT
       rather than a silent "no data". The two were indistinguishable before:
       an unbound subscription and an empty one gave the same answer, so a
       caller could poll a broken handle forever and read it as quiet. The flag
       must also be left untouched on that path. */
    bool hd = true;
    rmw_ret_t hd_rc = g_received_vtable->has_data(&sub, &hd);
    if (hd_rc != NROS_RMW_RET_INVALID_ARGUMENT) {
        fprintf(stderr, "FAIL: has_data on NULL backend_data returned %d, expected INVALID_ARGUMENT\n",
                (int)hd_rc);
        return EXIT_FAILURE;
    }
    if (!hd) {
        fprintf(stderr, "FAIL: has_data wrote the out-parameter on an error path\n");
        return EXIT_FAILURE;
    }

    /* Phase 115.K.2.3 — service paths must reach the backend. With a
     * NULL session, create_service returns INVALID_ARGUMENT
     * (no longer UNSUPPORTED stub). */
    rmw_service_t srv = {0};
    rmw_ret_t srv_r = g_received_vtable->create_service(
        NULL, "/foo", "Foo_", NULL, 0, NULL, &srv);
    if (srv_r != NROS_RMW_RET_INVALID_ARGUMENT) {
        fprintf(stderr,
                "FAIL: create_service with NULL session returned %d, expected INVALID_ARGUMENT\n",
                (int)srv_r);
        return EXIT_FAILURE;
    }

    /* take_request / has_request / send_response / send_request_raw
     * on NULL backend_data also reach the backend. */
    int64_t seq = 0;
    size_t tr_len = 0;
    bool tr_took = false;
    rmw_ret_t tr =
        g_received_vtable->take_request(&srv, NULL, 0, &seq, &tr_len, &tr_took);
    if (tr != NROS_RMW_RET_INVALID_ARGUMENT) {
        fprintf(stderr,
                "FAIL: take_request on NULL backend_data returned %d, expected INVALID_ARGUMENT\n",
                (int)tr);
        return EXIT_FAILURE;
    }

    if (check_stream_loop() != 0) {
        return EXIT_FAILURE;
    }

    rmw_client_t cli = {0};
    /* Issue 0778 — `send_request` reports the id it assigned. NULL is accepted
     * for a caller that does not want it; this one is checking the reject
     * path, so it passes NULL. */
    int32_t cr = g_received_vtable->send_request(&cli, NULL, 0, NULL);
    if (cr != NROS_RMW_RET_INVALID_ARGUMENT) {
        fprintf(stderr,
                "FAIL: send_request_raw on NULL backend_data returned %d, expected INVALID_ARGUMENT\n",
                (int)cr);
        return EXIT_FAILURE;
    }

    /* Phase 115.K.2.4 — custom-transport bridge:
     *  (a) `nros_rmw_xrce_init_custom_transport` returns UNSUPPORTED
     *      until the runtime drain symbol lands.
     *  (b) `nros_rmw_xrce_set_custom_transport_ops` rejects NULL.
     *  (c) opening a `custom://` session without first arming the
     *      bridge returns INVALID_ARGUMENT (no UNSUPPORTED stub).
     *  (d) After arming the bridge with a NULL-call vtable, the
     *      open path tries to use it and fails at the agent level
     *      (write returns 0 because read returns 0 — OK / -1, anything
     *      non-OK is acceptable, just not UNSUPPORTED). */
    rmw_ret_t r4 = nros_rmw_xrce_init_custom_transport(0);
    if (r4 != NROS_RMW_RET_UNSUPPORTED) {
        fprintf(stderr,
                "FAIL: nros_rmw_xrce_init_custom_transport returned %d, "
                "expected UNSUPPORTED (K.2.4 drain-from-runtime gap)\n",
                (int)r4);
        return EXIT_FAILURE;
    }

    rmw_ret_t r4_null = nros_rmw_xrce_set_custom_transport_ops(NULL, 0);
    if (r4_null != NROS_RMW_RET_INVALID_ARGUMENT) {
        fprintf(stderr,
                "FAIL: set_custom_transport_ops(NULL) returned %d, expected INVALID_ARGUMENT\n",
                (int)r4_null);
        return EXIT_FAILURE;
    }

    rmw_session_t cust_session = {0};
    rmw_ret_t cret = g_received_vtable->create_session(
        "custom://noop", 0, 0, "smoke-custom", NULL, &cust_session);
    if (cret != NROS_RMW_RET_INVALID_ARGUMENT) {
        fprintf(stderr,
                "FAIL: custom:// open without armed bridge returned %d, "
                "expected INVALID_ARGUMENT\n",
                (int)cret);
        return EXIT_FAILURE;
    }

    printf("ok: pub/sub + services + custom-transport bridge wired (K.2.2/2.3/2.4)\n");
    return EXIT_SUCCESS;
}
