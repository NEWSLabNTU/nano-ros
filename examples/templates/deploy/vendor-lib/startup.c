/*
 * Phase 172.V — vendor-lib deploy template (startup shim).
 *
 * The "vendor-lib" ownership model: nano-ros owns the toolchain and emits the
 * generated wiring as a COMPILED entry library (`lib<sys>.a` + `<sys>.h`); the
 * vendor's final binary links that staticlib plus this startup object against
 * the vendor SDK. Canonical example: NVIDIA Orin SPE (`libtegra_aon_fsp.a`).
 *
 * Copy this dir to `deploy/<name>/` and point `[deploy.<name>].self` at it.
 * The root `nros.toml` `[deploy.<name>].build` link step references this file
 * and the generated artifacts via the runner var-set, e.g.:
 *
 *   [deploy.orin]
 *   kind   = "vendor-lib"
 *   target = "armv7r-none-eabihf"
 *   self   = "deploy/orin"
 *   emit   = "compiled"
 *   vendor.dir = { env = "NV_SPE_FSP_DIR" }
 *   vendor.pin = "spe-fsp 36.3"
 *   build = [
 *     "arm-none-eabi-gcc {self}/startup.c {entry_lib} -I{self} \
 *      -L{vendor.dir}/lib -ltegra_aon_fsp -T {self}/spe.ld -o build/orin/spe.elf",
 *   ]
 *   package = ["python3 {vendor.dir}/tools/spe_sign.py build/orin/spe.elf -o build/orin/spe.bin"]
 *
 * `nros deploy orin` emits `lib<sys>.a` + `<sys>.h`, substitutes the var-set,
 * and runs the link + package steps. Replace `mysys` below with your
 * `[system]` name lowercased (non-alphanumeric -> `_`); include the generated
 * header (its basename is `<sys>.h`, on the include path via `-I{self}` if you
 * symlink it, or copy it into `deploy/<name>/`).
 */
#include <stdint.h>

#include "mysys.h" /* generated entry-lib C ABI header ({entry_header}) */

/*
 * Vendor entry point. Rename / wire to whatever symbol the vendor SDK calls
 * after its own board bring-up (the SDK owns reset + clocks + the scheduler;
 * nano-ros only opens the session and spins its executor).
 */
int nros_app_start(void) {
    /* cfg = NULL => env/baked config; precedence is param > env > baked. */
    NrosExecutor *executor = nros_mysys_build_executor(NULL);
    if (executor == NULL) {
        return 1; /* session open failed */
    }
    if (nros_mysys_register_all(executor) != 0) {
        nros_mysys_destroy(executor);
        return 2; /* node/callback registration failed */
    }

    /* Hand control to the nano-ros executor; returns on shutdown. */
    int32_t rc = nros_mysys_spin(executor);

    nros_mysys_destroy(executor);
    return rc == 0 ? 0 : 3;
}
