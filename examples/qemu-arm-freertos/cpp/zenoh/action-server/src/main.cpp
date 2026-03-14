/// @file main.cpp
/// @brief C++ action server — Fibonacci (FreeRTOS QEMU)

#include <cstdio>
#include <nros/nros.hpp>
#include "example_interfaces.hpp"

extern "C" void app_main(void) {
    printf("nros C++ Action Server (FreeRTOS)\n");
    nros::Result ret = nros::init("tcp/192.0.3.1:7447", 0);
    if (!ret.ok()) { printf("init failed: %d\n", ret.raw()); return; }

    nros::Node node;
    ret = nros::create_node(node, "cpp_action_server");
    if (!ret.ok()) { printf("create_node failed\n"); nros::shutdown(); return; }
    printf("Node created\n");

    nros::ActionServer<example_interfaces::action::Fibonacci> srv;
    ret = node.create_action_server(srv, "/fibonacci");
    if (!ret.ok()) { printf("create_action_server failed\n"); nros::shutdown(); return; }

    printf("Action server ready\n");
    int goal_count = 0;
    for (int poll = 0; poll < 50000; poll++) {
        nros::spin_once(10);
        example_interfaces::action::Fibonacci::Goal goal;
        uint8_t goal_id[16];
        while (srv.try_recv_goal(goal, goal_id)) {
            goal_count++;
            printf("Goal received: order=%d\n", goal.order);
            int32_t a = 0, b = 1;
            example_interfaces::action::Fibonacci::Result result;
            for (int32_t i = 0; i < goal.order && i < 64; i++) {
                result.sequence.push_back(a);
                if (i > 0 && (i % 3 == 0 || i == goal.order - 1)) {
                    example_interfaces::action::Fibonacci::Feedback fb;
                    for (uint32_t k = 0; k < result.sequence.length(); k++)
                        fb.sequence.push_back(result.sequence[k]);
                    srv.publish_feedback(goal_id, fb);
                }
                int32_t next = a + b; a = b; b = next;
            }
            ret = srv.complete_goal(goal_id, result);
            if (ret.ok()) printf("Goal completed\n");
            else printf("complete_goal failed: %d\n", ret.raw());
            for (int s = 0; s < 2000; s++) {
                nros::spin_once(10);
                srv.try_handle_get_result();
            }
        }
    }
    printf("Action server done (%d goals)\n", goal_count);
    nros::shutdown();
}
