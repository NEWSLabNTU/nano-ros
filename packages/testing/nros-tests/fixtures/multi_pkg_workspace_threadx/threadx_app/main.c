/* Phase 212.H.4 fixture — minimal host entry that invokes the
 * generated `nros_system_main()`. Stands in for the per-board startup.c
 * (e.g. ThreadX-Linux's `nros-board-threadx-linux/startup.c` calling
 * `tx_kernel_enter()` which spawns the app thread).
 */
#include <stdio.h>

extern int nros_system_main(void);

int main(void) {
    setvbuf(stdout, NULL, _IOLBF, 0);
    return nros_system_main();
}
