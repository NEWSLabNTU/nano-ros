# packages/api/nros-c/CMakeLists.txt
#
# Builds the nros-c static library via Corrosion (Cargo integration).
# Installs library (RMW + platform suffixed), headers, and CMake config package.
#
# This file is included from the top-level CMakeLists.txt via add_subdirectory().
# Two variables control the build:
#   NANO_ROS_RMW      — RMW backend: zenoh, xrce, or dds (default: zenoh)
#   NANO_ROS_PLATFORM — target platform: posix, freertos_armcm3, nuttx_armv7a
#                       (default: posix)

# ---- Platform (default: posix) ----
if(NOT DEFINED NANO_ROS_PLATFORM)
    set(NANO_ROS_PLATFORM "posix")
endif()

# Phase 123.A.11 — nros-c is RMW-agnostic at the Cargo level. The
# `cffi-{xrce,dds,zenoh}-*` features still exist as no-op aliases
# of `rmw-cffi` (Cargo backwards compat) but choosing a backend
# happens at LINK time via `find_package(NrosRmw<X>)` +
# `nano_ros_link_rmw`. Each backend's `.init_array` ctor (phase
# 104.A) registers it before `main()`; nros-c itself stays
# unchanged across all RMW selections.
#
# The `NANO_ROS_RMW` value still drives downstream linkage
# (NanoRosCTargets.cmake pulls the right standalone archive) +
# the install lib name suffix, but feeds NOTHING extra into
# nros-c's Cargo build. Result: one `libnros_c.a` per target
# triple.
# ---- Cargo features: ONE computation, shared with nros-cpp + the umbrella ----
# phase-314 — was ~90 lines of inline rmw/platform mapping that hardcoded
# `ros-humble` and applied capabilities only on posix. See
# cmake/NanoRosFeatureSet.cmake and issue 0311 for why the duplication was a
# defect, not a tidy-up.
# Relative to THIS file, not ${NANO_ROS_ROOT}: a raw `add_subdirectory(<repo>)`
# consumer (the phase-138 smoke shape) never defines NANO_ROS_ROOT.
include("${CMAKE_CURRENT_LIST_DIR}/../../../cmake/NanoRosFeatureSet.cmake")
option(NANO_ROS_SAFETY_E2E "Enable E2E message-integrity (CRC) — zenoh only" OFF)
set(_caps ${NANO_ROS_FEATURES})
if(NANO_ROS_SAFETY_E2E)
    list(APPEND _caps safety)
endif()
# issue 0805 — the append does not check, and a declaration that ALREADY names
# `safety` produced `caps=safety,safety`. Cargo dedupes features so nothing was
# mis-built, but the list is also an input to the shared-cargo key, and a key
# that varies with how a list was assembled rather than with the configuration
# makes two identical leaves stop sharing. The key normalises defensively; this
# fixes the source so the duplicate never travels.
list(REMOVE_DUPLICATES _caps)
# phase-323 W2 — capabilities come from the DECLARATION on every platform.
#
# posix used to get `param_services` + `lifecycle` unconditionally. That was not
# a convenience: until W1 it was the ONLY route those axes took on hosted, since
# `NANO_ROS_FEATURES` was never populated on the workspace path (issue 0353). So
# hosted could not fail when a declaration was missing, and the same
# `system.toml` produced different runtimes depending on where it was built.
#
# W1 made `nano_ros_workspace()` resolve the axes from `SYSTEM` before the
# import, so the declaration now arrives on its own and this can go.
nros_feature_set(_features
    CRATE        c
    RMW          "${NANO_ROS_RMW}"
    PLATFORM     "${NANO_ROS_PLATFORM}"
    BOARD        "${NANO_ROS_BOARD}"
    CAPABILITIES "${_caps}")


# ---- Build nros-c via Corrosion ----
# phase-336 — PROFILE is passed explicitly. Without it Corrosion falls back to
# its own CMAKE_BUILD_TYPE mapping (Debug→dev, EVERYTHING else→release), so a
# Debug or MinSizeRel build of the C/C++ half still got a fat-LTO cargo build.
# Relative to THIS file, like the FeatureSet include above — a raw
# `add_subdirectory(<repo>)` consumer never defines NANO_ROS_ROOT.
include("${CMAKE_CURRENT_LIST_DIR}/../../../cmake/NanoRosCargoProfile.cmake")

