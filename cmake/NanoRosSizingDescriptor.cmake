# NanoRosSizingDescriptor.cmake — phase-454 W4, RFC-0100 D4
#
# The CMake side of the one sizing descriptor `nros sync` writes per entry:
#
#     <build>/nros/sizing/<entry>.toml
#
# CMake does NOT parse it. The schema has exactly one reader
# (`packages/tooling/nros-sizing-descriptor`), and a second parser written in
# CMake would be the drift class `check-ffi-struct-mirrors` and
# `check-platform-abi-mirror` police one layer down — two spellings of one
# format, held equal by nothing. So this module asks the CLI
# (`nros ws sizing-descriptor --output-cmake`) and `include()`s the answer.
#
# ## The freshness rule this module owes — issue 1018
#
# `execute_process()` has already run by the time ninja decides anything, so a
# configure-time emitter has no `DEPENDS` to carry its inputs. The freshness of
# what it emits therefore reduces to *does a configure happen*, and the only way
# to make one happen is `CMAKE_CONFIGURE_DEPENDS`. Two things go on that list
# here, and leaving out either is the failure 1018 measured:
#
#   * the DESCRIPTOR, so editing a contract and re-syncing re-configures;
#   * the TOOL, through `nros_codegen_tool_reconfigure()`, so rebuilding `nros`
#     re-configures. That is the half #182 registered at one of four sites and
#     the half the Zephyr interfaces generator was missing, which left a
#     single-example image holding museum generated code after a CLI rebuild.
#
# ## What a caller reads, and how a refusal reaches it — RFC-0100 D6
#
# A STATED field becomes `set(NROS_SIZING_<FIELD> <value>)`. A REFUSED one gets
# NO value variable at all and a `set(NROS_SIZING_<FIELD>_REFUSED "<reason>")`
# beside it; an ABSENT one gets `set(NROS_SIZING_<FIELD>_ABSENT TRUE)`. So
#
#     if(DEFINED NROS_SIZING_TARGET_POINTER_BYTES)
#
# is the only road to a number, and there is no spelling of "read it, and if it
# is empty use 8" that skips the check. That is D6 in CMake's vocabulary: a
# consumer either reads a value this model derived or reads nothing at all.
#
# The per-endpoint table arrives as parallel lists — `NROS_SIZING_ENDPOINT_KIND`,
# `_TYPE`, `_TOPIC`, `_DEPTH`, `_REGISTRATION_PATH`, `_STORAGE_BYTES` — each with
# `NROS_SIZING_ENDPOINT_COUNT` elements, indexed together. A refused cell is the
# literal `REFUSED` and an absent one `ABSENT`, never an empty element: an empty
# element vanishes on the next `list()` operation, which would shorten one column
# and silently mis-align every row after it.

include_guard(GLOBAL)

# nros_sizing_descriptor_path(<out_var> <build_dir> <entry>)
#
# The ONE path rule, mirroring `nros_sizing_descriptor::descriptor_path`. Spelled
# here rather than inline at each call site for the reason `model_location`
# exists one artifact over: three consumers each derived the SystemModel path
# independently and two of them drifted.
function(nros_sizing_descriptor_path _out_var _build_dir _entry)
    set(${_out_var} "${_build_dir}/nros/sizing/${_entry}.toml" PARENT_SCOPE)
endfunction()

