# Cyclone DDS type-support codegen helpers (Phase 117.2 / 117.5).
#
# Two public functions:
#
#   nros_rmw_cyclonedds_idlc_compile(<output_var>
#       IDL_FILE     <path/to/foo.idl>
#       OUTPUT_DIR   <build/dir>
#       [TYPE_NAME   nros_test::msg::TestString]   # optional, for self-reg
#   )
#       Runs Cyclone DDS's `idlc` over a single IDL file and emits
#       `<base>.c` + `<base>.h` + (when TYPE_NAME is given)
#       `<base>_register.c` — a tiny static-init translation unit
#       that registers the generated `dds_topic_descriptor_t` with the
#       backend's runtime registry under TYPE_NAME. Sets <output_var>
#       to the list of generated source files.
#
#   nros_rmw_cyclonedds_add_idl_library(<target>
#       IDL_FILES    <a.idl> [<b.idl> ...]
#       [REGISTER_TYPES <name1=full::cpp::Type1> ...]
#   )
#       Convenience wrapper: produces a STATIC IMPORTED-style library
#       containing the descriptor table for every IDL_FILE, plus
#       optional auto-registration translation units.
#
# Notes:
#  - Cyclone 0.10.5's idlc currently fails when emitting XTypes
#    type-discovery metadata. The `-t` flag skips that section; the
#    produced descriptor still works for pub/sub + services against
#    `rmw_cyclonedds_cpp` peers (the metadata is optional on the wire
#    — peers fall back to typename matching). Phase 117.X.6 makes the
#    `-t` choice opt-out: define `NROS_RMW_CYCLONEDDS_INCLUDE_TYPE_INFO`
#    (cache var or env) to drop `-t` once Cyclone is upgraded past the
#    bug. The helper validates the option at configure time by running
#    `idlc -l c` on a synthetic minimal IDL — if the upstream bug is
#    still present the option is rejected with a clear error rather
#    than silently producing truncated descriptors at build time.
#  - Generated .c / .h are written to `${CMAKE_CURRENT_BINARY_DIR}` so
#    consumers don't have to manage their own scratch dirs.

# Standalone-POSIX consumers link `CycloneDDS::ddsc` (from
# find_package). Embedded consumers (the Zephyr nros module) compile
# the Cyclone DDS sources directly into the app and have no imported
# target — they only need idlc to generate descriptors, which they
# supply via a pre-set `IDLC_EXECUTABLE`. Accept either.
if(NOT TARGET CycloneDDS::ddsc AND NOT IDLC_EXECUTABLE)
    message(FATAL_ERROR
        "NrosRmwCycloneddsTypeSupport.cmake requires CycloneDDS::ddsc "
        "(include it after find_package(CycloneDDS)) or a pre-set "
        "IDLC_EXECUTABLE for direct-compile (embedded) builds.")
endif()

# Locate idlc — Cyclone exports it as `CycloneDDS::idlc` when it's
# installed alongside ddsc.
if(NOT TARGET CycloneDDS::idlc)
    # idlc is a HOST build tool (it runs on the build machine to emit C
    # descriptors), so search the host even in a cross build —
    # NO_CMAKE_FIND_ROOT_PATH ignores the toolchain's find-root mode
    # (some set MODE_PROGRAM=ONLY, which would otherwise hide host idlc).
    # Phase 186.3: a self-provisioned build with no `just` step resolves idlc
    # from PATH (e.g. a ROS 2 install) or a pre-set IDLC_EXECUTABLE.
    find_program(IDLC_EXECUTABLE idlc
        HINTS
            "${CycloneDDS_DIR}/../../../bin"
            "${CMAKE_INSTALL_PREFIX}/bin"
            "$ENV{CYCLONEDDS_INSTALL_DIR}/bin"
        NO_CMAKE_FIND_ROOT_PATH
        DOC "Cyclone DDS IDL compiler (host tool)")
    if(NOT IDLC_EXECUTABLE)
        message(FATAL_ERROR
            "idlc (Cyclone DDS IDL compiler, a host tool) not found.\n"
            "  Put it on PATH (e.g. a ROS 2 / CycloneDDS install), or pass "
            "-DIDLC_EXECUTABLE=<path-to-idlc>.")
    endif()
