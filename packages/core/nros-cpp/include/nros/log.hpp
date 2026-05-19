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
#if defined(NROS_CPP_STD) || (__STDC_HOSTED__ + 0)
#include <cstdio>
#define NROS_LOG_SINK(level, file, line, ...)                                                      \
    do {                                                                                           \
        ::std::fprintf(stderr, "[" level "] %s:%d ", (file), (line));                              \
        ::std::fprintf(stderr, __VA_ARGS__);                                                       \
        ::std::fputc('\n', stderr);                                                                \
    } while (0)
#else
#define NROS_LOG_SINK(level, file, line, ...) ((void)(level), (void)(file), (void)(line))
#endif
#endif

/// Print an INFO-level log line.
#define NROS_INFO(...) NROS_LOG_SINK("INFO", __FILE__, __LINE__, __VA_ARGS__)
/// Print a WARN-level log line.
#define NROS_WARN(...) NROS_LOG_SINK("WARN", __FILE__, __LINE__, __VA_ARGS__)
/// Print an ERROR-level log line.
#define NROS_ERROR(...) NROS_LOG_SINK("ERROR", __FILE__, __LINE__, __VA_ARGS__)
/// Print a DEBUG-level log line. Compiled out when `NDEBUG` is set.
#ifdef NDEBUG
#define NROS_DEBUG(...) ((void)0)
#else
#define NROS_DEBUG(...) NROS_LOG_SINK("DEBUG", __FILE__, __LINE__, __VA_ARGS__)
#endif

/* ---- Phase 88.12 — node-/logger-keyed surface ----
 *
 * The macros above are legacy (Phase 123.B.7) — file:line-prefixed
 * stderr printf with no per-logger routing. The macros below carry
 * a Logger handle through to the post-Phase-88 dispatcher
 * (`nros_log_emit_fmt` → per-platform sinks, see
 * `<nros/platform.h>` for the ABI).
 *
 * Obtain the handle from a Node via `node.get_logger()`:
 *
 * ```cpp
 * nros::Node node;
 * NROS_TRY(nros::create_node(node, "my_node"));
 * auto logger = node.get_logger();
 * NROS_LOG_INFO(logger, "started; domain=%u", 42);
 * ```
 *
 * Below-threshold filtering happens runtime-side via the
 * `nros_log::Logger`'s `set_level`; compile-time filtering is via
 * `nros-log/max-level-*` Cargo features (compiled into the nros-c
 * staticlib that ships `nros_log_emit_fmt`). */

#include <nros/log.h>

#define NROS_LOG_TRACE(logger, ...) ::nros_log_emit_fmt((logger), ::NROS_LOG_SEVERITY_TRACE, __VA_ARGS__)
#define NROS_LOG_DEBUG(logger, ...) ::nros_log_emit_fmt((logger), ::NROS_LOG_SEVERITY_DEBUG, __VA_ARGS__)
#define NROS_LOG_INFO(logger, ...)  ::nros_log_emit_fmt((logger), ::NROS_LOG_SEVERITY_INFO,  __VA_ARGS__)
#define NROS_LOG_WARN(logger, ...)  ::nros_log_emit_fmt((logger), ::NROS_LOG_SEVERITY_WARN,  __VA_ARGS__)
#define NROS_LOG_ERROR(logger, ...) ::nros_log_emit_fmt((logger), ::NROS_LOG_SEVERITY_ERROR, __VA_ARGS__)
#define NROS_LOG_FATAL(logger, ...) ::nros_log_emit_fmt((logger), ::NROS_LOG_SEVERITY_FATAL, __VA_ARGS__)

#endif // NROS_CPP_LOG_HPP
