/* Phase 139.3 — minimal PlatformIO talker example.
 *
 * Link-correctness demo: pulls one symbol from nano-ros (the
 * support_t zero-initialiser) and returns. Real publisher loops
 * live in `examples/` at the repo root; this file is the
 * minimum a PlatformIO Library Manager auto-discovery requires.
 */

#include <nros/init.h>

int main(void) {
    nros_support_t support = nros_support_get_zero_initialized();
    (void)support;
    return 0;
}
