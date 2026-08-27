# NanoRosLink.cmake
#
# User-facing helpers for linking nano-ros onto application targets.
#
# Phase 140 deleted the install-layout `find_package(NanoRos)` consumer
# path; the root `CMakeLists.txt` (pulled via `add_subdirectory(<repo>)`)
# is the only supported entry. That root already builds + links
# `NanoRos::NanoRos` against the platform shim + RMW staticlib chosen
# by `NANO_ROS_PLATFORM` / `NANO_ROS_RMW`. These helpers exist purely
# to:
#   * record per-target platform / RMW choice as CMake properties
#     (useful for downstream introspection),
#   * emit the `nros_app_register_backends()` strong-stub TU that
#     overrides the weak no-op in `libnros_c_weak_stubs.a` (Phase
#     104.B.6) — bare-metal builds without `.init_array` rely on this
#     stub being the only `nros_rmw_<x>_register` call site.
#
# Functions
# ^^^^^^^^^
#
# ``nano_ros_link_platform(<target> [PLATFORM <plat>])``
#   Annotates the target with its chosen platform. Linkage to
#   ``NanoRos::NanoRos`` is the caller's responsibility (already on
#   the target via the standard add_subdirectory + target_link_libraries
#   recipe).
#
# ``nano_ros_link_rmw(<target> [RMW <rmw>])``
#   Annotates + emits the register stub described above. RMW resolves
#   as: explicit ``RMW`` arg → ``NANO_ROS_DEFAULT_RMW`` cache var →
#   ``NANO_ROS_RMW``. Multiple invocations accumulate (e.g. a bridge
#   node registering zenoh + xrce).

# Phase 249 P2(a) — the generated RMW dispatch (`nros_rmw_dispatch(<rmw>)`), the SSoT
# for per-backend link data incl. `NROS_RMW_NEEDS_CXX_LINKER`. Generated from
# cargo-nano-ros `resolve_rmw()` (W13/R1), drift-guarded.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosRmwDispatch.cmake")

function(_nano_ros_resolve_choice OUT_VAR KIND REQUESTED FALLBACK_LIST)
    if(REQUESTED)
        set(${OUT_VAR} "${REQUESTED}" PARENT_SCOPE)
        return()
    endif()
    foreach(_var IN LISTS FALLBACK_LIST)
        if(DEFINED ${_var} AND NOT "${${_var}}" STREQUAL "")
            set(${OUT_VAR} "${${_var}}" PARENT_SCOPE)
            return()
        endif()
    endforeach()
    message(FATAL_ERROR
        "nano_ros_link_${KIND}: no ${KIND} resolved. "
        "Pass ${KIND} <name> explicitly, or set "
        "${FALLBACK_LIST} before calling.")
endfunction()

function(nano_ros_link_platform TARGET)
    cmake_parse_arguments(ARG "" "PLATFORM" "" ${ARGN})
    if(ARG_UNPARSED_ARGUMENTS)
        message(FATAL_ERROR
            "nano_ros_link_platform: unexpected argument(s): "
            "${ARG_UNPARSED_ARGUMENTS}")
    endif()
    _nano_ros_resolve_choice(_chosen "platform" "${ARG_PLATFORM}"
        "NANO_ROS_DEFAULT_PLATFORM;NANO_ROS_PLATFORM")
    set_property(TARGET ${TARGET} PROPERTY NANO_ROS_PLATFORM "${_chosen}")
endfunction()

