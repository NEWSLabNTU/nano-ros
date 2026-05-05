/// @file main.cpp
/// @brief C++ listener — subscribes to std_msgs/Int32 on /chatter (ThreadX RISC-V QEMU)

#include <cstdio>

#define NROS_TRY_LOG(file, line, expr, ret) \
    printf("[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/nros.hpp>
#include "std_msgs.hpp"

extern "C" void app_main(void) {
    printf("nros C++ Listener (ThreadX RISC-V QEMU)\n");
    NROS_CHECK(nros::init(APP_ZENOH_LOCATOR, APP_DOMAIN_ID));

    nros::Node node;
    NROS_CHECK(nros::create_node(node, "cpp_listener"));
    printf("Node created\n");

    nros::Subscription<std_msgs::msg::Int32> sub;
    NROS_CHECK(node.create_subscription(sub, "/chatter"));

    // Alternative: use Stream::wait_next for blocking reception
    // std_msgs::msg::Int32 msg;
    // sub.stream().wait_next(executor_handle, 1000, msg);

    printf("Waiting for messages...\n");
    for (;;) {
        nros::spin_once(10);
        std_msgs::msg::Int32 msg;
        while (sub.try_recv(msg)) {
            printf("Received: %d\n", msg.data);
        }
    }
}
