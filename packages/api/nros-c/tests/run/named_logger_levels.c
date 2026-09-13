/* phase-417 W4.d — two nodes in ONE C image log under DISTINCT names, and
 * their thresholds move independently.
 *
 * This is the C half of the property `nros-log`'s
 * `named_loggers_levels_and_throttle.rs` proves for Rust. It is a RUN and not a
 * signature probe because every shape of this defect compiles: the accessors
 * existed, were declared, were called, and answered the SAME catch-all logger
 * for every node in the image (phase-427 W5 — three of four accessors resolved
 * through a lookup-only `get_logger`, so no record said which node wrote it).
 * A header check cannot tell two handles from one.
 *
 * The five things asserted here, in the order they would have failed:
 *
 *   1. two nodes -> two DISTINCT logger handles, neither of them the catch-all;
 *   2. each handle answers its OWN node's name;
 *   3. `nros_log_get_logger(name)` returns the SAME handle the node's accessor
 *      did — one resolve-or-create helper behind both, not a second table. This
 *      is the assertion that keeps a future C-side named-logger surface from
 *      re-implementing what `nros_log::resolve_logger` already does (RFC-0019);
 *   4. a threshold set on one logger does not move its sibling's, MEASURED
 *      through an installed sink rather than through the getter, because a
 *      `set_level`/`get_level` pair over one shared cell would agree with
 *      itself and still drop the wrong records;
 *   5. `nros_logger_is_enabled` agrees with the level that was set.
 *
 * The RMW backend is the shared stub (`stub_rmw_backend.c`), which this TU
 * links: a node needs an executor and an executor needs a session, and nothing
 * here touches the wire.
 */

#include <nros/log.h>
#include <nros/nros.h>

#include <stdio.h>
#include <string.h>

#include "stub_rmw_backend.h"

/* The registration seam. There is no weak default on the C path. */
void nros_app_register_backends(void);
void nros_app_register_backends(void) {
    (void)nros_stub_rmw_register();
}

/* ---- assertions -------------------------------------------------------- */

static int g_failures;

#define CHECK(cond, ...)                                                                           \
    do {                                                                                           \
        if (!(cond)) {                                                                             \
            ++g_failures;                                                                          \
            printf("FAIL %s:%d: ", __FILE__, __LINE__);                                            \
            printf(__VA_ARGS__);                                                                   \
            printf("\n");                                                                          \
        }                                                                                          \
    } while (0)

/* ---- the capture sink -------------------------------------------------- */

#define MAX_RECORDS 32

struct record {
    nros_log_severity_t severity;
    char logger[64];
    char message[128];
};

static struct record g_records[MAX_RECORDS];
static size_t g_record_count;

static void copy_bounded(char* dst, size_t dst_len, const char* src, size_t src_len) {
    size_t n = (src_len < dst_len - 1) ? src_len : dst_len - 1;
    if (src != NULL && n > 0) {
        memcpy(dst, src, n);
    }
    dst[n] = '\0';
}

static void capture_sink(void* user_data, nros_log_severity_t severity, const char* logger_name,
                         size_t logger_name_len, const char* message, size_t message_len,
                         const char* file, size_t file_len, uint32_t line, uint64_t timestamp_ns) {
    (void)user_data;
    (void)file;
    (void)file_len;
    (void)line;
    (void)timestamp_ns;
    if (g_record_count >= MAX_RECORDS) {
        return;
    }
    g_records[g_record_count].severity = severity;
    copy_bounded(g_records[g_record_count].logger, sizeof(g_records[0].logger), logger_name,
                 logger_name_len);
    copy_bounded(g_records[g_record_count].message, sizeof(g_records[0].message), message,
                 message_len);
    ++g_record_count;
}

/* How many captured records carry `needle` in their message AND name `logger`?
 *
 * Both halves, because a record that arrived under the WRONG name is the
 * defect, not a pass with a cosmetic flaw. */
static int records_from(const char* logger, const char* needle) {
    int n = 0;
    size_t i;
    for (i = 0; i < g_record_count; ++i) {
        if (strstr(g_records[i].message, needle) != NULL &&
            strcmp(g_records[i].logger, logger) == 0) {
            ++n;
        }
    }
    return n;
}

/* Any record carrying `needle`, under any name. Separates "filtered out" from
 * "emitted under the wrong logger", which are different bugs with the same
 * symptom at the call site. */
static int records_anywhere(const char* needle) {
    int n = 0;
    size_t i;
    for (i = 0; i < g_record_count; ++i) {
        if (strstr(g_records[i].message, needle) != NULL) {
            ++n;
        }
    }
    return n;
}

