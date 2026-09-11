// phase-417 stage 3 — the loudness that only a RUN can answer.
//
// Two ledger rows and one issue, all of whose defects are behaviours rather
// than signatures, so no `-fsyntax-only` probe can reach any of them:
//
//  * `cpp:RCLCPP_FATAL` + issue 1019. The `RCLCPP_*` family expanded to
//    `(void)(logger); NROS_<LEVEL>(...)`, so (a) on every freestanding target
//    the legacy sink is a no-op and a ported node's entire log output was
//    compiled away, (b) the logger was discarded, so per-logger levels applied
//    to none of the family, and (c) `RCLCPP_FATAL` lowered to `NROS_ERROR`
//    because the legacy family has no fatal level. The family routes at
//    `NROS_LOG_*` now; this measures that a record ARRIVES, at the RIGHT
//    SEVERITY, carrying its MESSAGE and its LOGGER NAME.
//
//  * `cpp:Executor::spin_once`, the VALUE half. Upstream's `-1` blocks
//    indefinitely; ours clamped it with `timeout_ms.max(0)` and POLLED. Only
//    the value carries that, so RFC-0089 §"Where the refusal fires" puts the
//    refusal at the call — which means a call is the only thing that can check
//    it. The SIGNATURE half (the no-argument form) is
//    `ros2_refuse_unbounded_spin_probe.cpp`.
//
// Links `libnros_cpp.a` because it must: `nros_log_add_sink` and the dispatcher
// behind `NROS_LOG_*` are the real thing, not a stub. A sink is the only
// observer that can tell "the record reached nros_log at FATAL" from "something
// printed to stderr", which is precisely the distinction issue 1019 is about.
//
// No session, no backend, no router: the logger surface stands alone and the
// executor is never initialised (the refusal fires before the `initialized_`
// check, on purpose — a caller must not have to stand up a session to be told
// its timeout is unsupported).

#include <cstdio>
#include <cstring>

#include <nros/nros.hpp>

namespace {

struct Record {
    nros_log_severity_t severity;
    char logger[64];
    char message[2048];
    bool used;
};

const size_t MAX_RECORDS = 32;
Record g_records[MAX_RECORDS];
size_t g_count = 0;

void copy_bounded(char* dst, size_t dst_len, const char* src, size_t src_len) {
    size_t n = src_len < dst_len - 1 ? src_len : dst_len - 1;
    if (src != nullptr && n > 0) ::memcpy(dst, src, n);
    dst[n] = '\0';
}

extern "C" void capture_sink(void* user_data, nros_log_severity_t severity, const char* logger_name,
                             size_t logger_name_len, const char* message, size_t message_len,
                             const char* file, size_t file_len, uint32_t line,
                             uint64_t timestamp_ns) {
    (void)user_data;
    (void)file;
    (void)file_len;
    (void)line;
    (void)timestamp_ns;
    if (g_count >= MAX_RECORDS) return;
    Record& r = g_records[g_count++];
    r.severity = severity;
    copy_bounded(r.logger, sizeof(r.logger), logger_name, logger_name_len);
    copy_bounded(r.message, sizeof(r.message), message, message_len);
    r.used = true;
}

int failures = 0;

void check(bool cond, const char* what) {
    if (!cond) {
        ::std::fprintf(stderr, "FAIL: %s\n", what);
        ++failures;
    }
}

/// Did any captured record carry `needle` at exactly `severity`?
bool saw(nros_log_severity_t severity, const char* needle) {
    for (size_t i = 0; i < g_count; ++i) {
        if (g_records[i].severity == severity &&
            ::strstr(g_records[i].message, needle) != nullptr) {
            return true;
        }
    }
    return false;
}

/// Did any captured record carrying `needle` name `logger`?
bool saw_from(const char* logger, const char* needle) {
    for (size_t i = 0; i < g_count; ++i) {
        if (::strstr(g_records[i].message, needle) != nullptr &&
            ::strcmp(g_records[i].logger, logger) == 0) {
            return true;
        }
    }
    return false;
}

} // namespace

// The one symbol `libnros_cpp.a` needs from the image: the backend-registration
// hook the generated entry normally provides. Empty on purpose — nothing here
// opens a session, and a registered backend would be a transport this probe
// would then have to tear down.
extern "C" void nros_app_register_backends(void) {}

