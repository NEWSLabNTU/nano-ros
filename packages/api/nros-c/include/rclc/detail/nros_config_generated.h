/* Phase 119.3 stub — DO NOT include from production C code without
 * the build system supplying the real header.
 *
 * `nros_config_generated.h` is produced per-build by `nros-c/build.rs`
 * and written ONLY to that build script's cargo `$OUT_DIR`, as
 *   <OUT_DIR>/nros-c-generated/rclc/detail/nros_config_generated.h
 * (issue 1824 retired the flat `$CARGO_TARGET_DIR/nros-c-generated/` copy,
 * which was whichever build wrote it last). The real header carries
 *   #define NROS_CONFIG_VARIANT "<sorted_underscore_joined_features>"
 * plus a reference to a matching symbol in libnros_c.a, so a header/archive
 * mismatch is an undefined reference NAMING the variant it wanted instead of a
 * silent `_opaque` overflow at runtime (issue 0360).
 *
 * How a build reaches it:
 *   - CMake `add_subdirectory(nano-ros)`: nros-c's CMakeLists places the
 *     header from the import's own cargo run and mirrors it into
 *     ${CMAKE_CURRENT_BINARY_DIR}/include/rclc/detail/, on nros_c-static's
 *     INTERFACE include path.
 *   - Zephyr, NuttX, PX4: each places it the same way (see each integration).
 *   - A cargo dependent: `DEP_NROS_C_CONFIG_INCLUDE` (the `links` channel).
 *   - Direct `cargo build`: run it under the placer and add the placed dir —
 *       python3 scripts/build/cargo-out-dir-headers.py --package nros-c
 *         --dest <dir> -- cargo build … --message-format=json-render-diagnostics
 *       -I"$(nros locate artifact c-include --placed-dir <dir>)"
 *
 * If this stub's `#error` fires, your build system has NOT been
 * configured to supply the real header.
 */

#ifndef NROS_CONFIG_GENERATED_H
#define NROS_CONFIG_GENERATED_H

/* Issue 1569 — the committed snapshot is for BUILDLESS header checks only
 * (`NROS_CONFIG_BUILDLESS`, which no build system defines — gated). It used to
 * be selected by `NROS_PLATFORM_NUTTX` and sized every NuttX image from a
 * hand-kept number; NuttX now gets the per-build header like every platform,
 * and a NuttX TU that reaches this stub is a build-system bug that must fail
 * here, not compile against a guess. */
#if defined(NROS_CONFIG_BUILDLESS)
#include "rclc/detail/nros_config_generated_buildless.h"
#else
// clang-format off
#error "nros_config_generated.h must be supplied per-build by the build system; see this stub for guidance."
// clang-format on
#endif

#endif /* NROS_CONFIG_GENERATED_H */
