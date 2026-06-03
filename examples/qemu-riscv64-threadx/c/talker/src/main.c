/// @file main.c
/// @brief ThreadX RISC-V QEMU C talker — publishes std_msgs/Int32 on /chatter

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <nros/app_main.h>
#include <nros/check.h>
#include <nros/executor.h>
#include <nros/init.h>
#include <nros/node.h>
#include <nros/publisher.h>

#include "std_msgs.h"

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

static struct {
    nros_support_t support;
    nros_node_t node;
    nros_publisher_t publisher;
    nros_executor_t executor;
} app;

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;

    printf("nros C Talker (ThreadX RISC-V QEMU)\n");

    memset(&app, 0, sizeof(app));

    const char *loc = getenv("NROS_LOCATOR");
    if (!loc) loc = "tcp/10.0.2.2:7553"; /* fixture default — qemu-riscv64-threadx talker port */
    int domain = 0;
    const char *d = getenv("ROS_DOMAIN_ID");
    if (d) domain = atoi(d);
    NROS_CHECK_RET(nros_support_init(&app.support, loc, domain), 1);
    NROS_CHECK_RET(nros_node_init(&app.node, &app.support, "c_talker", "/"), 1);
    NROS_CHECK_RET(nros_publisher_init(&app.publisher, &app.node,
                                   std_msgs_msg_int32_get_type_support(), "/chatter"), 1);
    NROS_CHECK_RET(nros_executor_init(&app.executor, &app.support, 4), 1);
    printf("Publisher created for topic: /chatter\n");

    std_msgs_msg_int32 message;
    std_msgs_msg_int32_init(&message);

    int count = 0;
    for (;;) {
        for (int j = 0; j < 100; j++) {
            nros_executor_spin_some(&app.executor, 10000000ULL);
        }

        message.data = count;
        uint8_t buffer[64];
        size_t serialized_size = 0;
        int32_t ser_ret = std_msgs_msg_int32_serialize(
            &message, buffer, sizeof(buffer), &serialized_size);

        if (ser_ret == 0 && serialized_size > 0) {
            NROS_SOFTCHECK(nros_publish_raw(&app.publisher, buffer, serialized_size));
            printf("Published: %d\n", message.data);
        } else {
            printf("Serialize failed: %d\n", ser_ret);
        }
        count++;
    }
}

NROS_APP_MAIN_REGISTER_VOID()
