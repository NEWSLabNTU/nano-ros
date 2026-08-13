# phase-314 — the ONE place the runtime's cargo feature list is computed.
#
# Before this, the list was assembled independently in three sites — nros-c,
# nros-cpp and the workspace umbrella in NanoRosRuntimeCrate. They did not
# agree: two hardcoded `ros-humble` while only the umbrella honoured the
# configured edition, the umbrella had no `NANO_ROS_BOARD` input so it could not
# split threadx-linux (std) from riscv64-qemu (no_std), and the capabilities
# existed on the direct paths only.
#
# The failures that produced were all SILENT:
#
#  * a consumer hook (`NROS_EXTRA_CPP_FEATURES`) added to one assembly did
#    nothing on the others — the phase-308 metadata probe linked a
#    libnros_cpp.a with no `nros_cpp_metadata_dump` in it (issue 0304);
#  * a non-humble build compiles the runtime as humble while codegen bakes
#    other type_hashes, which links, boots and fails to interoperate.
#
# See issue 0311 and docs/roadmap/phase-314-feature-set-ssot.md.

include_guard(GLOBAL)

# phase-347 W3 — `NROS_RMW_KNOWN` / `nros_rmw_is_known()`, derived from the
# per-backend `nros-rmw.toml` descriptors (RFC-0071).
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosRmwDispatch.cmake")

