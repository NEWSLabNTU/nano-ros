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

if(NOT TARGET CycloneDDS::ddsc)
    message(FATAL_ERROR
        "NrosRmwCycloneddsTypeSupport.cmake requires CycloneDDS::ddsc; "
        "include it after find_package(CycloneDDS).")
endif()

# Locate idlc — Cyclone exports it as `CycloneDDS::idlc` when it's
# installed alongside ddsc.
if(NOT TARGET CycloneDDS::idlc)
    find_program(IDLC_EXECUTABLE idlc
        HINTS
            "${CycloneDDS_DIR}/../../../bin"
            "${CMAKE_INSTALL_PREFIX}/bin"
            "$ENV{CYCLONEDDS_INSTALL_DIR}/bin"
        DOC "Cyclone DDS IDL compiler")
    if(NOT IDLC_EXECUTABLE)
        message(FATAL_ERROR
            "idlc not found. Set IDLC_EXECUTABLE or run `just cyclonedds setup`.")
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
    set(_one    IDL_FILE OUTPUT_DIR TYPE_NAME)
    set(_multi  TYPE_NAMES)
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

    if(TARGET CycloneDDS::idlc)
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

    add_custom_command(
        OUTPUT  "${_gen_c}" "${_gen_h}"
        COMMAND "${_idlc}" ${_idlc_flags} -o "${_arg_OUTPUT_DIR}" "${_idl_abs}"
        DEPENDS "${_idl_abs}"
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
        set(_ctor "register_${_idl_stem}_${_idx}")
        set(_reg "${_arg_OUTPUT_DIR}/${_idl_stem}_register_${_idx}.c")
        file(WRITE "${_reg}.in"
"/* Auto-generated by nros_rmw_cyclonedds_idlc_compile() — do not edit. */
#include \"dds/dds.h\"
#include \"${_idl_stem}.h\"

extern const dds_topic_descriptor_t ${_desc_sym};

void nros_rmw_cyclonedds_register_descriptor(
    const char *type_name, const dds_topic_descriptor_t *desc);

__attribute__((constructor))
static void ${_ctor}(void) {
    nros_rmw_cyclonedds_register_descriptor(
        \"${_tn}\", &${_desc_sym});
}
")
        configure_file("${_reg}.in" "${_reg}" COPYONLY)
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
    set(_one    PKG_NAME PKG_DIR OUTPUT_DIR)
    set(_multi  INTERFACES)
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
    set(_idl_dir "${_arg_OUTPUT_DIR}/idl")
    set(_gen_dir "${_arg_OUTPUT_DIR}/gen")
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

    foreach(_iface IN LISTS _arg_INTERFACES)
        get_filename_component(_iface_stem "${_iface}" NAME_WE)
        get_filename_component(_iface_ext  "${_iface}" EXT)
        set(_idl_path "${_idl_dir}/${_iface_stem}.idl")

        # Run the converter once per interface so each .idl has a
        # focused custom_command DEPENDS line.
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

        # Decide which type name(s) to register based on the
        # extension. .msg → one name, .srv → two (Request + Response).
        if(_iface_ext STREQUAL ".msg")
            nros_rmw_cyclonedds_idlc_compile(_gen
                IDL_FILE  "${_idl_path}"
                OUTPUT_DIR "${_gen_dir}"
                TYPE_NAME "${_arg_PKG_NAME}::msg::dds_::${_iface_stem}_"
            )
        elseif(_iface_ext STREQUAL ".srv")
            nros_rmw_cyclonedds_idlc_compile(_gen
                IDL_FILE  "${_idl_path}"
                OUTPUT_DIR "${_gen_dir}"
                TYPE_NAMES
                    "${_arg_PKG_NAME}::srv::dds_::${_iface_stem}_Request_"
                    "${_arg_PKG_NAME}::srv::dds_::${_iface_stem}_Response_"
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
