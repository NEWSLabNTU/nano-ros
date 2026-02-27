/**
 * @file types.h
 * @brief Shared types, constants, and error codes for the nros C API.
 *
 * This header centralises every type that is used across multiple nros
 * modules: return codes, size limits, time/duration, QoS settings,
 * message/service type descriptors, and the callback invocation mode.
 *
 * Individual per-module headers (publisher.h, node.h, etc.) include
 * this file so that cross-cutting types are always available.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_TYPES_H
#define NROS_TYPES_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

#include "nros/visibility.h"
#include "nros/platform.h"

#ifdef __cplusplus
extern "C" {
#endif

/* ===================================================================
 * Size Constants
 * =================================================================== */

/** Maximum length of a zenoh locator string (e.g., "tcp/127.0.0.1:7447"). */
#define NROS_MAX_LOCATOR_LEN 128

/** Maximum length of a node name. */
#define NROS_MAX_NAME_LEN 64

/** Maximum length of a node namespace. */
#define NROS_MAX_NAMESPACE_LEN 128

/** Maximum length of a topic name. */
#define NROS_MAX_TOPIC_LEN 256

/** Maximum length of a service name. */
#define NROS_MAX_SERVICE_NAME_LEN 256

/** Maximum length of an action name. */
#define NROS_MAX_ACTION_NAME_LEN 256

/** Maximum length of a type name (e.g., "std_msgs::msg::dds_::Int32_"). */
#define NROS_MAX_TYPE_NAME_LEN 256

/** Maximum length of a type hash (RIHS format). */
#define NROS_MAX_TYPE_HASH_LEN 128

/**
 * Maximum number of concurrent goals per action server.
 *
 * This is a fixed constant (not configurable via env var) because it
 * affects the @ref nros_action_server_t struct layout.  Changing it
 * requires recompiling both Rust and C code.
 */
#define NROS_MAX_CONCURRENT_GOALS 4

/** Maximum length of a parameter name. */
#define NROS_MAX_PARAM_NAME_LEN 64

/** Maximum length of a string parameter value. */
#define NROS_MAX_PARAM_STRING_LEN 128

/* ===================================================================
 * Return Type and Error Codes
 * =================================================================== */

/**
 * Return type for nros C API functions.
 *
 * Compatible with rcl_ret_t for familiarity.
 */
typedef int nros_ret_t;

/** Success. */
#define NROS_RET_OK 0

/** Generic error. */
#define NROS_RET_ERROR -1

/** Timeout occurred. */
#define NROS_RET_TIMEOUT -2

/** Invalid argument passed. */
#define NROS_RET_INVALID_ARGUMENT -3

/** Resource not found. */
#define NROS_RET_NOT_FOUND -4

/** Resource already exists. */
#define NROS_RET_ALREADY_EXISTS -5

/** Resource limit reached (e.g., max handles). */
#define NROS_RET_FULL -6

/** Not initialized. */
#define NROS_RET_NOT_INIT -7

/** Bad sequence (e.g., wrong order of operations). */
#define NROS_RET_BAD_SEQUENCE -8

/** Service call failed. */
#define NROS_RET_SERVICE_FAILED -9

/** Publish failed. */
#define NROS_RET_PUBLISH_FAILED -10

/** Subscription failed. */
#define NROS_RET_SUBSCRIPTION_FAILED -11

/** Operation not allowed (e.g., goal not in correct state). */
#define NROS_RET_NOT_ALLOWED -12

/** Request was rejected (e.g., goal rejected by server). */
#define NROS_RET_REJECTED -13

/* ===================================================================
 * Time and Duration
 * =================================================================== */

/**
 * Time representation compatible with builtin_interfaces/msg/Time.
 */
typedef struct nros_time_t {
    /** Seconds component. */
    int32_t sec;
    /** Nanoseconds component (0 to 999,999,999). */
    uint32_t nanosec;
} nros_time_t;

/**
 * Duration representation compatible with builtin_interfaces/msg/Duration.
 */
typedef struct nros_duration_t {
    /** Seconds component (can be negative). */
    int32_t sec;
    /** Nanoseconds component (0 to 999,999,999). */
    uint32_t nanosec;
} nros_duration_t;

/* ===================================================================
 * QoS Types
 * =================================================================== */

/** QoS reliability policy. */
typedef enum nros_qos_reliability_t {
    /** Best effort delivery — no guarantees. */
    NROS_QOS_RELIABILITY_BEST_EFFORT = 0,
    /** Reliable delivery — retransmit if needed. */
    NROS_QOS_RELIABILITY_RELIABLE = 1,
} nros_qos_reliability_t;

