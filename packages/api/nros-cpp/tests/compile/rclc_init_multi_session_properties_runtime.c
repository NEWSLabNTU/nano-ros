/* Issue 2123 — the C side of per-session RMW properties: `rclc_init_multi`
 * hands EACH session its own rows (`rclc_session_spec_t::properties`, its
 * domain's RMW file lowered on the host, RFC-0109 D8).
 *
 * `init_multi_session_properties_runtime.cpp` runs the C++ entry point
 * (`nros_cpp_init_multi`). This one runs the other reader, which lives in
 * `nros-bridge` and reaches a C or C++ binary only through `nros-cpp`'s
 * `bridge` feature, so no lane linked it until this probe. Both call
 * `nros_node::lent_session_properties`, but each splits the rows per spec on
 * its own, and that split is what a shared table gets wrong. Run against the
 * shared stub backend, which records the properties each session was opened
 * with (copies: the runtime lends the rows for the open only).
 *
 * Asserted:
 *   1. three specs, three backends, three different tables (one empty) ->
 *      each session saw its own rows, in order;
 *   2. a NULL table with a non-zero length is refused before anything opens;
 *   3. more rows than one session open carries is refused, never truncated.
 *
 * Plain C, compiled by `cc`: the header is a C API, and this is the language
 * a C caller meets it in. */

#include <rclc/bridge.h>

#include <nros/rmw_ret.h>

#include "stub_rmw_backend.h"

#include <stdio.h>
#include <string.h>

/* The strong hook `rclc_init_multi` calls before it opens anything. */
void nros_app_register_backends(void);
void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
    (void)nros_stub_rmw_register_as("stub_b");
    (void)nros_stub_rmw_register_as("stub_c");
}

static int g_failures = 0;

static void check(bool cond, const char* what) {
    if (!cond) {
        ++g_failures;
        fprintf(stderr, "FAIL: %s\n", what);
    }
}

static bool row_is(uint32_t session, size_t i, const char* key, const char* value) {
    const char* k = NULL;
    const char* v = NULL;
    return nros_stub_rmw_session_property(session, i, &k, &v) && strcmp(k, key) == 0 &&
           strcmp(v, value) == 0;
}

static rclc_session_spec_t spec(const char* rmw, const rclc_session_property_t* rows, size_t len) {
    rclc_session_spec_t s = {rmw, "", 0, "probe", NULL, rows, len};
    return s;
}

static const rclc_session_property_t kEcu[] = {
    {"mode", "client"},
    {"connect", "tcp/10.0.0.1:7447"},
};
static const rclc_session_property_t kCtl[] = {
    {"config", "<CycloneDDS><Domain id=\"any\"/></CycloneDDS>"},
};

int main(void) {
    /* 1. Each session gets its own table; a spec with none gets none. */
    {
        const uint32_t first = nros_stub_rmw_sessions_opened();
        rclc_session_spec_t specs[3];
        specs[0] = spec(NROS_STUB_RMW_NAME, kEcu, 2);
        specs[1] = spec("stub_b", kCtl, 1);
        specs[2] = spec("stub_c", NULL, 0);
        rclc_executor_handle_t exec = NULL;
        int32_t rc = rclc_init_multi(specs, 3, &exec);
        check(rc == NROS_RMW_RET_OK, "three specs open");
        if (rc == NROS_RMW_RET_OK) {
            check(exec != NULL, "an opened executor has a handle");
            check(nros_stub_rmw_sessions_opened() == first + 3, "three sessions opened");
            check(nros_stub_rmw_session_property_count(first) == 2, "session 0 saw two rows");
            check(row_is(first, 0, "mode", "client"), "session 0 row 0 is its own");
            check(row_is(first, 1, "connect", "tcp/10.0.0.1:7447"), "session 0 row 1 is its own");
            check(nros_stub_rmw_session_property_count(first + 1) == 1, "session 1 saw one row");
            check(row_is(first + 1, 0, "config", "<CycloneDDS><Domain id=\"any\"/></CycloneDDS>"),
                  "session 1 saw ITS table, not session 0's");
            check(nros_stub_rmw_session_property_count(first + 2) == 0,
                  "a spec with no table opens with none");
            rclc_fini_multi(exec);
        }
    }

    /* 2. A NULL table with rows is refused before any session opens. */
    {
        const uint32_t before = nros_stub_rmw_sessions_opened();
        rclc_session_spec_t specs[1];
        specs[0] = spec(NROS_STUB_RMW_NAME, NULL, 1);
        rclc_executor_handle_t exec = NULL;
        check(rclc_init_multi(specs, 1, &exec) == NROS_RMW_RET_INVALID_ARGUMENT,
              "NULL rows with a length are INVALID_ARGUMENT");
        check(exec == NULL, "a refused open leaves no handle");
        check(nros_stub_rmw_sessions_opened() == before, "nothing opened for a refused table");
    }

    /* 3. More rows than one session open carries: refused, never truncated. */
    {
        const uint32_t before = nros_stub_rmw_sessions_opened();
        rclc_session_property_t many[17];
        for (size_t i = 0; i < 17; ++i) {
            many[i].key = "k";
            many[i].value = "v";
        }
        rclc_session_spec_t specs[1];
        specs[0] = spec(NROS_STUB_RMW_NAME, many, 17);
        rclc_executor_handle_t exec = NULL;
        check(rclc_init_multi(specs, 1, &exec) == NROS_RMW_RET_INVALID_ARGUMENT,
              "17 rows for one session are INVALID_ARGUMENT");
        check(nros_stub_rmw_sessions_opened() == before, "nothing opened for an oversized table");
    }

    if (g_failures != 0) {
        fprintf(stderr, "rclc_init_multi_session_properties_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    printf("rclc_init_multi_session_properties_runtime: OK\n");
    return 0;
}
