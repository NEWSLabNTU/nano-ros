/// issue 1589 — the C++ twin of `bins/rosout-talker`.
///
/// Logs through its NODE's logger (RFC-0102 D4: on Humble only node loggers
/// reach `/rosout`) and spins. Nothing else: since phase-556 W13 the executor
/// creates the node's `/rosout` writer and publishes the log queue from every
/// spin, as rclcpp's node does. Prints the markers the Rust probe prints so
/// `rosout_interop.rs` reads all three languages with one set of assertions.
///
///   ROSOUT_PROBE_RECORDS    records to log (default 60)
///   ROSOUT_PROBE_PERIOD_MS  spin period per record (default 250)

#include <stdio.h>
#include <stdlib.h>

#include <rclcpp/logging.hpp>
#include <rclcpp/rclcpp.hpp>
#include <rclcpp/rosout.hpp>

static int env_int(const char* key, int fallback) {
    const char* v = getenv(key);
    return v != nullptr ? atoi(v) : fallback;
}

static int run(rclcpp::Node& node, int records, int period_ms);

int main(int argc, char** argv) {
#ifdef _IOLBF
    setvbuf(stdout, nullptr, _IOLBF, 0);
#endif
    (void)argc;
    (void)argv;
    const int records = env_int("ROSOUT_PROBE_RECORDS", 60);
    const int period_ms = env_int("ROSOUT_PROBE_PERIOD_MS", 250);

    if (!rclcpp::rosout::enabled()) {
        printf("ROSOUT_PROBE_NOT_BUILT\n");
        return 3;
    }
    auto init = rclcpp::init_in();
    if (!init.ok()) {
        fprintf(stderr, "rclcpp::init failed: %d\n", init.raw());
        return 1;
    }
    rclcpp::Node node;
    auto created = rclcpp::create_node(node, "rosout_talker");
    if (!created.ok()) {
        fprintf(stderr, "create_node failed: %d\n", created.raw());
        return 1;
    }

    int rc = run(node, records, period_ms);
    rclcpp::shutdown();
    return rc;
}

static int run(rclcpp::Node& node, int records, int period_ms) {
    auto probe = node.get_logger();
    printf("ROSOUT_PROBE_READY\n");

    for (int i = 0; i < records; i++) {
        RCLC_LOG_INFO(probe, "nros rosout probe record %d", i);
        if (i % 2 == 1) {
            RCLC_LOG_WARN(probe, "nros rosout probe warning at %d", i);
        }
        (void)rclcpp::spin_once(period_ms);
    }
    printf("ROSOUT_PROBE_DONE\n");
    return 0;
}
