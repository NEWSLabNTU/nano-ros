# _NrosFindRosMsgPackage.cmake — Phase 210.A.2
#
# Smart Find-stub helper. The per-package Find<pkg>.cmake stubs delegate here
# with `_nros_find_ros_msg_package(<pkg>)`. This walks the layered interface-
# package search path, locates the named ROS msg package, runs nano-ros codegen
# on its IDL files, and emits the canonical `${pkg}::${pkg}` IMPORTED INTERFACE
# alias on top of `${pkg}__nano_ros_cpp`.
#
# Search path (highest → lowest priority):
#   1. NROS_INTERFACE_SEARCH_PATH  (env or cmake var; colon/semicolon separated)
#       Each entry is a colcon-`src/`-style root; immediate subdirs that hold a
#       `package.xml` are candidates.
#   2. AMENT_PREFIX_PATH           (env; ROS 2 install-prefix layout —
#       `<prefix>/share/<pkg>/{msg,srv,action}/`).
#   3. Bundled                     (`<nano-ros>/packages/interfaces/<pkg>/` and
#       `<nano-ros>/share/nano-ros/interfaces/<pkg>/`).
#
# Shadowing: a workspace shadowing an AMENT pkg takes the higher layer; we
# log a `message(STATUS ...)` line so the user knows which copy won.

# Idempotent include guard — the per-pkg stubs may pull this multiple times
# across a single configure pass.
if(_NROS_FIND_ROS_MSG_PACKAGE_INCLUDED)
    return()
endif()
set(_NROS_FIND_ROS_MSG_PACKAGE_INCLUDED TRUE)

# --- workspace-pkg Find-stub emission ----------------------------------------
#
# nano-ros ships Find-stubs for the well-known ROS msg packages (std_msgs,
# builtin_interfaces, …) — those route through `_nros_find_ros_msg_package`
# below. Workspace-local pkgs (`NROS_INTERFACE_SEARCH_PATH=src/`) have no
# pre-shipped Find-stub. `_nros_emit_workspace_find_stubs()` scans the
# search path, identifies each pkg by `<member_of_group>rosidl_interface_
# packages</member_of_group>` (the upstream marker for "this is a msg pkg")
# or by the presence of `msg/`/`srv/`/`action/` dirs, and writes a 2-line
# Find-stub per pkg into `${CMAKE_BINARY_DIR}/nros-find-stubs/` so a stock
# `find_package(<pkg>)` call from a consumer resolves through the smart
# helper. Idempotent; safe to call multiple times.
function(_nros_emit_workspace_find_stubs)
    set(_search_roots "")
    if(DEFINED NROS_INTERFACE_SEARCH_PATH AND NOT NROS_INTERFACE_SEARCH_PATH STREQUAL "")
        string(REPLACE ":" ";" _entries "${NROS_INTERFACE_SEARCH_PATH}")
        list(APPEND _search_roots ${_entries})
    endif()
    if(DEFINED ENV{NROS_INTERFACE_SEARCH_PATH} AND NOT "$ENV{NROS_INTERFACE_SEARCH_PATH}" STREQUAL "")
        string(REPLACE ":" ";" _env_entries "$ENV{NROS_INTERFACE_SEARCH_PATH}")
        list(APPEND _search_roots ${_env_entries})
    endif()

    if(NOT _search_roots)
        return()
    endif()

    set(_emit_dir "${CMAKE_BINARY_DIR}/nros-find-stubs")
    file(MAKE_DIRECTORY "${_emit_dir}")
    if(NOT "${_emit_dir}" IN_LIST CMAKE_MODULE_PATH)
        list(PREPEND CMAKE_MODULE_PATH "${_emit_dir}")
        set(CMAKE_MODULE_PATH "${CMAKE_MODULE_PATH}" PARENT_SCOPE)
    endif()

    foreach(_root ${_search_roots})
        if(NOT IS_DIRECTORY "${_root}")
            continue()
        endif()
        file(GLOB _pxs RELATIVE "${_root}" "${_root}/*/package.xml")
        foreach(_pxrel ${_pxs})
            get_filename_component(_pxdir "${_root}/${_pxrel}" DIRECTORY)
            file(READ "${_root}/${_pxrel}" _pxbody)
            # Extract the pkg's own <name>.
            if(NOT _pxbody MATCHES "<name>[ \t\r\n]*([A-Za-z0-9_-]+)[ \t\r\n]*</name>")
                continue()
            endif()
            string(REGEX REPLACE ".*<name>[ \t\r\n]*([A-Za-z0-9_-]+)[ \t\r\n]*</name>.*" "\\1" _pname "${_pxbody}")
            # Heuristic: emit a stub iff the pkg looks like a msg pkg —
            # member_of_group=rosidl_interface_packages OR has msg/srv/action
            # dirs. Non-msg pkgs (consumer apps) DON'T get a stub; find_package
            # for them goes through cmake's normal resolution.
            set(_is_msg_pkg FALSE)
            if(_pxbody MATCHES "<member_of_group>[ \t\r\n]*rosidl_interface_packages[ \t\r\n]*</member_of_group>")
                set(_is_msg_pkg TRUE)
            elseif(IS_DIRECTORY "${_pxdir}/msg" OR IS_DIRECTORY "${_pxdir}/srv" OR IS_DIRECTORY "${_pxdir}/action")
                set(_is_msg_pkg TRUE)
            endif()
            if(NOT _is_msg_pkg)
                continue()
            endif()
            set(_stub "${_emit_dir}/Find${_pname}.cmake")
            if(NOT EXISTS "${_stub}")
                file(WRITE "${_stub}"
                    "# Auto-emitted Find-stub for ${_pname} (Phase 210.A.4 workspace pkg).\n"
                    "include(\"${CMAKE_CURRENT_LIST_DIR}/_NrosFindRosMsgPackage.cmake\")\n"
                    "_nros_find_ros_msg_package(${_pname})\n"
                )
            endif()
        endforeach()
    endforeach()
