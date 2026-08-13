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
 * `nros_platform_clock_ms` / `nros_platform_clock_us`.
 *
 * Both `uxr_millis` and `uxr_nanos` must be backed by the *monotonic*
 * clock service, not the wall-clock time service. micro-XRCE uses them
 * only for relative deadline deltas (`remaining = timeout - (now - start)`);
 * a wall clock that steps (NTP) or is unsupported (Zephyr without
 * CONFIG_RTC, where `nros_platform_time_now_ms` returns 0) breaks those
 * loops. `nros_platform_clock_ms` / `nros_platform_clock_us` share one
 * monotonic epoch (see nros/platform.h) and never decrease.
 */

#include <stdint.h>

#include "nros/platform.h"

/* issue 0548 — call the NANOSECOND clock directly.
 *
 * RFC-0073 / phase-350 retired `nros_platform_clock_{ms,us}` as ABI symbols: no
 * port defines them any more, and `<nros/platform.h>` carries `static inline`
 * wrappers instead. This shim kept calling them, so every Zephyr XRCE leaf
 * failed at LINK with `undefined reference` — the include here resolves to a
 * stale copy of the header on that path, and an inline that is not in scope is
 * an extern call to a symbol nobody exports.
 *
 * Calling `clock_ns` is the fix that does not depend on WHICH platform.h wins:
 * it is a real exported symbol on every port. It also stops `uxr_nanos` from
 * scaling microseconds back up by 1000, which threw away precision the
 * nanosecond clock already had — the loss RFC-0073 existed to remove. */
int64_t uxr_millis(void) {
    return (int64_t) (nros_platform_clock_ns() / 1000000u);
}

int64_t uxr_nanos(void) {
    return (int64_t) nros_platform_clock_ns();
}
