/* Issue 2576 — an Agent that forgot the session is SAID so, and nothing else is.
 *
 * After an Agent restart the client kept spinning: `drive_io` answered OK,
 * publishes failed with MESSAGE_TOO_LARGE once the reliable window filled, and
 * `ping_session` answered TIMEOUT although the new Agent was replying to every
 * ping with "available, no session". This drives the backend against a fake
 * Agent that answers GET_INFO with a real Agent's INFO bytes (captured from
 * Agent v2.4.3, in and out of session), so NO AGENT is needed:
 *
 *   - ping in session is OK, ping out of session is SESSION_LOST, and the
 *     latch makes every later drive / ping / refusal say so;
 *   - a quiet link is probed once per XRCE_SESSION_PROBE_MS by `drive_io`
 *     alone, and an in-session answer never latches;
 *   - a busy link is not probed at all;
 *   - silence (an Agent that is down) is TIMEOUT, never a verdict;
 *   - a write refused for a full window is WOULD_BLOCK, not MESSAGE_TOO_LARGE.
 */

#include "internal.h"
#include "nros/rmw_entity.h"
#include "nros/rmw_ret.h"

#include <stdio.h>
#include <string.h>

static int failures = 0;

#define CHECK(cond, what)                                                                          \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            printf("  FAIL: %s\n", (what));                                                        \
            failures++;                                                                            \
        } else {                                                                                   \
            printf("  ok    %s\n", (what));                                                        \
        }                                                                                          \
    } while (0)

rmw_ret_t nros_rmw_cffi_register_named(const char* name, const nros_rmw_vtable_t* vtable) {
    (void)name;
    (void)vtable;
    return NROS_RMW_RET_OK;
}

/* Agent v2.4.3's INFO answer to the client's GET_INFO. The two differ in one
 * byte, the reply's implementation status: 1 holds the session, 0 does not. */
static const uint8_t INFO_IN_SESSION[] = {0x81, 0x00, 0x00, 0x00, 0x06, 0x01, 0x1c, 0x00, 0x00,
                                          0x0a, 0xff, 0xfd, 0x00, 0x01, 0x01, 0x0d, 0x58, 0x52,
                                          0x43, 0x45, 0x01, 0x00, 0x01, 0x0f, 0x00, 0x01, 0x0d,
                                          0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00};
static const uint8_t INFO_NO_SESSION[] = {0x81, 0x00, 0x00, 0x00, 0x06, 0x01, 0x1c, 0x00, 0x00,
                                          0x0a, 0xff, 0xfd, 0x00, 0x00, 0x01, 0x0d, 0x58, 0x52,
                                          0x43, 0x45, 0x01, 0x00, 0x01, 0x0f, 0x00, 0x01, 0x0d,
                                          0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00};

enum agent_mode { AGENT_DOWN, AGENT_IN_SESSION, AGENT_NO_SESSION };
static enum agent_mode g_mode = AGENT_DOWN;
static int g_get_infos = 0;
static const uint8_t* g_pending = NULL;
static size_t g_pending_len = 0;

static bool t_open(uxrCustomTransport* t) {
    (void)t;
    return true;
}
static bool t_close(uxrCustomTransport* t) {
    (void)t;
    return true;
}
/* Every write is taken; a GET_INFO (submessage id 2 after the 4-byte session
 * header) queues the mode's answer, and nothing is ever acknowledged. */
static size_t t_write(uxrCustomTransport* t, const uint8_t* buf, size_t len, uint8_t* err) {
    (void)t;
    (void)err;
    if (len > 4 && buf[4] == 0x02) {
        g_get_infos++;
        if (g_mode == AGENT_IN_SESSION) {
            g_pending = INFO_IN_SESSION;
            g_pending_len = sizeof(INFO_IN_SESSION);
        } else if (g_mode == AGENT_NO_SESSION) {
            g_pending = INFO_NO_SESSION;
            g_pending_len = sizeof(INFO_NO_SESSION);
        }
    }
    return len;
}
static size_t t_read(uxrCustomTransport* t, uint8_t* buf, size_t len, int timeout, uint8_t* err) {
    (void)t;
    (void)err;
    if (g_pending != NULL && len >= g_pending_len) {
        memcpy(buf, g_pending, g_pending_len);
        size_t n = g_pending_len;
        g_pending = NULL;
        return n;
    }
    nros_platform_sleep_ms((uint32_t)(timeout > 0 ? timeout : 0));
    return 0;
}

static xrce_session_state_t* open_fake(rmw_session_t* rs) {
    xrce_session_state_t* st =
        (xrce_session_state_t*)nros_xrce_calloc(1, sizeof(xrce_session_state_t));
    if (st == NULL) {
        return NULL;
    }
    uxr_set_custom_transport_callbacks(&st->custom, false, t_open, t_close, t_write, t_read);
    (void)uxr_init_custom_transport(&st->custom, NULL);
    uxr_init_session(&st->session, &st->custom.comm, 0x2576AAAAu);
    st->output_reliable =
        uxr_create_output_reliable_stream(&st->session, st->output_reliable_buf,
                                          sizeof(st->output_reliable_buf), XRCE_STREAM_HISTORY);
    st->input_reliable = uxr_create_input_reliable_stream(
        &st->session, st->input_reliable_buf, sizeof(st->input_reliable_buf), XRCE_STREAM_HISTORY);
    xrce_liveness_tap(st, xrce_now_ms());
    memset(rs, 0, sizeof(*rs));
    rs->backend_data = st;
    g_pending = NULL;
    g_get_infos = 0;
    return st;
}

