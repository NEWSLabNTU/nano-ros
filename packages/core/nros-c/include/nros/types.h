/**
 * nros common types and return codes
 *
 * Core type definitions shared across the nros C API.
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

// ============================================================================
// Constants
// ============================================================================

/** Maximum length of a zenoh locator string (e.g., "tcp/127.0.0.1:7447") */
#define NROS_MAX_LOCATOR_LEN 128

/** Maximum length of a node name */
#define NROS_MAX_NAME_LEN 64

/** Maximum length of a node namespace */
#define NROS_MAX_NAMESPACE_LEN 128

/** Maximum length of a topic name */
#define NROS_MAX_TOPIC_LEN 256

/** Maximum length of a service name */
#define NROS_MAX_SERVICE_NAME_LEN 256

/** Maximum length of a type name (e.g., "std_msgs::msg::dds_::Int32_") */
#define NROS_MAX_TYPE_NAME_LEN 256

/** Maximum length of a type hash (RIHS format) */
#define NROS_MAX_TYPE_HASH_LEN 128

// Legacy compatibility (without NROS_ prefix)
#define MAX_LOCATOR_LEN NROS_MAX_LOCATOR_LEN
#define MAX_NAME_LEN NROS_MAX_NAME_LEN
#define MAX_NAMESPACE_LEN NROS_MAX_NAMESPACE_LEN
#define MAX_TOPIC_LEN NROS_MAX_TOPIC_LEN
#define MAX_SERVICE_NAME_LEN NROS_MAX_SERVICE_NAME_LEN
#define MAX_TYPE_NAME_LEN NROS_MAX_TYPE_NAME_LEN
#define MAX_TYPE_HASH_LEN NROS_MAX_TYPE_HASH_LEN

// ============================================================================
// Return Codes
// ============================================================================

/**
 * Return type for nros C API functions.
 *
 * Compatible with rcl_ret_t for familiarity.
 */
typedef int nano_ros_ret_t;

/** Success */
#define NROS_RET_OK 0

/** Generic error */
#define NROS_RET_ERROR (-1)

/** Timeout occurred */
#define NROS_RET_TIMEOUT (-2)

/** Invalid argument passed */
#define NROS_RET_INVALID_ARGUMENT (-3)

/** Resource not found */
#define NROS_RET_NOT_FOUND (-4)

/** Resource already exists */
#define NROS_RET_ALREADY_EXISTS (-5)

/** Resource limit reached (e.g., max handles) */
#define NROS_RET_FULL (-6)

/** Not initialized */
#define NROS_RET_NOT_INIT (-7)

/** Bad sequence (e.g., wrong order of operations) */
#define NROS_RET_BAD_SEQUENCE (-8)

/** Service call failed */
#define NROS_RET_SERVICE_FAILED (-9)

/** Publish failed */
#define NROS_RET_PUBLISH_FAILED (-10)

/** Subscription failed */
#define NROS_RET_SUBSCRIPTION_FAILED (-11)

/** Operation not allowed (e.g., goal not in correct state) */
#define NROS_RET_NOT_ALLOWED (-12)

/** Request was rejected (e.g., goal rejected by server) */
#define NROS_RET_REJECTED (-13)

// ============================================================================
// QoS Types
// ============================================================================

/** QoS reliability policy */
typedef enum nano_ros_qos_reliability_t {
    /** Best effort delivery - no guarantees */
    NROS_QOS_RELIABILITY_BEST_EFFORT = 0,
    /** Reliable delivery - retransmit if needed */
    NROS_QOS_RELIABILITY_RELIABLE = 1,
} nano_ros_qos_reliability_t;

/** QoS durability policy */
typedef enum nano_ros_qos_durability_t {
    /** Volatile - no persistence */
    NROS_QOS_DURABILITY_VOLATILE = 0,
    /** Transient local - persist for late joiners */
    NROS_QOS_DURABILITY_TRANSIENT_LOCAL = 1,
} nano_ros_qos_durability_t;

/** QoS history policy */
typedef enum nano_ros_qos_history_t {
    /** Keep last N samples */
    NROS_QOS_HISTORY_KEEP_LAST = 0,
    /** Keep all samples */
    NROS_QOS_HISTORY_KEEP_ALL = 1,
} nano_ros_qos_history_t;

/** QoS settings structure */
typedef struct nano_ros_qos_t {
    /** Reliability policy */
    nano_ros_qos_reliability_t reliability;
    /** Durability policy */
    nano_ros_qos_durability_t durability;
    /** History policy */
    nano_ros_qos_history_t history;
    /** History depth (for KEEP_LAST) */
    int depth;
} nano_ros_qos_t;

/** Default QoS profile */
NROS_PUBLIC extern const nano_ros_qos_t NROS_QOS_DEFAULT;

/** Sensor data QoS profile (best effort, volatile) */
NROS_PUBLIC extern const nano_ros_qos_t NROS_QOS_SENSOR_DATA;

/** Services QoS profile (reliable) */
NROS_PUBLIC extern const nano_ros_qos_t NROS_QOS_SERVICES;

// ============================================================================
// Message Type Information
// ============================================================================

/**
 * Message type information.
 *
 * This structure describes a ROS message type for use with publishers
 * and subscribers.
 */
typedef struct nano_ros_message_type_t {
    /** Type name (e.g., "std_msgs::msg::dds_::Int32_") */
    const char *type_name;
    /** Type hash (RIHS format) */
    const char *type_hash;
    /** Maximum serialized size (0 = dynamic/unknown) */
    size_t serialized_size_max;
} nano_ros_message_type_t;

// ============================================================================
// Service Type Information
// ============================================================================

/**
 * Service type information (generated by codegen).
 *
 * Provides type name and hash for a ROS 2 service type.
 */
typedef struct nano_ros_service_type_t {
    /** Type name (e.g., "example_interfaces::srv::dds_::AddTwoInts_") */
    const char *type_name;
    /** Type hash (RIHS format) */
    const char *type_hash;
} nano_ros_service_type_t;

// ============================================================================
// Action Type Information
// ============================================================================

/**
 * Action type information.
 *
 * Contains type names and hashes for goal, result, and feedback messages.
 */
typedef struct nano_ros_action_type_t {
    /** Action type name (e.g., "example_interfaces::action::Fibonacci") */
    const char *type_name;
    /** Action type hash */
    const char *type_hash;
    /** Maximum serialized size of goal message */
    size_t goal_serialized_size_max;
    /** Maximum serialized size of result message */
    size_t result_serialized_size_max;
    /** Maximum serialized size of feedback message */
    size_t feedback_serialized_size_max;
} nano_ros_action_type_t;

#ifdef __cplusplus
}
#endif

#endif // NROS_TYPES_H
