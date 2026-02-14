/**
 * nros visibility macros
 *
 * Export/import macros for shared library builds.
 * For embedded/static builds, these are no-ops.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NANO_ROS_VISIBILITY_H
#define NANO_ROS_VISIBILITY_H

#ifdef __cplusplus
extern "C" {
#endif

// Visibility macros for shared library support
#if defined(_WIN32)
    #if defined(NANO_ROS_BUILDING_DLL)
        #define NANO_ROS_PUBLIC __declspec(dllexport)
    #elif defined(NANO_ROS_USING_DLL)
        #define NANO_ROS_PUBLIC __declspec(dllimport)
    #else
        #define NANO_ROS_PUBLIC
    #endif
    #define NANO_ROS_LOCAL
#else
    #if __GNUC__ >= 4
        #define NANO_ROS_PUBLIC __attribute__((visibility("default")))
        #define NANO_ROS_LOCAL __attribute__((visibility("hidden")))
    #else
        #define NANO_ROS_PUBLIC
        #define NANO_ROS_LOCAL
    #endif
#endif

// Deprecation macro
#if defined(__GNUC__) || defined(__clang__)
    #define NANO_ROS_DEPRECATED __attribute__((deprecated))
#elif defined(_MSC_VER)
    #define NANO_ROS_DEPRECATED __declspec(deprecated)
#else
    #define NANO_ROS_DEPRECATED
#endif

// Warn unused result
#if defined(__GNUC__) || defined(__clang__)
    #define NANO_ROS_WARN_UNUSED __attribute__((warn_unused_result))
#else
    #define NANO_ROS_WARN_UNUSED
#endif

#ifdef __cplusplus
}
#endif

#endif // NANO_ROS_VISIBILITY_H
