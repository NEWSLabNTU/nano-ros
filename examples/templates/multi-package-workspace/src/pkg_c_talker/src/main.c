// Phase 123.A.10 — minimal C talker demonstrating Pattern A.
//
// Publishes std_msgs/Int32 at 1 Hz on /chatter. Trimmed from
// examples/native/c/talker — this version omits parameter
// + clock demos so the source stays focused on the multi-package
// integration story.

#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <nros/app_main.h>
#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>
#include <nros/publisher.h>
#include <nros/timer.h>

#include "std_msgs.h"

typedef struct {
    nros_publisher_t *publisher;
    std_msgs_msg_int32 message;
    int count;
} talker_ctx_t;

static struct {
    nros_support_t support;
    nros_node_t node;
    nros_publisher_t publisher;
    talker_ctx_t talker;
    nros_timer_t timer;
    nros_executor_t executor;
} app;

static volatile sig_atomic_t g_running = 1;
static nros_executor_t *g_executor = NULL;

static void signal_handler(int signum) {
    (void)signum;
    g_running = 0;
    if (g_executor) {
        nros_executor_stop(g_executor);
    }
}

static void timer_cb(struct nros_timer_t *timer, void *context) {
    (void)timer;
    talker_ctx_t *ctx = (talker_ctx_t *)context;
    ctx->count++;
    ctx->message.data = ctx->count;
    NROS_SOFTCHECK(std_msgs_msg_int32_publish(ctx->publisher, &ctx->message));
    printf("[pkg_c_talker] sent: %d\n", ctx->message.data);
}

int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;
    printf("pkg_c_talker — multi-package-workspace demo\n");

    const char *locator = getenv("NROS_LOCATOR");
    if (!locator) locator = "tcp/127.0.0.1:7447";
    const char *domain_str = getenv("ROS_DOMAIN_ID");
    uint8_t domain_id = (uint8_t)(domain_str ? atoi(domain_str) : 0);

    memset(&app, 0, sizeof(app));

    NROS_CHECK_RET(nros_support_init(&app.support, locator, domain_id), 1);
    NROS_CHECK_RET(nros_node_init(&app.node, &app.support, "pkg_c_talker", "/"), 1);
    NROS_CHECK_RET(nros_publisher_init(&app.publisher, &app.node,
                                       std_msgs_msg_int32_get_type_support(),
                                       "/chatter"),
                   1);
    app.talker = (talker_ctx_t){.publisher = &app.publisher, .message = {0}, .count = 0};
    std_msgs_msg_int32_init(&app.talker.message);

    NROS_CHECK_RET(nros_timer_init(&app.timer, &app.support, 1000000000ULL,
                                   timer_cb, &app.talker),
                   1);
    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 4), 1);
    g_executor = &app.executor;
    NROS_CHECK_RET(nros_executor_register_timer(&app.executor, &app.timer), 1);

    signal(SIGINT, signal_handler);
    signal(SIGTERM, signal_handler);

    printf("[pkg_c_talker] publishing /chatter @ 1 Hz (Ctrl-C to exit)\n");
    nros_ret_t ret = nros_executor_spin_period(&app.executor, 100000000ULL);
    if (ret != NROS_RET_OK && g_running) {
        fprintf(stderr, "spin failed: %d\n", ret);
    }

    nros_executor_fini(&app.executor);
    nros_timer_fini(&app.timer);
    nros_publisher_fini(&app.publisher);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);
    return 0;
}

NROS_APP_MAIN_REGISTER_POSIX()
