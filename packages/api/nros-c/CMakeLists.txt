# packages/api/nros-c/CMakeLists.txt
#
# The C API's CMake surface. Its archive is nros-cpp's (one cargo run per
# image, phase-578 W8): see `nros_c-static` below.
#
# This file is included from the top-level CMakeLists.txt via add_subdirectory().
# Two variables control the build:
#   NROS_RMW      — RMW backend: zenoh, xrce, or dds (default: zenoh)
#   NROS_PLATFORM — target platform: posix, freertos_armcm3, nuttx_armv7a
#                       (default: posix)

# ---- Platform (default: posix) ----
if(NOT DEFINED NROS_PLATFORM)
    set(NROS_PLATFORM "posix")
endif()

# Phase 123.A.11 — nros-c is RMW-agnostic at the Cargo level. The
# `cffi-{xrce,dds,zenoh}-*` features still exist as no-op aliases
# of `rmw-cffi` (Cargo backwards compat) but choosing a backend
# happens at LINK time via `find_package(NrosRmw<X>)` +
# `nano_ros_link_rmw`. Each backend's `.init_array` ctor (phase
# 104.A) registers it before `main()`; nros-c itself stays
# unchanged across all RMW selections.
#
# The `NROS_RMW` value still drives downstream linkage
# (NanoRosCTargets.cmake pulls the right standalone archive) +
# the install lib name suffix, but feeds NOTHING extra into
# nros-c's Cargo build. Result: one `libnros_c.a` per target
# triple.
# ---- Cargo features: ONE computation, shared with nros-cpp + the umbrella ----
# phase-314 — was ~90 lines of inline rmw/platform mapping that hardcoded
# `ros-humble` and applied capabilities only on posix. See
# cmake/NanoRosFeatureSet.cmake and issue 0311 for why the duplication was a
# defect, not a tidy-up.
# Relative to THIS file, not ${NROS_REPO_DIR}: a raw `add_subdirectory(<repo>)`
# consumer (the phase-138 smoke shape) never defines NROS_REPO_DIR.
include("${CMAKE_CURRENT_LIST_DIR}/../../../cmake/NanoRosFeatureSet.cmake")
option(NROS_SAFETY_E2E "Enable E2E message-integrity (CRC) — zenoh only" OFF)
set(_caps ${NROS_FEATURES})
if(NROS_SAFETY_E2E)
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
# `NROS_FEATURES` was never populated on the workspace path (issue 0353). So
# hosted could not fail when a declaration was missing, and the same
# `system.toml` produced different runtimes depending on where it was built.
#
# W1 made `nano_ros_workspace()` resolve the axes from `SYSTEM` before the
# import, so the declaration now arrives on its own and this can go.
nros_feature_set(_features
    CRATE        c
    RMW          "${NROS_RMW}"
    PLATFORM     "${NROS_PLATFORM}"
    BOARD        "${NROS_BOARD}"
    CAPABILITIES "${_caps}")


