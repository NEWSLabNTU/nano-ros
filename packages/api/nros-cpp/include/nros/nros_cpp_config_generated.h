/* Phase 119.3 stub — DO NOT include from production C++ code without
 * the build system supplying the real header.
 *
 * `nros_cpp_config_generated.h` is produced per-build by
 * `nros-cpp/build.rs` and written to
 *   $CARGO_TARGET_DIR/nros-cpp-generated/nros/nros_cpp_config_generated.h
 * Issue 0360 — the `<variant_slug>/` level documented here was never
 * implemented; the path is FLAT, so two feature sets sharing a target dir
 * overwrite each other. The real header instead carries
 * `#define NROS_CPP_CONFIG_VARIANT "<features>"` and references a matching
 * symbol in the archive, making a mismatch a link error rather than a silent
 * `_opaque` overflow
 * (e.g. `platform-posix_rmw-zenoh_ros-humble_std`).
 *
 * Build systems pick the right variant header (Phase 140):
 *   - CMake `add_subdirectory(nano-ros)`: nros-cpp's CMakeLists mirrors
 *     the per-build header into ${CMAKE_CURRENT_BINARY_DIR}/include/nros/
 *     and adds it to nros-cpp-headers' INTERFACE include path BEFORE
 *     the source-tree stub.
 *   - Zephyr: zephyr/CMakeLists.txt prepends `${CMAKE_BINARY_DIR}/nros-rust`
 *     (the cargo target dir) so the per-build header wins.
 *   - Direct `cargo build`: add the per-build path to your compile
 *     flags manually, e.g.
 *       -I$CARGO_TARGET_DIR/nros-cpp-generated
 *
 * If this stub's `#error` fires, your build system has NOT been
 * configured to supply the real header. See
 * docs/roadmap/phase-119-3-cmake-setup.md for the dispatch model.
 */

#if defined(NROS_CPP_CONFIG_OPTIONAL) && !defined(NROS_PLATFORM_NUTTX) &&           \
    !defined(__cplusplus)

/* Issue 0282 — OPTIONAL probe (see nros-c/include/nros/component.h): the
 * includer only wants the generated `NROS_CPP_*_STORAGE_SIZE` values when
 * this build produces them and falls back to its own static defaults
 * otherwise. `__has_include` cannot tell this stub from the real header, so
 * probes announce themselves and we contribute NOTHING — deliberately
 * including NOT defining the include guard,, so a later MANDATORY include in
 * the same TU (e.g. nros-cpp/client.hpp, which needs
 * NROS_SERVICE_CLIENT_SIZE) still reaches the hard error below instead of
 * silently compiling against missing macros.
 *
 * The silent path is C-ONLY. A C++ TU that reaches this stub genuinely needs
 * the generated sizes (`nros-cpp/publisher.hpp` etc. use NROS_*_SIZE without
 * including this header themselves, relying on an earlier include), and has
 * no static fallback — so it must keep failing LOUDLY here rather than
 * cascading into a pile of `'storage_' was not declared` errors far from the
 * cause. */

#else

#ifndef NROS_CPP_CONFIG_GENERATED_H
#define NROS_CPP_CONFIG_GENERATED_H

#if defined(NROS_PLATFORM_NUTTX)
#include "nros/nros_cpp_config_generated_nuttx.h"
#else
#error "nros_cpp_config_generated.h must be supplied per-build by the build system; see the comment in this stub for guidance."
#endif

#endif /* NROS_CPP_CONFIG_GENERATED_H */

#endif /* optional probe */
