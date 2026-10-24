/* Phase 119.3 stub — DO NOT include from production C++ code without
 * the build system supplying the real header.
 *
 * `nros_cpp_config_generated.h` is produced per-build by
 * `nros-cpp/build.rs` and written ONLY to that build script's cargo
 * `$OUT_DIR`, as
 *   <OUT_DIR>/nros-cpp-generated/rclcpp/detail/nros_cpp_config_generated.h
 * (issue 1824 retired the flat `$CARGO_TARGET_DIR/nros-cpp-generated/` copy).
 * The real header carries `#define NROS_CPP_CONFIG_VARIANT "<features>"` and
 * references a matching symbol in the archive, making a mismatch a link error
 * rather than a silent `_opaque` overflow (issue 0360).
 *
 * How a build reaches it — the same roads as the C header (see
 * `nros-c/include/rclc/detail/nros_config_generated.h`). A C++ image also
 * compiles against the C header, which nros-c's build script writes, so a
 * direct `cargo build` places BOTH packages:
 *       python3 scripts/build/cargo-out-dir-headers.py --package nros-cpp
 *         --package nros-c --dest <dir> -- cargo build …
 *         --message-format=json-render-diagnostics
 *       -I"$(nros locate artifact cpp-include --placed-dir <dir>)"
 *       -I"$(nros locate artifact c-include --placed-dir <dir>)"
 *
 * If this stub's `#error` fires, your build system has NOT been
 * configured to supply the real header. See
 * docs/roadmap/phase-119-3-cmake-setup.md for the dispatch model.
 */

#if defined(NROS_CPP_CONFIG_OPTIONAL) && !defined(NROS_CONFIG_BUILDLESS) && !defined(__cplusplus)

/* Issue 0282 — OPTIONAL probe (see nros-c/include/rclc/component.h): the
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

/* Issue 1569 — BUILDLESS header checks only; see the C stub. No platform
 * dispatches here any more: NuttX took the committed snapshot until its FFI
 * build was given the per-build header, and a snapshot sized every NuttX image
 * from a hand-kept number that fell below the build four times. */
#if defined(NROS_CONFIG_BUILDLESS)
#include "rclcpp/detail/nros_cpp_config_generated_buildless.h"
#else
#error "nros_cpp_config_generated.h must be supplied per-build by the build system; see the comment in this stub for guidance."
#endif

#endif /* NROS_CPP_CONFIG_GENERATED_H */

#endif /* optional probe */