endfunction()

# Emit workspace-pkg stubs immediately on first include — picks up any
# NROS_INTERFACE_SEARCH_PATH set BEFORE NrosRclcppCompat.cmake is pulled in.
_nros_emit_workspace_find_stubs()

# --- locate the codegen module so we can call nros_generate_interfaces ----
# The smart stub may be invoked before the consumer pulls in
# NanoRosGenerateInterfaces. Pull it in lazily (idempotent — guarded by the
# module's own load-once pattern).
get_filename_component(_nrm_stub_dir "${CMAKE_CURRENT_LIST_DIR}" ABSOLUTE)
get_filename_component(_nrm_cmake_dir "${_nrm_stub_dir}/../.." ABSOLUTE)
if(NOT COMMAND nros_generate_interfaces AND EXISTS "${_nrm_cmake_dir}/NanoRosGenerateInterfaces.cmake")
    include("${_nrm_cmake_dir}/NanoRosGenerateInterfaces.cmake")
endif()

# --- search-path resolution ---------------------------------------------------

# Walks the layered search path; sets <out_var> to the absolute pkg root dir
# (i.e. the dir containing `package.xml`), or NOTFOUND.
function(_nros_find_msg_package_root pkg out_var)
    set(${out_var} "NOTFOUND" PARENT_SCOPE)

    # Layer 1 — NROS_INTERFACE_SEARCH_PATH (env then cmake var; cmake wins).
    set(_search_roots "")
    if(DEFINED NROS_INTERFACE_SEARCH_PATH AND NOT NROS_INTERFACE_SEARCH_PATH STREQUAL "")
        string(REPLACE ":" ";" _entries "${NROS_INTERFACE_SEARCH_PATH}")
        list(APPEND _search_roots ${_entries})
    endif()
    if(DEFINED ENV{NROS_INTERFACE_SEARCH_PATH} AND NOT "$ENV{NROS_INTERFACE_SEARCH_PATH}" STREQUAL "")
        string(REPLACE ":" ";" _env_entries "$ENV{NROS_INTERFACE_SEARCH_PATH}")
        list(APPEND _search_roots ${_env_entries})
    endif()

    foreach(_root ${_search_roots})
        # Subdir-level match: <root>/<pkg>/package.xml
        if(EXISTS "${_root}/${pkg}/package.xml")
            set(${out_var} "${_root}/${pkg}" PARENT_SCOPE)
            return()
        endif()
        # File-glob fallback: any immediate subdir whose package.xml names <pkg>.
        # Useful when the dir name differs from the pkg name (rare; supported
        # for parity with colcon).
        file(GLOB _candidates RELATIVE "${_root}" "${_root}/*/package.xml")
        foreach(_pxrel ${_candidates})
            get_filename_component(_pxdir "${_root}/${_pxrel}" DIRECTORY)
            file(READ "${_root}/${_pxrel}" _pxbody)
            if(_pxbody MATCHES "<name>[ \t\r\n]*${pkg}[ \t\r\n]*</name>")
                set(${out_var} "${_pxdir}" PARENT_SCOPE)
                return()
            endif()
        endforeach()
    endforeach()

    # Layer 2 — AMENT_PREFIX_PATH.
    if(DEFINED ENV{AMENT_PREFIX_PATH})
        string(REPLACE ":" ";" _ament_paths "$ENV{AMENT_PREFIX_PATH}")
        foreach(_prefix ${_ament_paths})
            set(_share_root "${_prefix}/share/${pkg}")
            if(EXISTS "${_share_root}/package.xml")
                set(${out_var} "${_share_root}" PARENT_SCOPE)
                return()
            endif()
            # AMENT install layout may also drop the IDLs directly in
            # `<prefix>/share/<pkg>/{msg,srv,action}/` without a package.xml
            # (some installers strip it). Synthesize the candidate dir so
            # the caller can still glob IDLs out of it.
            if(EXISTS "${_share_root}/msg" OR EXISTS "${_share_root}/srv" OR EXISTS "${_share_root}/action")
                set(${out_var} "${_share_root}" PARENT_SCOPE)
                return()
            endif()
        endforeach()
    endif()

    # Layer 3 — bundled (in-tree packages/interfaces/<pkg> + share/nano-ros/).
    # _NANO_ROS_PREFIX is populated by NanoRosGenerateInterfaces.cmake on
    # first include.
    if(DEFINED _NANO_ROS_PREFIX AND NOT _NANO_ROS_PREFIX STREQUAL "")
        if(EXISTS "${_NANO_ROS_PREFIX}/packages/interfaces/${pkg}/package.xml")
            set(${out_var} "${_NANO_ROS_PREFIX}/packages/interfaces/${pkg}" PARENT_SCOPE)
            return()
        endif()
        if(EXISTS "${_NANO_ROS_PREFIX}/share/nano-ros/interfaces/${pkg}")
            set(${out_var} "${_NANO_ROS_PREFIX}/share/nano-ros/interfaces/${pkg}" PARENT_SCOPE)
            return()
        endif()
    endif()
