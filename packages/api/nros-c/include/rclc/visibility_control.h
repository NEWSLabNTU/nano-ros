/**
 * nros visibility macros
 *
 * Export/import macros for shared library builds.
 * For embedded/static builds, these are no-ops.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_VISIBILITY_H
#define NROS_VISIBILITY_H

#ifdef __cplusplus
extern "C" {
#endif

// Visibility macros for shared library support
#if defined(_WIN32)
#if defined(NROS_BUILDING_DLL)
#define NROS_PUBLIC __declspec(dllexport)
#elif defined(NROS_USING_DLL)
#define NROS_PUBLIC __declspec(dllimport)
#else
#define NROS_PUBLIC
#endif
#define NROS_LOCAL
#else
#if __GNUC__ >= 4
#define NROS_PUBLIC __attribute__((visibility("default")))
#define NROS_LOCAL __attribute__((visibility("hidden")))
#else
#define NROS_PUBLIC
#define NROS_LOCAL
#endif
#endif

// Warn unused result
#if defined(__GNUC__) || defined(__clang__)
#define NROS_WARN_UNUSED __attribute__((warn_unused_result))
#else
#define NROS_WARN_UNUSED
#endif

// A deprecated entry point: still compiles, and the warning names its
// replacement. `NROS_NO_DEPRECATION_WARNINGS` silences it for a caller that
// exercises the old entry point on purpose (its own tests).
#if (defined(__GNUC__) || defined(__clang__)) && !defined(NROS_NO_DEPRECATION_WARNINGS)
#define NROS_DEPRECATED(msg) __attribute__((deprecated(msg)))
#else
#define NROS_DEPRECATED(msg)
#endif

#ifdef __cplusplus
}
#endif

#endif // NROS_VISIBILITY_H
