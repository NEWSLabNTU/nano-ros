// phase-476 W4 — the `RCLCPP_*_STREAM` family exists on EVERY target and
// formats builtins the same way on every target.
//
// Built TWICE by `just check cpp`: once with `-DNROS_CPP_STD=1` (where the
// hosted interop overload for user types exists) and once without it, the
// configuration a freestanding image compiles. Both runs assert the same
// builtin text, so a difference between the two is a failure here rather than
// a surprise on a board. The hosted run also asserts that the text matches
// what `std::ostringstream` prints for the same values, which is the promise
// the formatter makes.
//
// Observed through a real `nros_log_add_sink`, like `ros2_loudness_runtime`:
// the record that reaches the logger is the thing a user reads.

#include <cstdio>
#include <cstring>

#include <nros/nros.hpp>

#if defined(NROS_CPP_STD)
#include <sstream>
#include <string>
#endif

namespace {

char g_last[512];
size_t g_last_len = 0;

extern "C" void capture_sink(void*, nros_log_severity_t, const char*, size_t, const char* message,
                             size_t message_len, const char*, size_t, uint32_t, uint64_t) {
    const size_t n = message_len < sizeof(g_last) - 1 ? message_len : sizeof(g_last) - 1;
    ::memcpy(g_last, message, n);
    g_last[n] = '\0';
    g_last_len = n;
}

int failures = 0;

void expect(const char* want, const char* what) {
    if (::strcmp(g_last, want) != 0) {
        ::std::fprintf(stderr, "FAIL: %s\n  want \"%s\"\n  got  \"%s\"\n", what, want, g_last);
        ++failures;
    }
}

#if defined(NROS_CPP_STD)
struct Point {
    int x;
    int y;
};
::std::ostream& operator<<(::std::ostream& os, const Point& p) {
    return os << "(" << p.x << "," << p.y << ")";
}
#endif

} // namespace

extern "C" void nros_app_register_backends(void) {}

int main() {
    if (!nros_log_add_sink(&capture_sink, nullptr)) {
        ::std::fprintf(stderr, "FAIL: nros_log_add_sink refused the sink\n");
        return 1;
    }
    rclcpp::Logger logger = rclcpp::get_logger("stream_probe");
    nros_logger_set_level(static_cast<const void*>(logger), NROS_LOG_SEVERITY_TRACE);

    RCLCPP_INFO_STREAM(logger, "i=" << 42 << " u=" << 7u << " neg=" << -3 << " ll="
                                    << 1234567890123LL << " s=" << static_cast<short>(-5));
    expect("i=42 u=7 neg=-3 ll=1234567890123 s=-5", "integers");

    RCLCPP_WARN_STREAM(logger, 3.14159265 << " " << 0.1f << " " << 1e21 << " " << 2.0);
    expect("3.14159 0.1 1e+21 2", "floating point at std::ostream's default precision");

    RCLCPP_ERROR_STREAM(logger, true << false << ' ' << 'x');
    expect("10 x", "bool as 1/0 and char verbatim");

    ::nros::FixedString<16> fs;
    fs = "fixed";
    RCLCPP_DEBUG_STREAM(logger, "fs=" << fs << " pct=100%d%s");
    expect("fs=fixed pct=100%d%s", "FixedString, and a % in the text is never a conversion");

    char long_text[400];
    ::memset(long_text, 'a', sizeof(long_text) - 1);
    long_text[sizeof(long_text) - 1] = '\0';
    RCLCPP_FATAL_STREAM(logger, long_text);
    if (g_last_len != NROS_LOG_FMT_BUFFER_SIZE - 1 ||
        ::strcmp(g_last + g_last_len - 3, "...") != 0) {
        ::std::fprintf(stderr, "FAIL: truncation — %zu bytes, tail \"%s\"\n", g_last_len,
                       g_last_len >= 3 ? g_last + g_last_len - 3 : g_last);
        ++failures;
    }

#if defined(NROS_CPP_STD)
    // The formatter's promise, measured against the thing it replaces.
    {
        ::std::ostringstream ref;
        ref << 3.14159265 << " " << 0.1f << " " << 1e21 << " " << 2.0;
        RCLCPP_INFO_STREAM(logger, 3.14159265 << " " << 0.1f << " " << 1e21 << " " << 2.0);
        expect(ref.str().c_str(), "doubles render exactly as std::ostream's defaults");
    }
    // Hosted interop: a user type with its own operator<<, and std::string.
    RCLCPP_INFO_STREAM(logger, "p=" << Point{1, 2} << " str=" << ::std::string("hello"));
    expect("p=(1,2) str=hello", "hosted interop for user types and std::string");
#endif

    if (failures != 0) {
        ::std::fprintf(stderr, "log_stream_runtime: %d failure(s)\n", failures);
        return 1;
    }
#if defined(NROS_CPP_STD)
    ::std::printf("log_stream_runtime (NROS_CPP_STD): OK\n");
#else
    ::std::printf("log_stream_runtime (no NROS_CPP_STD): OK\n");
#endif
    return 0;
}
