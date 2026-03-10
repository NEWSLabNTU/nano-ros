/**
 * @file clock.h
 * @brief Clock and time utilities.
 *
 * Provides clocks for reading wall-clock or monotonic time, and
 * arithmetic helpers for @ref nros_time_t and @ref nros_duration_t.
 */

#ifndef NROS_CLOCK_H
#define NROS_CLOCK_H

#include "nros/types.h"

#ifdef __cplusplus
extern "C" {
#endif

/* ===================================================================
 * Types
 * =================================================================== */

/** Clock type enumeration. */
typedef enum nros_clock_type_t {
    /** Uninitialized clock. */
    NROS_CLOCK_UNINITIALIZED = 0,
    /** ROS time (follows /clock topic if available, otherwise system time). */
    NROS_CLOCK_ROS_TIME = 1,
    /** System time (wall clock from the operating system). */
    NROS_CLOCK_SYSTEM_TIME = 2,
    /** Steady time (monotonic, not affected by system time changes). */
    NROS_CLOCK_STEADY_TIME = 3,
} nros_clock_type_t;

/** Clock state enumeration. */
typedef enum nros_clock_state_t {
    /** Not initialized. */
    NROS_CLOCK_STATE_UNINITIALIZED = 0,
    /** Initialized and ready. */
    NROS_CLOCK_STATE_READY = 1,
    /** Shutdown. */
    NROS_CLOCK_STATE_SHUTDOWN = 2,
} nros_clock_state_t;

/** Clock structure. */
typedef struct nros_clock_t {
    /** Clock type. */
    enum nros_clock_type_t type;
    /** Current state. */
    enum nros_clock_state_t state;
    /** Internal: steady clock epoch (nanoseconds since process start). */
    uint64_t _steady_epoch_ns;
} nros_clock_t;

/* ===================================================================
 * Clock Functions
 * =================================================================== */

/**
 * @brief Get a zero-initialized clock.
 * @return Zero-initialized @ref nros_clock_t.
 */
NROS_PUBLIC struct nros_clock_t nros_clock_get_zero_initialized(void);

/**
 * @brief Initialise a clock.
 *
 * @param clock      Pointer to a zero-initialized clock.
 * @param clock_type Clock type to initialise.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p clock is NULL.
 * @retval NROS_RET_ERROR             on initialisation failure.
 */
NROS_PUBLIC
nros_ret_t nros_clock_init(struct nros_clock_t* clock, enum nros_clock_type_t clock_type);

/**
 * @brief Get the current time from a clock.
 *
 * @param clock    Pointer to an initialized clock.
 * @param time_out Output: current time.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if any pointer is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 */
NROS_PUBLIC
nros_ret_t nros_clock_get_now(const struct nros_clock_t* clock, struct nros_time_t* time_out);

/**
 * @brief Get the current time from a clock as nanoseconds.
 *
 * @param clock       Pointer to an initialized clock.
 * @param nanoseconds Output: current time in nanoseconds.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if any pointer is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 */
NROS_PUBLIC
nros_ret_t nros_clock_get_now_ns(const struct nros_clock_t* clock, int64_t* nanoseconds);

/**
 * @brief Check if a clock is valid (initialized and not shutdown).
 *
 * @param clock  Pointer to a clock.
 * @return @c true if valid, @c false otherwise.
 */
NROS_PUBLIC bool nros_clock_is_valid(const struct nros_clock_t* clock);

/**
 * @brief Get the clock type.
 *
 * @param clock  Pointer to an initialized clock.
 * @return Clock type, or @ref NROS_CLOCK_UNINITIALIZED if invalid.
 */
NROS_PUBLIC enum nros_clock_type_t nros_clock_get_type(const struct nros_clock_t* clock);

/**
 * @brief Finalise a clock.
 *
 * @param clock  Pointer to an initialized clock.
 *
 * @retval NROS_RET_OK               on success.
 * @retval NROS_RET_INVALID_ARGUMENT  if @p clock is NULL.
 * @retval NROS_RET_NOT_INIT          if not initialized.
 */
NROS_PUBLIC nros_ret_t nros_clock_fini(struct nros_clock_t* clock);

/* ===================================================================
 * Time Functions
 * =================================================================== */

/**
 * @brief Convert nanoseconds to a @ref nros_time_t structure.
 *
 * @param nanoseconds  Nanosecond value to convert.
 * @return Equivalent @ref nros_time_t.
 */
NROS_PUBLIC struct nros_time_t nros_time_from_nanoseconds(int64_t nanoseconds);

/**
 * @brief Convert a @ref nros_time_t to nanoseconds.
 *
 * @param time  Pointer to a time value.
 * @return Time as nanoseconds.
 */
NROS_PUBLIC int64_t nros_time_to_nanoseconds(const struct nros_time_t* time);

/**
 * @brief Add a duration to a time.
 *
 * @param time     Base time.
 * @param duration Duration to add.
 * @return Resulting time.
 */
NROS_PUBLIC
struct nros_time_t nros_time_add(struct nros_time_t time, struct nros_duration_t duration);

/**
 * @brief Subtract a duration from a time.
 *
 * @param time     Base time.
 * @param duration Duration to subtract.
 * @return Resulting time.
 */
NROS_PUBLIC
struct nros_time_t nros_time_sub(struct nros_time_t time, struct nros_duration_t duration);

/**
 * @brief Compare two times.
 *
 * @param a  First time.
 * @param b  Second time.
 * @return Negative if @p a < @p b, zero if equal, positive if @p a > @p b.
 */
NROS_PUBLIC int nros_time_compare(struct nros_time_t a, struct nros_time_t b);

#ifdef __cplusplus
}
#endif

#endif /* NROS_CLOCK_H */
