/// @file main.c
/// @brief NuttX C talker example - publishes std_msgs/Int32 on /chatter

#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#include <nros/app_main.h>
#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>
#include <nros/publisher.h>
#include <nros/timer.h>

#include <nros/app_config.h>
#include "std_msgs.h"

typedef struct {
    nros_publisher_t* publisher;
    std_msgs_msg_int32 message;
    int count;
    int max_count;
} talker_context_t;

static struct {
    nros_support_t support;
    nros_node_t node;
    nros_publisher_t publisher;
    talker_context_t talker_ctx;
    nros_timer_t timer;
    nros_executor_t executor;
} app;

static void timer_callback(struct nros_timer_t* timer, void* context) {
    (void)timer;
    talker_context_t* ctx = (talker_context_t*)context;

    ctx->count++;
    ctx->message.data = ctx->count;

    NROS_SOFTCHECK(std_msgs_msg_int32_publish(ctx->publisher, &ctx->message));
    printf("Published: %d\n", ctx->message.data);
    fflush(stdout);
    fflush(stderr);

    (void)0; // runs forever via timer
}

int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;


    printf("nros NuttX C Talker\n");
    printf("Locator: %s\n", NROS_APP_CONFIG.zenoh.locator);

    memset(&app, 0, sizeof(app));

    // Re-seed /dev/urandom with a per-example unique value. NuttX's
    // xorshift128 PRNG starts with a fixed seed, so two QEMU instances
    // otherwise generate identical Zenoh session IDs and zenohd rejects
    // the second connection with MAX_LINKS. Writing bytes to /dev/urandom
    // reseeds the PRNG state. Mirrors the approach in the Rust board
    // crate (packages/boards/nros-board-nuttx-qemu-arm/src/node.rs::init_hardware).
    //
    // The literal bytes don't matter — they just need to be distinct per
    // example. We match the Rust config IPs (10.0.2.30 = talker,
    // .31 = listener, .32 = service-server, .33 = service-client,
    // .34 = action-server, .35 = action-client) for consistency.
    {
        FILE* urandom = fopen("/dev/urandom", "wb");
        if (urandom != NULL) {
            const uint8_t seed[4] = {10, 0, 2, 30};
            fwrite(seed, 1, sizeof(seed), urandom);
            fclose(urandom);
        }
    }

    // Wait for NuttX networking to become ready before attempting the
    // zenoh TCP session. NuttX's poll()/select() don't cooperate with
    // blocking connect() well enough to rely on connect_timeout, so we
    // just sleep for a few seconds after boot and let the virtio-net
    // driver + DHCP/static IP setup finish. Mirrors the 5-second wait
    // in packages/boards/nros-board-nuttx-qemu-arm/src/node.rs::run().
    fflush(stdout);
    sleep(5);

    NROS_CHECK_RET(nros_support_init(&app.support, NROS_APP_CONFIG.zenoh.locator, NROS_APP_CONFIG.zenoh.domain_id), 1);
    NROS_CHECK_RET(nros_node_init(&app.node, &app.support, "nuttx_c_talker", "/"), 1);
    NROS_CHECK_RET(nros_publisher_init(&app.publisher, &app.node,
        std_msgs_msg_int32_get_type_support(), "/chatter"), 1);

    app.talker_ctx = (talker_context_t){
        .publisher = &app.publisher,
        .message = {0},
        .count = 0,
        .max_count = 10,
    };
    std_msgs_msg_int32_init(&app.talker_ctx.message);

    NROS_CHECK_RET(nros_timer_init(&app.timer, &app.support, 1000000000ULL,
        timer_callback, &app.talker_ctx), 1);
    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 4), 1);
    NROS_SOFTCHECK(nros_executor_add_timer(&app.executor, &app.timer));

    printf("Publishing messages...\n\n");
    // See rationale in timer_callback (line 60). Flush here too so the
    // readiness marker is captured before the spin loop begins.
    fflush(stdout);
    nros_executor_spin_period(&app.executor, 100000000ULL);

    nros_executor_fini(&app.executor);
    nros_timer_fini(&app.timer);
    nros_publisher_fini(&app.publisher);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);

}

NROS_APP_MAIN_REGISTER_VOID()
