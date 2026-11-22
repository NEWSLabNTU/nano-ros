// phase-582 W2 (RFC-0141 D6, issue 2691) -- what FreeRTOS's libc clocks answer.
//
// Reads every clock the board's libc shims answer, in two phases:
//
//   unsynced  no wall-clock source: the port's `nros_platform_time_now_ns`
//             (0, "no wall clock") is what every wall path must read;
//   stub      a stub board clock answers the symbol with a fixed instant
//             (`kStubNs`), and every wall path must read that instead.
//
// Each line is `CLOCK_PROBE <phase> <clock>=<value>`, the value in decimal.
// `tests/freertos_libc_clock.rs` holds the expectations; this program only
// reports, so a wrong answer is printed rather than hidden behind a verdict.
//
// The monotonic clocks are read twice around a sleep in each phase, so the
// test can show them ADVANCING by at least the sleep, and the stub phase shows
// they never follow the wall clock.

#include <chrono>
#include <cstdint>
#include <cstdio>
#include <ctime>
#include <thread>

#include <sys/time.h>
#include <time.h>
#include <unistd.h>

extern "C" uint64_t nros_platform_clock_ns(void);

// newlib declares clock_gettime only under _POSIX_TIMERS, which its
// arm-none-eabi build leaves unset; the board's shim defines it all the same.
// (A host C library that declares it, for the host census build, keeps its own.)
#if !defined(_POSIX_TIMERS)
extern "C" int clock_gettime(clockid_t clock_id, struct timespec* ts);
#endif

// newlib names CLOCK_MONOTONIC only under _POSIX_MONOTONIC_CLOCK, which its
// arm-none-eabi build leaves unset; the number is newlib's own.
#ifndef CLOCK_MONOTONIC
#define CLOCK_MONOTONIC ((clockid_t)4)
#endif

namespace {

// 2023-11-14T22:13:20.123456789Z: far from 0 and from anything a tick count
// reaches in a test, and with digits in every unit gettimeofday keeps.
constexpr uint64_t kStubNs = 1700000000123456789ULL;

// How long each phase sleeps between its two monotonic readings.
constexpr int kSleepMs = 1500;

volatile bool g_stub_on = false;

// newlib-nano's printf has no %lld / %llu, so 64-bit values are formatted here.
void print_value(const char* phase, const char* name, bool negative, uint64_t magnitude) {
    char digits[24];
    int n = 0;
    do {
        digits[n++] = static_cast<char>('0' + (magnitude % 10u));
        magnitude /= 10u;
    } while (magnitude != 0u);
    char out[26];
    int o = 0;
    if (negative) {
        out[o++] = '-';
    }
    while (n > 0) {
        out[o++] = digits[--n];
    }
    out[o] = '\0';
    std::printf("CLOCK_PROBE %s %s=%s\n", phase, name, out);
}

void print_u64(const char* phase, const char* name, uint64_t v) {
    print_value(phase, name, false, v);
}

void print_i64(const char* phase, const char* name, int64_t v) {
    print_value(phase, name, v < 0,
                v < 0 ? 0u - static_cast<uint64_t>(v) : static_cast<uint64_t>(v));
}

uint64_t timespec_ns(const struct timespec& ts) {
    return static_cast<uint64_t>(ts.tv_sec) * 1000000000ULL + static_cast<uint64_t>(ts.tv_nsec);
}

void read_wall(const char* phase) {
    struct timeval tv = {};
    const int gtod_rc = gettimeofday(&tv, nullptr);
    print_i64(phase, "gettimeofday_rc", gtod_rc);
    print_u64(phase, "gettimeofday_us",
              static_cast<uint64_t>(tv.tv_sec) * 1000000ULL + static_cast<uint64_t>(tv.tv_usec));

    struct timespec rt = {};
    const int rt_rc = clock_gettime(CLOCK_REALTIME, &rt);
    print_i64(phase, "realtime_rc", rt_rc);
    print_u64(phase, "realtime_ns", timespec_ns(rt));

    print_i64(phase, "time_s", static_cast<int64_t>(std::time(nullptr)));
    print_i64(phase, "system_clock_ns",
              std::chrono::duration_cast<std::chrono::nanoseconds>(
                  std::chrono::system_clock::now().time_since_epoch())
                  .count());
}

void read_monotonic(const char* phase, const char* suffix) {
    char name[32];
    struct timespec mt = {};
    const int mt_rc = clock_gettime(CLOCK_MONOTONIC, &mt);
    std::snprintf(name, sizeof name, "monotonic_rc_%s", suffix);
    print_i64(phase, name, mt_rc);
    std::snprintf(name, sizeof name, "monotonic_ns_%s", suffix);
    print_u64(phase, name, timespec_ns(mt));
    std::snprintf(name, sizeof name, "steady_clock_ns_%s", suffix);
    print_i64(phase, name,
              std::chrono::duration_cast<std::chrono::nanoseconds>(
                  std::chrono::steady_clock::now().time_since_epoch())
                  .count());
    std::snprintf(name, sizeof name, "platform_clock_ns_%s", suffix);
    print_u64(phase, name, nros_platform_clock_ns());
}

void run_phase(const char* phase) {
    read_monotonic(phase, "a");
    std::this_thread::sleep_for(std::chrono::milliseconds(kSleepMs));
    read_wall(phase);
    read_monotonic(phase, "b");
}

} // namespace

// The stub board clock (see CMakeLists.txt): every reference to
// `nros_platform_time_now_ns` outside the port's TU lands here.
extern "C" uint64_t __real_nros_platform_time_now_ns(void);
extern "C" uint64_t __wrap_nros_platform_time_now_ns(void) {
    return g_stub_on ? kStubNs : __real_nros_platform_time_now_ns();
}

int main(int argc, char** argv) {
    (void)argc;
    (void)argv;
    std::printf("CLOCK_PROBE start sleep_ms=%d\n", kSleepMs);
    print_u64("start", "stub_ns", kStubNs);

    run_phase("unsynced");

    // An unknown clock is refused, not answered with one of the two above.
    struct timespec other = {};
    print_i64("unsynced", "cputime_rc", clock_gettime(static_cast<clockid_t>(2), &other));

    g_stub_on = true;
    run_phase("stub");

    std::printf("CLOCK_PROBE done\n");
    return 0;
}
