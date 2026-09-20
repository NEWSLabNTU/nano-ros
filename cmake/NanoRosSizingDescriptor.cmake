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

# nros_sizing_descriptor_from_model(<out_var>) — phase-454 W14
#
# WRITE a descriptor for this entry from its resolved SystemModel, and remember
# the path so the cargo lane can be told about it.
#
# ## Why a cmake road needs its own producer
#
# `nros sync` writes a descriptor for a single-package cargo LEAF, where the CLI
# has the leaf's `metadata/` probe and its `generated/` bound tables to hand. A
# cmake / Zephyr west / NuttX entry has NEITHER. Its one statement of what the
# image declares is the resolved SystemModel — so through phase-454 W12 this
# road wrote no descriptor at all and every RFC-0100 D5 derivation was inert on
# it, which is exactly what W11 measured.
#
# What the model-only producer may CLAIM is settled by RFC-0100 D6: the counts
# and all four QoS policies are STATED, and every field that needs a leaf —
# `wire_bound_bytes`, `storage_bytes`, `[types]`'s three maxima and
# `registration_path` — is REFUSED, each refusal naming the issue that tracks
# closing the gap. `Fact::stated()` is the only accessor that yields a value, so
# a consumer cannot read one of those refusals as a default.
#
# ## Three things this function does NOT do
#
# * It does not write a file for a model that describes no wiring. The CLI
#   reports that and writes nothing, so `nros_sizing_descriptor_read()` below
#   finds none and every consumer keeps its defaults — "no contract, no change",
#   which is phase-454 W12's own control held on this road.
# * It does not fail a configure. A model this producer cannot read leaves the
#   build exactly where it was, for the reason `resolve_image` states one
#   artifact over: making a descriptor a new way for a build to stop would be a
#   regression paid by every image for the benefit of the few that derive.
# * It does not pass a target triple. A cross cmake entry therefore gets a
#   REFUSED `[target]`, naming the board rule (RFC-0100 D1) — the board
#   descriptor is not resolved in this scope. `--host-build` is passed when this
#   configure is not cross-compiling, which is the one case a host answer IS the
#   target's answer.
function(nros_sizing_descriptor_from_model _out_var)
    cmake_parse_arguments(_nsw "" "CLI;MODEL;ENTRY;BUILD_DIR;RMW" "" ${ARGN})
    set(${_out_var} "" PARENT_SCOPE)

    if(NOT _nsw_ENTRY OR NOT _nsw_MODEL OR NOT EXISTS "${_nsw_MODEL}")
        return()
    endif()
    if(NOT _nsw_CLI OR NOT EXISTS "${_nsw_CLI}")
        return()
    endif()
    set(_build_dir "${_nsw_BUILD_DIR}")
    if(NOT _build_dir)
        set(_build_dir "${CMAKE_BINARY_DIR}")
    endif()

    # Issue 1018 — the MODEL is an input to a CONFIGURE-TIME emitter, so its
    # freshness reduces to "does a configure happen". The tool half is
    # registered by `nros_sizing_descriptor_read()` below, which every caller of
    # this function calls next.
    set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_nsw_MODEL}")

    set(_host_arg "")
    if(NOT CMAKE_CROSSCOMPILING)
        set(_host_arg --host-build)
    endif()
    set(_rmw_arg "")
    if(_nsw_RMW)
        set(_rmw_arg --rmw "${_nsw_RMW}")
    endif()

    execute_process(
        COMMAND "${_nsw_CLI}" ws sizing-descriptor
                --from-model "${_nsw_MODEL}"
                --build-dir "${_build_dir}"
                --entry "${_nsw_ENTRY}"
                --road "a cmake entry"
                ${_host_arg} ${_rmw_arg}
        OUTPUT_VARIABLE _out
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)
    if(NOT _rc EQUAL 0)
        string(REGEX REPLACE "\n+" " " _why "${_err}")
        message(STATUS
            "nano-ros: no sizing descriptor written for `${_nsw_ENTRY}` -- ${_why}. "
            "Every consumer keeps its own default sizes (RFC-0100 D6).")
        return()
    endif()
    if(_out STREQUAL "")
        # The model describes no wiring. The CLI already said so on stderr.
        return()
    endif()
    set(${_out_var} "${_out}" PARENT_SCOPE)
    set_property(GLOBAL APPEND PROPERTY NROS_SIZING_DESCRIPTOR_PATHS "${_out}")
endfunction()

# nros_sizing_descriptor_cargo_env(<out_var>) — phase-454 W14, issue 0460
#
# The `KEY=VALUE` row that names this configure's descriptor to CARGO, or empty.
#
# Issue 0460 is the whole reason this is a row rather than a `set(ENV{...})`:
# that only touches the configure-time process, the C lane re-bakes its own
# command and zephyr-lang-rust's `rust_cargo_application` inherits nothing — so
# a knob published that way reaches one lane and not the other, which is how
# `MAX_QUERYABLES` came to be 16 in one TU and 8 in another. It rides the same
# carrier as the entity facts, onto the same Corrosion targets, at the same
# deferred moment.
#
# EXACTLY ONE OR NONE. A configure that declared several entries has several
# descriptors and one shared staticlib, and `NROS_SIZING_DESCRIPTOR` names a
# single file: handing cargo one of N would size the shared archive from one
# image and call it derived. The entity facts take a MAX across models for the
# same collision; a descriptor is a whole per-endpoint table and has no max, so
# this refuses instead and says so.
function(nros_sizing_descriptor_cargo_env _out_var)
    set(${_out_var} "" PARENT_SCOPE)
    get_property(_paths GLOBAL PROPERTY NROS_SIZING_DESCRIPTOR_PATHS)
    if(NOT _paths)
        return()
    endif()
    list(REMOVE_DUPLICATES _paths)
    list(LENGTH _paths _n)
    if(_n GREATER 1)
        message(STATUS
            "nano-ros: ${_n} sizing descriptors in this configure and one shared cargo "
            "archive, so none is named to cargo -- the Rust half keeps its own defaults "
            "rather than sizing every image from one of them (RFC-0100 D6).")
        return()
    endif()
    list(GET _paths 0 _one)
    set(${_out_var} "NROS_SIZING_DESCRIPTOR=${_one}" PARENT_SCOPE)
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