# nros_sizing_descriptor_read(<descriptor> [QUIET])
#
# Read a descriptor at CONFIGURE time and define the `NROS_SIZING_*` variables in
# the calling scope.
#
# A MISSING descriptor is not an error: an image nobody has run `nros sync` for
# has none, and every consumer is required to keep its own defaults in that case.
# It is REPORTED, because "the fallback decided" and "the declaration decided"
# are otherwise indistinguishable in the log (issue 0973's rule). A descriptor
# that EXISTS and does not read is a hard `FATAL_ERROR` — somebody generated it,
# and sizing from our own literals while a user believes they supplied numbers is
# the silent default this whole artifact exists to remove.
function(nros_sizing_descriptor_read _descriptor)
    cmake_parse_arguments(_nsd "QUIET" "" "" ${ARGN})

    if(_descriptor STREQUAL "")
        message(FATAL_ERROR "nros_sizing_descriptor_read: no descriptor path given")
    endif()

    # Registered BEFORE the existence check, deliberately. CMake re-configures
    # when a listed file appears as well as when it changes, so a build that has
    # no descriptor today picks one up on the sync that creates it — without
    # this, the first `nros sync` after a configure would be invisible until
    # something else happened to re-configure.
    set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_descriptor}")

    if(NOT EXISTS "${_descriptor}")
        if(NOT _nsd_QUIET)
            message(STATUS
                "nano-ros: no sizing descriptor at ${_descriptor}, so every consumer keeps "
                "its own default sizes (RFC-0100 D6). Run `nros sync` in the entry's "
                "workspace to generate one.")
        endif()
        return()
    endif()

    if(NOT COMMAND nros_resolve_cli)
        message(FATAL_ERROR
            "nros_sizing_descriptor_read: the CLI resolver is not available, so "
            "${_descriptor} cannot be read. Include NanoRosCodegenCore.cmake first.")
    endif()
    nros_resolve_cli(_nros OPTIONAL
        CONTEXT "nros_sizing_descriptor_read (${_descriptor})")
    if(NOT _nros OR _nros STREQUAL "NOTFOUND" OR NOT EXISTS "${_nros}")
        message(FATAL_ERROR
            "nros_sizing_descriptor_read: a sizing descriptor exists at ${_descriptor} "
            "but the `nros` CLI that reads it was not found. Sizing from our own "
            "defaults here would ignore numbers this image declared.")
    endif()

    # Issue 1018 — the TOOL half. Without it the emitted fragment is as fresh as
    # the last configure, whenever that was, and a CLI rebuild does not cause one.
    if(COMMAND nros_codegen_tool_reconfigure)
        nros_codegen_tool_reconfigure("${_nros}")
    endif()

    get_filename_component(_stem "${_descriptor}" NAME_WE)
    set(_fragment "${CMAKE_CURRENT_BINARY_DIR}/nros-sizing-${_stem}.cmake")
    execute_process(
        COMMAND "${_nros}" ws sizing-descriptor
                --descriptor "${_descriptor}"
                --output-cmake "${_fragment}"
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc)
    if(NOT _rc EQUAL 0)
        string(REGEX REPLACE "\n+" " " _why "${_err}")
        message(FATAL_ERROR
            "nano-ros: the sizing descriptor ${_descriptor} exists and could not be read "
            "-- ${_why}\n"
            "It is a generated artifact: re-run `nros sync` rather than editing it.")
    endif()

    # The verb writes write-if-changed, so an unchanged descriptor leaves this
    # fragment's mtime alone and does not re-arm the next configure.
    include("${_fragment}")

    # Re-export into the caller's scope: `include()` inside a function puts the
    # variables in the FUNCTION frame, which pops. Same trap as the `_NROS_ENTRY_DIR`
    # one in AGENTS.md's CMake pitfalls, reached a different way.
    #
    # `get_cmake_property(... VARIABLES)` and NOT `get_directory_property`: the
    # fragment's variables live in this FUNCTION frame, and the directory
    # property does not see them. Measured before relying on it.
    get_cmake_property(_vars VARIABLES)
    foreach(_v IN LISTS _vars)
        if(_v MATCHES "^NROS_SIZING_")
            set(${_v} "${${_v}}" PARENT_SCOPE)
        endif()
    endforeach()

    if(NOT _nsd_QUIET)
        message(STATUS
            "nano-ros: sizing descriptor ${_stem} -- status ${NROS_SIZING_STATUS}, "
            "basis ${NROS_SIZING_BASIS}, ${NROS_SIZING_ENDPOINT_COUNT} endpoint(s)")
    endif()
endfunction()
