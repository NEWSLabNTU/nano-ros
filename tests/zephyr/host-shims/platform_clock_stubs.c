/* The platform symbols the shims' wall-clock resolution reaches
 * (`nros/clock_resolve.h`), which a real image gets from
 * `nros-platform-zephyr/src/platform.c`. The thread-slot gate links the shims
 * alone, so it supplies them here. None is called by the slot cases. */
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

uint64_t nros_platform_clock_ns(void) { return 0; }
void nros_platform_sleep_ms(size_t ms) { (void)ms; }
uint32_t nros_platform_critical_section_acquire(void) { return 0; }
void nros_platform_critical_section_release(uint32_t token) { (void)token; }
_Noreturn void nros_platform_panic(const char *msg, size_t len) {
    (void)msg;
    (void)len;
    abort();
}
