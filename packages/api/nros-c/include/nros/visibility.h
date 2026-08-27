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

// Deprecation macros.
//
// NROS_DEPRECATED marks an item without saying what to use instead;
// NROS_DEPRECATED_MSG(msg) carries the migration target into the compiler's
// diagnostic, which is the difference between a warning a user can act on and
// one they can only silence. Prefer the _MSG form for a renamed entry point.
//
// Both attach to a FUNCTION or an OBJECT. C has no portable way to deprecate a
// `typedef` -- GCC/Clang accept the attribute there but MSVC does not, and no C
// standard before C23 has `[[deprecated]]` -- so a renamed TYPE gets a plain
// alias and a comment, never one of these macros. Do not pretend a typedef
// warns.
#if defined(__GNUC__) || defined(__clang__)
#define NROS_DEPRECATED __attribute__((deprecated))
#define NROS_DEPRECATED_MSG(msg) __attribute__((deprecated(msg)))
#elif defined(_MSC_VER)
#define NROS_DEPRECATED __declspec(deprecated)
#define NROS_DEPRECATED_MSG(msg) __declspec(deprecated(msg))
#else
#define NROS_DEPRECATED
#define NROS_DEPRECATED_MSG(msg)
#endif

// Warn unused result
#if defined(__GNUC__) || defined(__clang__)
#define NROS_WARN_UNUSED __attribute__((warn_unused_result))
#else
#define NROS_WARN_UNUSED
#endif

#ifdef __cplusplus
}
#endif

#endif // NROS_VISIBILITY_H
