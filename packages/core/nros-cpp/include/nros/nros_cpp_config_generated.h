/* Phase 119.3 stub — DO NOT include from production C++ code without
 * the build system supplying the real header.
 *
 * `nros_cpp_config_generated.h` is produced per-build by
 * `nros-cpp/build.rs` and written to
 *   $CARGO_TARGET_DIR/nros-cpp-generated/<variant_slug>/nros/nros_cpp_config_generated.h
 * where <variant_slug> = sorted underscore-joined cargo feature list
 * (e.g. `platform-posix_rmw-zenoh_ros-humble_std`).
 *
 * Build systems pick the right variant header:
 *   - CMake (find_package(NanoRos)):  NanoRosCppTargets.cmake prepends
 *     `include/nros_cpp_<rmw>[_<platform>]` to the include path; user
 *     code's `#include "nros/nros_cpp_config_generated.h"` resolves
 *     there before this stub.
 *   - Zephyr: zephyr/CMakeLists.txt prepends `${CMAKE_BINARY_DIR}/nros-rust`
 *     (the cargo target dir) so the per-build header wins.
 *   - Direct `cargo build`: add the per-build path to your compile
 *     flags manually, e.g.
 *       -I$CARGO_TARGET_DIR/nros-cpp-generated/<variant_slug>
 *
 * If this stub's `#error` fires, your build system has NOT been
 * configured to supply the real header. See
 * docs/roadmap/phase-119-3-cmake-setup.md for the dispatch model.
 */

#ifndef NROS_CPP_CONFIG_GENERATED_H
#define NROS_CPP_CONFIG_GENERATED_H

#error "nros_cpp_config_generated.h must be supplied per-build by the build system; see the comment in this stub for guidance."

#endif /* NROS_CPP_CONFIG_GENERATED_H */
