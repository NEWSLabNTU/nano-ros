/* issue 0727 — WEAK host stubs for the platform log delivery pair.
 *
 * `PlatformSink` calls `nros_platform_log_write`/`_flush`, which are
 * link-time requirements a `nros-platform-<rtos>` port satisfies in every
 * real image. A HOST test binary that links nros-log through a library edge
 * (nros-rmw-cffi, nros-tests, …) has no port, and whether the unreferenced
 * sink vtable gets GC'd before the link is codegen luck — the workspace
 * `--no-default-features` test-compile lane lost that bet on tier 2's first
 * run. Weak no-ops make the link deterministic: any port's strong definition
 * wins; a portless host test binary gets records dropped, which is the only
 * meaning a portless binary could have.
 *
 * Compiled by build.rs ONLY when TARGET == HOST (a cross build never sees
 * this file, so an embedded image that forgot its port still fails loud at
 * link — the #708 defect class stays caught where it matters).
 */
#include <stddef.h>
#include <stdint.h>

__attribute__((weak)) void nros_platform_log_write(
    uint8_t severity,
    const uint8_t *name_ptr, size_t name_len,
    const uint8_t *msg_ptr, size_t msg_len)
{
    (void)severity;
    (void)name_ptr; (void)name_len;
    (void)msg_ptr;  (void)msg_len;
}

__attribute__((weak)) void nros_platform_log_flush(void) {}