endfunction()

# --- dependency scrape ------------------------------------------------------

# Extracts <depend>/<build_depend>/<exec_depend>/<run_depend>/<member_of_group>
# from a package.xml. Returns a list of dep package names in <out_var>.
function(_nros_parse_pkg_deps pxml out_var)
    set(_deps "")
    if(EXISTS "${pxml}")
        file(READ "${pxml}" _body)
        # Scrape the common ROS 2 dep tags. Lazy regex — captures the inner
        # text trimmed of whitespace. Covers <depend>, <build_depend>,
        # <exec_depend>, <run_depend>, <build_export_depend>.
        string(REGEX MATCHALL "<(depend|build_depend|exec_depend|run_depend|build_export_depend)[^>]*>[ \t\r\n]*([A-Za-z0-9_-]+)[ \t\r\n]*</(depend|build_depend|exec_depend|run_depend|build_export_depend)>" _matches "${_body}")
        foreach(_m ${_matches})
            string(REGEX REPLACE "<[^>]+>[ \t\r\n]*([A-Za-z0-9_-]+)[ \t\r\n]*</[^>]+>" "\\1" _name "${_m}")
            # Filter out non-msg deps (rosidl_default_generators / ament*).
            if(NOT _name MATCHES "^(rosidl|ament|rclcpp|rclpy|rcl|rmw|rosgraph|launch|catkin)")
                list(APPEND _deps "${_name}")
            endif()
        endforeach()
        list(REMOVE_DUPLICATES _deps)
    endif()
    set(${out_var} "${_deps}" PARENT_SCOPE)
