/* Phase 212.H.1 adapter-shim fixture main (issue 0154).
 *
 * Pre-258 this app had NO sources: `nros_system_generate()` attached the
 * baked `system_main.c` (extern `nros_component_*_register` decls + a
 * spin loop). Phase-258 retired that TU with the install seam, so the
 * shim now bakes CONFIG only (`system_config.h` / `system_config.cmake`)
 * — and Zephyr requires `app` to own at least one source. This stub is
 * that source: it proves the adapter-shim contract end to end by
 * compiling against the baked header and printing its values at boot
 * (the e2e asserts boot output, not runtime pub/sub — the fixture's
 * component pkgs are stubs).
 */
#include <zephyr/kernel.h>

#include "system_config.h"

int main(void)
{
	printk("nros adapter shim: rmw=%s domain=%u components=%d\n",
	       NROS_SYSTEM_RMW, (unsigned)NROS_SYSTEM_DOMAIN_ID,
	       NROS_SYSTEM_COMPONENT_COUNT);
	return 0;
}
