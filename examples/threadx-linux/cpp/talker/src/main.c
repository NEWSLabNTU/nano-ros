/* Phase 212.L Component pkg — ThreadX entrypoint.
 *
 * The synthesised `nros_system_main()` (from
 * `nros_threadx_codegen_system`) owns the per-component spawn; this
 * thin C `main` just calls it. The system_main banner + per-component
 * dispatch entries are emitted at cmake configure time.
 */

#include <stdio.h>

extern int nros_system_main(void);

int main(void) {
    return nros_system_main();
}
