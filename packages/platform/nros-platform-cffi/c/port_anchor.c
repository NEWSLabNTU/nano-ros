/**
 * @file port_anchor.c
 * @brief Makes the POSIX port resolvable for the crates BELOW this one,
 * whatever the link order (`nros-platform-cffi`, feature `posix-c-port`).
 *
 * Two crates this one depends on call the port directly:
 *   - `nros-log`'s `__timestamp_ns` calls `nros_platform_clock_ns`, whenever
 *     the platform declares the clock capability;
 *   - `nros-platform-api` calls `nros_platform_task_*`,
 *     `nros_platform_yield_now` and the stack probes.
 * Both crates link AFTER this crate, so they also link after the port it
 * compiles. GNU ld scans each archive once. When nothing on the line has
 * already pulled `platform.o`, their references arrive after the port has been
 * scanned and stay undefined:
 *   `undefined reference to nros_platform_clock_ns`
 * `bins/log-arena-probe` (no executor, no RMW, no sink) was that binary. Every
 * other image linked only because some caller above this crate happened to
 * pull the member first.
 *
 * This object is linked `+whole-archive`, AHEAD of the port, which the build
 * script links `-bundle` so it gets its own place on the line. Every symbol
 * those crates call lives in `platform.c`, so one reference here pulls that
 * member at this crate's position, and with it every definition they need.
 *
 * The port itself stays demand-driven. That is what lets
 * `nros-rmw-xrce-cffi`'s copy of the same sources coexist with this one
 * (issue 1779), and what keeps `termination.c` its own member (issue 1776).
 */

#include <nros/platform.h>

/* Referenced only so the linker resolves it. `--gc-sections` may drop this
 * object afterwards, once `platform.o` is already in. */
uint64_t (*const nros_platform_cffi_port_anchor)(void) = nros_platform_clock_ns;
