// Phase 123.A.10 — minimal C++ listener demonstrating Pattern A.
//
// Subscribes to std_msgs/Int32 on /chatter via nano-ros's C++ API.
// Pairs with pkg_c_talker — same RMW + platform + std_msgs codegen
// reused across packages.

#include <csignal>
#include <cstdio>
#include <cstdlib>

#include <nros/app_main.h>
#include <nros/nros.hpp>

#include "std_msgs.hpp"

static volatile sig_atomic_t g_running = 1;

static void signal_handler(int signum) {
    (void)signum;
    g_running = 0;
}

int nros_app_main(int argc, char **argv) {
    (void)argc;
    (void)argv;
    std::printf("pkg_cpp_listener — multi-package-workspace demo\n");

    const char *locator = std::getenv("NROS_LOCATOR");
    if (!locator) locator = "tcp/127.0.0.1:7447";
    const char *domain_str = std::getenv("ROS_DOMAIN_ID");
    uint8_t domain_id =
        static_cast<uint8_t>(domain_str ? std::atoi(domain_str) : 0);

    NROS_TRY_RET(nros::init(locator, domain_id), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "pkg_cpp_listener"), 1);

    nros::Subscription<std_msgs::msg::Int32> sub;
    NROS_TRY_RET(node.create_subscription(sub, "/chatter"), 1);

    std::signal(SIGINT, signal_handler);
    std::signal(SIGTERM, signal_handler);

    std::printf("[pkg_cpp_listener] subscribed to /chatter (Ctrl-C to exit)\n");

    int count = 0;
    while (g_running && nros::ok()) {
        nros::spin_once(100);
        std_msgs::msg::Int32 msg;
        while (sub.try_recv(msg)) {
            count++;
            std::printf("[pkg_cpp_listener] received: %d (total=%d)\n",
                        msg.data, count);
        }
    }

    nros::shutdown();
    std::printf("[pkg_cpp_listener] received %d total\n", count);
    return 0;
}

NROS_APP_MAIN_REGISTER_POSIX()
