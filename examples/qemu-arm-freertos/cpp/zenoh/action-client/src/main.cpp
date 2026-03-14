/// @file main.cpp
/// @brief C++ action client — Fibonacci (FreeRTOS QEMU)

#include <cstdio>
#include <nros/nros.hpp>
#include "example_interfaces.hpp"

extern "C" void app_main(void) {
    printf("nros C++ Action Client (FreeRTOS)\n");
    nros::Result ret = nros::init("tcp/192.0.3.1:7447", 0);
    if (!ret.ok()) { printf("init failed: %d\n", ret.raw()); return; }

    nros::Node node;
    ret = nros::create_node(node, "cpp_action_client");
    if (!ret.ok()) { printf("create_node failed\n"); nros::shutdown(); return; }
    printf("Node created\n");

    nros::ActionClient<example_interfaces::action::Fibonacci> client;
    ret = node.create_action_client(client, "/fibonacci");
    if (!ret.ok()) { printf("create_action_client failed\n"); nros::shutdown(); return; }

    printf("Action client ready\n");
    example_interfaces::action::Fibonacci::Goal goal;
    goal.order = 5;
    printf("Sending goal: order=%d\n", goal.order);

    uint8_t goal_id[16];
    ret = client.send_goal(goal, goal_id);
    if (!ret.ok()) { printf("send_goal failed: %d\n", ret.raw()); nros::shutdown(); return; }
    printf("Goal accepted\n");

    int fb_count = 0;
    for (int i = 0; i < 5000; i++) {
        nros::spin_once(10);
        example_interfaces::action::Fibonacci::Feedback fb;
        while (client.try_recv_feedback(fb)) {
            fb_count++;
            printf("Feedback #%d: [", fb_count);
            for (uint32_t k = 0; k < fb.sequence.length(); k++) {
                if (k > 0) printf(", ");
                printf("%d", fb.sequence[k]);
            }
            printf("]\n");
        }
    }

    example_interfaces::action::Fibonacci::Result result;
    ret = client.get_result(goal_id, result);
    if (ret.ok()) {
        printf("Result: [");
        for (uint32_t i = 0; i < result.sequence.length(); i++) {
            if (i > 0) printf(", ");
            printf("%d", result.sequence[i]);
        }
        printf("]\n");
        printf("Action completed successfully\n");
    } else {
        printf("get_result failed: %d\n", ret.raw());
    }
    nros::shutdown();
}
