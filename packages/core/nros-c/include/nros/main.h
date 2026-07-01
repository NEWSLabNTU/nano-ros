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

/* Phase 274.W2 (RFC-0015 Model 1) — per-tier spec for
 * `nros_board_native_run_tiers`.
 *
 * `name`           — tier name (null-terminated), informational.
 * `groups`         — array of `n_groups` null-terminated callback-group names;
 *                    NULL / 0 means wildcard (accept all groups).
 * `n_groups`       — number of entries in `groups`.
 * `priority`       — raw POSIX nice level (advisory).
 * `stack_bytes`    — informational on native (std::thread manages the stack).
 * `spin_period_us` — sleep between spin_once calls; 0 uses a 1 ms floor.
 * `setup`          — called once on the tier thread (after set_active_groups)
 *                    with the tier executor handle; returns 0 on success or
 *                    NULL to skip setup (tier receives no nodes of its own). */
typedef struct {
    const char* name;
    const char* const* groups;
    size_t n_groups;
    int64_t priority;
    size_t stack_bytes;
    uint64_t spin_period_us;
    nros_c_entry_setup_fn setup;
} nros_native_tier_spec_t;

/* Phase 274.W2 (RFC-0015 Model 1) — run a multi-tier native entry over one
 * shared RMW session. Opens ONE session on the boot thread; spawns one
 * std::thread per non-boot tier, each with a borrowed executor (no second
 * RMW session, no double-close). Each tier: open borrowed executor →
 * set_active_groups → setup(executor) → spin at spin_period_us. Boot thread
 * runs tier[0] on the owning executor and respects $NROS_ENTRY_SPIN_MS.
 * Returns after boot spin exits (NROS_ENTRY_SPIN_MS or spin error) after
 * joining all tier threads and closing the session. Defined in nros-cpp. */
NROS_PUBLIC int32_t nros_board_native_run_tiers(const char* session_name,
                                                const nros_native_tier_spec_t* tiers,
                                                size_t n_tiers);

/* Phase 274.W3 (RFC-0015 Model 1) — run a multi-tier embedded C/C++ entry on
 * FreeRTOS: open ONE RMW session, spawn one FreeRTOS task per non-boot tier
 * (each with a borrowed executor sharing the session), run the boot tier on the
 * caller's task (the startup.c app task). `locator` is the connect endpoint;
 * `domain_id` is the ROS domain id; `session_name` names the primary session.
 * Defined in nros-board-freertos (compiled by board's build.rs glue). */
NROS_PUBLIC int32_t nros_board_freertos_run_tiers(const char* locator, uint8_t domain_id,
                                                  const char* session_name,
                                                  const nros_native_tier_spec_t* tiers,
                                                  size_t n_tiers);

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
