#[=======================================================================[.rst:
NanoRosLink
-----------

CMake helper functions for linking nano-ros platform + RMW
backends onto user targets.

Phase 123.A.6 (locked in
`docs/roadmap/phase-123-build-and-api-revision.md`). The
functions are the user-visible API surface for the decoupled
core / platform / RMW archive design — see the phase doc.
Today's implementation maps onto the existing combined
``libnros_{c,cpp}_<rmw>[_<platform>].a`` archive layout +
emits the right link arguments. Once the platform-cffi /
RMW-cffi canonical-ABI binary split lands (Phase 121 +
Stream A follow-ups), the function bodies swap to linking
three separate archives without the user CMakeLists
changing a line.

Functions
^^^^^^^^^

``nano_ros_link_platform(<target> [PLATFORM <plat>])``
  Link the nano-ros platform backend onto ``<target>``.
  Resolves the platform as: explicit ``PLATFORM`` arg →
  ``NANO_ROS_DEFAULT_PLATFORM`` cache var →
  ``NANO_ROS_PLATFORM`` (the value NanoRos was installed with).
  Errors out if no platform can be resolved or if the resolved
  value isn't supported by the install.

``nano_ros_link_rmw(<target> [RMW <rmw>])``
  Link the nano-ros RMW backend onto ``<target>``. Same
  resolution chain via ``NANO_ROS_DEFAULT_RMW`` /
  ``NANO_ROS_RMW``. Errors out if the resolved RMW's
  archive isn't present in the install.

Cache variables
^^^^^^^^^^^^^^^

``NANO_ROS_DEFAULT_PLATFORM``
  Workspace-level default platform for
  ``nano_ros_link_platform()`` calls without an explicit
  ``PLATFORM`` argument. Set via ``-D`` at CMake configure
  time or in a top-level workspace ``CMakeLists.txt``.

``NANO_ROS_DEFAULT_RMW``
  Workspace-level default RMW for ``nano_ros_link_rmw()``
  calls without an explicit ``RMW`` argument.

Both default to the values NanoRos itself was built with
(``NANO_ROS_PLATFORM`` / ``NANO_ROS_RMW``) so that one-archive
installs Just Work without extra cache vars.

#]=======================================================================]

# ----------------------------------------------------------------------------
# Internal — resolve a (kind, requested, default) tuple into a final value.
# ----------------------------------------------------------------------------
function(_nano_ros_resolve_choice OUT_VAR KIND REQUESTED FALLBACK_LIST)
    if(REQUESTED)
        set(${OUT_VAR} "${REQUESTED}" PARENT_SCOPE)
        return()
    endif()
    foreach(_candidate IN LISTS FALLBACK_LIST)
        if(${_candidate})
            set(${OUT_VAR} "${${_candidate}}" PARENT_SCOPE)
            return()
        endif()
    endforeach()
    message(FATAL_ERROR
        "nano_ros_link_${KIND}(): no ${KIND} specified and no fallback found.\n"
        "  Pass `${KIND} <value>` explicitly, or set NANO_ROS_DEFAULT_${KIND} / "
        "NANO_ROS_${KIND} via -D at CMake configure time.")
endfunction()

# Convert "platform" / "rmw" → "PLATFORM" / "RMW" for cache var lookups.
function(_nano_ros_kind_upper OUT_VAR KIND)
    string(TOUPPER "${KIND}" _u)
    set(${OUT_VAR} "${_u}" PARENT_SCOPE)
endfunction()

