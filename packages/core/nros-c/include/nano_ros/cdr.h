/**
 * nros CDR serialization helpers
 *
 * Functions for serializing and deserializing ROS messages in CDR format.
 *
 * Copyright 2024 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NANO_ROS_CDR_H
#define NANO_ROS_CDR_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#include "nano_ros/visibility.h"

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// CDR Write Functions
// ============================================================================

/**
 * Write a boolean value to the buffer.
 *
 * @param ptr Pointer to current write position (updated on success)
 * @param end Pointer to end of buffer
 * @param value Value to write
 *
 * @return 0 on success, -1 on error (buffer overflow)
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_write_bool(uint8_t **ptr, const uint8_t *end, bool value);

/**
 * Write a uint8 value to the buffer.
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_write_u8(uint8_t **ptr, const uint8_t *end, uint8_t value);

/**
 * Write an int8 value to the buffer.
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_write_i8(uint8_t **ptr, const uint8_t *end, int8_t value);

/**
 * Write a uint16 value to the buffer (with alignment).
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_write_u16(uint8_t **ptr, const uint8_t *end, uint16_t value);

/**
 * Write an int16 value to the buffer (with alignment).
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_write_i16(uint8_t **ptr, const uint8_t *end, int16_t value);

/**
 * Write a uint32 value to the buffer (with alignment).
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_write_u32(uint8_t **ptr, const uint8_t *end, uint32_t value);

/**
 * Write an int32 value to the buffer (with alignment).
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_write_i32(uint8_t **ptr, const uint8_t *end, int32_t value);

/**
 * Write a uint64 value to the buffer (with alignment).
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_write_u64(uint8_t **ptr, const uint8_t *end, uint64_t value);

/**
 * Write an int64 value to the buffer (with alignment).
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_write_i64(uint8_t **ptr, const uint8_t *end, int64_t value);

/**
 * Write a float32 value to the buffer (with alignment).
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_write_f32(uint8_t **ptr, const uint8_t *end, float value);

/**
 * Write a float64 value to the buffer (with alignment).
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_write_f64(uint8_t **ptr, const uint8_t *end, double value);

/**
 * Write a string to the buffer (length-prefixed).
 *
 * CDR strings are encoded as: u32 length (including null terminator) + bytes + null terminator
 *
 * @param ptr Pointer to current write position (updated on success)
 * @param end Pointer to end of buffer
 * @param value Null-terminated string to write
 *
 * @return 0 on success, -1 on error
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_write_string(uint8_t **ptr, const uint8_t *end, const char *value);

// ============================================================================
// CDR Read Functions
// ============================================================================

/**
 * Read a boolean value from the buffer.
 *
 * @param ptr Pointer to current read position (updated on success)
 * @param end Pointer to end of buffer
 * @param value Output: read value
 *
 * @return 0 on success, -1 on error (buffer underflow)
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_read_bool(const uint8_t **ptr, const uint8_t *end, bool *value);

/**
 * Read a uint8 value from the buffer.
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_read_u8(const uint8_t **ptr, const uint8_t *end, uint8_t *value);

/**
 * Read an int8 value from the buffer.
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_read_i8(const uint8_t **ptr, const uint8_t *end, int8_t *value);

/**
 * Read a uint16 value from the buffer (with alignment).
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_read_u16(const uint8_t **ptr, const uint8_t *end, uint16_t *value);

/**
 * Read an int16 value from the buffer (with alignment).
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_read_i16(const uint8_t **ptr, const uint8_t *end, int16_t *value);

/**
 * Read a uint32 value from the buffer (with alignment).
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_read_u32(const uint8_t **ptr, const uint8_t *end, uint32_t *value);

/**
 * Read an int32 value from the buffer (with alignment).
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_read_i32(const uint8_t **ptr, const uint8_t *end, int32_t *value);

/**
 * Read a uint64 value from the buffer (with alignment).
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_read_u64(const uint8_t **ptr, const uint8_t *end, uint64_t *value);

/**
 * Read an int64 value from the buffer (with alignment).
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_read_i64(const uint8_t **ptr, const uint8_t *end, int64_t *value);

/**
 * Read a float32 value from the buffer (with alignment).
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_read_f32(const uint8_t **ptr, const uint8_t *end, float *value);

/**
 * Read a float64 value from the buffer (with alignment).
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_read_f64(const uint8_t **ptr, const uint8_t *end, double *value);

/**
 * Read a string from the buffer into a fixed-size buffer.
 *
 * CDR strings are encoded as: u32 length (including null terminator) + bytes + null terminator
 *
 * @param ptr Pointer to current read position (updated on success)
 * @param end Pointer to end of buffer
 * @param value Output buffer for the string
 * @param max_len Maximum length of output buffer (including null terminator)
 *
 * @return 0 on success, -1 on error
 */
NANO_ROS_PUBLIC
int32_t nano_ros_cdr_read_string(
    const uint8_t **ptr,
    const uint8_t *end,
    char *value,
    size_t max_len);

#ifdef __cplusplus
}
#endif

#endif // NANO_ROS_CDR_H