function(nano_ros_link_rmw TARGET)
    cmake_parse_arguments(ARG "" "RMW" "" ${ARGN})
    if(ARG_UNPARSED_ARGUMENTS)
        message(FATAL_ERROR
            "nano_ros_link_rmw: unexpected argument(s): "
            "${ARG_UNPARSED_ARGUMENTS}")
    endif()
    _nano_ros_resolve_choice(_chosen "rmw" "${ARG_RMW}"
        "NANO_ROS_DEFAULT_RMW;NANO_ROS_RMW")
    set_property(TARGET ${TARGET} PROPERTY NANO_ROS_RMW "${_chosen}")

    # Issue 0475 — make a backend REBUILD relink this target.
    #
    # The Linux/BSD path whole-archives the RMW backend through a raw
    # `-Wl,--whole-archive,$<TARGET_FILE:...>` flag (root CMakeLists), because the
    # registration object must not be GC'd. CMake cannot see a file inside a flag
    # string, so it emits no link-rule dependency on it; the root's
    # `add_dependencies()` supplies only build ORDER, which ninja renders as an
    # ORDER-ONLY (`||`) edge — "must exist before linking", never "relink when it
    # changes".
    #
    # Consequence measured on examples/native/c/talker/build-cyclonedds: the
    # archive rebuilt at 14:15 while `c_talker` stayed at 12:28 and
    # `cmake --build` exited 0 doing nothing. The executable kept the OLD backend
    # indefinitely — museum binaries by construction — and only `rm -rf` on the
    # build dir cleared it (~687 s per leaf; Cyclone self-provisions from source).
    #
    # `LINK_DEPENDS` is the file-level edge the flag cannot carry: it attaches to
    # THIS target's link rule, so a changed archive relinks. Applied here rather
    # than at the root because this function is the one seam every consumer goes
    # through, whatever verb created the target.
    #
    # Verify: the archive must appear under `|` (implicit), not `||`, in
    #   ninja -C <build-dir> -t query <exe>
    if(TARGET nros_rmw_${_chosen})
        set_property(TARGET ${TARGET} APPEND PROPERTY
            LINK_DEPENDS "$<TARGET_FILE:nros_rmw_${_chosen}>")
    endif()

    # Issue 0837 — the SAME edge for every other file named in that flag.
    #
    # The fix above covered the backend archive and stopped there, but the flag
    # is a list: on Linux/BSD the Cyclone path whole-archives
    # `<nros_rmw_cyclonedds.a>,<libddsc.a>` in one string. `libddsc.a` had no
    # edge, so bumping the cyclonedds submodule rebuilt it and relinked nothing
    # — `lib/libddsc.a` at 18:06 against `c_talker` at 15:45, `cmake --build`
    # exiting 0 having done nothing, and the executable still carrying the old
    # CycloneDDS. The test-side staleness probe caught it (it walks the
    # submodule); the build side reported OK, which is how it survived.
    #
    # The producers APPEND to `NANO_ROS_LINK_DEPEND_FILES` on `NanoRos` rather
    # than each adding a line here, so an archive added to either flag in future
    # gets its edge by existing. That is the difference between fixing this site
    # and fixing the class — and this issue exists because the first fix did the
    # former.
    #
    # Verify: the file must appear under `|` (implicit), not `||` (order-only),
    # in `ninja -C <build-dir> -t query <exe>`.
    if(TARGET NanoRos)
        get_target_property(_nros_link_dep_files NanoRos NANO_ROS_LINK_DEPEND_FILES)
        if(_nros_link_dep_files)
            foreach(_f IN LISTS _nros_link_dep_files)
                if(_f)
                    set_property(TARGET ${TARGET} APPEND PROPERTY LINK_DEPENDS "${_f}")
                endif()
            endforeach()
        endif()
    endif()

    # Issue 0737 — bind the CycloneDDS this image was COMPILED against.
    #
    # `nano-ros-posix.cmake` already names this hazard on the self-provision
    # path, and answers it by building a STATIC ddsc so "there is no runtime
    # libddsc.so, hence no rpath needed and, crucially, no risk of ld.so
    # resolving the app's `libddsc.so.0` against a *different* system /opt/ros
    # Cyclone". The find_package path has the same exposure and had no guard.
    #
    # What that costs, measured: the `freertos-posix` cells compiled against the
    # SDK fork (`~/.nros/sdk/cyclonedds/0.10.5-nros1`, chosen by find_package)
    # and at RUNTIME loaded `/opt/ros/humble/lib/x86_64-linux-gnu/libddsc.so.0`,
    # because both carry SONAME `libddsc.so.0` and a sourced ROS puts its lib dir
    # on `LD_LIBRARY_PATH`. CMake had written the right directory into the
    # binary — as **DT_RUNPATH**, which ld.so searches AFTER `LD_LIBRARY_PATH`,
    # so it lost. The mismatch surfaced as `dds_stream_write_sample` refusing to
    # re-serialise every taken sample; the executor then discarded it and the
    # cell "published but never received" on a ROS-sourced host and passed on a
    # host without one. Forcing the matching library made the same binary deliver.
    #
    # DT_RPATH is searched BEFORE `LD_LIBRARY_PATH`, which is exactly the
    # property wanted here: an environment must not be able to substitute a
    # different build of the same SONAME under a running nano-ros image. That it
    # is no longer overridable is the point, not a side effect.
    # Keyed on the resolved library PATH, not on `TARGET CycloneDDS::ddsc`: that
    # imported target is created by `find_package` inside the backend's own
    # directory scope and is not visible from the leaf that builds the image, so
    # a `if(TARGET …)` here is silently false — the first cut of this block was,
    # and left the RUNPATH in place.
    if(_chosen STREQUAL "cyclonedds" AND UNIX AND NOT APPLE
       AND DEFINED NROS_RMW_CYCLONEDDS_DDSC_LIBRARY
       AND NROS_RMW_CYCLONEDDS_DDSC_LIBRARY MATCHES "\\.so(\\.[0-9]+)*$")
        set_property(TARGET ${TARGET} APPEND PROPERTY
            LINK_OPTIONS "-Wl,--disable-new-dtags")
    endif()

    # Phase 104.B.6 — accumulate the chosen RMW into the target's
    # `_NANO_ROS_LINKED_RMWS` list and (re)generate the strong-stub
    # `nros_app_register_backends()` TU. Idempotent across repeat calls.
    set(_stub_dir "${CMAKE_CURRENT_BINARY_DIR}/_nano_ros_link/${TARGET}")
    set(_stub_path "${_stub_dir}/nros_app_register_backends.c")
    get_target_property(_existing_rmws ${TARGET} _NANO_ROS_LINKED_RMWS)
    if(NOT _existing_rmws)
        set(_existing_rmws "")
    endif()
    list(APPEND _existing_rmws "${_chosen}")
    list(REMOVE_DUPLICATES _existing_rmws)
    set_property(TARGET ${TARGET} PROPERTY
        _NANO_ROS_LINKED_RMWS "${_existing_rmws}")

    file(MAKE_DIRECTORY "${_stub_dir}")
    set(_stub_content "/* Auto-generated by nano_ros_link_rmw().\n")
    string(APPEND _stub_content
        " * Strong def of nros_app_register_backends() overrides the\n")
    string(APPEND _stub_content
        " * weak no-op in libnros_c_weak_stubs.a (Phase 104.B.6).\n")
    string(REPLACE ";" ", " _rmw_list_pretty "${_existing_rmws}")
    string(APPEND _stub_content " * Backends registered: ${_rmw_list_pretty}\n")
    string(APPEND _stub_content " */\n")
    foreach(_rmw_name IN LISTS _existing_rmws)
        string(APPEND _stub_content
            "extern int nros_rmw_${_rmw_name}_register(void);\n")
    endforeach()
    string(APPEND _stub_content "void nros_app_register_backends(void) {\n")
    foreach(_rmw_name IN LISTS _existing_rmws)
        string(APPEND _stub_content
            "    (void)nros_rmw_${_rmw_name}_register();\n")
    endforeach()
    string(APPEND _stub_content "}\n")
    file(WRITE "${_stub_path}" "${_stub_content}")
    target_sources(${TARGET} PRIVATE "${_stub_path}")

    # Phase 177.27 / 249 P2(a) — some backends need the C++ linker driver on the
    # final line (libstdc++): cyclonedds's wrapper is C++ (operator new/delete,
    # std::nothrow), and when a C executable links it CMake's link-language
    # propagation can be lost (transitive pull / whole-archive) → the C driver is
    # picked and fails on unresolved C++ runtime symbols. WHICH backends need this
    # is now sourced from the R1 dispatch manifest (`NROS_RMW_NEEDS_CXX_LINKER`),
    # not a hardcoded `cyclonedds` literal — one SSoT. Idempotent / harmless for
    # C++ apps (already CXX) and hosts where propagation already works.
    foreach(_rmw_name IN LISTS _existing_rmws)
        nros_rmw_dispatch("${_rmw_name}")
        if(NROS_RMW_NEEDS_CXX_LINKER)
            set_target_properties(${TARGET} PROPERTIES LINKER_LANGUAGE CXX)
        endif()
    endforeach()
endfunction()
