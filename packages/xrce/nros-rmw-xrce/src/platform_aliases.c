/* Phase 129.D.2 — XRCE platform symbol aliases.
 *
 * Carved out of `xrce-platform-shim` (retired) so the parent
 * crate can be deleted. Provides the C symbols micro-XRCE-DDS-Client
 * expects (`uxr_millis`, `uxr_nanos`) on top of the canonical
 * `nros_platform_*` ABI.
 *
 * Compiled by `nros-rmw-xrce-cffi/build.rs` always — every
 * supported target needs these. The platform-provider library
 * (POSIX, Zephyr, FreeRTOS, ThreadX, ESP-IDF) supplies
 * `nros_platform_time_now_ms` / `nros_platform_clock_us`.
 */

#include <stdint.h>

#include "nros/platform.h"

int64_t uxr_millis(void) {
    return (int64_t) nros_platform_time_now_ms();
}

int64_t uxr_nanos(void) {
    /* `nros_platform_clock_us` returns microseconds. Scale to
     * nanoseconds for micro-XRCE's `uxr_nanos` contract. */
    return (int64_t) nros_platform_clock_us() * 1000;
}
