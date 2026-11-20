/**
 * @file wall_clock_state.h
 * @brief `nros_wall_clock_state_t`, what the wall-clock stamps mean right
 *        now, and its `NROS_WALL_CLOCK_SOURCE_*` values.
 *
 * Returned by `nros_wall_clock_state()`. Defined under
 * `NROS_DEFINED_nros_wall_clock_state_t`, the same guard the platform
 * header wraps its copy in, so whichever is included first defines it and
 * the other is a no-op. The two copies are one struct, member for member.
 */

#ifndef NROS_RCLC_DETAIL_WALL_CLOCK_STATE_H
#define NROS_RCLC_DETAIL_WALL_CLOCK_STATE_H

#include <stdbool.h>
#include <stdint.h>

#ifndef NROS_DEFINED_nros_wall_clock_state_t
#define NROS_DEFINED_nros_wall_clock_state_t
/** Which source answers the wall clock (`nros_wall_clock_state_t::source`). */
#define NROS_WALL_CLOCK_SOURCE_PLATFORM 0u  /**< the port's own clock */
#define NROS_WALL_CLOCK_SOURCE_SNTP 1u      /**< an SNTP exchange set the offset */
#define NROS_WALL_CLOCK_SOURCE_ZENOH_HLC 2u /**< zenoh HLC timestamps set it */
#define NROS_WALL_CLOCK_SOURCE_BOARD 3u     /**< the board's clock hook */

/** What the wall-clock stamps mean right now. */
typedef struct nros_wall_clock_state_t {
    /** `true`: the wall clock answers real UNIX time. `false`: it answers 0,
     *  and every `header.stamp` is ROS's "unset" time. */
    bool synced;
    /** The source answering, or that will answer once it syncs: one of
     *  `NROS_WALL_CLOCK_SOURCE_*`. */
    uint8_t source;
    /** How many times the answering source has CHANGED since boot. 0 for an
     *  image whose stamps all come from one clock. */
    uint32_t steps;
} nros_wall_clock_state_t;
#endif

#endif /* NROS_RCLC_DETAIL_WALL_CLOCK_STATE_H */
