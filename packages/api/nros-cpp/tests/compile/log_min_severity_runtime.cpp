// Issue 2881 -- the compile-time log floor: `RCLCPP_LOG_MIN_SEVERITY` (upstream's
// symbol and six values) and its C twin `RCLC_LOG_MIN_SEVERITY`.
//
// Built by `just check cpp` once per floor arm (none, `RCLCPP_..._INFO`,
// `RCLCPP_..._NONE`, `RCLC_..._WARN`), with and without `NROS_CPP_STD`, under
// `-Wall -Wextra -Werror`. The test reads the floor the arm compiled with and
// holds, for every level of every family:
//
//   1. BELOW the floor the macro is compiled out: none of its operands -- the
//      logger, the clock, the format arguments, the stream operands -- is
//      evaluated, and no record reaches the sink.
//   2. AT OR ABOVE the floor it evaluates its operands and logs, at its own
//      severity, as before.
//
// Operands are counted through functions with external linkage, so an
// evaluation the floor failed to remove shows as a count, not as a warning.
// The runtime filters are opened wide (the logger's level is TRACE and the
// image's `log.max_level` is the builtin `trace`), so a missing record can
// only be the floor's doing.

#include <cstdio>
#include <cstring>

#include <rclcpp/rclcpp.hpp>

namespace {

struct Captured {
    rcl_log_severity_t severity;
};

Captured g_records[512];
size_t g_count = 0;

extern "C" void capture_sink(void*, rcl_log_severity_t severity, const char*, size_t, const char*,
                             size_t, const char*, size_t, uint32_t, uint64_t) {
    if (g_count < sizeof(g_records) / sizeof(g_records[0])) {
        g_records[g_count++].severity = severity;
    }
}

int failures = 0;

void expect(long got, long want, const char* what) {
    if (got != want) {
        ::std::fprintf(stderr, "FAIL: %s -- want %ld, got %ld\n", what, want, got);
        ++failures;
    }
}

} // namespace

// External linkage on purpose: a compiled-out call site must not leave these
// unused-and-static under -Werror, and the counts must be observable.
long g_evals = 0;
rclcpp::Logger* g_logger = nullptr;
rclcpp::Clock* g_clock = nullptr;

int arg(int v) {
    ++g_evals;
    return v;
}
const rclcpp::Logger& lg() {
    ++g_evals;
    return *g_logger;
}
const rclcpp::Clock& clk() {
    ++g_evals;
    return *g_clock;
}
rclc_logger_t c_lg() {
    ++g_evals;
    return static_cast<rclc_logger_t>(*g_logger);
}

extern "C" void nros_app_register_backends(void) {}

// Runs `body` once and checks what it cost and what it emitted.
#define NROS_TEST_SITE(name, kept, sev, evals_when_kept, records_when_kept, body)                  \
    do {                                                                                           \
        const long evals0 = g_evals;                                                               \
        const size_t count0 = g_count;                                                             \
        body;                                                                                      \
        size_t at_sev = 0;                                                                         \
        for (size_t i = count0; i < g_count; ++i) {                                                \
            at_sev += g_records[i].severity == (sev) ? 1u : 0u;                                    \
        }                                                                                          \
        expect(g_evals - evals0, (kept) ? (evals_when_kept) : 0, name ": operands evaluated");     \
        expect(static_cast<long>(g_count - count0), (kept) ? (records_when_kept) : 0,              \
               name ": records");                                                                  \
        expect(static_cast<long>(at_sev), (kept) ? (records_when_kept) : 0,                        \
               name ": records at its own severity");                                              \
    } while (0)