nros_resolve_cargo_profile()

# Issue 0805 — collapse this leaf's private Corrosion cargo dir onto a shared
# one. No-op unless the caller passed `-DNROS_SHARED_CARGO_ROOT`.
#
# The KEY is deliberately the full input set `nros_feature_set()` above is a
# function of, plus the things that change the artifact without changing the
# feature list (profile, target triple). Equal key therefore implies equal
# `nros-c` AND `nros-cpp` features by construction — which matters because
# cargo uplifts the archive to an UNHASHED `libnros_c.a`, so two leaves with
# different features sharing a directory would overwrite each other's archive
# and one would silently link the other's.
#
# Placement is load-bearing and cost one debugging round. It must be AFTER
# BOTH `nros_feature_set()` (capabilities) AND `nros_resolve_cargo_profile()`
# (NROS_CARGO_PROFILE). Sitting between them made the key UNSTABLE: the
# profile is a cache variable, so it read empty on a fresh configure and
# `release` on the next one, and the same leaf computed two different keys.
# The mismatch guard caught it — which is the argument for the guard, and
# for printing the key TEXT rather than two hashes. Corrosion only needs the
# link before the BUILD, not before the import, so late placement is free.
#
# Each field is LABELLED, and that is load-bearing rather than decorative: an
# empty element vanishes from a cmake list, so passing the bare values would
# make (board="", caps="x") and (board="x", caps="") hash IDENTICALLY — two
# different configurations sharing one directory, which is precisely the defect
# being fixed. A label keeps every field non-empty and positional.
if(COMMAND nros_share_corrosion_cargo_dir)
    string(REPLACE ";" "," _caps_flat "${_caps}")
    nros_share_corrosion_cargo_dir(KEY
        "platform=${NANO_ROS_PLATFORM}"
        "rmw=${NANO_ROS_RMW}"
        "board=${NANO_ROS_BOARD}"
        "caps=${_caps_flat}"
        "profile=${NROS_CARGO_PROFILE}"
        "target=${Rust_CARGO_TARGET}")
endif()

corrosion_import_crate(
    MANIFEST_PATH "${PROJECT_SOURCE_DIR}/Cargo.toml"
    CRATES        nros-c
    CRATE_TYPES   staticlib
    NO_DEFAULT_FEATURES
    FEATURES      ${_features}
    PROFILE       ${NROS_CARGO_PROFILE}
    LOCKED
)
# The preset's definition, so the crate resolves `--profile nros-…` without a
# `[profile.*]` block of its own. No-op for a user-owned profile.
nros_cargo_profile_env(nros_c-static)
# issue 0657 — the riscv64 float ABI, per target (see the helper's comment).
nros_riscv64_rustflags_env(nros_c-static)
# phase-372 W1 — the cortex-r52 FPU cflags, same class one arch over.
nros_armv8r_cflags_env(nros_c-static)

