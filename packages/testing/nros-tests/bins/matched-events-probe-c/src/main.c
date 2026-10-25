/// @file main.c
/// @brief Issue 2039 — the C API's matched and status-event callbacks, driven
///        by a STOCK ROS 2 peer.
///
/// Holds one publisher on `/i2039_out` and one subscription on `/i2039_in`
/// (both `std_msgs/String`), and registers, through the public C setters:
///
///   - `rclc_publisher_set_matched_callback`     (a stock subscriber appears / leaves);
///   - `rclc_subscription_set_matched_callback`  (a stock publisher appears / leaves);
///   - `rclc_subscription_set_liveliness_changed_callback` (the same publisher's
///     liveliness).
///
/// Each callback prints one marker line (`nros_tests::output::MATCHED_PROBE_*`)
/// with the status the backend reported, so
/// `tests/c_matched_events_interop.rs` can assert the counts RISE and FALL.
///
/// The publisher's matched status is observed on its poll points
/// (`rcl_publish`), so the loop publishes every 100 ms. Exits 0 after
/// `MATCHED_EVENTS_PROBE_TIMEOUT_MS` (default 60 s), or 3 when a setter is
/// refused (the backend does not generate that event).

#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

#include <rclc/rclc.h>

#include "std_msgs.h"

#define PROBE "matched-events-probe-c: "

static struct {
    rclc_support_t support;
    rcl_node_t node;
    rcl_publisher_t publisher;
    rcl_subscription_t subscription;
    rclc_executor_t executor;
    std_msgs__msg__String out_msg;
    std_msgs__msg__String__Storage out_storage;
    std_msgs__msg__String in_msg;
    std_msgs__msg__String__Storage in_storage;
} app;

static volatile sig_atomic_t g_running = 1;

static void on_signal(int signum) {
    (void)signum;
    g_running = 0;
}

static void on_publisher_matched(struct rcl_publisher_t* pub, struct rclc_matched_status_t st,
                                 void* ctx) {
    (void)pub;
    (void)ctx;
    printf(PROBE "PUB_MATCHED current=%u change=%d total=%u\n", (unsigned)st.current_count,
           (int)st.current_count_change, (unsigned)st.total_count);
}

static void on_subscription_matched(struct rcl_subscription_t* sub,
                                    struct rclc_matched_status_t st, void* ctx) {
    (void)sub;
    (void)ctx;
    printf(PROBE "SUB_MATCHED current=%u change=%d total=%u\n", (unsigned)st.current_count,
           (int)st.current_count_change, (unsigned)st.total_count);
}

static void on_liveliness_changed(struct rcl_subscription_t* sub,
                                  struct rclc_liveliness_changed_status_t st, void* ctx) {
    (void)sub;
    (void)ctx;
    printf(PROBE "LIVELINESS alive=%u alive_change=%d\n", (unsigned)st.alive_count,
           (int)st.alive_count_change);
}

static void on_message(const void* msgin, void* ctx) {
    (void)msgin;
    (void)ctx;
}

static int registered(const char* what, rcl_ret_t rc) {
    if (rc != RCL_RET_OK) {
        printf(PROBE "UNSUPPORTED %s rc=%d\n", what, (int)rc);
        return 0;
    }
    return 1;
}

static long now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (long)ts.tv_sec * 1000L + ts.tv_nsec / 1000000L;
}

int main(int argc, char** argv) {
#ifdef _IOLBF
    setvbuf(stdout, NULL, _IOLBF, 0);
#endif
    memset(&app, 0, sizeof(app));
    long timeout_ms = 60000;
    const char* t = getenv("MATCHED_EVENTS_PROBE_TIMEOUT_MS");
    if (t != NULL && *t != '\0') timeout_ms = strtol(t, NULL, 10);

    rcl_allocator_t allocator = rcl_get_default_allocator();
    RCLC_CHECK_RET(rclc_support_init(&app.support, argc, argv, &allocator), 1);
    RCLC_CHECK_RET(rclc_node_init_default(&app.node, "matched_events_probe", "/", &app.support),
                   1);
    RCLC_CHECK_RET(rclc_publisher_init_default(&app.publisher, &app.node,
                                               ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String),
                                               "/i2039_out"),
                   1);
    RCLC_CHECK_RET(rclc_subscription_init_default(
                       &app.subscription, &app.node,
                       ROSIDL_GET_MSG_TYPE_SUPPORT(std_msgs, msg, String), "/i2039_in"),
                   1);
    RCLC_CHECK_RET(rclc_executor_init(&app.executor, &app.support.context, 2, NULL), 1);
    std_msgs__msg__String__init_in(&app.in_msg, &app.in_storage);
    std_msgs__msg__String__init_in(&app.out_msg, &app.out_storage);
    RCLC_CHECK_RET(rclc_executor_add_subscription_with_context(
                       &app.executor, &app.subscription, &app.in_msg, on_message, NULL,
                       ON_NEW_DATA),
                   1);

    /* The subscription's events register on the executor-owned subscriber, so
     * they come after `rclc_executor_add_subscription*`. */
    int ok = 1;
    ok &= registered("publisher_matched",
                     rclc_publisher_set_matched_callback(&app.publisher, on_publisher_matched,
                                                         NULL));
    ok &= registered("subscription_matched",
                     rclc_subscription_set_matched_callback(&app.subscription,
                                                            on_subscription_matched, NULL));
    ok &= registered("liveliness_changed",
                     rclc_subscription_set_liveliness_changed_callback(
                         &app.subscription, on_liveliness_changed, NULL));
    if (!ok) return 3;

    signal(SIGINT, on_signal);
    signal(SIGTERM, on_signal);
    printf(PROBE "READY\n");

    const long start = now_ms();
    int n = 0;
    while (g_running && now_ms() - start < timeout_ms) {
        (void)rclc_executor_spin_some(&app.executor, 100000000ULL);
        snprintf(app.out_msg.data.data, app.out_msg.data.capacity, "i2039 %d", ++n);
        app.out_msg.data.size = strlen(app.out_msg.data.data);
        /* The publisher's matched status is observed here. */
        (void)rcl_publish(&app.publisher, &app.out_msg, NULL);
    }

    printf(PROBE "DONE\n");
    rclc_executor_fini(&app.executor);
    rcl_subscription_fini(&app.subscription, &app.node);
    rcl_publisher_fini(&app.publisher, &app.node);
    rcl_node_fini(&app.node);
    rclc_support_fini(&app.support);
    return 0;
}
