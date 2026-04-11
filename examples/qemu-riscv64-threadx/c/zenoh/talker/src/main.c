/// @file main.c
/// @brief ThreadX RISC-V QEMU C talker — publishes std_msgs/Int32 on /chatter

#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <nros/init.h>
#include <nros/node.h>
#include <nros/publisher.h>
#include <nros/executor.h>

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
    printf("nros C Talker (ThreadX RISC-V QEMU)\n");

    memset(&app, 0, sizeof(app));

    nros_ret_t ret = nros_support_init(&app.support, APP_ZENOH_LOCATOR, APP_DOMAIN_ID);
    if (ret != NROS_RET_OK) {
        printf("Failed to initialize support: %d\n", ret);
        return;
    }
    printf("Support initialized\n");

    ret = nros_node_init(&app.node, &app.support, "c_talker", "/");
    if (ret != NROS_RET_OK) {
        printf("Failed to initialize node: %d\n", ret);
        nros_support_fini(&app.support);
        return;
    }

    ret = nros_publisher_init(&app.publisher, &app.node,
                              std_msgs_msg_int32_get_type_support(), "/chatter");
    if (ret != NROS_RET_OK) {
        printf("Failed to initialize publisher: %d\n", ret);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return;
    }
    printf("Publisher created for topic: /chatter\n");

    ret = nros_executor_init(&app.executor, &app.support, 4);
    if (ret != NROS_RET_OK) {
        printf("Failed to initialize executor: %d\n", ret);
        nros_publisher_fini(&app.publisher);
        nros_node_fini(&app.node);
        nros_support_fini(&app.support);
        return;
    }

    printf("\nPublishing messages...\n");

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
            nros_ret_t pub_ret = nros_publish_raw(&app.publisher, buffer, serialized_size);
            if (pub_ret == NROS_RET_OK) {
                printf("Published: %d\n", message.data);
            } else {
                printf("Publish failed: %d\n", pub_ret);
            }
        } else {
            printf("Serialize failed: %d\n", ser_ret);
        }
        count++;
    }
}
