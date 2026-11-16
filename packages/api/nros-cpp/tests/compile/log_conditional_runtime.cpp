// GitHub #4010 -- the conditional `RCLCPP_*` families: `_THROTTLE`,
// `_SKIPFIRST_THROTTLE`, `_ONCE` and `_SKIPFIRST`, at every level.
//
// Built TWICE by `just check cpp`, with and without `-DNROS_CPP_STD=1`, because
// the per-site state must work in the configuration a freestanding image
// compiles. Observed through a real `rclc_log_add_sink`, like
// `log_stream_runtime`: a record that reaches the sink is what a user reads.
//
// What it holds:
//   1. THROTTLE measures its window on the `clock` ARGUMENT: a ROS-time clock
//      driven by the override (so the test controls time exactly) logs once,
//      is suppressed inside the window, and logs again once the clock -- not
//      the wall -- has moved past it.
//   2. ...and on a real steady clock: suppressed immediately, admitted after
//      the window has elapsed in wall time.
//   3. ONCE logs exactly once over N calls; SKIPFIRST drops only the first;
//      SKIPFIRST_THROTTLE drops the first admitted record and keeps the window.
//   4. A record the level filters consumes no ONCE.
//   5. An invalid clock is named once per site, at WARN, and the site is still
//      throttled, on the platform's monotonic clock: neither silenced nor
//      flooded.
//   6. Every one of the twenty macros emits at its own severity.

#include <cstdio>
#include <cstring>

#include <rclcpp/rclcpp.hpp>

namespace {

struct Captured {
    rcl_log_severity_t severity;
    char message[160];
};

Captured g_records[256];
size_t g_count = 0;

extern "C" void capture_sink(void*, rcl_log_severity_t severity, const char*, size_t,
                             const char* message, size_t message_len, const char*, size_t, uint32_t,
                             uint64_t) {
    if (g_count >= sizeof(g_records) / sizeof(g_records[0])) {
        return;
    }
    Captured& slot = g_records[g_count++];
    const size_t n =
        message_len < sizeof(slot.message) - 1 ? message_len : sizeof(slot.message) - 1;
    ::memcpy(slot.message, message, n);
    slot.message[n] = '\0';
    slot.severity = severity;
}

/// Records whose text contains `needle`.
size_t count(const char* needle) {
    size_t n = 0;
    for (size_t i = 0; i < g_count; ++i) {
        if (::strstr(g_records[i].message, needle) != nullptr) {
            ++n;
        }
    }
    return n;
}

int failures = 0;

void expect(size_t got, size_t want, const char* what) {
    if (got != want) {
        ::std::fprintf(stderr, "FAIL: %s -- want %zu record(s), got %zu\n", what, want, got);
        ++failures;
    }
}

rcl_clock_t g_sim;
const int64_t MS = 1000 * 1000;

void set_sim(int64_t ns) {
    (void)rcl_set_ros_time_override(&g_sim, ns);
}

void throttle_sim_site(const rclcpp::Logger& logger, const rclcpp::Clock& clock) {
    RCLCPP_WARN_THROTTLE(logger, clock, 100, "sim-throttle %d", 1);
}

void skipfirst_throttle_sim_site(const rclcpp::Logger& logger, const rclcpp::Clock& clock) {
    RCLCPP_INFO_SKIPFIRST_THROTTLE(logger, clock, 100, "sim-skipfirst-throttle");
}

void steady_site(const rclcpp::Logger& logger, const rclcpp::Clock& clock) {
    RCLCPP_ERROR_THROTTLE(logger, clock, 20, "steady-throttle");
}

void invalid_site(const rclcpp::Logger& logger, const rclcpp::Clock& clock) {
    RCLCPP_INFO_THROTTLE(logger, clock, 30, "invalid-clock-record");
}

} // namespace

extern "C" void nros_app_register_backends(void) {}