/** QoS durability policy. */
typedef enum nros_qos_durability_t {
    /** Volatile — no persistence. */
    NROS_QOS_DURABILITY_VOLATILE = 0,
    /** Transient local — persist for late joiners. */
    NROS_QOS_DURABILITY_TRANSIENT_LOCAL = 1,
} nros_qos_durability_t;

/** QoS history policy. */
typedef enum nros_qos_history_t {
    /** Keep last N samples. */
    NROS_QOS_HISTORY_KEEP_LAST = 0,
    /** Keep all samples. */
    NROS_QOS_HISTORY_KEEP_ALL = 1,
} nros_qos_history_t;

/** QoS settings structure. */
typedef struct nros_qos_t {
    /** Reliability policy. */
    enum nros_qos_reliability_t reliability;
    /** Durability policy. */
    enum nros_qos_durability_t durability;
    /** History policy. */
    enum nros_qos_history_t history;
    /** History depth (for KEEP_LAST). */
    int depth;
} nros_qos_t;

/** Default QoS profile (reliable, keep-last 10). */
extern const struct nros_qos_t NROS_QOS_DEFAULT;

/** Sensor data QoS profile (best effort, small depth). */
extern const struct nros_qos_t NROS_QOS_SENSOR_DATA;

/** Services QoS profile (reliable). */
extern const struct nros_qos_t NROS_QOS_SERVICES;

/* ===================================================================
 * Message and Service Type Descriptors
 * =================================================================== */

/**
 * Message type information.
 *
 * Describes a ROS message type for use with publishers and subscribers.
 * Instances are normally generated by @c nano_ros_generate_interfaces().
 */
typedef struct nros_message_type_t {
    /** Type name (e.g., "std_msgs::msg::dds_::Int32"). */
    const char *type_name;
    /** Type hash (RIHS format). */
    const char *type_hash;
    /** Maximum serialized size (0 = dynamic/unknown). */
    size_t serialized_size_max;
} nros_message_type_t;

/** @cond INTERNAL */
#ifndef NROS_SERVICE_TYPE_DEFINED
#define NROS_SERVICE_TYPE_DEFINED
/** @endcond */
/**
 * Service type information (generated by codegen).
 *
 * Provides type name and hash for a ROS 2 service type.  This type is
 * used by generated C code from @c nano_ros_generate_interfaces() but
 * is not referenced by any nros-c function, so cbindgen does not
 * include it in nros_generated.h.
 */
typedef struct nros_service_type_t {
    /** Type name (e.g., "example_interfaces::srv::dds_::AddTwoInts_"). */
    const char *type_name;
    /** Type hash (RIHS format). */
    const char *type_hash;
} nros_service_type_t;
/** @cond INTERNAL */
#endif
/** @endcond */

/** @cond INTERNAL */
#ifndef NROS_ACTION_TYPE_DEFINED
#define NROS_ACTION_TYPE_DEFINED
/** @endcond */
/**
 * Action type information (generated by codegen).
 *
 * Provides type name, hash, and serialized size limits for a ROS 2
 * action type.  This type is used by generated C code from
 * @c nano_ros_generate_interfaces() and also by @c nros_action_server_init()
 * and @c nros_action_client_init() in @c nros/action.h.
 */
typedef struct nros_action_type_t {
    /** Action type name (e.g., "example_interfaces::action::Fibonacci"). */
    const char *type_name;
    /** Action type hash. */
    const char *type_hash;
    /** Maximum serialized size of goal message. */
    size_t goal_serialized_size_max;
    /** Maximum serialized size of result message. */
    size_t result_serialized_size_max;
    /** Maximum serialized size of feedback message. */
    size_t feedback_serialized_size_max;
} nros_action_type_t;
/** @cond INTERNAL */
#endif
/** @endcond */

/* ===================================================================
 * Callback Invocation Mode
 * =================================================================== */

/** Callback invocation mode for executor subscriptions. */
typedef enum nros_executor_invocation_t {
    /** Only invoke callback when new data is available. */
    NROS_EXECUTOR_ON_NEW_DATA = 0,
    /** Always invoke callback (even with NULL data). */
    NROS_EXECUTOR_ALWAYS = 1,
} nros_executor_invocation_t;

#ifdef __cplusplus
}
#endif

#endif /* NROS_TYPES_H */