/* Make the link look silent for longer than the probe interval. */
static void go_quiet(xrce_session_state_t* st) {
    uint64_t past = xrce_now_ms() - 2u * XRCE_SESSION_PROBE_MS;
    st->last_rx_ms = past;
    st->last_probe_ms = past;
}

int main(void) {
    printf("issue 2576: a lost session is reported, and only a lost one\n");
    rmw_session_t rs;

    /* 1. The explicit ping: OK, TIMEOUT and SESSION_LOST are three answers. */
    xrce_session_state_t* st = open_fake(&rs);
    if (st == NULL) {
        printf("  FAIL: could not allocate a session state\n");
        return 1;
    }
    g_mode = AGENT_IN_SESSION;
    CHECK(xrce_session_ping(&rs, 100) == NROS_RMW_RET_OK, "ping in session is OK");
    g_mode = AGENT_DOWN;
    CHECK(xrce_session_ping(&rs, 50) == NROS_RMW_RET_TIMEOUT,
          "ping with no Agent is TIMEOUT, not a verdict on the session");
    CHECK(!st->session_lost, "silence does not latch the session lost");
    g_mode = AGENT_NO_SESSION;
    CHECK(xrce_session_ping(&rs, 100) == NROS_RMW_RET_SESSION_LOST,
          "ping answered 'no session' is SESSION_LOST");
    g_mode = AGENT_IN_SESSION;
    CHECK(xrce_session_ping(&rs, 100) == NROS_RMW_RET_SESSION_LOST,
          "the latch holds: a later ping still says SESSION_LOST");
    CHECK(xrce_session_drive_io(&rs, 0) == NROS_RMW_RET_SESSION_LOST, "and so does every drive");
    CHECK(xrce_output_refusal(st) == NROS_RMW_RET_SESSION_LOST, "and every refused write");
    nros_xrce_free(st);

    /* 2. The passive probe: `drive_io` alone finds the loss on a quiet link. */
    st = open_fake(&rs);
    g_mode = AGENT_NO_SESSION;
    go_quiet(st);
    rmw_ret_t rc = xrce_session_drive_io(&rs, 50);
    CHECK(g_get_infos == 1, "a quiet link gets exactly one probe per drive window");
    CHECK(rc == NROS_RMW_RET_SESSION_LOST, "drive_io answers SESSION_LOST after a 'no session'");
    nros_xrce_free(st);

    /* 3. An Agent that still holds the session: probed, never latched. */
    st = open_fake(&rs);
    g_mode = AGENT_IN_SESSION;
    go_quiet(st);
    rc = xrce_session_drive_io(&rs, 50);
    CHECK(g_get_infos == 1 && rc == NROS_RMW_RET_OK && !st->session_lost,
          "an in-session answer to the probe leaves the session alone");
    nros_xrce_free(st);

    /* 4. A link that has just heard from the Agent is not probed. */
    st = open_fake(&rs);
    g_mode = AGENT_NO_SESSION;
    rc = xrce_session_drive_io(&rs, 50);
    CHECK(g_get_infos == 0 && rc == NROS_RMW_RET_OK, "a link that spoke recently is not probed");
    nros_xrce_free(st);

    /* 5. A full window is WOULD_BLOCK (the Agent stopped acknowledging), not
     * MESSAGE_TOO_LARGE. Nothing acknowledges here, so the window fills. */
    st = open_fake(&rs);
    g_mode = AGENT_DOWN;
    uint8_t sample[8] = {0};
    uxrObjectId oid = uxr_object_id(1, UXR_DATAWRITER_ID);
    int buffered = 0;
    while (buffered < 100000 && uxr_buffer_topic(&st->session, st->output_reliable, oid, sample,
                                                 sizeof(sample)) != UXR_INVALID_REQUEST_ID) {
        buffered++;
    }
    printf("        %d sample(s) buffered before the window filled\n", buffered);
    CHECK(buffered > 0 && buffered < 100000, "the unacknowledged window fills");
    CHECK(xrce_output_refusal(st) == NROS_RMW_RET_WOULD_BLOCK,
          "a write refused for a full window is WOULD_BLOCK");
    nros_xrce_free(st);

    /* 6. And a sample bigger than any slot is still MESSAGE_TOO_LARGE. */
    st = open_fake(&rs);
    static uint8_t huge[sizeof(((xrce_session_state_t*)0)->output_reliable_buf)];
    CHECK(uxr_buffer_topic(&st->session, st->output_reliable, oid, huge, sizeof(huge)) ==
              UXR_INVALID_REQUEST_ID,
          "a sample bigger than a slot is refused");
    CHECK(xrce_output_refusal(st) == NROS_RMW_RET_MESSAGE_TOO_LARGE,
          "on an empty window that refusal is MESSAGE_TOO_LARGE");
    nros_xrce_free(st);

    if (failures != 0) {
        printf("%d check(s) failed\n", failures);
        return 1;
    }
    printf("all checks passed\n");
    return 0;
}