int main() {
    if (!rclc_log_add_sink(&capture_sink, nullptr)) {
        ::std::fprintf(stderr, "FAIL: rclc_log_add_sink refused the sink\n");
        return 1;
    }
    rclcpp::Logger logger = rclcpp::get_logger("conditional_probe");
    rclc_logger_set_level(static_cast<const void*>(logger), RCLC_LOG_SEVERITY_TRACE);

    // 1 -- THROTTLE on a clock the test drives. Frozen at t0 = 10 s.
    g_sim = rclc_clock_get_zero_initialized();
    if (rcl_clock_init(RCL_ROS_TIME, &g_sim, nullptr) != RCL_RET_OK) {
        ::std::fprintf(stderr, "FAIL: ROS clock init\n");
        return 1;
    }
    set_sim(10000 * MS);
    rclcpp::Clock ros(RCL_ROS_TIME);
    if (!ros.ros_time_is_active()) {
        ::std::fprintf(stderr, "FAIL: the override does not drive the ROS clock\n");
        return 1;
    }
    throttle_sim_site(logger, ros);
    expect(count("sim-throttle"), 1, "THROTTLE: the first call logs");
    for (int i = 0; i < 5; ++i) {
        throttle_sim_site(logger, ros);
    }
    expect(count("sim-throttle"), 1, "THROTTLE: suppressed with the clock frozen");
    set_sim(10099 * MS);
    throttle_sim_site(logger, ros);
    expect(count("sim-throttle"), 1, "THROTTLE: suppressed 1 ms inside the window");
    set_sim(10100 * MS);
    throttle_sim_site(logger, ros);
    expect(count("sim-throttle"), 2, "THROTTLE: logs again once the clock reaches the window");
    throttle_sim_site(logger, ros);
    expect(count("sim-throttle"), 2, "THROTTLE: the window re-arms from the emitted record");

    // 3 -- SKIPFIRST_THROTTLE on the same clock.
    skipfirst_throttle_sim_site(logger, ros);
    skipfirst_throttle_sim_site(logger, ros);
    expect(count("sim-skipfirst-throttle"), 0, "SKIPFIRST_THROTTLE: the first admitted is dropped");
    set_sim(10200 * MS);
    skipfirst_throttle_sim_site(logger, ros);
    expect(count("sim-skipfirst-throttle"), 1, "SKIPFIRST_THROTTLE: the next window's record logs");
    (void)rcl_disable_ros_time_override(&g_sim);

    // 2 -- a real steady clock, and the node-clock shape on system time.
    {
        rclcpp::Clock steady(RCL_STEADY_TIME);
        steady_site(logger, steady);
        steady_site(logger, steady);
        expect(count("steady-throttle"), 1, "steady THROTTLE: suppressed inside the window");
        const int64_t t0 = steady.now().nanoseconds();
        while (steady.now().nanoseconds() - t0 < 30 * MS) {
        }
        steady_site(logger, steady);
        expect(count("steady-throttle"), 2, "steady THROTTLE: logs after the window elapses");

        rclcpp::Clock node_clock(RCL_ROS_TIME); // no override: system time
        for (int i = 0; i < 3; ++i) {
            RCLCPP_WARN_THROTTLE(logger, node_clock, 10000, "node-clock-throttle");
        }
        expect(count("node-clock-throttle"), 1, "ROS-time clock on system time is throttled");
    }

    // 3 -- ONCE and SKIPFIRST.
    for (int i = 0; i < 10; ++i) {
        RCLCPP_INFO_ONCE(logger, "once-record %d", i);
    }
    expect(count("once-record"), 1, "ONCE: exactly one record over 10 calls");
    expect(count("once-record 0"), 1, "ONCE: and it is the first call's");
    for (int i = 0; i < 5; ++i) {
        RCLCPP_WARN_SKIPFIRST(logger, "skipfirst-record");
    }
    expect(count("skipfirst-record"), 4, "SKIPFIRST: every call but the first");

    // 4 -- a filtered record does not consume ONCE.
    for (int i = 0; i < 3; ++i) {
        rclc_logger_set_level(static_cast<const void*>(logger),
                              i == 0 ? RCLC_LOG_SEVERITY_WARN : RCLC_LOG_SEVERITY_TRACE);
        RCLCPP_INFO_ONCE(logger, "filtered-once %d", i);
    }
    expect(count("filtered-once"), 1, "ONCE after a filtered call: one record");
    expect(count("filtered-once 1"), 1, "...and it is the first ENABLED call's");

    // 5 -- an invalid clock is named once, at WARN, and the site is still
    // throttled: on the platform's monotonic clock, not let through.
    {
        rclcpp::Clock bad(RCL_CLOCK_UNINITIALIZED);
        if (bad.is_valid()) {
            ::std::fprintf(stderr, "FAIL: an RCL_CLOCK_UNINITIALIZED clock reads valid\n");
            ++failures;
        }
        for (int i = 0; i < 5; ++i) {
            invalid_site(logger, bad);
        }
        expect(count("the clock is invalid"), 1, "invalid clock: named once per site");
        expect(count("invalid-clock-record"), 1, "invalid clock: still throttled, not flooded");
        if (rclc_log_timestamp_available()) {
            rclcpp::Clock steady(RCL_STEADY_TIME);
            const int64_t t0 = steady.now().nanoseconds();
            while (steady.now().nanoseconds() - t0 < 40 * MS) {
            }
            invalid_site(logger, bad);
            expect(count("invalid-clock-record"), 2,
                   "invalid clock: the monotonic window elapses and the site logs again");
        }
        for (size_t i = 0; i < g_count; ++i) {
            if (::strstr(g_records[i].message, "the clock is invalid") != nullptr &&
                g_records[i].severity != RCLC_LOG_SEVERITY_WARN) {
                ::std::fprintf(stderr, "FAIL: the invalid-clock notice is not at WARN\n");
                ++failures;
            }
        }
    }

    // 6 -- every level of every family, at its own severity.
    {
        rclcpp::Clock steady(RCL_STEADY_TIME);
        const size_t before = g_count;
        for (int i = 0; i < 2; ++i) {
            RCLCPP_DEBUG_ONCE(logger, "lvl");
            RCLCPP_INFO_ONCE(logger, "lvl");
            RCLCPP_WARN_ONCE(logger, "lvl");
            RCLCPP_ERROR_ONCE(logger, "lvl");
            RCLCPP_FATAL_ONCE(logger, "lvl");
            RCLCPP_DEBUG_SKIPFIRST(logger, "lvl");
            RCLCPP_INFO_SKIPFIRST(logger, "lvl");
            RCLCPP_WARN_SKIPFIRST(logger, "lvl");
            RCLCPP_ERROR_SKIPFIRST(logger, "lvl");
            RCLCPP_FATAL_SKIPFIRST(logger, "lvl");
            RCLCPP_DEBUG_THROTTLE(logger, steady, 10000, "lvl");
            RCLCPP_INFO_THROTTLE(logger, steady, 10000, "lvl");
            RCLCPP_WARN_THROTTLE(logger, steady, 10000, "lvl");
            RCLCPP_ERROR_THROTTLE(logger, steady, 10000, "lvl");
            RCLCPP_FATAL_THROTTLE(logger, steady, 10000, "lvl");
            RCLCPP_DEBUG_SKIPFIRST_THROTTLE(logger, steady, 0, "lvl");
            RCLCPP_INFO_SKIPFIRST_THROTTLE(logger, steady, 0, "lvl");
            RCLCPP_WARN_SKIPFIRST_THROTTLE(logger, steady, 0, "lvl");
            RCLCPP_ERROR_SKIPFIRST_THROTTLE(logger, steady, 0, "lvl");
            RCLCPP_FATAL_SKIPFIRST_THROTTLE(logger, steady, 0, "lvl");
        }
        // Per level: ONCE 1 + SKIPFIRST 1 + THROTTLE 1 + SKIPFIRST_THROTTLE 1.
        const rcl_log_severity_t levels[5] = {RCLC_LOG_SEVERITY_DEBUG, RCLC_LOG_SEVERITY_INFO,
                                              RCLC_LOG_SEVERITY_WARN, RCLC_LOG_SEVERITY_ERROR,
                                              RCLC_LOG_SEVERITY_FATAL};
        for (size_t l = 0; l < 5; ++l) {
            size_t n = 0;
            for (size_t i = before; i < g_count; ++i) {
                if (g_records[i].severity == levels[l] &&
                    ::strcmp(g_records[i].message, "lvl") == 0) {
                    ++n;
                }
            }
            if (n != 4) {
                ::std::fprintf(stderr, "FAIL: severity %d -- want 4 records, got %zu\n",
                               static_cast<int>(levels[l]), n);
                ++failures;
            }
        }
    }

    if (failures != 0) {
        ::std::fprintf(stderr, "log_conditional_runtime: %d failure(s)\n", failures);
        return 1;
    }
#if defined(NROS_CPP_STD)
    ::std::printf("log_conditional_runtime (NROS_CPP_STD): OK\n");
#else
    ::std::printf("log_conditional_runtime (no NROS_CPP_STD): OK\n");
#endif
    return 0;
}
