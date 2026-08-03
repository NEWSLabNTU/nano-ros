/* Phase 225.O follow-up (known-issue #18) — empty NuttX builtins table.
 *
 * The standalone nano-ros image provides its OWN `nsh_main` (the
 * flat-build `CONFIG_INIT_ENTRYPOINT`), so it never dispatches NSH
 * builtin commands and needs no builtins registry.
 *
 * NuttX's prebuilt `libapps.a` ships `builtin_list.o`, which DEFINES
 * `g_builtins[]` / `g_builtin_count` and REFERENCES every registered
 * app's `*_main`. When the apps tree was last `make export`ed with the
 * nano-ros C/C++ example apps staged into `apps/external/`, that table
 * references `nuttx_c_talker_main`, `nros_cpp_init`, ... — C/C++ FFI
 * symbols a Rust-only image does not provide. libc's
 * `lib_builtin_forindex.o` references `g_builtins`, so `builtin_list.o`
 * gets pulled, dragging in all those example objects -> undefined
 * `nros_*` link errors.
 *
 * Defining `g_builtins` / `g_builtin_count` HERE (force-linked before
 * `-lapps`) satisfies libc's reference with OUR empty table, so the
 * contaminated `builtin_list.o` is never pulled and the example apps
 * never reach the link. `g_builtin_count == 0`, so the single
 * zero-initialized sentinel entry is never read at runtime
 * (`builtin_for_index` / `builtin_isavail` bound-check against the
 * count first).
 *
 * FLAT build only (NUTTX_BUILD=flat) — the array variant of the
 * `g_builtins` declaration in <nuttx/lib/builtin.h>.
 */

#include <nuttx/config.h>
#include <nuttx/lib/builtin.h>

const struct builtin_s g_builtins[] = {
    {0},
};
const int g_builtin_count = 0;
