/**
 * nros common types and return codes
 *
 * Core type definitions shared across the nros C API.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NANO_ROS_TYPES_H
#define NANO_ROS_TYPES_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>

#include "nano_ros/visibility.h"
#include "nano_ros/platform.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Constants
// ============================================================================

/** Maximum length of a zenoh locator string (e.g., "tcp/127.0.0.1:7447") */
#define NANO_ROS_MAX_LOCATOR_LEN 128

/** Maximum length of a node name */
#define NANO_ROS_MAX_NAME_LEN 64

/** Maximum length of a node namespace */
#define NANO_ROS_MAX_NAMESPACE_LEN 128

/** Maximum length of a topic name */
#define NANO_ROS_MAX_TOPIC_LEN 256

/** Maximum length of a service name */
#define NANO_ROS_MAX_SERVICE_NAME_LEN 256

/** Maximum length of a type name (e.g., "std_msgs::msg::dds_::Int32_") */
#define NANO_ROS_MAX_TYPE_NAME_LEN 256

/** Maximum length of a type hash (RIHS format) */
#define NANO_ROS_MAX_TYPE_HASH_LEN 128

// Legacy compatibility (without NANO_ROS_ prefix)
#define MAX_LOCATOR_LEN NANO_ROS_MAX_LOCATOR_LEN
#define MAX_NAME_LEN NANO_ROS_MAX_NAME_LEN
#define MAX_NAMESPACE_LEN NANO_ROS_MAX_NAMESPACE_LEN
#define MAX_TOPIC_LEN NANO_ROS_MAX_TOPIC_LEN
#define MAX_SERVICE_NAME_LEN NANO_ROS_MAX_SERVICE_NAME_LEN
#define MAX_TYPE_NAME_LEN NANO_ROS_MAX_TYPE_NAME_LEN
#define MAX_TYPE_HASH_LEN NANO_ROS_MAX_TYPE_HASH_LEN

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
#define NANO_ROS_RET_OK 0

/** Generic error */
#define NANO_ROS_RET_ERROR (-1)

/** Timeout occurred */
#define NANO_ROS_RET_TIMEOUT (-2)

/** Invalid argument passed */
#define NANO_ROS_RET_INVALID_ARGUMENT (-3)

/** Resource not found */
#define NANO_ROS_RET_NOT_FOUND (-4)

/** Resource already exists */
#define NANO_ROS_RET_ALREADY_EXISTS (-5)

/** Resource limit reached (e.g., max handles) */
#define NANO_ROS_RET_FULL (-6)

/** Not initialized */
#define NANO_ROS_RET_NOT_INIT (-7)

/** Bad sequence (e.g., wrong order of operations) */
#define NANO_ROS_RET_BAD_SEQUENCE (-8)

/** Service call failed */
#define NANO_ROS_RET_SERVICE_FAILED (-9)

/** Publish failed */
#define NANO_ROS_RET_PUBLISH_FAILED (-10)

/** Subscription failed */
#define NANO_ROS_RET_SUBSCRIPTION_FAILED (-11)

/** Operation not allowed (e.g., goal not in correct state) */
#define NANO_ROS_RET_NOT_ALLOWED (-12)

/** Request was rejected (e.g., goal rejected by server) */
#define NANO_ROS_RET_REJECTED (-13)

// ============================================================================
// QoS Types
// ============================================================================

/** QoS reliability policy */
typedef enum nano_ros_qos_reliability_t {
    /** Best effort delivery - no guarantees */
    NANO_ROS_QOS_RELIABILITY_BEST_EFFORT = 0,
    /** Reliable delivery - retransmit if needed */
    NANO_ROS_QOS_RELIABILITY_RELIABLE = 1,
} nano_ros_qos_reliability_t;

/** QoS durability policy */
typedef enum nano_ros_qos_durability_t {
    /** Volatile - no persistence */
    NANO_ROS_QOS_DURABILITY_VOLATILE = 0,
    /** Transient local - persist for late joiners */
    NANO_ROS_QOS_DURABILITY_TRANSIENT_LOCAL = 1,
} nano_ros_qos_durability_t;

/** QoS history policy */
typedef enum nano_ros_qos_history_t {
    /** Keep last N samples */
    NANO_ROS_QOS_HISTORY_KEEP_LAST = 0,
    /** Keep all samples */
    NANO_ROS_QOS_HISTORY_KEEP_ALL = 1,
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
NANO_ROS_PUBLIC extern const nano_ros_qos_t NANO_ROS_QOS_DEFAULT;

/** Sensor data QoS profile (best effort, volatile) */
NANO_ROS_PUBLIC extern const nano_ros_qos_t NANO_ROS_QOS_SENSOR_DATA;

/** Services QoS profile (reliable) */
NANO_ROS_PUBLIC extern const nano_ros_qos_t NANO_ROS_QOS_SERVICES;

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

#ifdef __cplusplus
}
#endif

#endif // NANO_ROS_TYPES_H
