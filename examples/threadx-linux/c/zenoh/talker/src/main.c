/// @file main.c
/// @brief ThreadX Linux C talker — publishes std_msgs/Int32 on /chatter

#include <stdint.h>
#include <stdio.h>
#include <string.h>

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
    printf("nros C Talker (ThreadX Linux)\n");

    memset(&app, 0, sizeof(app));

    NROS_CHECK(nros_support_init(&app.support, APP_ZENOH_LOCATOR, APP_DOMAIN_ID));
    NROS_CHECK(nros_node_init(&app.node, &app.support, "c_talker", "/"));
    NROS_CHECK(nros_publisher_init(&app.publisher, &app.node,
                                   std_msgs_msg_int32_get_type_support(), "/chatter"));
    NROS_CHECK(nros_executor_init(&app.executor, &app.support, 4));
    printf("Publisher created for topic: /chatter\n");

    std_msgs_msg_int32 message;
    std_msgs_msg_int32_init(&message);

    for (int i = 0; i < 10; i++) {
        for (int j = 0; j < 100; j++) {
            nros_executor_spin_some(&app.executor, 10000000ULL);
        }

        message.data = i;
        NROS_SOFTCHECK(std_msgs_msg_int32_publish(&app.publisher, &message));
        printf("Published: %d\n", message.data);
    }

    printf("\nDone publishing 10 messages.\n");

    nros_executor_fini(&app.executor);
    nros_publisher_fini(&app.publisher);
    nros_node_fini(&app.node);
    nros_support_fini(&app.support);
}
