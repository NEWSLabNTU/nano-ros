/**
 * @file main.h
 * @ingroup grp_node
 * @brief Phase 219.E — `<nros/main.h>` Entry-pkg header (C variant).
 *
 * Symmetric to `<nros/main.hpp>`: the cmake fn `nano_ros_entry(LAUNCH
 * "<bringup>:<file>.launch.xml")` drives per-Entry-pkg codegen via
 * `nros codegen entry --lang c`, then appends the generated TU to the
 * executable target's sources.
 *
 * This header provides:
 *
 *   1. `NROS_MAIN_C(<board_id>, "<bringup>:<file>.launch.xml")` —
 *      empty-expansion macro the user's TU may carry as a doc / IDE
 *      hint. Declarative only; cmake fn drives codegen.
 *
 *   2. `nros_board_native_run(nros_node_register_fn entry)` — the
 *      C-FFI Board adapter the generated TU calls. Owns the
 *      `nros::init() → entry(context) → nros::spin() →
 *      nros::shutdown()` lifecycle.
 *
 * Phase 212.L.2 keeps Entry pkgs `native`-only at the cmake surface
 * for v1.
 */

#ifndef NROS_MAIN_H
#define NROS_MAIN_H

#include "nros/node_pkg.h"
#include "nros/visibility.h"

#ifdef __cplusplus
extern "C" {
#endif

/* Phase 257 (W0-A, RFC-0043) — typed C Entry lifecycle. The C-ABI sibling of
 * the C++ `NativeBoard::run_components`; the generated typed C TU (emitted by
 * `nros codegen entry --lang c --typed`) calls this from `main`. `setup` is
 * invoked once after `init`, with the executor handle, to create each node and
 * `configure` its component on the real executor; then this pumps the executor
 * (init → setup → spin → shutdown). Returns 0 on graceful exit, else the first
 * non-zero `setup` / spin code. Defined in nros-cpp (the typed runtime). */
typedef int32_t (*nros_c_entry_setup_fn)(void* executor);
NROS_PUBLIC int32_t nros_board_native_run_components(nros_c_entry_setup_fn setup);

/* Phase 266 (W5b) — named variant: `session_name` sets the primary session /
 * node name visible via `ros2 node list` (the #98 fix for C entries). NULL or
 * empty → falls back to `"node"`. The generated typed C entry (emitted by
 * `nros codegen entry --lang c --typed`) calls this from `main`, passing
 * `nros_boot_config_node_name(&NROS_BOOT_CONFIG)`. Defined in nros-cpp. */
NROS_PUBLIC int32_t nros_board_native_run_components_named(const char* session_name,
                                                           nros_c_entry_setup_fn setup);

#ifdef __cplusplus
} /* extern "C" */
#endif

/* Phase 219.E — `NROS_MAIN_C(<board_id>, "<launch_spec>")` declarative
 * marker. Expands to a sentinel TU-local symbol; the cmake fn detects
 * presence via `target_compile_definitions` to avoid double-emit when
 * the user wrote it. The generated TU (emitted by
 * `nano_ros_entry(LAUNCH …)`) carries the canonical `int main()` body
 * either way.
 *
 * Usage:
 *
 *   #include <nros/main.h>
 *   NROS_MAIN_C(nros_board_native, "demo_bringup:system.launch.xml")
 */
#define NROS_MAIN_C(BoardId, LaunchSpec)                                                           \
    NROS_PUBLIC const unsigned char __nros_entry_macro_present = 1;                                \
    _Static_assert(sizeof(LaunchSpec) > 1, "NROS_MAIN_C: launch spec must be a non-empty literal")

#endif /* NROS_MAIN_H */
