// issue 2053 — `diagnostic_updater`'s task classes, RUN on the real
// `libnros_cpp.a` + the stub backend.
//
//   1. a `FrequencyStatus` ticked by a 50 Hz timer reports OK inside its band,
//      WARN "Frequency too low." under it and "Frequency too high." over it,
//      and ERROR "No events recorded." with nothing ticked;
//   2. a `TimeStampStatus` reports OK for a current stamp, ERROR for one ten
//      seconds old, and WARN with no stamp since the last run;
//   3. `Heartbeat` reports OK "Alive";
//   4. a `CompositeDiagnosticTask` reports its worst child;
//   5. `Updater::add(DiagnosticTask&)` runs the task on every publish, and a
//      `HeaderlessTopicDiagnostic` adds itself.
//
// The rate bands are wide (the "inside" band is 20-100 Hz around a 50 Hz
// timer) because the test runs beside a parallel build; what is asserted is
// which side of a band the rate falls on, never the rate.

#include <diagnostic_updater/diagnostic_updater.hpp>
#include <diagnostic_updater/publisher.hpp>
#include <diagnostic_updater/update_functions.hpp>
#include <rclcpp/rclcpp.hpp>

extern "C" {
#include "stub_rmw_backend.h"
}

#include <cstdio>
#include <cstring>
#include <ctime>
#include <memory>
#include <string>

extern "C" void nros_app_register_backends(void);
extern "C" void nros_app_register_backends(void) {
    nros_stub_rmw_set_accept_entities(true);
    nros_stub_rmw_set_entity_wake_supported(false);
    (void)nros_stub_rmw_register();
}

namespace {

int g_failures = 0;

void check(bool ok, const char* what) {
    if (!ok) {
        ++g_failures;
        std::fprintf(stderr, "FAIL: %s\n", what);
    }
}

template <typename P> bool spin_until(const std::shared_ptr<rclcpp::Node>& node, P pred) {
    for (int i = 0; i < 400; ++i) {
        rclcpp::spin_some(node);
        if (pred()) return true;
        struct timespec ts = {0, 5 * 1000 * 1000};
        nanosleep(&ts, nullptr);
    }
    return false;
}

bool reports(diagnostic_updater::DiagnosticTask& task, uint8_t level, const char* message) {
    diagnostic_updater::DiagnosticStatusWrapper w;
    task.run(w);
    if (w.level != level || std::strcmp(w.message.c_str(), message) != 0) {
        std::fprintf(stderr, "  %s reported level %d \"%s\", expected %d \"%s\"\n",
                     task.getName().c_str(), static_cast<int>(w.level), w.message.c_str(),
                     static_cast<int>(level), message);
        return false;
    }
    return true;
}

/// A task that counts its runs, to see the updater call it.
class CountingTask : public diagnostic_updater::DiagnosticTask {
  public:
    CountingTask() : DiagnosticTask("counting") {}
    void run(diagnostic_updater::DiagnosticStatusWrapper& stat) override {
        ++runs;
        stat.summary(diagnostic_updater::OK, "counted");
    }
    int runs = 0;
};

} // namespace

int main(int argc, char** argv) {
    rclcpp::init(argc, argv);
    {
        auto node = std::make_shared<rclcpp::Node>("diag_tasks");

        double inside_min = 20.0, inside_max = 100.0;
        double low_min = 1000.0, low_max = 1000.0;
        double high_min = 1.0, high_max = 2.0;
        diagnostic_updater::FrequencyStatus inside(
            diagnostic_updater::FrequencyStatusParam(&inside_min, &inside_max), "inside");
        diagnostic_updater::FrequencyStatus low(
            diagnostic_updater::FrequencyStatusParam(&low_min, &low_max), "low");
        diagnostic_updater::FrequencyStatus high(
            diagnostic_updater::FrequencyStatusParam(&high_min, &high_max), "high");
        diagnostic_updater::FrequencyStatus idle(
            diagnostic_updater::FrequencyStatusParam(&inside_min, &inside_max), "idle");

        int ticks = 0;
        auto timer =
            node->create_wall_timer(rclcpp::Duration::from_nanoseconds(20 * 1000 * 1000), [&]() {
                ++ticks;
                inside.tick();
                low.tick();
                high.tick();
            });
        check(spin_until(node, [&]() { return ticks >= 15; }), "the 50 Hz timer fires");
        timer.reset();

        // (1)
        check(reports(inside, diagnostic_updater::OK, "Desired frequency met"),
              "FrequencyStatus: a rate inside the band is OK");
        check(reports(low, diagnostic_updater::WARN, "Frequency too low."),
              "FrequencyStatus: a rate under the band is WARN");
        check(reports(high, diagnostic_updater::WARN, "Frequency too high."),
              "FrequencyStatus: a rate over the band is WARN");
        check(reports(idle, diagnostic_updater::ERROR, "No events recorded."),
              "FrequencyStatus: nothing ticked is ERROR");
        {
            diagnostic_updater::DiagnosticStatusWrapper w;
            high.run(w);
            // The stub message's sequence holds 4; upstream reports up to 7.
            check(w.values.length() == 4, "FrequencyStatus reports upstream's key/value pairs");
        }

        // (2)
        diagnostic_updater::TimeStampStatus stamps(diagnostic_updater::TimeStampStatusParam(-1, 5),
                                                   "stamps", node->get_clock());
        check(reports(stamps, diagnostic_updater::WARN, "No data since last update."),
              "TimeStampStatus: no stamp is WARN");
        stamps.tick(node->now());
        check(reports(stamps, diagnostic_updater::OK, "Timestamps are reasonable."),
              "TimeStampStatus: a current stamp is OK");
        stamps.tick(node->now() - rclcpp::Duration::from_nanoseconds(10LL * 1000 * 1000 * 1000));
        check(reports(stamps, diagnostic_updater::ERROR, "Timestamps too far in past seen."),
              "TimeStampStatus: a stamp ten seconds old is ERROR");

        // (3)
        diagnostic_updater::Heartbeat heartbeat;
        check(reports(heartbeat, diagnostic_updater::OK, "Alive"), "Heartbeat is OK");

        // (4) Heartbeat (OK) + idle FrequencyStatus (ERROR): the worst wins, and
        // its message replaces the OK child's.
        diagnostic_updater::CompositeDiagnosticTask composite("composite");
        composite.addTask(&heartbeat);
        composite.addTask(&idle);
        check(reports(composite, diagnostic_updater::ERROR, "No events recorded."),
              "CompositeDiagnosticTask reports its worst child");

        // (5)
        CountingTask counting;
        diagnostic_updater::Updater updater(node, 60.0);
        updater.add(counting);
        double topic_min = 0.0, topic_max = 1000.0;
        diagnostic_updater::HeaderlessTopicDiagnostic topic(
            "/out", updater, diagnostic_updater::FrequencyStatusParam(&topic_min, &topic_max));
        topic.tick();
        const uint32_t before = nros_stub_rmw_publish_count();
        updater.force_update();
        check(counting.runs == 1, "Updater::add(DiagnosticTask&) runs the task on a publish");
        check(nros_stub_rmw_publish_count() == before + 1, "the updater publishes the array");
        updater.removeByName("counting");
        updater.force_update();
        check(counting.runs == 1, "a removed task is not run");
    }
    rclcpp::shutdown();
    if (g_failures != 0) {
        std::fprintf(stderr, "diagnostic_tasks_runtime: %d failure(s)\n", g_failures);
        return 1;
    }
    std::printf("diagnostic_tasks_runtime: OK\n");
    return 0;
}
