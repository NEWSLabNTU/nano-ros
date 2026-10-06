/// issue 1589 — the C++ twin of `bins/rosout-talker`.
///
/// Logs through its NODE's logger (RFC-0102 D4: on Humble only node loggers
/// reach `/rosout`) and pumps `nros::rosout::Publisher` from its spin loop,
/// printing the markers the Rust probe prints so `rosout_interop.rs` reads
/// all three languages with one set of assertions.
///
///   ROSOUT_PROBE_RECORDS    records to log (default 60)
///   ROSOUT_PROBE_PERIOD_MS  spin period per record (default 250)

#include <stdio.h>
#include <stdlib.h>

#include <nros/app_main.h>
#include <nros/log.hpp>
#include <nros/nros.hpp>
#include <nros/rosout.hpp>

static int env_int(const char* key, int fallback) {
    const char* v = getenv(key);
    return v != nullptr ? atoi(v) : fallback;
}

static int run(rclcpp::Node& node, int records, int period_ms);

int nros_app_main(int argc, char** argv) {
#ifdef _IOLBF
    setvbuf(stdout, nullptr, _IOLBF, 0);
#endif
    (void)argc;
    (void)argv;
    const int records = env_int("ROSOUT_PROBE_RECORDS", 60);
    const int period_ms = env_int("ROSOUT_PROBE_PERIOD_MS", 250);

    if (!nros::rosout::enabled()) {
        printf("ROSOUT_PROBE_NOT_BUILT\n");
        return 3;
    }
    auto init = nros::init();
    if (!init.ok()) {
        fprintf(stderr, "nros::init failed: %d\n", init.raw());
        return 1;
    }
    rclcpp::Node node;
    auto created = nros::create_node(node, "rosout_talker");
    if (!created.ok()) {
        fprintf(stderr, "create_node failed: %d\n", created.raw());
        return 1;
    }

    int rc = run(node, records, period_ms);
    rclcpp::shutdown();
    return rc;
}

// Its own frame, so the `/rosout` publisher is destroyed BEFORE
// `rclcpp::shutdown()` closes the session it lives on.
static int run(rclcpp::Node& node, int records, int period_ms) {
    nros::rosout::Publisher rosout;
    auto made = rosout.create(node);
    if (!made.ok()) {
        fprintf(stderr, "rosout.create failed: %d\n", made.raw());
        return 1;
    }
    auto probe = node.get_logger();
    if (!nros::rosout::enable().ok()) {
        printf("ROSOUT_PROBE_SINK_FULL\n");
        return 4;
    }
    printf("ROSOUT_PROBE_READY\n");

    size_t total = 0;
    for (int i = 0; i < records; i++) {
        NROS_LOG_INFO(probe, "nros rosout probe record %d", i);
        if (i % 2 == 1) {
            NROS_LOG_WARN(probe, "nros rosout probe warning at %d", i);
        }
        (void)nros::spin_once(period_ms);
        size_t sent = 0;
        auto pumped = rosout.pump(&sent);
        total += sent;
        if (!pumped.ok()) {
            printf("ROSOUT_PROBE_PUBLISH_ERR pumped=%zu ret=%d\n", total, pumped.raw());
            return 5;
        }
        printf("ROSOUT_PROBE_PUMPED %zu\n", total);
    }
    printf("ROSOUT_PROBE_DONE pumped=%zu\n", total);
    return 0;
}

NROS_APP_MAIN_REGISTER()
