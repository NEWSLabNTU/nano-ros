/// @file main.c
/// @brief NuttX C talker example - publishes std_msgs/Int32 on /chatter

#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>

#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>
#include <nros/publisher.h>
#include <nros/timer.h>

#include "std_msgs.h"

// NuttX embedded config — matches board crate defaults (talker = 192.0.3.10)
#ifndef APP_ZENOH_LOCATOR
#define APP_ZENOH_LOCATOR "tcp/192.0.3.1:7447"
#endif
#ifndef APP_DOMAIN_ID
#define APP_DOMAIN_ID 0
#endif

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

    uint8_t buffer[64];
    size_t serialized_size = 0;
    int32_t ret = std_msgs_msg_int32_serialize(
        &ctx->message, buffer, sizeof(buffer), &serialized_size);

    if (ret == 0 && serialized_size > 0) {
        nros_ret_t pub_ret = nros_publish_raw(ctx->publisher, buffer, serialized_size);
        if (pub_ret == NROS_RET_OK) {
            printf("Published: %d\n", ctx->message.data);
        } else {
            fprintf(stderr, "Publish failed: %d\n", pub_ret);
        }
        fflush(stdout);
        fflush(stderr);
    }

    (void)0; // runs forever via timer
}

void app_main(void) {

    printf("nros NuttX C Talker\n");
    printf("Locator: %s\n", APP_ZENOH_LOCATOR);

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

    NROS_CHECK(nros_support_init(&app.support, APP_ZENOH_LOCATOR, APP_DOMAIN_ID));
    NROS_CHECK(nros_node_init(&app.node, &app.support, "nuttx_c_talker", "/"));
    NROS_CHECK(nros_publisher_init(&app.publisher, &app.node,
        std_msgs_msg_int32_get_type_support(), "/chatter"));

    app.talker_ctx = (talker_context_t){
        .publisher = &app.publisher,
        .message = {0},
        .count = 0,
        .max_count = 10,
    };
    std_msgs_msg_int32_init(&app.talker_ctx.message);

    NROS_CHECK(nros_timer_init(&app.timer, &app.support, 1000000000ULL,
        timer_callback, &app.talker_ctx));
    NROS_CHECK(nros_executor_init(&app.executor, &app.support, 4));
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
