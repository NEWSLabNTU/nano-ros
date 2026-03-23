/// @file main.cpp
/// @brief C++ action server — Fibonacci on /fibonacci (FreeRTOS QEMU)

#include <cstdio>
#include <nros/nros.hpp>
#include "example_interfaces.hpp"

extern "C" void app_main(void) {
    printf("nros C++ Action Server (FreeRTOS)\n");
    nros::Result ret = nros::init(APP_ZENOH_LOCATOR, APP_DOMAIN_ID);
    if (!ret.ok()) { printf("init failed: %d\n", ret.raw()); return; }

    nros::Node node;
    ret = nros::create_node(node, "cpp_action_server");
    if (!ret.ok()) { printf("create_node failed\n"); nros::shutdown(); return; }
    printf("Node created\n");

    nros::ActionServer<example_interfaces::action::Fibonacci> srv;
    ret = node.create_action_server(srv, "/fibonacci");
    if (!ret.ok()) { printf("create_action_server failed: %d\n", ret.raw()); nros::shutdown(); return; }

    printf("Action server ready on /fibonacci\n");
    printf("Waiting for goals...\n");

    int goal_count = 0;
    for (int poll = 0; poll < 50000; poll++) {
        nros::spin_once(10);

        example_interfaces::action::Fibonacci::Goal goal;
        uint8_t goal_id[16];
        while (srv.try_recv_goal(goal, goal_id)) {
            goal_count++;
            printf("Goal request [%d]: order=%d\n", goal_count, goal.order);

            if (goal.order < 0 || goal.order >= 64) {
                printf("  -> order out of range, skipping\n");
                continue;
            }

            // Compute Fibonacci and publish feedback
            example_interfaces::action::Fibonacci::Feedback fb;
            fb.sequence.size = 0;
            for (int32_t i = 0; i <= goal.order; i++) {
                int32_t val;
                if (i == 0) val = 0;
                else if (i == 1) val = 1;
                else val = fb.sequence.data[i - 1] + fb.sequence.data[i - 2];
                fb.sequence.data[i] = val;
                fb.sequence.size = static_cast<uint32_t>(i + 1);

                ret = srv.publish_feedback(goal_id, fb);
                if (ret.ok()) {
                    printf("  Feedback: [");
                    for (uint32_t j = 0; j < fb.sequence.size; j++) {
                        if (j > 0) printf(", ");
                        printf("%d", fb.sequence.data[j]);
                    }
                    printf("]\n");
                }
            }

            // Complete goal with final result
            example_interfaces::action::Fibonacci::Result result;
            result.sequence.size = fb.sequence.size;
            for (uint32_t i = 0; i < fb.sequence.size; i++) {
                result.sequence.data[i] = fb.sequence.data[i];
            }

            ret = srv.complete_goal(goal_id, result);
            if (ret.ok()) {
                printf("  Goal SUCCEEDED\n");
            } else {
                printf("  complete_goal failed: %d\n", ret.raw());
            }
        }
    }

    printf("Server shutting down.\n");
    nros::shutdown();
}