endfunction()

# --- main entry --------------------------------------------------------------

# `_nros_find_ros_msg_package(<pkg>)` — the per-package Find<pkg>.cmake stub
# calls this, plus `set(<pkg>_FOUND ...)` to communicate success up to
# find_package's caller.
function(_nros_find_ros_msg_package pkg)
    # Idempotent — already wired this pkg in this configure pass?
    if(TARGET ${pkg}__nano_ros_cpp)
        if(NOT TARGET ${pkg}::${pkg})
            add_library(${pkg}::${pkg} ALIAS ${pkg}__nano_ros_cpp)
        endif()
        # Re-export the cached codegen output vars to the caller's scope so
        # multi-level dep chains (consumer → sensor_msgs → std_msgs) still
        # see std_msgs's GENERATED_RS_FILES even when std_msgs hit the
        # idempotent fast-return path (an earlier find_package(local_msgs)
        # already wired std_msgs; cmake fn-PARENT_SCOPE only propagates ONE
        # level so std_msgs's vars aren't in sensor_msgs's caller scope
        # without re-export here). Source the cache var stashed below.
        if(DEFINED _NROS_PKG_${pkg}_GENERATED_RS_FILES)
            set(${pkg}_GENERATED_RS_FILES "${_NROS_PKG_${pkg}_GENERATED_RS_FILES}" PARENT_SCOPE)
        endif()
        if(DEFINED _NROS_PKG_${pkg}_GENERATED_HEADERS)
            set(${pkg}_GENERATED_HEADERS "${_NROS_PKG_${pkg}_GENERATED_HEADERS}" PARENT_SCOPE)
        endif()
        if(DEFINED _NROS_PKG_${pkg}_GENERATED_SOURCES)
            set(${pkg}_GENERATED_SOURCES "${_NROS_PKG_${pkg}_GENERATED_SOURCES}" PARENT_SCOPE)
        endif()
        if(DEFINED _NROS_PKG_${pkg}_INCLUDE_DIRS)
            set(${pkg}_INCLUDE_DIRS "${_NROS_PKG_${pkg}_INCLUDE_DIRS}" PARENT_SCOPE)
        endif()
        set(${pkg}_LIBRARIES "${pkg}__nano_ros_cpp" PARENT_SCOPE)
        set(${pkg}_FOUND TRUE PARENT_SCOPE)
        return()
    endif()

    _nros_find_msg_package_root(${pkg} _pkg_root)
    if(_pkg_root STREQUAL "NOTFOUND")
        # Package isn't in any layer — let find_package() succeed silently
        # so a `rclcpp_compat`-style optional dep doesn't hard-fail; the
        # consumer just won't get a `${pkg}::${pkg}` target. This mirrors
        # the legacy no-op Find-stub behaviour.
        set(${pkg}_FOUND TRUE PARENT_SCOPE)
        return()
    endif()
    message(STATUS "nros: find_package(${pkg}) -> ${_pkg_root}")

    # Validate-only mode (RFC-0048): under `find_package(nano_ros)` the ament
    # `find_package(<msg>)` line only validates the dependency; the actual
    # codegen is driven by the `nano_ros_add_*` verb in the leaf's language (it
    # reads package.xml + shells `nros codegen resolve-deps`). Resolve to
    # confirm the pkg exists, then stop before generating — no CPP interface lib,
    # no CPP FFI build, no CXX target-features pulled into a C leaf's scope. The
    # rclcpp-compat workspace path never sets this flag, so it still generates.
    if(NROS_FIND_PACKAGE_VALIDATE_ONLY)
        set(${pkg}_FOUND TRUE PARENT_SCOPE)
        return()
    endif()

    # Glob IDLs out of the resolved root. Standard ROS layout:
    # <root>/{msg,srv,action}/*.{msg,srv,action}.
    file(GLOB _msgs "${_pkg_root}/msg/*.msg")
    file(GLOB _srvs "${_pkg_root}/srv/*.srv")
    file(GLOB _acts "${_pkg_root}/action/*.action")
    set(_ifaces ${_msgs} ${_srvs} ${_acts})

    if(NOT _ifaces)
        # No IDLs — pkg exists but is meta-only (e.g. `rcl_interfaces` umbrella).
        # Treat as found-but-empty so dependents can still resolve.
        set(${pkg}_FOUND TRUE PARENT_SCOPE)
        return()
    endif()

    # Parse package.xml to discover dependencies; recurse to ensure each dep
    # is wired before we generate this pkg.
    _nros_parse_pkg_deps("${_pkg_root}/package.xml" _deps)
    foreach(_dep ${_deps})
        if(NOT TARGET ${_dep}__nano_ros_cpp)
            _nros_find_ros_msg_package(${_dep})
        endif()
    endforeach()

    # Drive the codegen. The IDL files are passed as absolute paths; the
    # function's `<files>` branch resolves them as-is.
    nros_generate_interfaces(${pkg}
        ${_ifaces}
        DEPENDENCIES ${_deps}
        LANGUAGE CPP
        SKIP_INSTALL
    )

    # Emit the upstream-shape consumer link target.
    if(TARGET ${pkg}__nano_ros_cpp AND NOT TARGET ${pkg}::${pkg})
        add_library(${pkg}::${pkg} ALIAS ${pkg}__nano_ros_cpp)
    endif()
    if(TARGET ${pkg}__nano_ros_cpp AND NOT TARGET ${pkg}::${pkg}__rosidl_typesupport_cpp)
        add_library(${pkg}::${pkg}__rosidl_typesupport_cpp ALIAS ${pkg}__nano_ros_cpp)
    endif()

    # Re-export the package's codegen output variables to the caller's scope
    # so the smart-stub recursion (find_package(local_msgs) → find_package
    # (std_msgs) → find_package(builtin_interfaces)) propagates each pkg's
    # `${pkg}_GENERATED_RS_FILES` etc. all the way back up. Without this,
    # only the IMMEDIATE PARENT_SCOPE set by `nros_generate_interfaces` is
    # visible inside its caller — one level up — and dependent FFI builds
    # see an empty `${dep}_GENERATED_RS_FILES` for non-direct deps.
    set(${pkg}_INCLUDE_DIRS       "${${pkg}_INCLUDE_DIRS}"       PARENT_SCOPE)
    set(${pkg}_LIBRARIES          "${${pkg}_LIBRARIES}"          PARENT_SCOPE)
    set(${pkg}_GENERATED_HEADERS  "${${pkg}_GENERATED_HEADERS}"  PARENT_SCOPE)
    set(${pkg}_GENERATED_SOURCES  "${${pkg}_GENERATED_SOURCES}"  PARENT_SCOPE)
    set(${pkg}_GENERATED_RS_FILES "${${pkg}_GENERATED_RS_FILES}" PARENT_SCOPE)

    # Stash in cache vars so a SECOND find_package(<pkg>) firing from a
    # different consumer (e.g. consumer-A finds std_msgs through local_msgs;
    # consumer-B finds std_msgs directly + later finds sensor_msgs which
    # also pulls std_msgs) re-exports the same closure on the fast-return
    # path. CACHE INTERNAL — invisible to ccmake, persists across the whole
    # configure pass.
    set(_NROS_PKG_${pkg}_GENERATED_RS_FILES "${${pkg}_GENERATED_RS_FILES}"
        CACHE INTERNAL "nros cached GENERATED_RS_FILES closure for ${pkg}" FORCE)
    set(_NROS_PKG_${pkg}_GENERATED_HEADERS "${${pkg}_GENERATED_HEADERS}"
        CACHE INTERNAL "nros cached GENERATED_HEADERS for ${pkg}" FORCE)
    set(_NROS_PKG_${pkg}_GENERATED_SOURCES "${${pkg}_GENERATED_SOURCES}"
        CACHE INTERNAL "nros cached GENERATED_SOURCES for ${pkg}" FORCE)
    set(_NROS_PKG_${pkg}_INCLUDE_DIRS "${${pkg}_INCLUDE_DIRS}"
        CACHE INTERNAL "nros cached INCLUDE_DIRS for ${pkg}" FORCE)

    set(${pkg}_FOUND TRUE PARENT_SCOPE)
endfunction()