endif()

# Resolve idlc to an absolute path *here*, where the imported
# `CycloneDDS::idlc` target is visible, and stash it in an INTERNAL
# cache var. Imported targets are directory-scoped, so a far-away
# consumer (e.g. an example calling `nros_generate_interfaces`) cannot
# expand `$<TARGET_FILE:CycloneDDS::idlc>` — the genex resolves to an
# empty string and idlc never runs. The cached absolute path is
# visible from every scope.
#
# Re-resolve when the cached path no longer exists, not just when it is
# unset: the value is a sticky INTERNAL cache entry, so a build dir
# configured under an older repo layout keeps a path that may now point
# through a deleted directory (e.g. Phase 180.B removed `examples/zephyr/
# cmake`, leaving stale `.../examples/zephyr/cmake/../../../build/install/
# bin/idlc` caches that fail to resolve → `idlc: not found` / exit 127).
# `NOT EXISTS` forces a fresh resolution from the current layout; the
# INTERNAL `set` below implies FORCE, so it overwrites the stale value.
if(NOT NROS_RMW_CYCLONEDDS_IDLC OR NOT EXISTS "${NROS_RMW_CYCLONEDDS_IDLC}")
    set(_idlc_loc "")
    # Prefer the imported target's location (covers per-config suffixes).
    if(TARGET CycloneDDS::idlc)
        foreach(_loc_prop
                IMPORTED_LOCATION
                IMPORTED_LOCATION_RELEASE
                IMPORTED_LOCATION_RELWITHDEBINFO
                IMPORTED_LOCATION_DEBUG
                IMPORTED_LOCATION_NOCONFIG)
            if(NOT _idlc_loc)
                get_target_property(_p CycloneDDS::idlc ${_loc_prop})
                if(_p)
                    set(_idlc_loc "${_p}")
                endif()
            endif()
        endforeach()
    endif()
    # Fall back to a real on-disk search so far consumers never depend
    # on the imported target being visible in their scope.
    if(NOT _idlc_loc)
        if(IDLC_EXECUTABLE)
            set(_idlc_loc "${IDLC_EXECUTABLE}")
        else()
            find_program(_idlc_found idlc
                HINTS
                    "${CycloneDDS_DIR}/../../../bin"
                    "${CMAKE_INSTALL_PREFIX}/bin"
                    "$ENV{CYCLONEDDS_INSTALL_DIR}/bin"
                NO_CMAKE_FIND_ROOT_PATH)
            if(_idlc_found)
                set(_idlc_loc "${_idlc_found}")
            endif()
        endif()
    endif()
    if(_idlc_loc)
        set(NROS_RMW_CYCLONEDDS_IDLC "${_idlc_loc}"
            CACHE INTERNAL "Absolute path to Cyclone DDS idlc")
    endif()
endif()

