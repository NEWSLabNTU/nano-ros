/// @file main.cpp
/// @brief C++ action server — Fibonacci on /fibonacci (FreeRTOS QEMU, callback-based)

#include <cstdio>
#include <nros/nros.hpp>
#include "example_interfaces.hpp"

using Fibonacci = example_interfaces::action::Fibonacci;

static nros::ActionServer<Fibonacci>* g_srv = nullptr;
static int g_goal_count = 0;

static nros::GoalResponse on_goal(const uint8_t uuid[16], const Fibonacci::Goal& goal) {
    if (goal.order < 0 || goal.order >= 64) {
        printf("Goal rejected (order out of range): %d\n", goal.order);
        return nros::GoalResponse::Reject;
    }

    g_goal_count++;
    printf("Goal request [%d]: order=%d\n", g_goal_count, goal.order);

    Fibonacci::Feedback fb;
    fb.sequence.size = 0;
    for (int32_t i = 0; i <= goal.order; i++) {
        int32_t val;
        if (i == 0) val = 0;
        else if (i == 1) val = 1;
        else val = fb.sequence.data[i - 1] + fb.sequence.data[i - 2];
        fb.sequence.data[i] = val;
        fb.sequence.size = static_cast<uint32_t>(i + 1);

        if (g_srv->publish_feedback(uuid, fb).ok()) {
            printf("  Feedback: [");
            for (uint32_t j = 0; j < fb.sequence.size; j++) {
                if (j > 0) printf(", ");
                printf("%d", fb.sequence.data[j]);
            }
            printf("]\n");
        }
    }

    Fibonacci::Result result;
    result.sequence.size = fb.sequence.size;
    for (uint32_t i = 0; i < fb.sequence.size; i++) {
        result.sequence.data[i] = fb.sequence.data[i];
    }

    if (g_srv->complete_goal(uuid, result).ok()) {
        printf("  Goal SUCCEEDED\n");
    }
    return nros::GoalResponse::AcceptAndExecute;
}

extern "C" void app_main(void) {
    printf("nros C++ Action Server (FreeRTOS)\n");
    nros::Result ret = nros::init(APP_ZENOH_LOCATOR, APP_DOMAIN_ID);
    if (!ret.ok()) { printf("init failed: %d\n", ret.raw()); return; }

    nros::Node node;
    ret = nros::create_node(node, "cpp_action_server");
    if (!ret.ok()) { printf("create_node failed\n"); nros::shutdown(); return; }
    printf("Node created\n");

    nros::ActionServer<Fibonacci> srv;
    ret = node.create_action_server(srv, "/fibonacci");
    if (!ret.ok()) { printf("create_action_server failed: %d\n", ret.raw()); nros::shutdown(); return; }

    g_srv = &srv;
    srv.set_goal_callback(on_goal);

    printf("Action server ready on /fibonacci\n");
    printf("Waiting for goals...\n");

    for (int poll = 0; poll < 50000; poll++) {
        nros::spin_once(10);
    }

    printf("Server shutting down.\n");
    nros::shutdown();
}
