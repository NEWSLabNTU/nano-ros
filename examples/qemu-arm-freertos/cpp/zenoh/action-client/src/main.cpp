/// @file main.cpp
/// @brief C++ action client — sends Fibonacci goal to /fibonacci (FreeRTOS QEMU)

#include <cstdio>
#include <nros/nros.hpp>
#include "example_interfaces.hpp"

extern "C" void app_main(void) {
    printf("nros C++ Action Client (FreeRTOS)\n");
    nros::Result ret = nros::init(APP_ZENOH_LOCATOR, APP_DOMAIN_ID);
    if (!ret.ok()) { printf("init failed: %d\n", ret.raw()); return; }

    nros::Node node;
    ret = nros::create_node(node, "cpp_action_client");
    if (!ret.ok()) { printf("create_node failed\n"); nros::shutdown(); return; }
    printf("Node created\n");

    nros::ActionClient<example_interfaces::action::Fibonacci> client;
    ret = node.create_action_client(client, "/fibonacci");
    if (!ret.ok()) { printf("create_action_client failed: %d\n", ret.raw()); nros::shutdown(); return; }

    printf("Action client ready for /fibonacci\n");

    // Warm-up: spin to allow Zenoh to discover the server's queryables
    for (int i = 0; i < 500; i++) {
        nros::spin_once(10);
    }

    example_interfaces::action::Fibonacci::Goal goal;
    goal.order = 5;

    printf("Sending goal: order=%d\n", goal.order);

    uint8_t goal_id[16];
    // Retry send_goal — Zenoh discovery may need time to find the server
    for (int attempt = 0; attempt < 5; attempt++) {
        ret = client.send_goal(goal, goal_id);
        if (ret.ok()) break;
        printf("Goal attempt %d failed (%d), retrying...\n", attempt + 1, ret.raw());
        for (int j = 0; j < 500; j++) {
            nros::spin_once(10);
        }
    }

    if (!ret.ok()) {
        printf("Failed to send goal after retries: %d\n", ret.raw());
        nros::shutdown();
        return;
    }

    printf("Goal accepted!\n");
    printf("Waiting for result...\n\n");

    example_interfaces::action::Fibonacci::Result result;
    ret = client.get_result(goal_id, result);

    if (ret.ok()) {
        printf("Result: [");
        for (uint32_t i = 0; i < result.sequence.size; i++) {
            if (i > 0) printf(", ");
            printf("%d", result.sequence.data[i]);
        }
        printf("]\n");
        printf("\nAction completed successfully.\n");
    } else {
        printf("Failed to get result: %d\n", ret.raw());
    }

    nros::shutdown();
}
