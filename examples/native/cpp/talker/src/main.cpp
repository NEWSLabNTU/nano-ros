/// @file main.cpp
/// @brief C++ talker example - publishes std_msgs/String "Hello World: N" at 1 Hz

#include <cstdio>
#include <cstdlib>
#include <csignal>

// Route NROS_TRY_RET through std::fprintf (we have stdio).
#define NROS_TRY_LOG(file, line, expr, ret)                                                        \
    std::fprintf(stderr, "[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))

#include <nros/nros.hpp>

// Generated C++ bindings for std_msgs/msg/String
#include "std_msgs.hpp"

// ----------------------------------------------------------------------------
// Application state
// ----------------------------------------------------------------------------

struct TalkerContext {
    nros::Publisher<std_msgs::msg::String>* publisher;
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

    // Pre-increment so the first payload is "Hello World: 1", matching the
    // official ROS 2 demo talker (demo_nodes_cpp `talker.cpp`).
    ctx->count++;
    char payload[64];
    std::snprintf(payload, sizeof(payload), "Hello World: %d", ctx->count);
    std_msgs::msg::String msg;
    msg.data = payload;

    nros::Result ret = ctx->publisher->publish(msg);
    if (ret.ok()) {
        std::printf("Publishing: '%s'\n", msg.data.c_str());
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

    // Phase 212.M.2 — `nros::init()` (no-arg) pulls locator + domain_id
    // from `$NROS_LOCATOR` / `$ROS_DOMAIN_ID` at runtime. Falling back
    // defaults match the prior hand-rolled env reads.
    NROS_TRY_RET(nros::init(), 1);

    nros::Node node;
    NROS_TRY_RET(nros::create_node(node, "talker"), 1);
    std::printf("Node created: %s\n", node.get_name());

    nros::Publisher<std_msgs::msg::String> pub;
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