# ----------------------------------------------------------------------------
# nano_ros_link_platform(<target> [PLATFORM <platform>])
# ----------------------------------------------------------------------------
# Phase 123.A.1.x.5 — map platform tag → (cmake-package, target-short-tag).
# Multiple input tags collapse to the same short tag because the
# install ships one `.a` per platform family (e.g.
# `freertos_armcm3` + a future `freertos_armcm4` both consume
# `libnros_platform_freertos.a`).
function(_nano_ros_platform_targets OUT_PKG OUT_SHORT PLAT)
    if(PLAT STREQUAL "posix")
        set(${OUT_PKG} "NrosPlatformPosix" PARENT_SCOPE)
        set(${OUT_SHORT} "posix" PARENT_SCOPE)
    elseif(PLAT MATCHES "^freertos")
        set(${OUT_PKG} "NrosPlatformFreertos" PARENT_SCOPE)
        set(${OUT_SHORT} "freertos" PARENT_SCOPE)
    elseif(PLAT MATCHES "^threadx")
        set(${OUT_PKG} "NrosPlatformThreadx" PARENT_SCOPE)
        set(${OUT_SHORT} "threadx" PARENT_SCOPE)
    elseif(PLAT MATCHES "^nuttx")
        set(${OUT_PKG} "NrosPlatformNuttx" PARENT_SCOPE)
        set(${OUT_SHORT} "nuttx" PARENT_SCOPE)
    elseif(PLAT STREQUAL "zephyr")
        set(${OUT_PKG} "NrosPlatformZephyr" PARENT_SCOPE)
        set(${OUT_SHORT} "zephyr" PARENT_SCOPE)
    else()
        set(${OUT_PKG} "" PARENT_SCOPE)
        set(${OUT_SHORT} "" PARENT_SCOPE)
    endif()
endfunction()

function(nano_ros_link_platform TARGET)
    cmake_parse_arguments(ARG "" "PLATFORM" "" ${ARGN})
    if(ARG_UNPARSED_ARGUMENTS)
        message(FATAL_ERROR
            "nano_ros_link_platform: unexpected argument(s): "
            "${ARG_UNPARSED_ARGUMENTS}")
    endif()

    _nano_ros_resolve_choice(_chosen "PLATFORM" "${ARG_PLATFORM}"
        "NANO_ROS_DEFAULT_PLATFORM;NANO_ROS_PLATFORM")

    # Track the resolved platform on the target as a custom property.
    set_property(TARGET ${TARGET} PROPERTY NANO_ROS_PLATFORM "${_chosen}")

    # Phase 123.A.1.x.5 — link the standalone platform archive.
    # Post-A.1.x.2/3, libnros_c.a references `nros_platform_*` symbols
    # as undefined; the platform archive supplies them. Per-target
    # platform override works because nros-c is platform-agnostic at
    # the symbol level.
    _nano_ros_platform_targets(_pkg _short "${_chosen}")
    if(_pkg)
        if(NOT TARGET ${_pkg}::nros_platform_${_short})
            include(CMakeFindDependencyMacro)
            find_dependency(${_pkg} CONFIG)
        endif()
        if(TARGET ${_pkg}::nros_platform_${_short})
            target_link_libraries(${TARGET}
                PRIVATE ${_pkg}::nros_platform_${_short})
        endif()
    endif()
endfunction()

# ----------------------------------------------------------------------------
# nano_ros_link_rmw(<target> [RMW <rmw>])
# ----------------------------------------------------------------------------
# Phase 123.A.1.x.5 — map RMW tag → installed CMake package + target.
function(_nano_ros_rmw_targets OUT_PKG OUT_NAME RMW)
    if(RMW STREQUAL "zenoh")
        set(${OUT_PKG} "NrosRmwZenoh" PARENT_SCOPE)
        set(${OUT_NAME} "NrosRmwZenoh" PARENT_SCOPE)
    elseif(RMW STREQUAL "xrce")
        set(${OUT_PKG} "NrosRmwXrce" PARENT_SCOPE)
        set(${OUT_NAME} "NrosRmwXrce" PARENT_SCOPE)
    elseif(RMW STREQUAL "cyclonedds")
        set(${OUT_PKG} "NrosRmwCyclonedds" PARENT_SCOPE)
        set(${OUT_NAME} "NrosRmwCyclonedds" PARENT_SCOPE)
    else()
        set(${OUT_PKG} "" PARENT_SCOPE)
        set(${OUT_NAME} "" PARENT_SCOPE)
    endif()
endfunction()