# ---- In-tree variant header (Phase 137) ----
# `nros-c`'s build.rs writes the per-build `nros_config_generated.h` to
# `${CORROSION_BUILD_DIR}/nros_config_generated.h` (== this file's
# `CMAKE_CURRENT_BINARY_DIR`). Consumers `#include <nros/nros_config_generated.h>`
# — i.e. through a `nros/` subdir. The install-time path materialises that
# layout under `<prefix>/include/nros_c/nros/`; for in-tree consumers
# (Phase 137 `add_subdirectory(...)`) we mirror the header into a sibling
# `include/nros/` dir and add it to `nros_c-static`'s INTERFACE so any
# target linking against `nros_c-static` (or the `NanoRos::NanoRos`
# umbrella declared at the root) gets the per-build sizes without an
# install step.
set(_nros_c_intree_include_dir "${CMAKE_CURRENT_BINARY_DIR}/include")
file(MAKE_DIRECTORY "${_nros_c_intree_include_dir}/nros")
# Issue 0088 — mirror as a first-class OUTPUT + custom target, NOT a POST_BUILD
# side effect. A `POST_BUILD` on `cargo-build_nros_c` with only `BYPRODUCTS`
# gives consumers no real Ninja edge to the mirrored file: `add_dependencies(
# <consumer> cargo-build_nros_c)` orders against the cargo *primary output*, so a
# consumer TU can compile before the POST_BUILD copy runs and pick up the in-tree
# stub header (intermittent by build order). An `OUTPUT` command + a custom target
# (`nros_c_config_header`) that consumers `add_dependencies` on is the proper
# producer→consumer edge.
set(_nros_c_gen_config_header "${_nros_c_intree_include_dir}/nros/nros_config_generated.h")
# Issue 0268 — `DEPENDS <target-name>` alone is ORDER-ONLY in the generated Ninja
# graph (`|| cargo-build_nros_c`): it guarantees the cargo build runs FIRST, but a
# rebuilt crate never makes this copy out of date. Once the mirror exists it is
# never refreshed again, so an incremental build that grows the Rust `Executor`
# (e.g. 296-W3b.4 adding `monitor_states`) leaves a MUSEUM mirror on the include
# path while `${CMAKE_CURRENT_BINARY_DIR}/nros_config_generated.h` carries the new
# size. The C `_opaque` buffer is then sized from the stale header and the Rust
# object is placement-constructed 336 bytes past its end — memory corruption that
# surfaces as unrelated runtime failures (freertos C: register_subscription -1,
# zenoh session self-close). Adding the staticlib — a REAL Ninja output of the
# cargo edge — as a file-level DEPENDS gives the copy an actual input edge, so any
# cargo rebuild re-mirrors. `copy_if_different` keeps the mtime (and therefore
# consumer recompiles) tied to genuine content changes.
if(TARGET nros_c-static)
    set(_nros_c_mirror_trigger "$<TARGET_FILE:nros_c-static>")
else()
    set(_nros_c_mirror_trigger "")
endif()
add_custom_command(
    OUTPUT "${_nros_c_gen_config_header}"
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
        "${CMAKE_CURRENT_BINARY_DIR}/nros_config_generated.h"
        "${_nros_c_gen_config_header}"
    DEPENDS cargo-build_nros_c ${_nros_c_mirror_trigger}
    COMMENT "nros-c: mirroring nros_config_generated.h for in-tree consumers"
    VERBATIM
)
add_custom_target(nros_c_config_header DEPENDS "${_nros_c_gen_config_header}")
# Issue 0090 — export the absolute mirror-header path so NanoRosNodeRegister can set
# a FILE-level OBJECT_DEPENDS on it. The threadx carrier's link-time
# `add_dependencies(<carrier> nros_c_config_header)` does not stop a TU from
# compiling before the per-build header is mirrored (it then reads the in-tree stub
# → `*_OPAQUE_U64S undeclared`); a per-source OBJECT_DEPENDS is the hard edge.
set_property(GLOBAL PROPERTY NROS_C_CONFIG_HEADER_FILE "${_nros_c_gen_config_header}")
if(TARGET nros_c-static)
    target_include_directories(nros_c-static INTERFACE
        "${CMAKE_CURRENT_SOURCE_DIR}/../../platform/nros-platform-api/include"
        "${_nros_c_intree_include_dir}"
        "${CMAKE_CURRENT_SOURCE_DIR}/include"
    )
    # Issue 0123 — the mirror dir above is on nros_c-static's INTERFACE ahead of the
    # in-tree `#error` stub, but the `nros_c_config_header` mirror target is only built
    # if some target depends on it. The message-lib / entry / carrier paths wire that
    # dep explicitly (issues 0114/0090); a plain rclcpp/ament consumer that pulls
    # nros-c only transitively (e.g. the workspace-shadowing `consumer` via the
    # std_msgs binding) never does, so the mirror never runs during `all` → its dir
    # stays empty → the consumer reads the stub. Make nros_c-static ITSELF depend on
    # the mirror so ANY consumer that links it (directly or transitively) builds the
    # per-build header before compiling. (Verified: manually building the mirror then
    # the shadowing consumer compiles clean.)
    if(TARGET nros_c_config_header)
        add_dependencies(nros_c-static nros_c_config_header)
    endif()
endif()

# Phase 140 — all install(...) rules deleted. nros-c is consumed
# in-tree via `add_subdirectory(<repo-root>)`; the root CMake's
# `NanoRos::NanoRos` INTERFACE target wires up the build-dir variant
# header path (`target_include_directories` above) directly.
