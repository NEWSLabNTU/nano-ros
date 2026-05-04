/// @file main.c
/// @brief FreeRTOS C talker — publishes std_msgs/Int32 on /chatter

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/app_config.h>
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

void app_main(void) {
    printf("nros C Talker (FreeRTOS)\n");

    memset(&app, 0, sizeof(app));

    NROS_CHECK(nros_support_init(&app.support,
                                 NROS_APP_CONFIG.zenoh.locator,
                                 NROS_APP_CONFIG.zenoh.domain_id));
    NROS_CHECK(nros_node_init(&app.node, &app.support, "c_talker", "/"));
    NROS_CHECK(nros_publisher_init(&app.publisher, &app.node,
                                   std_msgs_msg_int32_get_type_support(), "/chatter"));
    NROS_CHECK(nros_executor_init(&app.executor, &app.support, 4));
    printf("Publisher created for topic: /chatter\n");

    std_msgs_msg_int32 message;
    std_msgs_msg_int32_init(&message);

    int count = 0;
    for (;;) {
        for (int j = 0; j < 100; j++) {
            nros_executor_spin_some(&app.executor, 10000000ULL);
        }

        message.data = count;
        NROS_SOFTCHECK(std_msgs_msg_int32_publish(&app.publisher, &message));
        printf("Published: %d\n", message.data);
        count++;
    }
}