int main(void) {
    struct nros_support_t support;
    struct nros_executor_t executor = rclc_executor_get_zero_initialized_executor();
    struct nros_node_t alpha;
    struct nros_node_t beta;
    nros_logger_t alpha_log;
    nros_logger_t beta_log;
    char name[64];

    memset(&support, 0, sizeof(support));
    memset(&alpha, 0, sizeof(alpha));
    memset(&beta, 0, sizeof(beta));

    nros_log_init();
    CHECK(nros_log_add_sink(capture_sink, NULL),
          "the capture sink must install — an uninstalled sink never fires, which is "
          "indistinguishable from a filter that dropped everything");

    CHECK(nros_support_init_rmw(&support, NULL, 0, "w4d_named_loggers", NROS_STUB_RMW_NAME) ==
              NROS_RET_OK,
          "support init");
    CHECK(nros_executor_init(&executor, &support, 8) == NROS_RET_OK, "executor init");
    CHECK(nros_executor_node_init(&executor, &alpha, "w4d_alpha", NULL) == NROS_RET_OK,
          "alpha node init");
    CHECK(nros_executor_node_init(&executor, &beta, "w4d_beta", NULL) == NROS_RET_OK,
          "beta node init");
    if (g_failures != 0) {
        printf("named_logger_levels: %d setup failure(s)\n", g_failures);
        return 1;
    }

    /* --- 1. two nodes, two loggers, neither the catch-all ---------------- */
    alpha_log = nros_node_get_logger(&alpha);
    beta_log = nros_node_get_logger(&beta);
    CHECK(alpha_log != NULL && beta_log != NULL, "both node loggers must be non-NULL");
    CHECK(alpha_log != beta_log,
          "two nodes answered ONE logger handle — this is phase-427 W5's defect, and every "
          "record in the image would name the same logger");
    CHECK(alpha_log != nros_log_default_logger() && beta_log != nros_log_default_logger(),
          "a node logger must not BE the catch-all: the catch-all's threshold is shared with "
          "every unnamed logger, so moving it moves everyone");

    /* --- 2. each answers its own name ------------------------------------ */
    (void)nros_logger_get_name(alpha_log, name, sizeof(name));
    CHECK(strcmp(name, "w4d_alpha") == 0, "alpha's logger names \"%s\", expected w4d_alpha", name);
    (void)nros_logger_get_name(beta_log, name, sizeof(name));
    CHECK(strcmp(name, "w4d_beta") == 0, "beta's logger names \"%s\", expected w4d_beta", name);

    /* --- 3. ONE resolve-or-create helper behind both spellings ----------- */
    CHECK(nros_log_get_logger("w4d_alpha") == alpha_log,
          "nros_log_get_logger(\"w4d_alpha\") answered a DIFFERENT handle than "
          "nros_node_get_logger did — two tables, so a level set through one spelling would "
          "not be the level the other filters on");
    CHECK(nros_log_get_logger("w4d_beta") == beta_log,
          "same, for beta: the node accessor and the named lookup must reach one helper");

    /* --- 4. the thresholds are INDEPENDENT, measured at the sink --------- */
    CHECK(nros_logger_set_level(alpha_log, NROS_LOG_SEVERITY_ERROR), "quiet alpha to ERROR");
    CHECK(nros_logger_set_level(beta_log, NROS_LOG_SEVERITY_INFO), "hold beta at INFO");
    CHECK(nros_logger_get_level(alpha_log) == NROS_LOG_SEVERITY_ERROR,
          "alpha's level must read back as it was set");
    CHECK(nros_logger_get_level(beta_log) == NROS_LOG_SEVERITY_INFO,
          "beta's level must read back as it was set");

    NROS_LOG_INFO(alpha_log, "w4d_marker_alpha_info");
    NROS_LOG_INFO(beta_log, "w4d_marker_beta_info");
    NROS_LOG_ERROR(alpha_log, "w4d_marker_alpha_error");

    CHECK(records_anywhere("w4d_marker_alpha_info") == 0,
          "an INFO record on a logger set to ERROR reached a sink (%d time(s))",
          records_anywhere("w4d_marker_alpha_info"));
    CHECK(records_from("w4d_beta", "w4d_marker_beta_info") == 1,
          "beta's INFO must survive under BETA's name — quieting alpha moved its sibling "
          "(found %d under w4d_beta, %d anywhere)",
          records_from("w4d_beta", "w4d_marker_beta_info"),
          records_anywhere("w4d_marker_beta_info"));
    CHECK(records_from("w4d_alpha", "w4d_marker_alpha_error") == 1,
          "alpha's ERROR must arrive under ALPHA's name (found %d under w4d_alpha, %d anywhere)",
          records_from("w4d_alpha", "w4d_marker_alpha_error"),
          records_anywhere("w4d_marker_alpha_error"));

    /* --- 5. the predicate agrees with the level -------------------------- */
    CHECK(!nros_logger_is_enabled(alpha_log, NROS_LOG_SEVERITY_INFO),
          "is_enabled(INFO) must be false on a logger set to ERROR");
    CHECK(nros_logger_is_enabled(alpha_log, NROS_LOG_SEVERITY_ERROR),
          "is_enabled(ERROR) must be true on a logger set to ERROR");
    CHECK(nros_logger_is_enabled(beta_log, NROS_LOG_SEVERITY_INFO),
          "is_enabled(INFO) must be true on a logger still at INFO");

    (void)rclc_executor_fini(&executor);
    (void)rclc_support_fini(&support);

    if (g_failures != 0) {
        printf("named_logger_levels: %d failure(s)\n", g_failures);
        return 1;
    }
    printf("named_logger_levels: ok (%d records captured)\n", (int)g_record_count);
    return 0;
}