int main() {
    if (!nros_log_add_sink(&capture_sink, nullptr)) {
        ::std::fprintf(stderr, "FAIL: nros_log_add_sink refused the sink; nothing below can be "
                               "measured, so this is a failure and not a skip\n");
        return 1;
    }

    // --- issue 1019 / cpp:RCLCPP_FATAL --------------------------------------
    //
    // A NAMED logger, because the name is half the defect: the family cast the
    // logger to void, so `rclcpp::get_logger("planner")` selected nothing.
    rclcpp::Logger logger = rclcpp::get_logger("loudness_probe");
    nros_logger_set_level(static_cast<const void*>(logger), NROS_LOG_SEVERITY_TRACE);

    RCLCPP_INFO(logger, "info-marker %d", 1);
    RCLCPP_WARN(logger, "warn-marker %d", 2);
    RCLCPP_ERROR(logger, "error-marker %d", 3);
    RCLCPP_FATAL(logger, "fatal-marker %d", 4);
    RCLCPP_DEBUG(logger, "debug-marker %d", 5);

    check(saw(NROS_LOG_SEVERITY_INFO, "info-marker 1"),
          "RCLCPP_INFO must reach nros_log at INFO with its message");
    check(saw(NROS_LOG_SEVERITY_WARN, "warn-marker 2"),
          "RCLCPP_WARN must reach nros_log at WARN with its message");
    check(saw(NROS_LOG_SEVERITY_ERROR, "error-marker 3"),
          "RCLCPP_ERROR must reach nros_log at ERROR with its message");
    // The row itself: FATAL is a DISTINCT severity and must not lower to ERROR.
    check(saw(NROS_LOG_SEVERITY_FATAL, "fatal-marker 4"),
          "RCLCPP_FATAL must emit at NROS_LOG_SEVERITY_FATAL, not lower to ERROR");
    check(!saw(NROS_LOG_SEVERITY_ERROR, "fatal-marker 4"),
          "RCLCPP_FATAL must not ALSO appear at ERROR");
    check(saw(NROS_LOG_SEVERITY_DEBUG, "debug-marker 5"),
          "RCLCPP_DEBUG must reach nros_log at DEBUG");

    // The logger is CARRIED, not discarded — issue 1019 defect 3.
    check(saw_from("loudness_probe", "fatal-marker 4"),
          "the record must name the logger RCLCPP_FATAL was handed, not the catch-all");

    // A logger with no handle behind it (a default-constructed `rclcpp::Logger`)
    // must still deliver. Routing at a dispatcher that returns early on a null
    // handle would have swapped one silent drop for another.
    rclcpp::Logger anonymous;
    RCLCPP_ERROR(anonymous, "anonymous-marker %d", 6);
    check(saw(NROS_LOG_SEVERITY_ERROR, "anonymous-marker 6"),
          "a Logger with a null handle must fall back to the catch-all logger, not drop");

#if defined(NROS_CPP_HAS_STD_SSTREAM)
    // issue 1019 defect 2 — the stream family used to expand to `"%s", ""`.
    RCLCPP_FATAL_STREAM(logger, "stream-marker " << 7);
    check(saw(NROS_LOG_SEVERITY_FATAL, "stream-marker 7"),
          "RCLCPP_FATAL_STREAM must carry its message at FATAL");
#endif

    // --- cpp:Executor::spin_once, the VALUE half ----------------------------
    size_t before = g_count;
    nros::Executor exec;
    nros::Result r = exec.spin_once(-1);
    check(r.code() == nros::ErrorCode::Unsupported,
          "spin_once(-1) must return ErrorCode::Unsupported, not poll with a 0 ms budget");
    check(r.code() != nros::ErrorCode::NotInitialized,
          "the -1 refusal must fire BEFORE the initialized_ check — the defect is in the value, "
          "and a caller must not have to open a session to be told");
    bool said_so = false;
    bool named_the_alternative = false;
    for (size_t i = before; i < g_count; ++i) {
        if (::strstr(g_records[i].message, "REFUSED by nano-ros") != nullptr) said_so = true;
        // The ALTERNATIVE, not just the constraint. nros_log's format buffer
        // DROPS a body that does not fit rather than truncating it, so a long
        // runtime refusal reaches the console as a lone ellipsis — which is why
        // the emitted form is short and `RUNTIME_REFUSAL_MAX` keeps it that way.
        if (::strstr(g_records[i].message, "spin_once(ms)") != nullptr) {
            named_the_alternative = true;
        }
    }
    check(said_so, "spin_once(-1) must SAY SO — a bare error code is not the refusal RFC-0089 "
                   "asks for");
    check(named_the_alternative,
          "the emitted refusal must name the ALTERNATIVE, and must be short enough to survive "
          "nros_log's format buffer (rclcpp::detail::RUNTIME_REFUSAL_MAX)");

    if (failures != 0) {
        ::std::fprintf(stderr, "ros2_loudness_runtime: %d failure(s), %zu record(s) captured\n",
                       failures, g_count);
        return 1;
    }
    ::std::printf("ros2_loudness_runtime: OK (%zu records)\n", g_count);
    return 0;
}
