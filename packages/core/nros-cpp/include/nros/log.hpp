// nros-cpp: lightweight logging macros
// Freestanding C++ — no STL, opt-in to stdio via NROS_CPP_STD or
// hosted-build detection.

/**
 * @file log.hpp
 * @ingroup grp_misc
 * @brief Phase 123.B.7 — `NROS_INFO` / `NROS_WARN` / `NROS_ERROR` /
 *        `NROS_DEBUG` printf-style log macros.
 *
 * Routes through a single configurable sink. By default, on hosted
 * builds (`__STDC_HOSTED__` or `NROS_CPP_STD` defined) the sink
 * writes to `stderr` with a `[level] file:line — fmt…` prefix.
 * Embedded builds without stdio fall through to a no-op so the
 * macros compile away.
 *
 * Override the sink with `#define NROS_LOG_SINK(level, file, line, fmt, ...)`
 * before including this header (or via `-DNROS_LOG_SINK=…`) to
 * route through `defmt`, semihosting, Zephyr's `LOG_INF`, etc.
 *
 * The macros take a `printf`-style format string + variadic
 * arguments. They evaluate `fmt` and the variadics exactly once.
 */

#ifndef NROS_CPP_LOG_HPP
#define NROS_CPP_LOG_HPP

#ifndef NROS_LOG_SINK
#  if defined(NROS_CPP_STD) || defined(__STDC_HOSTED__)
#    include <cstdio>
#    define NROS_LOG_SINK(level, file, line, ...)                                                    \
        do {                                                                                         \
            std::fprintf(stderr, "[" level "] %s:%d ", (file), (line));                              \
            std::fprintf(stderr, __VA_ARGS__);                                                       \
            std::fputc('\n', stderr);                                                                \
        } while (0)
#  else
#    define NROS_LOG_SINK(level, file, line, ...)                                                    \
        ((void)(level), (void)(file), (void)(line))
#  endif
#endif

/// Print an INFO-level log line.
#define NROS_INFO(...)  NROS_LOG_SINK("INFO",  __FILE__, __LINE__, __VA_ARGS__)
/// Print a WARN-level log line.
#define NROS_WARN(...)  NROS_LOG_SINK("WARN",  __FILE__, __LINE__, __VA_ARGS__)
/// Print an ERROR-level log line.
#define NROS_ERROR(...) NROS_LOG_SINK("ERROR", __FILE__, __LINE__, __VA_ARGS__)
/// Print a DEBUG-level log line. Compiled out when `NDEBUG` is set.
#ifdef NDEBUG
#  define NROS_DEBUG(...) ((void)0)
#else
#  define NROS_DEBUG(...) NROS_LOG_SINK("DEBUG", __FILE__, __LINE__, __VA_ARGS__)
#endif

#endif // NROS_CPP_LOG_HPP
