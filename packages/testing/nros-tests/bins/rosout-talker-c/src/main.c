/*
 * issue 1589 — the C twin of `bins/rosout-talker`.
 *
 * Logs through its NODE's logger (RFC-0102 D4: on Humble only node loggers
 * reach `/rosout`) and pumps `nros_rosout_*` from its spin loop, printing the
 * same markers the Rust probe prints so `rosout_interop.rs` reads all three
 * languages with one set of assertions.
 *
 *   ROSOUT_PROBE_RECORDS    records to log (default 60)
 *   ROSOUT_PROBE_PERIOD_MS  spin period per record (default 250)
 */

#include <nros/app_main.h>
#include <nros/nros.h>

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int env_int(const char* key, int fallback) {
    const char* v = getenv(key);
    return v != NULL ? atoi(v) : fallback;
}

static struct {
    nros_support_t support;
    nros_node_t node;
    nros_publisher_t rosout;
    nros_executor_t executor;
} app;

int nros_app_main(int argc, char** argv) {
    (void)argc;
    (void)argv;
#ifdef _IOLBF
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif

    const char* locator = getenv("NROS_LOCATOR");
    if (locator == NULL) {
        locator = NROS_ENTRY_LOCATOR;
    }
    const char* domain_str = getenv("ROS_DOMAIN_ID");
    uint8_t domain_id = domain_str != NULL ? (uint8_t)atoi(domain_str) : (uint8_t)NROS_ENTRY_DOMAIN_ID;
    int records = env_int("ROSOUT_PROBE_RECORDS", 60);
    int period_ms = env_int("ROSOUT_PROBE_PERIOD_MS", 250);

    memset(&app, 0, sizeof(app));
    if (!nros_logging_rosout_enabled()) {
        printf("ROSOUT_PROBE_NOT_BUILT\n");
        return 3;
    }
    NROS_CHECK_RET(nros_support_init(&app.support, locator, domain_id), 1);
    NROS_CHECK_RET(rclc_node_init_default(&app.node, "rosout_talker", "/", &app.support), 1);
    NROS_CHECK_RET(nros_rosout_publisher_init(&app.rosout, &app.node, NULL), 1);
    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 1), 1);

    nros_logger_t probe = nros_node_get_logger(&app.node);
    if (nros_rosout_enable() != NROS_RET_OK) {
        printf("ROSOUT_PROBE_SINK_FULL\n");
        return 4;
    }
    printf("ROSOUT_PROBE_READY\n");

    size_t total = 0;
    for (int i = 0; i < records; i++) {
        NROS_LOG_INFO(probe, "nros rosout probe record %d", i);
        if (i % 2 == 1) {
            NROS_LOG_WARN(probe, "nros rosout probe warning at %d", i);
        }
        (void)rclc_executor_spin_some(&app.executor, (uint64_t)period_ms * 1000000ULL);
        size_t sent = 0;
        nros_ret_t ret = nros_rosout_pump(&app.rosout, &sent);
        total += sent;
        if (ret != NROS_RET_OK) {
            printf("ROSOUT_PROBE_PUBLISH_ERR pumped=%zu ret=%d\n", total, (int)ret);
            return 5;
        }
        printf("ROSOUT_PROBE_PUMPED %zu\n", total);
    }
    printf("ROSOUT_PROBE_DONE pumped=%zu\n", total);

    rclc_executor_fini(&app.executor);
    nros_publisher_fini(&app.rosout);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);
    return 0;
}

NROS_APP_MAIN_REGISTER()
