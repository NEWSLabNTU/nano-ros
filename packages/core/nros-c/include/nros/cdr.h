/**
 * @file cdr.h
 * @brief CDR serialization helpers.
 *
 * Low-level read/write functions for CDR (Common Data Representation)
 * encoding.  These are used by generated message code and can also be
 * called directly for manual serialization.
 *
 * All functions advance the @c ptr cursor.  They return 0 on success
 * or a negative value if the buffer is too small.
 */

#ifndef NROS_CDR_H
#define NROS_CDR_H

#include "nros/types.h"

#ifdef __cplusplus
extern "C" {
#endif

/* ===================================================================
 * Write Functions
 * =================================================================== */

/**
 * @brief Write a boolean value to the buffer.
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Boolean value to write.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_write_bool(uint8_t **ptr,
                            const uint8_t *end,
                            const uint8_t *origin,
                            bool value);

/**
 * @brief Write a u8 value to the buffer.
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Value to write.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_write_u8(uint8_t **ptr,
                          const uint8_t *end,
                          const uint8_t *origin,
                          uint8_t value);

/**
 * @brief Write an i8 value to the buffer.
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Value to write.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_write_i8(uint8_t **ptr,
                          const uint8_t *end,
                          const uint8_t *origin,
                          int8_t value);

/**
 * @brief Write a u16 value to the buffer (with alignment).
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Value to write.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_write_u16(uint8_t **ptr,
                           const uint8_t *end,
                           const uint8_t *origin,
                           uint16_t value);

/**
 * @brief Write an i16 value to the buffer.
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Value to write.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_write_i16(uint8_t **ptr,
                           const uint8_t *end,
                           const uint8_t *origin,
                           int16_t value);

/**
 * @brief Write a u32 value to the buffer (with alignment).
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Value to write.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_write_u32(uint8_t **ptr,
                           const uint8_t *end,
                           const uint8_t *origin,
                           uint32_t value);

/**
 * @brief Write an i32 value to the buffer.
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Value to write.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_write_i32(uint8_t **ptr,
                           const uint8_t *end,
                           const uint8_t *origin,
                           int32_t value);

/**
 * @brief Write a u64 value to the buffer (with alignment).
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Value to write.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_write_u64(uint8_t **ptr,
                           const uint8_t *end,
                           const uint8_t *origin,
                           uint64_t value);

/**
 * @brief Write an i64 value to the buffer.
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Value to write.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_write_i64(uint8_t **ptr,
                           const uint8_t *end,
                           const uint8_t *origin,
                           int64_t value);

/**
 * @brief Write a f32 value to the buffer (with alignment).
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Value to write.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_write_f32(uint8_t **ptr,
                           const uint8_t *end,
                           const uint8_t *origin,
                           float value);

/**
 * @brief Write a f64 value to the buffer (with alignment).
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Value to write.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_write_f64(uint8_t **ptr,
                           const uint8_t *end,
                           const uint8_t *origin,
                           double value);

/**
 * @brief Write a string to the buffer (length-prefixed).
 *
 * CDR strings are encoded as: u32 length (including null terminator)
 * + bytes + null terminator.
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Null-terminated string to write.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_write_string(uint8_t **ptr,
                              const uint8_t *end,
                              const uint8_t *origin,
                              const char *value);

/* ===================================================================
 * Read Functions
 * =================================================================== */

/**
 * @brief Read a boolean value from the buffer.
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Output: the value read.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_read_bool(const uint8_t **ptr,
                           const uint8_t *end,
                           const uint8_t *origin,
                           bool *value);

/**
 * @brief Read a u8 value from the buffer.
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Output: the value read.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_read_u8(const uint8_t **ptr,
                         const uint8_t *end,
                         const uint8_t *origin,
                         uint8_t *value);

/**
 * @brief Read an i8 value from the buffer.
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Output: the value read.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_read_i8(const uint8_t **ptr,
                         const uint8_t *end,
                         const uint8_t *origin,
                         int8_t *value);

/**
 * @brief Read a u16 value from the buffer (with alignment).
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Output: the value read.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_read_u16(const uint8_t **ptr,
                          const uint8_t *end,
                          const uint8_t *origin,
                          uint16_t *value);

/**
 * @brief Read an i16 value from the buffer.
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Output: the value read.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_read_i16(const uint8_t **ptr,
                          const uint8_t *end,
                          const uint8_t *origin,
                          int16_t *value);

/**
 * @brief Read a u32 value from the buffer (with alignment).
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Output: the value read.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_read_u32(const uint8_t **ptr,
                          const uint8_t *end,
                          const uint8_t *origin,
                          uint32_t *value);

/**
 * @brief Read an i32 value from the buffer.
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Output: the value read.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_read_i32(const uint8_t **ptr,
                          const uint8_t *end,
                          const uint8_t *origin,
                          int32_t *value);

/**
 * @brief Read a u64 value from the buffer (with alignment).
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Output: the value read.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_read_u64(const uint8_t **ptr,
                          const uint8_t *end,
                          const uint8_t *origin,
                          uint64_t *value);

/**
 * @brief Read an i64 value from the buffer.
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Output: the value read.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_read_i64(const uint8_t **ptr,
                          const uint8_t *end,
                          const uint8_t *origin,
                          int64_t *value);

/**
 * @brief Read a f32 value from the buffer.
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Output: the value read.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_read_f32(const uint8_t **ptr,
                          const uint8_t *end,
                          const uint8_t *origin,
                          float *value);

/**
 * @brief Read a f64 value from the buffer.
 *
 * @param ptr    Pointer to the cursor (advanced on success).
 * @param end    Pointer past the end of the buffer.
 * @param origin Buffer origin (for alignment calculations).
 * @param value  Output: the value read.
 * @return 0 on success, negative on overflow.
 */
NROS_PUBLIC
int32_t nros_cdr_read_f64(const uint8_t **ptr,
                          const uint8_t *end,
                          const uint8_t *origin,
                          double *value);

/**
 * @brief Read a string from the buffer into a fixed-size buffer.
 *
 * CDR strings are encoded as: u32 length (including null terminator)
 * + bytes + null terminator.
 *
 * @param ptr     Pointer to the cursor (advanced on success).
 * @param end     Pointer past the end of the buffer.
 * @param origin  Buffer origin (for alignment calculations).
 * @param value   Output buffer for the string.
 * @param max_len Maximum length of the output buffer.
 * @return 0 on success, negative on overflow or truncation.
 */
NROS_PUBLIC
int32_t nros_cdr_read_string(const uint8_t **ptr,
                             const uint8_t *end,
                             const uint8_t *origin,
                             char *value,
                             size_t max_len);

#ifdef __cplusplus
}
#endif

#endif /* NROS_CDR_H */