# Phase 117.X.1: locate the .msg/.srv → mangled-IDL converter.
#
# Resolution order (no source-tree-relative HINTs — see CLAUDE.md
# "CMake Path Convention" — callers must pass absolute paths):
#   1. Cache var `NROS_RMW_CYCLONEDDS_MSG_TO_IDL` (e.g. set via
#      `-DNROS_RMW_CYCLONEDDS_MSG_TO_IDL=…` on cmake configure).
#   2. Env var `NROS_RMW_CYCLONEDDS_SCRIPTS_DIR` containing the
#      installed `msg_to_cyclone_idl.py`.
#   3. `share/nros-rmw-cyclonedds/` next to the installed CMake
#      config (this is a CMake-install layout convention, not a
#      project-source-tree assumption — `CMAKE_CURRENT_LIST_DIR`
#      resolves to `<prefix>/lib/cmake/NrosRmwCyclonedds` for
#      installed consumers, and `share` is a sibling). For in-tree
#      development, the consumer (e.g. the project's own
#      `tests/CMakeLists.txt`) sets the cache var directly.
if(NOT NROS_RMW_CYCLONEDDS_MSG_TO_IDL)
    find_program(NROS_RMW_CYCLONEDDS_MSG_TO_IDL
        NAMES msg_to_cyclone_idl.py
        HINTS
            "$ENV{NROS_RMW_CYCLONEDDS_SCRIPTS_DIR}"
            "${CMAKE_CURRENT_LIST_DIR}/../../../share/nros-rmw-cyclonedds"
            # phase-292 W2 (ASI wall #7) — Zephyr-module / source-tree
            # consumption: this file lives at
            # packages/dds/nros-rmw-cyclonedds/cmake/, the converter at
            # <repo>/scripts/cyclonedds/. Without this hint the descriptor
            # codegen silently degrades to the legacy path and every
            # find_descriptor() fails at runtime (create_subscription -100).
            "${CMAKE_CURRENT_LIST_DIR}/../../../../scripts/cyclonedds"
        DOC ".msg/.srv → Cyclone-shaped IDL converter"
    )
endif()
if(NOT NROS_RMW_CYCLONEDDS_MSG_TO_IDL)
    # Soft warning — the legacy hand-authored-IDL path still works
    # without it; only callers of nros_rmw_cyclonedds_generate_from_msg
    # need it.
    message(STATUS
        "msg_to_cyclone_idl.py not found; "
        "nros_rmw_cyclonedds_generate_from_msg() will fail. "
        "Pass -DNROS_RMW_CYCLONEDDS_MSG_TO_IDL=<abs path> or set "
        "NROS_RMW_CYCLONEDDS_SCRIPTS_DIR.")
endif()

# Phase 117.X.6 — validate the type-info opt-in at configure time.
# Cyclone 0.10.5's idlc produces a truncated `.c` (just the ops
# array, no descriptor) when type-info emission is requested. If the
# consumer opts in we run idlc on a synthetic minimal IDL and check
# the descriptor symbol lands in the output; otherwise we error out
# with a clear pointer to the upstream bug rather than letting the
# build fail later with confusing link errors.
if(NROS_RMW_CYCLONEDDS_INCLUDE_TYPE_INFO OR
   "$ENV{NROS_RMW_CYCLONEDDS_INCLUDE_TYPE_INFO}")
    if(TARGET CycloneDDS::idlc)
        get_target_property(_probe_idlc CycloneDDS::idlc IMPORTED_LOCATION)
        if(NOT _probe_idlc)
            get_target_property(_probe_idlc CycloneDDS::idlc IMPORTED_LOCATION_RELEASE)
        endif()
    else()
        set(_probe_idlc "${IDLC_EXECUTABLE}")
    endif()
    set(_probe_dir "${CMAKE_CURRENT_BINARY_DIR}/_nros_rmw_cyclonedds_xtypes_probe")
    file(MAKE_DIRECTORY "${_probe_dir}")
    file(WRITE "${_probe_dir}/probe.idl"
        "@final struct NrosRmwCycloneddsTypeinfoProbe { long x; };\n")
    execute_process(
        COMMAND "${_probe_idlc}" -l c -o "${_probe_dir}" "${_probe_dir}/probe.idl"
        OUTPUT_QUIET ERROR_QUIET
        RESULT_VARIABLE _probe_rc
    )
    set(_probe_c "${_probe_dir}/probe.c")
    set(_probe_ok FALSE)
    if(_probe_rc EQUAL 0 AND EXISTS "${_probe_c}")
        file(READ "${_probe_c}" _probe_contents)
        if(_probe_contents MATCHES
                "NrosRmwCycloneddsTypeinfoProbe_desc[ \t]*=")
            set(_probe_ok TRUE)
        endif()
    endif()
    if(NOT _probe_ok)
        message(FATAL_ERROR
            "NROS_RMW_CYCLONEDDS_INCLUDE_TYPE_INFO is ON but the bundled "
            "Cyclone DDS idlc fails to emit XTypes type-info "
            "(produces a truncated descriptor). This is a known upstream "
            "bug in Cyclone 0.10.5. Either upgrade the Cyclone pin past "
            "the fixed release or unset the option. See "
            "docs/reference/cyclonedds-known-limitations.md.")
    endif()
    message(STATUS "Cyclone idlc XTypes type-info probe: OK")