# ---- Build nros-c via Corrosion ----
# phase-336 — PROFILE is passed explicitly. Without it Corrosion falls back to
# its own CMAKE_BUILD_TYPE mapping (Debug→dev, EVERYTHING else→release), so a
# Debug or MinSizeRel build of the C/C++ half still got a fat-LTO cargo build.
# Relative to THIS file, like the FeatureSet include above — a raw
# `add_subdirectory(<repo>)` consumer never defines NROS_REPO_DIR.
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
#
# issue 0835 — the first field is the resolved FEATURE SET, not
# `platform=${NROS_PLATFORM}`. `NROS_PLATFORM` carries two meanings at
# once: a platform FAMILY for cmake dispatch and a platform VARIANT for tier
# selection, and this directory only ever cared about the second. Keying on the
# label hashed a cmake-dispatch detail that merely CORRELATES with the artifacts.
#
# Measured before the change, on one board, from the `.key` files themselves:
#
#   platform=threadx         |rmw=zenoh     |board=rv-virt-threadx|…
#   platform=threadx_riscv64 |rmw=zenoh     |board=rv-virt-threadx|…
#   platform=threadx         |rmw=cyclonedds|board=rv-virt-threadx|…
#   platform=threadx_riscv64 |rmw=cyclonedds|board=rv-virt-threadx|…
#
# — four groups differing in nothing but the label, where two would do, because
# `NanoRosFeatureSet.cmake` resolves BOTH spellings to `alloc platform-threadx`
# on this board (`threadx` takes its `_cross` arm; rv-virt-threadx IS cross).
# ThreadX was the only platform that split: freertos has one group, native's four
# and threadx-linux's two differ by `rmw` or `caps`, which are real.
#
# The label is safe to drop because it is not load-bearing for CARGO, checked
# rather than assumed: `NROS_PLATFORM` reaches no cargo environment and no
# build script reads it. It stays load-bearing for cmake dispatch, where five
# sites key on the exact string — this changes what the DIRECTORY is keyed on,
# not what the variable means.
#
# Sorted, because a key must not depend on the order the features happened to be
# appended in.
#
# phase-439 W1 (RFC-0094 D4) — plus the KNOB values, which this key carried none
# of. The fields above are the inputs `nros_feature_set()` is a function of, and
# a pool size is not one of them: two images at the same features/rmw/board/caps
# differing only in `ZPICO_MAX_QUERYABLES` hashed IDENTICALLY and the second took
# the first's uplifted `libnros_c.a`. `nros_knob_key_fields()` is the ONE
# spelling of "what is the knob half" — never a list here.
if(COMMAND nros_share_corrosion_cargo_dir)
    string(REPLACE ";" "," _caps_flat "${_caps}")
    set(_features_key ${_features})
    list(SORT _features_key)
    string(REPLACE ";" "," _features_flat "${_features_key}")
    nros_knob_key_fields(_knob_fields)
    set(_nros_c_shared_key
        "features=${_features_flat}"
        "rmw=${NROS_RMW}"
        "board=${NROS_BOARD}"
        "caps=${_caps_flat}"
        "profile=${NROS_CARGO_PROFILE}"
        "target=${Rust_CARGO_TARGET}"
        )
    nros_share_corrosion_cargo_dir(KEY ${_nros_c_shared_key} ${_knob_fields})
    # Issue 1700 — the key is not complete yet. The entity facts
    # (`NROS_SIZING_DESCRIPTOR`, `NROS_DECLARED_*`, …) reach this crate's cargo
    # command through `nros_entity_facts_env_deferred` below, i.e. only at the
    # END of the configure, after every entry has registered — and they change
    # what the build scripts probe (executor size 20664 with a leaf's
    # descriptor, 90648 without). Keyed without them, a C leaf and a C++ leaf
    # shared one directory, one leaf's header was the other's, and `nros-cpp`
    # refused the build. The deferred flush re-keys on them; it needs this half.
    if(NROS_SHARED_CARGO_DIR)
        set_property(GLOBAL PROPERTY NROS_SHARED_CARGO_BASE_KEY "${_nros_c_shared_key}")
    endif()
endif()

# ONE cargo run per image (phase-578 W8). The C API's archive is the C++ one.
#
# `libnros_cpp.a` bundles nros-c (nros-cpp depends on it as an rlib), so it
# defines the whole C ABI, and every image on this road links it: the root adds
# both crates, so `nros_runtime_umbrella_expr` always picks the C++ umbrella. This
# file used to import nros-c as a crate of its own too. Its archive was linked by
# almost nothing, but its cargo run (about 20 s of a first build) was the sole
# producer of the C sizes header. So a C image compiled against one cargo run's
# header and linked another run's archive: the two-producer shape behind issues
# 0805, 0978 and 1824, held off only by the variant stamp turning a mismatch into
# a link error.
#
# Now `nros_c-static` is an INTERFACE over `nros_cpp-static`. The C header comes
# from the nros-cpp run that builds the linked archive: nros-cpp places and
# mirrors nros-c's header as well as its own, and puts that mirror on this
# target's include path (packages/api/nros-cpp/CMakeLists.txt). No
# `cargo-build_nros_c` exists, so every `if(TARGET cargo-build_nros_c)` ordering
# site stands aside, and `nros_c_config_header` orders on the nros-cpp mirror.
#
# What stays here is what was never per-crate. The shared cargo dir is keyed on
# nros-c's feature set (nros-cpp's names the same nros-c unit, issue 1100), set
# up before nros-cpp's import so its cargo run rides the link. The witness that
# cargo really wrote into it is armed on the crate that now builds, from
# nros-cpp; the dir is passed through a global because that file is a sibling
# scope.
# NuttX never reaches this file (the root skips both crates there), and on bare
# metal the board's startup crate and linker scripts ride nros-cpp's run
# (`LINK_SCRIPTS` there).
set_property(GLOBAL PROPERTY NROS_SHARED_CARGO_DIR_FOR_RUNTIME "${NROS_SHARED_CARGO_DIR}")
add_library(nros_c-static INTERFACE)
target_link_libraries(nros_c-static INTERFACE nros_cpp-static)
target_include_directories(nros_c-static INTERFACE
    "${CMAKE_CURRENT_SOURCE_DIR}/../../platform/nros-platform-api/include"
    "${CMAKE_CURRENT_SOURCE_DIR}/include"
)

# Phase 140 — all install(...) rules deleted. nros-c is consumed
# in-tree via `add_subdirectory(<repo-root>)`; the root CMake's
# `NanoRos::NanoRos` INTERFACE target wires up the build-dir variant
# header path (`target_include_directories` above) directly.