function(nano_ros_link_rmw TARGET)
    cmake_parse_arguments(ARG "" "RMW" "" ${ARGN})
    if(ARG_UNPARSED_ARGUMENTS)
        message(FATAL_ERROR
            "nano_ros_link_rmw: unexpected argument(s): "
            "${ARG_UNPARSED_ARGUMENTS}")
    endif()

    _nano_ros_resolve_choice(_chosen "RMW" "${ARG_RMW}"
        "NANO_ROS_DEFAULT_RMW;NANO_ROS_RMW")

    # Phase 123.A.11 — RMW mismatch guard removed. nros-c is now
    # truly RMW-agnostic at the binary level (libnros_c.a is
    # identical across NANO_ROS_RMW selections — verified by
    # sha256 equality in A.11.6). Per-target RMW override works.
    set_property(TARGET ${TARGET} PROPERTY NANO_ROS_RMW "${_chosen}")

    # Phase 249 P2b — the universal registration stub. This block writes the
    # single STRONG def of `nros_app_register_backends()` per `nano_ros_link_rmw`
    # invocation, calling each linked backend's `nros_rmw_<x>_register` fn.
    # Multiple calls (e.g. zenoh + xrce on a bridge node) accumulate into one
    # stub. There is NO weak default anymore (removed in P4a): if this stub is
    # not emitted, `nros_app_register_backends` is undefined → LINK ERROR, never
    # a silent no-op opening a session with no backend (the #48-class hazard).
    #
    # POSIX hosts: the strong def harmlessly duplicates the work the backend's
    # .init_array ctor (phase 104.A) does. `nros_rmw_<x>_register` is idempotent
    # — the runtime vtable slot accepts the most-recent-write.
    # Bare-metal targets without .init_array: the strong def IS the only register
    # call site; without this stub no backend would ever register and
    # `Executor::open` would fail.
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
    set(_stub_content "/* Phase 249 P2b — auto-generated by nano_ros_link_rmw().\n")
    string(APPEND _stub_content " * The SOLE (strong) def of nros_app_register_backends() — there is no\n")
    string(APPEND _stub_content " * weak default; absence is a link error (phase-249 P4a).\n")
    string(REPLACE ";" ", " _rmw_list_pretty "${_existing_rmws}")
    string(APPEND _stub_content " * Backends registered: ${_rmw_list_pretty}\n")
    string(APPEND _stub_content " */\n")
    foreach(_rmw_name IN LISTS _existing_rmws)
        string(APPEND _stub_content
            "extern int nros_rmw_${_rmw_name}_register(void);\n")
    endforeach()
    string(APPEND _stub_content
        "void nros_app_register_backends(void) {\n")
    foreach(_rmw_name IN LISTS _existing_rmws)
        string(APPEND _stub_content
            "    (void)nros_rmw_${_rmw_name}_register();\n")
    endforeach()
    string(APPEND _stub_content "}\n")
    file(WRITE "${_stub_path}" "${_stub_content}")
    target_sources(${TARGET} PRIVATE "${_stub_path}")

    # Some RTOS installs keep the RMW registration symbols inside the
    # platform-specific NanoRos archive (`libnros_{c,cpp}_zenoh_<platform>.a`).
    # The standalone `NrosRmwZenoh::NrosRmwZenoh` target is the host POSIX
    # archive, so linking it into these fixtures either produces a file-format
    # error (cross targets) or opens the wrong transport stack (ThreadX Linux,
    # ThreadX RISC-V QEMU).
    if((NANO_ROS_PLATFORM STREQUAL "freertos_armcm3"
            OR NANO_ROS_PLATFORM STREQUAL "threadx_linux"
            OR NANO_ROS_PLATFORM STREQUAL "threadx_riscv64")
            AND _chosen STREQUAL "zenoh")
        return()
    endif()

    # Phase 123.A.1.x.5 — link the standalone RMW archive. nros-c
    # references `nros_rmw_<rmw>_register` as undefined; the
    # standalone archive supplies it. `--allow-multiple-definition`
    # reconciles per-archive copies of `compiler_builtins` +
    # `nros-rmw-cffi` rlib content.
    _nano_ros_rmw_targets(_pkg _name "${_chosen}")
    if(_pkg)
        if(NOT TARGET ${_pkg}::${_name})
            include(CMakeFindDependencyMacro)
            find_dependency(${_pkg} CONFIG)
        endif()
        if(TARGET ${_pkg}::${_name})
            target_link_libraries(${TARGET} PRIVATE ${_pkg}::${_name})
        endif()
        # RFC-0042 D3 — force the backend register entry via `-u` (the cffi C ABI
        # is single-definition now via the nros-rmw-cffi-provider archive, so the
        # blind `--allow-multiple-definition` mask is gone). Always route through
        # `-Wl,…` — the gcc/lld compiler driver does the link.
        target_link_options(${TARGET} PRIVATE
            "-Wl,-u,nros_rmw_${_chosen}_register")
    endif()
endfunction()
