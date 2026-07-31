/* Phase 119.3 stub — DO NOT include from production C code without
 * the build system supplying the real header.
 *
 * `nros_config_generated.h` is produced per-build by `nros-c/build.rs`
 * and written to
 *   $CARGO_TARGET_DIR/nros-c-generated/<variant_slug>/nros/nros_config_generated.h
 * where <variant_slug> = sorted underscore-joined cargo feature list.
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
