/* Issue 2641 — `rclc_trace_marker` hands an application event to the
 * installed trace sink, and refuses an id outside the application block.
 *
 * Compiled, LINKED against libnros_c.a and RUN by `just check c`, twice:
 *
 *   - against an archive built WITH `trace-callbacks`: the capturing sink
 *     receives exactly the in-range `(id, arg)` pairs, in order, and nothing
 *     for a refused id;
 *   - with `-DNROS_TEST_TRACE_OFF`, against the shipped archive (no
 *     `trace-callbacks`): the sink receives nothing at all, and the range
 *     check still refuses, so an image's ids are validated even when its
 *     tracing is compiled out.
 *
 * Every check prints and exits non-zero rather than `assert()`, so an NDEBUG
 * compile cannot turn this into a binary that passes by doing nothing.
 *
 * Copyright 2026 nros contributors
 * Licensed under Apache-2.0
 */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#include <rclc/rclc.h>

/* The registration hook every C image owes; this harness opens no session. */
void nros_app_register_backends(void);
void nros_app_register_backends(void) {}

static int failures = 0;

#define CHECK(cond)                                                                                \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            fprintf(stderr, "FAIL %s:%d: %s\n", __FILE__, __LINE__, #cond);                        \
            failures++;                                                                            \
        }                                                                                          \
    } while (0)

#define MAX_EVENTS 16
static uint32_t ev_id[MAX_EVENTS];
static uint32_t ev_arg[MAX_EVENTS];
static int ev_n = 0;

static void capture(uint32_t id, uint32_t arg) {
    if (ev_n < MAX_EVENTS) {
        ev_id[ev_n] = id;
        ev_arg[ev_n] = arg;
    }
    ev_n++;
}

int main(void) {
    CHECK(RCLC_TRACE_APP_MARKER_FIRST == 1);
    CHECK(RCLC_TRACE_APP_MARKER_LAST == 15);

    /* No sink: an in-range call is a quiet OK. */
    rclc_set_trace_sink(NULL);
    CHECK(rclc_trace_marker(3, 7) == RCL_RET_OK);

    rclc_set_trace_sink(capture);
    CHECK(rclc_trace_marker(RCLC_TRACE_APP_MARKER_FIRST, 0xdeadbeefu) == RCL_RET_OK);
    CHECK(rclc_trace_marker(RCLC_TRACE_APP_MARKER_LAST, 42u) == RCL_RET_OK);

    /* Every executor id is refused, loudly, and emits nothing. */
    for (uint32_t id = 16; id <= 27; id++) {
        rcl_reset_error();
        CHECK(rclc_trace_marker(id, 1u) == RCL_RET_INVALID_ARGUMENT);
        CHECK(rcl_error_is_set());
    }
    CHECK(rclc_trace_marker(0, 1u) == RCL_RET_INVALID_ARGUMENT);
    CHECK(rclc_trace_marker(UINT32_MAX, 1u) == RCL_RET_INVALID_ARGUMENT);
    rcl_reset_error();
    rclc_set_trace_sink(NULL);

#ifdef NROS_TEST_TRACE_OFF
    CHECK(ev_n == 0);
#else
    CHECK(ev_n == 2);
    CHECK(ev_id[0] == 1 && ev_arg[0] == 0xdeadbeefu);
    CHECK(ev_id[1] == 15 && ev_arg[1] == 42u);
#endif

    if (failures != 0) {
        fprintf(stderr, "trace_marker: %d failure(s)\n", failures);
        return 1;
    }
    printf("trace_marker: ok\n");
    return 0;
}
