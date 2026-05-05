/// @file main.cpp
/// @brief C++ talker example - publishes std_msgs/Int32 at 1 Hz using a timer

#include <cstdio>
#include <cstdlib>
#include <csignal>

#define NROS_TRY_LOG(file, line, expr, ret) \
    std::fprintf(stderr, "[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/nros.hpp>

// Generated C++ bindings for std_msgs/msg/Int32
#include "std_msgs.hpp"

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

struct TalkerContext {
    nros::Publisher<std_msgs::msg::Int32>* publisher;
    int count;
};

static volatile sig_atomic_t g_running = 1;

// ----------------------------------------------------------------------------
// Signal handler for graceful shutdown
// ----------------------------------------------------------------------------

static void signal_handler(int signum) {
    (void)signum;
    g_running = 0;
}

// ----------------------------------------------------------------------------
// Timer callback - publish a message
// ----------------------------------------------------------------------------

static void timer_callback(void* context) {
    TalkerContext* ctx = static_cast<TalkerContext*>(context);
    ctx->count++;

    std_msgs::msg::Int32 msg;
    msg.data = ctx->count;

    nros::Result ret = ctx->publisher->publish(msg);
    if (ret.ok()) {
        std::printf("Published: %d\n", ctx->count);
    } else {
        std::fprintf(stderr, "Publish failed: %d\n", ret.raw());
    }
}

// ----------------------------------------------------------------------------
// Main
// ----------------------------------------------------------------------------

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;

    std::printf("nros C++ Talker\n");
    std::printf("===================\n");

    // Get configuration from environment
    const char* locator = std::getenv("NROS_LOCATOR");
    if (!locator) {
        locator = "tcp/127.0.0.1:7447";
    }

    uint8_t domain_id = 0;
    const char* domain_str = std::getenv("ROS_DOMAIN_ID");
    if (domain_str) {
        domain_id = static_cast<uint8_t>(std::atoi(domain_str));
    }

    std::printf("Locator: %s\n", locator);
    std::printf("Domain ID: %d\n", domain_id);

    NROS_TRY_RET(nros::init(locator, domain_id), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "cpp_talker"), 1);
    std::printf("Node created: %s\n", node.get_name());

    nros::Publisher<std_msgs::msg::Int32> pub;
    NROS_TRY_RET(node.create_publisher(pub, "/chatter"), 1);

    TalkerContext ctx;
    ctx.publisher = &pub;
    ctx.count = 0;

    nros::Timer timer;
    NROS_TRY_RET(node.create_timer(timer, 1000, timer_callback, &ctx), 1);

    // Set up signal handler
    std::signal(SIGINT, signal_handler);
    std::signal(SIGTERM, signal_handler);

    std::printf("\nPublishing messages (Ctrl+C to exit)...\n\n");

    // Spin
    while (g_running && nros::ok()) {
        nros::spin_once(100);
    }

    // Cleanup
    std::printf("\nShutting down...\n");
    nros::shutdown();

    std::printf("Goodbye!\n");
    return 0;
}
