/* Phase 119.3 stub — DO NOT include from production C code without
 * the build system supplying the real header.
 *
 * `nros_config_generated.h` is produced per-build by `nros-c/build.rs`
 * and written to
 *   $CARGO_TARGET_DIR/nros-c-generated/nros/nros_config_generated.h
 *
 * Issue 0360 — this used to document a `<variant_slug>/` directory level that
 * NOTHING EVER IMPLEMENTED. The path is flat, so two feature sets sharing a
 * target dir overwrite each other's header (and each other's archive). Rather
 * than describe a mechanism that does not exist, the real header now carries
 *   #define NROS_CONFIG_VARIANT "<sorted_underscore_joined_features>"
 * plus a reference to a matching symbol in libnros_c.a, so a header/archive
 * mismatch is an undefined reference NAMING the variant it wanted instead of a
 * silent `_opaque` overflow at runtime.
 *
 * Build systems pick the right variant header — see the matching stub
 * in `nros-cpp/include/nros/nros_cpp_config_generated.h` for the full
 * dispatch model. Short form (Phase 140):
 *   - CMake `add_subdirectory(nano-ros)`: nros-c's CMakeLists mirrors
 *     the per-build header into ${CMAKE_CURRENT_BINARY_DIR}/include/nros/
 *     and adds it to nros_c-static's INTERFACE include path.
 *   - Zephyr: zephyr/CMakeLists.txt prepends the cargo target dir.
 *   - Direct `cargo build`: add `-I$CARGO_TARGET_DIR/nros-c-generated/<slug>`.
 *
 * If this stub's `#error` fires, your build system has NOT been
 * configured to supply the real header.
 */

#ifndef NROS_CONFIG_GENERATED_H
#define NROS_CONFIG_GENERATED_H

#if defined(NROS_PLATFORM_NUTTX)
#include "nros/nros_config_generated_nuttx.h"
#else
// clang-format off
#error "nros_config_generated.h must be supplied per-build by the build system; see this stub for guidance."
// clang-format on
#endif

#endif /* NROS_CONFIG_GENERATED_H */