// Every family at one level. Operand counts per call, when kept: logger 1,
// clock 1, one format/stream argument 1 -- except that the C throttle names
// its logger twice (the level test and the emit), and a record a site drops
// (`_SKIPFIRST`'s first) never reaches its format arguments. `_SKIPFIRST` and
// `_SKIPFIRST_THROTTLE` run twice so one record is expected; `_ONCE` and
// `_THROTTLE` run once.
#define NROS_TEST_LEVEL(LVL, kept, sev)                                                            \
    do {                                                                                           \
        NROS_TEST_SITE("RCLCPP_" #LVL, kept, sev, 2, 1, RCLCPP_##LVL(lg(), "v=%d", arg(1)));       \
        NROS_TEST_SITE("RCLCPP_" #LVL "_ONCE", kept, sev, 2, 1,                                    \
                       RCLCPP_##LVL##_ONCE(lg(), "v=%d", arg(1)));                                 \
        NROS_TEST_SITE(                                                                            \
            "RCLCPP_" #LVL "_SKIPFIRST", kept, sev, 3, 1,                                          \
            for (int k = 0; k < 2; ++k) { RCLCPP_##LVL##_SKIPFIRST(lg(), "v=%d", arg(k)); });      \
        NROS_TEST_SITE("RCLCPP_" #LVL "_THROTTLE", kept, sev, 3, 1,                                \
                       RCLCPP_##LVL##_THROTTLE(lg(), clk(), 10000, "v=%d", arg(1)));               \
        NROS_TEST_SITE(                                                                            \
            "RCLCPP_" #LVL "_SKIPFIRST_THROTTLE", kept, sev, 5, 1, for (int k = 0; k < 2; ++k) {   \
                RCLCPP_##LVL##_SKIPFIRST_THROTTLE(lg(), clk(), 0, "v=%d", arg(k));                 \
            });                                                                                    \
        NROS_TEST_SITE("RCLCPP_" #LVL "_STREAM", kept, sev, 2, 1,                                  \
                       RCLCPP_##LVL##_STREAM(lg(), "v=" << arg(1)));                               \
        NROS_TEST_SITE("RCLC_LOG_" #LVL, kept_c_##LVL, sev, 2, 1,                                  \
                       RCLC_LOG_##LVL(c_lg(), "v=%d", arg(1)));                                    \
        NROS_TEST_SITE("RCLC_LOG_" #LVL "_THROTTLE", kept_c_##LVL, sev, 3, 1,                      \
                       RCLC_LOG_##LVL##_THROTTLE(c_lg(), 10000, "v=%d", arg(1)));                  \
    } while (0)

int main() {
    if (!rclc_log_add_sink(&capture_sink, nullptr)) {
        ::std::fprintf(stderr, "FAIL: rclc_log_add_sink refused the sink\n");
        return 1;
    }
    rclcpp::Logger logger = rclcpp::get_logger("min_severity_probe");
    rclc_logger_set_level(static_cast<const void*>(logger), RCLC_LOG_SEVERITY_TRACE);
    rclcpp::Clock steady(RCL_STEADY_TIME);
    g_logger = &logger;
    g_clock = &steady;

    // What the arm compiled with. A RCLCPP_* level survives only at or above
    // both floors; a RCLC_LOG_* level only at or above the C one.
    const int cpp_floor = RCLCPP_LOG_MIN_SEVERITY;
    const int c_floor = RCLC_LOG_MIN_SEVERITY;
    const bool kept_c_TRACE = c_floor <= RCLC_LOG_MIN_SEVERITY_TRACE;
    const bool kept_c_DEBUG = c_floor <= RCLC_LOG_MIN_SEVERITY_DEBUG;
    const bool kept_c_INFO = c_floor <= RCLC_LOG_MIN_SEVERITY_INFO;
    const bool kept_c_WARN = c_floor <= RCLC_LOG_MIN_SEVERITY_WARN;
    const bool kept_c_ERROR = c_floor <= RCLC_LOG_MIN_SEVERITY_ERROR;
    const bool kept_c_FATAL = c_floor <= RCLC_LOG_MIN_SEVERITY_FATAL;

    NROS_TEST_LEVEL(DEBUG, cpp_floor <= RCLCPP_LOG_MIN_SEVERITY_DEBUG && kept_c_DEBUG,
                    RCLC_LOG_SEVERITY_DEBUG);
    NROS_TEST_LEVEL(INFO, cpp_floor <= RCLCPP_LOG_MIN_SEVERITY_INFO && kept_c_INFO,
                    RCLC_LOG_SEVERITY_INFO);
    NROS_TEST_LEVEL(WARN, cpp_floor <= RCLCPP_LOG_MIN_SEVERITY_WARN && kept_c_WARN,
                    RCLC_LOG_SEVERITY_WARN);
    NROS_TEST_LEVEL(ERROR, cpp_floor <= RCLCPP_LOG_MIN_SEVERITY_ERROR && kept_c_ERROR,
                    RCLC_LOG_SEVERITY_ERROR);
    NROS_TEST_LEVEL(FATAL, cpp_floor <= RCLCPP_LOG_MIN_SEVERITY_FATAL && kept_c_FATAL,
                    RCLC_LOG_SEVERITY_FATAL);
    // TRACE is the C API's own level (rcutils and rclcpp have none).
    NROS_TEST_SITE("RCLC_LOG_TRACE", kept_c_TRACE, RCLC_LOG_SEVERITY_TRACE, 2, 1,
                   RCLC_LOG_TRACE(c_lg(), "v=%d", arg(1)));
    NROS_TEST_SITE("RCLC_LOG_TRACE_THROTTLE", kept_c_TRACE, RCLC_LOG_SEVERITY_TRACE, 3, 1,
                   RCLC_LOG_TRACE_THROTTLE(c_lg(), 10000, "v=%d", arg(1)));

    // The issue's own line: below the floor, `f` is not called.
    {
        const long evals0 = g_evals;
        RCLCPP_DEBUG(logger, "%d", arg(7));
        expect(g_evals - evals0, cpp_floor <= RCLCPP_LOG_MIN_SEVERITY_DEBUG && kept_c_DEBUG ? 1 : 0,
               "RCLCPP_DEBUG(logger, \"%d\", f()) calls f only at or above the floor");
    }

    if (failures != 0) {
        ::std::fprintf(stderr, "log_min_severity_runtime (rclcpp %d, rclc %d): %d failure(s)\n",
                       cpp_floor, c_floor, failures);
        return 1;
    }
    ::std::printf("log_min_severity_runtime (rclcpp floor %d, rclc floor %d%s): OK\n", cpp_floor,
                  c_floor,
#if defined(NROS_CPP_STD)
                  ", NROS_CPP_STD"
#else
                  ""
#endif
    );
    return 0;
}
