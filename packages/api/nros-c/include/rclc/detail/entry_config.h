/* SPDX-License-Identifier: Apache-2.0 */
/**
 * @file entry_config.h
 * @brief The compile-time connect configuration an entry bakes: `NROS_ENTRY_LOCATOR`
 *        and `NROS_ENTRY_DOMAIN_ID`.
 *
 * ONE derivation, in a C header, because both entry languages need it.
 *
 * ## Why this file exists
 *
 * The ladder used to live in `<rclcpp/detail/main.hpp>` — a C++ header — while the C
 * sibling `<rclc/detail/app_main.h>` defined the same two macros as `""` and `0` with
 * no derivation at all. Both are consumed as VALUES by `run_components` /
 * `run_sched_tasks` and by the CLI entry codegen, so the two spellings never
 * conflicted at link time; they simply disagreed about what the image dials.
 *
 * Today that costs nothing, because an embedded C entry is routed to the C++
 * emitter and compiles as a `.cpp` that includes `main.hpp`. The moment a pure
 * C entry becomes possible it stops being free: the image
 * would compile, link, boot — and dial nothing, because `""` reaches the
 * backend as "absent". That is a byte-for-byte re-run of an earlier silent no-connect, which the
 * XRCE arm below exists to fix, and it would arrive with no diagnostic.
 *
 * Same class as a fix one layer down: that gate made the CMAKE-side
 * locator ladders one producer. This makes the HEADER-side ones one producer.
 *
 * ## The contract
 *
 * Preprocessor only — no types, no includes, no linkage. Both `main.hpp` and
 * `app_main.h` include it at the point their own ladder used to sit, so an
 * entry may still bake a literal by defining either macro BEFORE the include
 * and the `#ifndef` guards leave it alone.
 *
 * Both macros must keep expanding to a value of the right kind:
 * `NROS_ENTRY_LOCATOR` to a STRING LITERAL (never a nullable pointer — callers
 * pass it by value) and `NROS_ENTRY_DOMAIN_ID` to an integer constant
 * expression.
 */
#ifndef NROS_ENTRY_CONFIG_H
#define NROS_ENTRY_CONFIG_H

/* ---- NROS_ENTRY_LOCATOR / NROS_ENTRY_DOMAIN_ID — the transport ROLES ----
 *
 * This header names no backend. It
 * used to read `CONFIG_NROS_ZENOH_LOCATOR`, synthesise XRCE's `addr:port` from
 * `CONFIG_NROS_XRCE_AGENT_{ADDR,PORT}` and prefer `CONFIG_NROS_CYCLONE_DOMAIN_ID`
 * under `NROS_RMW_CYCLONEDDS` — three backends' spellings in an API header, so
 * a fourth backend had nowhere to put its own.
 *
 * Who sets them now:
 *   * Zephyr — each backend's module file maps ITS Kconfig onto the role
 *     (`zephyr/cmake/nros_rmw_<backend>.cmake`, `zephyr_compile_definitions`):
 *     zenoh's locator, XRCE's agent `addr:port`, Cyclone's domain.
 *   * NuttX / FreeRTOS / ThreadX — `nano_ros_entry()` defines them from the
 *     entry-locator resolver (`cmake/NanoRosEntryLocator.cmake`).
 *
 * The bottom rungs are backend-neutral: the locator's is the EMPTY
 * STRING — "absent, let the backend discover" — which travels through
 * `rclc_support_init_with_locator` / `nros_cpp_init` to the boot-config
 * resolver's empty rung and then to the linked backend's own default. The
 * domain is compile-time on embedded, never a runtime env.
 */
#ifndef NROS_ENTRY_LOCATOR
#define NROS_ENTRY_LOCATOR ""
#endif /* NROS_ENTRY_LOCATOR */

#ifndef NROS_ENTRY_DOMAIN_ID
#if defined(CONFIG_NROS_DOMAIN_ID)
#define NROS_ENTRY_DOMAIN_ID CONFIG_NROS_DOMAIN_ID
#else
#define NROS_ENTRY_DOMAIN_ID 0
#endif
#endif /* NROS_ENTRY_DOMAIN_ID */

/* ---- NROS_ENTRY_DOMAIN_ID_C_ABI — the domain as a runner's `uint8_t` ----
 * The C ABI reads 0 as "not stated" and 255 (NROS_DOMAIN_ID_EXPLICIT_ZERO) as
 * an explicit domain 0. A host image whose system.toml STATES `domain_id = 0`
 * is baked with NROS_ENTRY_DOMAIN_STATED (cmake), so a differing
 * `$ROS_DOMAIN_ID` is reported as overriding it rather than read as nothing
 * stated. Without the flag this is NROS_ENTRY_DOMAIN_ID unchanged, which is
 * every RTOS image. */
#ifndef NROS_ENTRY_DOMAIN_ID_C_ABI
#if defined(NROS_ENTRY_DOMAIN_STATED) && (NROS_ENTRY_DOMAIN_ID) == 0
#define NROS_ENTRY_DOMAIN_ID_C_ABI 255
#else
#define NROS_ENTRY_DOMAIN_ID_C_ABI (NROS_ENTRY_DOMAIN_ID)
#endif
#endif /* NROS_ENTRY_DOMAIN_ID_C_ABI */

#endif /* NROS_ENTRY_CONFIG_H */