endif()

#
# nros_rmw_cyclonedds_idlc_compile
#
# An IDL file may contain multiple `@topic`-eligible structs. Pass
# one TYPE_NAME (single-type) or TYPE_NAMES (one per struct, all
# registered) — both forms emit one constructor per name.
#
function(nros_rmw_cyclonedds_idlc_compile output_var)
    set(_options "")
    set(_one    IDL_FILE OUTPUT_DIR TYPE_NAME PKG_NAME)
    set(_multi  TYPE_NAMES INCLUDE_DIRS EXTRA_DEPENDS)
    cmake_parse_arguments(_arg "${_options}" "${_one}" "${_multi}" ${ARGN})

    if(NOT _arg_IDL_FILE)
        message(FATAL_ERROR "nros_rmw_cyclonedds_idlc_compile: IDL_FILE required")
    endif()
    if(NOT _arg_OUTPUT_DIR)
        set(_arg_OUTPUT_DIR "${CMAKE_CURRENT_BINARY_DIR}/cyclonedds-types")
    endif()
    file(MAKE_DIRECTORY "${_arg_OUTPUT_DIR}")

    get_filename_component(_idl_abs "${_arg_IDL_FILE}" ABSOLUTE)
    get_filename_component(_idl_stem "${_arg_IDL_FILE}" NAME_WE)
    set(_gen_c "${_arg_OUTPUT_DIR}/${_idl_stem}.c")
    set(_gen_h "${_arg_OUTPUT_DIR}/${_idl_stem}.h")

    # Use the absolute path cached at module-load (see top of file) so
    # this works from any scope, not only where CycloneDDS::idlc is
    # visible. `$<TARGET_FILE:…>` would expand to "" for far consumers.
    if(NROS_RMW_CYCLONEDDS_IDLC)
        set(_idlc "${NROS_RMW_CYCLONEDDS_IDLC}")
    elseif(TARGET CycloneDDS::idlc)
        set(_idlc "$<TARGET_FILE:CycloneDDS::idlc>")
    else()
        set(_idlc "${IDLC_EXECUTABLE}")
    endif()

    # Phase 117.X.6 — opt-in XTypes type-info emission. Default keeps
    # the `-t` flag (omits type-info) because Cyclone 0.10.5's idlc
    # produces truncated descriptors when type-info is requested.
    # Downstream consumers on a fixed Cyclone build flip the flag via
    # `-DNROS_RMW_CYCLONEDDS_INCLUDE_TYPE_INFO=ON` (cache var) or
    # `NROS_RMW_CYCLONEDDS_INCLUDE_TYPE_INFO=1` (env).
    set(_idlc_flags "-t" "-l" "c")
    if(NROS_RMW_CYCLONEDDS_INCLUDE_TYPE_INFO OR
       "$ENV{NROS_RMW_CYCLONEDDS_INCLUDE_TYPE_INFO}")
        set(_idlc_flags "-l" "c")
    endif()

    # Composite messages `#include` sibling / cross-package IDLs using
    # the rosidl-style `<pkg>/msg/<Type>.idl` path. idlc resolves those
    # against `-I <root>` dirs where the package-nested layout lives.
    foreach(_inc IN LISTS _arg_INCLUDE_DIRS)
        list(APPEND _idlc_flags "-I" "${_inc}")
    endforeach()

    add_custom_command(
        OUTPUT  "${_gen_c}" "${_gen_h}"
        COMMAND "${_idlc}" ${_idlc_flags} -o "${_arg_OUTPUT_DIR}" "${_idl_abs}"
        DEPENDS "${_idl_abs}" ${_arg_EXTRA_DEPENDS}
        COMMENT "idlc ${_idl_stem}.idl"
        VERBATIM
    )

    set(_out_files "${_gen_c}")

    # Normalise the single + multi forms into one list.
    set(_all_types "")
    if(_arg_TYPE_NAME)
        list(APPEND _all_types "${_arg_TYPE_NAME}")
    endif()
    if(_arg_TYPE_NAMES)
        list(APPEND _all_types ${_arg_TYPE_NAMES})
    endif()

    set(_idx 0)
    foreach(_tn IN LISTS _all_types)
        # Per-type self-registration TU. The descriptor symbol is
        # `<TYPE_NAME with :: → _>_desc`; matches Cyclone idlc's
        # mangling. `_<idx>` keeps each register TU's filename
        # unique when multiple types share the same IDL.
        string(REPLACE "::" "_" _desc_sym "${_tn}_desc")
        # Sanitise the constructor's symbol name — descriptor symbol
        # has only A-Za-z0-9_ already so it's safe to reuse.
        # Issue #177 — namespace the ctor by package when the caller says
        # which one: ROS ships the SAME type stem in several packages
        # (std_msgs/Int32 vs example_interfaces/Int32, String, the whole
        # *MultiArray family), so bare `register_<stem>_<idx>` symbols
        # collide at link the moment a fixture pulls both ts archives.
        # Callers without PKG_NAME (the hand-IDL graph TU, legacy
        # add_idl_library users) keep the historical name.
        if(_arg_PKG_NAME)
            set(_ctor "register_${_arg_PKG_NAME}_${_idl_stem}_${_idx}")
        else()
            set(_ctor "register_${_idl_stem}_${_idx}")
        endif()
        set(_reg "${_arg_OUTPUT_DIR}/${_idl_stem}_register_${_idx}.c")
        file(WRITE "${_reg}.in"
"/* Auto-generated by nros_rmw_cyclonedds_idlc_compile() — do not edit. */
#include \"dds/dds.h\"
#include \"${_idl_stem}.h\"

extern const dds_topic_descriptor_t ${_desc_sym};

void nros_rmw_cyclonedds_register_descriptor(
    const char *type_name, const dds_topic_descriptor_t *desc);

void ${_ctor}(void) {
    nros_rmw_cyclonedds_register_descriptor(
        \"${_tn}\", &${_desc_sym});
}

__attribute__((constructor))
static void ${_ctor}_constructor(void) {
    ${_ctor}();
}
")
        configure_file("${_reg}.in" "${_reg}" COPYONLY)
        # The register TU `#include`s the idlc-generated `<stem>.h`.
        # idlc emits `.c` + `.h` from one custom_command, but only the
        # `.c` is a tracked source — nothing makes the register TU's
        # compile wait for the header, so a parallel build races and
        # fails with "<stem>.h: No such file or directory". Pin the
        # ordering with an explicit object dependency on the header.
        set_source_files_properties("${_reg}" PROPERTIES
            OBJECT_DEPENDS "${_gen_h}")
        list(APPEND _out_files "${_reg}")
        math(EXPR _idx "${_idx} + 1")
    endforeach()

    set(${output_var} "${_out_files}" PARENT_SCOPE)
endfunction()

#
# nros_rmw_cyclonedds_add_idl_library
#
function(nros_rmw_cyclonedds_add_idl_library tgt)
    set(_options "")
    set(_one    "")
    set(_multi  IDL_FILES REGISTER_TYPES)
    cmake_parse_arguments(_arg "${_options}" "${_one}" "${_multi}" ${ARGN})

    if(NOT _arg_IDL_FILES)
        message(FATAL_ERROR
            "nros_rmw_cyclonedds_add_idl_library: IDL_FILES required")
    endif()

    set(_all_sources "")
    foreach(_idl IN LISTS _arg_IDL_FILES)
        get_filename_component(_idl_stem "${_idl}" NAME_WE)
        set(_type_for_this "")
        # REGISTER_TYPES is a list of "<idl_stem>=<full::cpp::Type>" pairs.
        foreach(_pair IN LISTS _arg_REGISTER_TYPES)
            string(REGEX MATCH "^([^=]+)=(.*)$" _m "${_pair}")
            if(_m)
                set(_lhs "${CMAKE_MATCH_1}")
                set(_rhs "${CMAKE_MATCH_2}")
                if(_lhs STREQUAL "${_idl_stem}")
                    set(_type_for_this "${_rhs}")
                endif()
            endif()
        endforeach()
        if(_type_for_this)
            nros_rmw_cyclonedds_idlc_compile(_gen
                IDL_FILE  "${_idl}"
                OUTPUT_DIR "${CMAKE_CURRENT_BINARY_DIR}/${tgt}-idl"
                TYPE_NAME "${_type_for_this}"
            )
        else()
            nros_rmw_cyclonedds_idlc_compile(_gen
                IDL_FILE  "${_idl}"
                OUTPUT_DIR "${CMAKE_CURRENT_BINARY_DIR}/${tgt}-idl"
            )
        endif()
        list(APPEND _all_sources ${_gen})
    endforeach()

    add_library(${tgt} STATIC ${_all_sources})
    target_include_directories(${tgt}
        PUBLIC "${CMAKE_CURRENT_BINARY_DIR}/${tgt}-idl")
    target_link_libraries(${tgt} PUBLIC CycloneDDS::ddsc)
    set_target_properties(${tgt} PROPERTIES POSITION_INDEPENDENT_CODE ON)
endfunction()

#
# nros_rmw_cyclonedds_generate_from_msg
#
# Phase 117.X.1: drive `.msg` / `.srv` → mangled IDL → idlc → static-
# init self-registration. Output type names match what stock
# `rmw_cyclonedds_cpp` emits, so a nano-ros publisher / service-server
# matches an `rclcpp` subscriber / client by `(topic_name, type_name)`.
#
#   nros_rmw_cyclonedds_generate_from_msg(<output_var>
#       PKG_NAME    <my_msgs>
#       PKG_DIR     <path/to/pkg-with-package.xml>
#       INTERFACES  <Foo.msg> <Bar.srv> ...
#       [OUTPUT_DIR <build/dir>]
#   )
#
# Sets <output_var> to the list of generated `.c` (descriptor +
# self-registration) source files.
#
# For each `.msg` Foo:
#     descriptor name: <PKG>::msg::dds_::Foo_
#     registry key:    "<PKG>::msg::dds_::Foo_"
# For each `.srv` Foo:
#     two descriptors registered:
#       <PKG>::srv::dds_::Foo_Request_
#       <PKG>::srv::dds_::Foo_Response_
#
function(nros_rmw_cyclonedds_generate_from_msg output_var)
    set(_options "")
    set(_one    PKG_NAME PKG_DIR OUTPUT_DIR INCLUDE_ROOT GEN_ROOT)
    set(_multi  INTERFACES IDL_DEPENDS)
    cmake_parse_arguments(_arg "${_options}" "${_one}" "${_multi}" ${ARGN})

    if(NOT _arg_PKG_NAME OR NOT _arg_PKG_DIR OR NOT _arg_INTERFACES)
        message(FATAL_ERROR
            "nros_rmw_cyclonedds_generate_from_msg: PKG_NAME, PKG_DIR, "
            "and INTERFACES are required.")
    endif()
    if(NOT NROS_RMW_CYCLONEDDS_MSG_TO_IDL)
        message(FATAL_ERROR
            "nros_rmw_cyclonedds_generate_from_msg requires "
            "msg_to_cyclone_idl.py — set NROS_RMW_CYCLONEDDS_SCRIPTS_DIR "
            "or check `find_program(NROS_RMW_CYCLONEDDS_MSG_TO_IDL …)` "
            "above.")
    endif()
    if(NOT _arg_OUTPUT_DIR)
        set(_arg_OUTPUT_DIR
            "${CMAKE_CURRENT_BINARY_DIR}/cyclonedds-from-msg/${_arg_PKG_NAME}")
    endif()
    # Composite messages cross-reference sibling / cross-package IDLs
    # via `#include "<pkg>/msg/<Type>.idl"`. When the caller supplies a
    # shared INCLUDE_ROOT, write each package's IDLs into the nested
    # `<root>/<pkg>/msg/` layout those includes expect and hand idlc
    # `-I <root>`. Packages generated earlier (declared DEPENDENCIES)
    # populate the same root, so cross-package includes resolve too.
    # Without INCLUDE_ROOT we keep the flat layout (legacy hand-IDL
    # callers register one self-contained type at a time).
    if(_arg_INCLUDE_ROOT)
        set(_idl_dir "${_arg_INCLUDE_ROOT}/${_arg_PKG_NAME}/msg")
        set(_idlc_includes "${_arg_INCLUDE_ROOT}")
    else()
        set(_idl_dir "${_arg_OUTPUT_DIR}/idl")
        set(_idlc_includes "")
    endif()
    # idlc emits each descriptor `.h` with `#include "<pkg>/msg/<Dep>.h"`
    # lines for composite members, so the generated `.c`/`.h` must also
    # live in the package-nested layout and compile with `-I <GEN_ROOT>`.
    # Without GEN_ROOT, keep the flat per-package gen dir (legacy path,
    # self-contained types only).
    if(_arg_GEN_ROOT)
        set(_gen_dir "${_arg_GEN_ROOT}/${_arg_PKG_NAME}/msg")
    else()
        set(_gen_dir "${_arg_OUTPUT_DIR}/gen")
    endif()
    file(MAKE_DIRECTORY "${_idl_dir}")
    file(MAKE_DIRECTORY "${_gen_dir}")

    # Resolve absolute interface paths so the script + custom_command
    # see the same files regardless of caller's CMAKE_CURRENT_SOURCE_DIR.
    set(_iface_args "")
    set(_iface_abs_list "")
    foreach(_iface IN LISTS _arg_INTERFACES)
        if(IS_ABSOLUTE "${_iface}")
            set(_abs "${_iface}")
        else()
            set(_abs "${_arg_PKG_DIR}/${_iface}")
        endif()
        list(APPEND _iface_args "--interface" "${_abs}")
        list(APPEND _iface_abs_list "${_abs}")
    endforeach()

    set(_all_outputs "")

    # Pass 1 — convert every .msg/.srv to mangled IDL first and collect
    # the .idl paths. idlc reads `#include`d sibling / cross-package
    # IDLs at generation time, so every idlc command in pass 2 must wait
    # for *all* of this package's .idl files (and, via the ts-lib target
    # ordering set up by the caller, the dependency packages' files in
    # the shared INCLUDE_ROOT).
    set(_pkg_idl_paths "")
    foreach(_iface IN LISTS _arg_INTERFACES)
        get_filename_component(_iface_stem "${_iface}" NAME_WE)
        set(_idl_path "${_idl_dir}/${_iface_stem}.idl")

        if(IS_ABSOLUTE "${_iface}")
            set(_iface_abs "${_iface}")
        else()
            set(_iface_abs "${_arg_PKG_DIR}/${_iface}")
        endif()
        add_custom_command(
            OUTPUT  "${_idl_path}"
            COMMAND "${CMAKE_COMMAND}" -E env
                    "${NROS_RMW_CYCLONEDDS_MSG_TO_IDL}"
                    --pkg-name "${_arg_PKG_NAME}"
                    --pkg-dir  "${_arg_PKG_DIR}"
                    --output-dir "${_idl_dir}"
                    --interface "${_iface_abs}"
            DEPENDS "${_iface_abs}" "${NROS_RMW_CYCLONEDDS_MSG_TO_IDL}"
            COMMENT "msg_to_cyclone_idl ${_arg_PKG_NAME}/${_iface}"
            VERBATIM
        )
        list(APPEND _pkg_idl_paths "${_idl_path}")
    endforeach()

    # Pass 2 — run idlc on each .idl, gated on all sibling .idl files.
    foreach(_iface IN LISTS _arg_INTERFACES)
        get_filename_component(_iface_stem "${_iface}" NAME_WE)
        get_filename_component(_iface_ext  "${_iface}" EXT)
        set(_idl_path "${_idl_dir}/${_iface_stem}.idl")

        # Decide which type name(s) to register based on the
        # extension. .msg → one name, .srv → two (Request + Response).
        if(_iface_ext STREQUAL ".msg")
            nros_rmw_cyclonedds_idlc_compile(_gen
                IDL_FILE  "${_idl_path}"
                OUTPUT_DIR "${_gen_dir}"
                INCLUDE_DIRS ${_idlc_includes}
                EXTRA_DEPENDS ${_pkg_idl_paths} ${_arg_IDL_DEPENDS}
                PKG_NAME  "${_arg_PKG_NAME}"
                TYPE_NAME "${_arg_PKG_NAME}::msg::dds_::${_iface_stem}_"
            )
        elseif(_iface_ext STREQUAL ".srv")
            nros_rmw_cyclonedds_idlc_compile(_gen
                IDL_FILE  "${_idl_path}"
                OUTPUT_DIR "${_gen_dir}"
                INCLUDE_DIRS ${_idlc_includes}
                EXTRA_DEPENDS ${_pkg_idl_paths} ${_arg_IDL_DEPENDS}
                PKG_NAME  "${_arg_PKG_NAME}"
                TYPE_NAMES
                    "${_arg_PKG_NAME}::srv::dds_::${_iface_stem}_Request_"
                    "${_arg_PKG_NAME}::srv::dds_::${_iface_stem}_Response_"
            )
        elseif(_iface_ext STREQUAL ".action")
            # `msg_to_cyclone_idl.py` synthesizes the eight action wrapper
            # types into one IDL (base Goal/Result/Feedback +
            # SendGoal/GetResult Request/Response + FeedbackMessage),
            # matching the nros action layer's wire framing. Register all
            # eight; the backend derives which one a given sub-service /
            # topic needs from its keyexpr role (Phase 171.0.b Piece 1).
            set(_act "${_arg_PKG_NAME}::action::dds_::${_iface_stem}")
            nros_rmw_cyclonedds_idlc_compile(_gen
                IDL_FILE  "${_idl_path}"
                OUTPUT_DIR "${_gen_dir}"
                INCLUDE_DIRS ${_idlc_includes}
                EXTRA_DEPENDS ${_pkg_idl_paths} ${_arg_IDL_DEPENDS}
                PKG_NAME  "${_arg_PKG_NAME}"
                TYPE_NAMES
                    "${_act}_Goal_"
                    "${_act}_Result_"
                    "${_act}_Feedback_"
                    "${_act}_SendGoal_Request_"
                    "${_act}_SendGoal_Response_"
                    "${_act}_GetResult_Request_"
                    "${_act}_GetResult_Response_"
                    "${_act}_FeedbackMessage_"
            )
        else()
            message(FATAL_ERROR
                "nros_rmw_cyclonedds_generate_from_msg: unsupported "
                "extension ${_iface_ext} on ${_iface}")
        endif()

        list(APPEND _all_outputs ${_gen})
    endforeach()

    set(${output_var} "${_all_outputs}" PARENT_SCOPE)
endfunction()
