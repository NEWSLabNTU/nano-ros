#[=======================================================================[.rst:
nros_find_interfaces (Zephyr)
-----------------------------

High-level ``package.xml`` SSoT entry point for Zephyr applications, the
companion to the native ``cmake/NanoRosGenerateInterfaces.cmake``
``nros_find_interfaces()``.

It reads the example's ``package.xml`` (the single source of truth for
interface dependencies), resolves the *transitive* interface closure via
the host ``nros codegen resolve-deps`` tool, then generates bindings for
every required package in topological order.

The native function delegates to the native ``nros_generate_interfaces``
(which emits a standalone static library). This Zephyr copy mirrors the
native resolve-deps behaviour 1:1 but delegates to the **Zephyr**
``nros_generate_interfaces`` (``zephyr/cmake/nros_generate_interfaces.cmake``),
which emits generated sources straight into the Zephyr ``app`` target. That
keeps example CMake uniform across platforms — a Zephyr C example writes the
exact same line a native C example does:

.. code-block:: cmake

  nros_find_interfaces(LANGUAGE C SKIP_INSTALL)

Arguments (match the native signature/semantics):
  ``PACKAGE_XML``  — path to package.xml (default: ``${CMAKE_CURRENT_SOURCE_DIR}/package.xml``)
  ``LANGUAGE``     — ``C`` or ``CPP`` (default ``CPP``)
  ``SKIP_INSTALL`` — accepted for parity; Zephyr emits into ``app`` (no
                     install layout) so it is recognised + threaded through
                     to ``nros_generate_interfaces`` but has no effect.
  ``ROS_EDITION``  — ``humble`` (default).

Prerequisites:
  ``nros_generate_interfaces.cmake`` must already be included so the Zephyr
  ``nros_generate_interfaces`` function (and the ``_NROS_ZEPHYR_CODEGEN_TOOL``
  it resolves) are available. ``zephyr/CMakeLists.txt`` includes both files
  together inside the C / C++ API paths.

#]=======================================================================]

# =========================================================================
# nros_find_interfaces([PACKAGE_XML <path>] [LANGUAGE C|CPP] [SKIP_INSTALL])
# =========================================================================
function(nros_find_interfaces)
  cmake_parse_arguments(_ARG
    "SKIP_INSTALL"
    "PACKAGE_XML;LANGUAGE;ROS_EDITION"
    ""
    ${ARGN}
  )

  if(NOT DEFINED _ARG_PACKAGE_XML OR _ARG_PACKAGE_XML STREQUAL "")
    set(_ARG_PACKAGE_XML "${CMAKE_CURRENT_SOURCE_DIR}/package.xml")
  endif()

  if(NOT EXISTS "${_ARG_PACKAGE_XML}")
    message(FATAL_ERROR
      "nros_find_interfaces: package.xml not found at ${_ARG_PACKAGE_XML}")
  endif()

  if(NOT DEFINED _ARG_LANGUAGE OR _ARG_LANGUAGE STREQUAL "")
    set(_ARG_LANGUAGE "CPP")
  endif()

  if(NOT DEFINED _ARG_ROS_EDITION OR _ARG_ROS_EDITION STREQUAL "")
    set(_ARG_ROS_EDITION "humble")
  endif()

  # The Zephyr nros_generate_interfaces.cmake resolves the host codegen
  # tool into `_NROS_ZEPHYR_CODEGEN_TOOL` at include time. Fall back to the
  # canonical `_NANO_ROS_CODEGEN_TOOL` so the function is robust if it is
  # ever called before the Zephyr resolver ran.
  set(_codegen_tool "${_NROS_ZEPHYR_CODEGEN_TOOL}")
  if(NOT _codegen_tool)
    set(_codegen_tool "${_NANO_ROS_CODEGEN_TOOL}")
  endif()
  if(NOT _codegen_tool)
    message(FATAL_ERROR
      "nros_find_interfaces: nros codegen tool not resolved. Include "
      "zephyr/cmake/nros_generate_interfaces.cmake first (zephyr/CMakeLists.txt "
      "does this inside the C / C++ API paths).")
  endif()

  # 1. Resolve the transitive interface closure at configure time. Identical
  #    invocation to the native function — resolve-deps is platform-agnostic;
  #    it emits a cmake script setting `_NROS_RESOLVED_PACKAGES` plus per-pkg
  #    `_NROS_RESOLVED_<pkg>_FILES` (absolute interface-file paths).
  set(_resolve_output "${CMAKE_CURRENT_BINARY_DIR}/_nros_resolved_deps.cmake")
  execute_process(
    COMMAND "${_codegen_tool}" codegen resolve-deps
            --package-xml "${_ARG_PACKAGE_XML}"
            --output-cmake "${_resolve_output}"
    RESULT_VARIABLE _result
    ERROR_VARIABLE _stderr
  )
  if(NOT _result EQUAL 0)
    message(FATAL_ERROR
      "nros-codegen resolve-deps failed (exit ${_result}):\n${_stderr}")
  endif()

  # 2. Pull in the resolved package list + per-package files.
  include("${_resolve_output}")

  if(NOT _NROS_RESOLVED_PACKAGES)
    message(WARNING
      "nros_find_interfaces: no interface packages resolved from ${_ARG_PACKAGE_XML}")
    return()
  endif()

  # 3. Generate each resolved package in topological order, delegating to the
  #    Zephyr nros_generate_interfaces (emit-into-`app`). As in the native
  #    function, pass ALL already-processed packages as DEPENDENCIES (a
  #    superset of the transitive closure) so the C++ FFI include!() chain
  #    sees every cross-package type. The C path ignores the surplus.
  set(_all_preceding_pkgs "")
  foreach(_pkg ${_NROS_RESOLVED_PACKAGES})
    set(_skip "")
    if(_ARG_SKIP_INSTALL)
      set(_skip "SKIP_INSTALL")
    endif()

    nros_generate_interfaces(${_pkg}
      ${_NROS_RESOLVED_${_pkg}_FILES}
      DEPENDENCIES ${_all_preceding_pkgs}
      LANGUAGE ${_ARG_LANGUAGE}
      ROS_EDITION ${_ARG_ROS_EDITION}
      ${_skip}
    )

    # Re-export the per-package vars nros_generate_interfaces set (CPP path)
    # to the caller's scope so a downstream consumer can read the closure.
    set(${_pkg}_GENERATED_RS_FILES "${${_pkg}_GENERATED_RS_FILES}" PARENT_SCOPE)

    list(APPEND _all_preceding_pkgs "${_pkg}")
  endforeach()
endfunction()