# nros_feature_set(<out>
#     CRATE        <c|cpp>               which crate's feature vocabulary
#     EDITION      <humble|iron|jazzy>   default: NANO_ROS_ROS_EDITION, else humble
#     RMW          <zenoh|xrce|cyclonedds|uorb|none>
#     PLATFORM     <posix|freertos|nuttx|threadx|esp_idf|…>
#     BOARD        <board id>            accepted but UNUSED since phase-338 W5.a
#                                        (the threadx tier now derives from
#                                        CMAKE_CROSSCOMPILING, not board identity);
#                                        kept so callers need not change
#     CAPABILITIES <param_services;lifecycle;safety;…>
#     [NO_STD_CROSS]                     force the embedded tier regardless of
#                                        CMAKE_CROSSCOMPILING (native_sim is a
#                                        cross build that wants the std tier)
# )
#
# Writes the cargo feature list into <out> in the caller's scope.
function(nros_feature_set out_var)
    cmake_parse_arguments(_FS "NO_STD_CROSS" "CRATE;EDITION;RMW;PLATFORM;BOARD" "CAPABILITIES" ${ARGN})

    # ---- edition -----------------------------------------------------------
    # RFC-0056: the edition drives the runtime keyexpr format, which must match
    # the codegen-baked type_hash. Hardcoding it here is a WIRE mismatch, not a
    # build error, so an unknown value fails loudly instead of defaulting.
    set(_edition "${_FS_EDITION}")
    if(NOT _edition AND DEFINED NANO_ROS_ROS_EDITION AND NOT NANO_ROS_ROS_EDITION STREQUAL "")
        set(_edition "${NANO_ROS_ROS_EDITION}")
    endif()
    if(NOT _edition)
        set(_edition humble)
    endif()
    if(NOT (_edition STREQUAL "humble" OR _edition STREQUAL "iron"
            OR _edition STREQUAL "jazzy"))
        message(FATAL_ERROR
            "nros_feature_set: unknown ROS edition '${_edition}' "
            "(expected: humble, iron, jazzy). The edition selects a cargo "
            "feature that must match the codegen-baked type_hash.")
    endif()
    set(_feats "ros-${_edition}")

    # ---- rmw ---------------------------------------------------------------
    # Selection happens at link time (`NanoRos::Rmw::<name>`); the feature only
    # decides which backend the umbrella bundles into the one Rust staticlib.
    # phase-347 W3 — validity comes from the DESCRIPTORS, not a list kept here.
    # This used to enumerate zenoh/xrce/cyclonedds/uorb/none while
    # `nros_rmw_dispatch` accepted only the first three and FATAL_ERROR'd on
    # uorb: two lists disagreeing about the same tree. Both now read
    # `NROS_RMW_KNOWN`, which is derived from `packages/rmw/*/*/nros-rmw.toml`.
    nros_rmw_is_known("${_FS_RMW}" _fs_rmw_known)
    if(NOT _fs_rmw_known AND NOT _FS_RMW STREQUAL "none")
        message(FATAL_ERROR
            "nros_feature_set: unknown RMW '${_FS_RMW}' "
            "(provided by a descriptor: ${NROS_RMW_KNOWN}; or 'none')")
    endif()
    # The two crates spell the same selection differently — nros-cpp has
    # `rmw-{zenoh,xrce}-cffi`, nros-c has `cffi-zenoh-cffi` / `cffi-xrce-c`.
    # That is a real vocabulary difference, not an alias, so CRATE selects the
    # spelling rather than the caller post-processing the list. (Renaming the
    # features to match would be a nicer end state, but it is a separate change
    # with its own blast radius.)
    if(_FS_CRATE STREQUAL "c")
        if(_FS_RMW STREQUAL "zenoh")
            list(APPEND _feats cffi-zenoh-cffi)
        elseif(_FS_RMW STREQUAL "xrce")
            list(APPEND _feats cffi-xrce-c)
        else()
            list(APPEND _feats rmw-cffi)
        endif()
    elseif(_FS_CRATE STREQUAL "cpp")
        if(_FS_RMW STREQUAL "zenoh")
            list(APPEND _feats rmw-zenoh-cffi)
        elseif(_FS_RMW STREQUAL "xrce")
            list(APPEND _feats rmw-xrce-cffi)
        else()
            list(APPEND _feats rmw-cffi)
        endif()
    else()
        message(FATAL_ERROR
            "nros_feature_set: CRATE must be 'c' or 'cpp' (got '${_FS_CRATE}') — "
            "the two crates spell the rmw feature differently.")
    endif()

    # ---- platform ----------------------------------------------------------
    # Kept from the DIRECT path, deliberately: the umbrella's old helper had no
    # BOARD input and so could not split the two threadx tiers. Unifying onto
    # the helper would have regressed threadx (phase-314 W1).
    set(_cross ${_FS_NO_STD_CROSS})
    if(NOT _cross AND CMAKE_CROSSCOMPILING)
        set(_cross TRUE)
    endif()
    if(_FS_PLATFORM STREQUAL "posix")
        list(APPEND _feats std platform-posix)
    elseif(_FS_PLATFORM STREQUAL "freertos" OR _FS_PLATFORM STREQUAL "freertos_armcm3"
           OR _FS_PLATFORM STREQUAL "esp_idf")
        # ESP-IDF is Espressif's FreeRTOS port — same no_std tier.
        list(APPEND _feats alloc panic-halt platform-freertos)
    elseif(_FS_PLATFORM STREQUAL "nuttx" OR _FS_PLATFORM STREQUAL "nuttx_armv7a")
        list(APPEND _feats std platform-nuttx)
    elseif(_FS_PLATFORM STREQUAL "threadx_linux")
        list(APPEND _feats std platform-threadx)
    elseif(_FS_PLATFORM STREQUAL "threadx_riscv64")
        list(APPEND _feats alloc panic-halt platform-threadx)
    elseif(_FS_PLATFORM STREQUAL "threadx")
        # phase-338 W5.a — what distinguishes the two ThreadX tiers is whether
        # the target has a hosted libc, and that is exactly `_cross`: the Linux
        # sim is a host build, the RV64 QEMU target is a cross build (its
        # `[board.cmake] toolchain_file` sets CMAKE_SYSTEM_NAME).
        #
        # This used to match the board NAME and FATAL_ERROR on anything else,
        # which meant a third ThreadX board could not exist without editing this
        # file — a board identity standing in for a property, the defect
        # RFC-0064 records. Deriving it from `_cross` generalizes: any future
        # ThreadX board lands in the right tier with no edit here, and the
        # `BOARD` argument is no longer load-bearing for this decision.
        if(_cross)
            list(APPEND _feats alloc panic-halt platform-threadx)
        else()
            list(APPEND _feats std platform-threadx)
        endif()
    elseif(_cross)
        # Unknown embedded cross target: no_std + alloc, matching the board tier
        # so nros-serdes / nros-params never pull `std`.
        list(APPEND _feats alloc panic-halt "platform-${_FS_PLATFORM}")
    else()
        message(FATAL_ERROR
            "nros_feature_set: unknown PLATFORM '${_FS_PLATFORM}' (expected: posix, "
            "freertos, freertos_armcm3, nuttx, nuttx_armv7a, threadx, threadx_linux, "
            "threadx_riscv64, esp_idf)")
    endif()

    # ---- capabilities ------------------------------------------------------
    # Image-level, not platform-level. They used to be a `PLATFORM STREQUAL
    # posix` test on the direct paths only, so a MIXED workspace (which takes
    # the umbrella) silently lost them — phase-314 W1 established that as a gap
    # rather than intent.
    #
    # `param_services` / `lifecycle` still imply hosted: both are alloc-gated,
    # so an embedded image opts in explicitly rather than getting them by
    # default.
    foreach(_cap IN LISTS _FS_CAPABILITIES)
        if(_cap STREQUAL "")
        elseif(_cap STREQUAL "param_services")
            list(APPEND _feats param-services)
        elseif(_cap STREQUAL "lifecycle")
            list(APPEND _feats lifecycle-services)
        elseif(_cap STREQUAL "safety")
            # zenoh-only: the CRC path lives in that backend.
            if(_FS_RMW STREQUAL "zenoh")
                list(APPEND _feats safety-e2e)
            else()
                message(WARNING
                    "nros_feature_set: capability 'safety' ignored — only the zenoh "
                    "RMW carries the CRC path (got RMW=${_FS_RMW}).")
            endif()
        else()
            message(FATAL_ERROR
                "nros_feature_set: unknown capability '${_cap}' "
                "(known: param_services, lifecycle, safety)")
        endif()
    endforeach()

    # ---- consumer extension point -----------------------------------------
    # Applied ONCE, by construction. This is the hook whose per-path duplication
    # caused issue 0304; there is now only one place for it to be missed.
    #
    # issue 0542 — and it is applied PER CRATE, because one place is not enough
    # when that place serves two. `NROS_EXTRA_CPP_FEATURES` used to be appended
    # to whatever crate was being assembled, so the metadata probe's
    # `set(NROS_EXTRA_CPP_FEATURES "metadata-mode")` reached `nros-c` too — and
    # `metadata-mode` exists only on `nros-cpp` (`git log -S` shows nros-c never
    # had it). Every C/C++ probe build then died with "the package 'nros-c' does
    # not contain this feature", so no C/C++ component could regenerate its
    # metadata sidecar.
    #
    # The variable already NAMED its crate; the assembly just ignored the name.
    # `CRATE` is a required argument here, so honouring it costs nothing and
    # makes the mistake unspellable rather than merely absent.
    if(_FS_CRATE STREQUAL "cpp" AND NROS_EXTRA_CPP_FEATURES)
        list(APPEND _feats ${NROS_EXTRA_CPP_FEATURES})
    endif()
    if(_FS_CRATE STREQUAL "c" AND NROS_EXTRA_C_FEATURES)
        list(APPEND _feats ${NROS_EXTRA_C_FEATURES})
    endif()

    list(REMOVE_DUPLICATES _feats)
    set(${out_var} "${_feats}" PARENT_SCOPE)
endfunction()
