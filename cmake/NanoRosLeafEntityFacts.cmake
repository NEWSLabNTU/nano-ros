# NanoRosLeafEntityFacts.cmake — issue 1142 (phase-412 W5): a STANDALONE CMake
# leaf sizes its pools from its own declaration instead of a guess.
#
# THE GAP THIS CLOSED. A copy-out project (`find_package(nano_ros)` +
# `nano_ros_add_executable`) has no bringup and no SystemModel, so the model
# roads never reach it, it is not a Zephyr image, and it is not a cargo leaf.
# Its pools were sized from `if hosted { 32 } else { 8 }`, a guess by
# construction; issue 1028 measured what a wrong guess on a sibling path costs
# (142,336 B of `.bss` on an image with zero queryables).
#
# THE ANSWER IS RFC-0098 D3/D8: the leaf states its board in `system.toml`,
# and a component the host cannot probe DECLARES its entities there --
# `entities = [...]` in the `EntityDecl::parse` grammar, or `"census"` (issue
# 1556). This module is the CMake side of reading that.
#
# NOTHING IS DERIVED HERE. Since phase-486 the declaration reaches the
# consumers through ONE carrier, the leaf's SIZING DESCRIPTOR
# (`nros ws sizing-descriptor --from-leaf`, phase-457 W0.b), which applies the
# model road's own counting rules and states the node count and the runtime's
# service families (`[image] model_nodes`, `infra_families`). The
# `nros ws entity-facts --leaf` verb and the `NROS_DECLARED_*` fold it fed are
# retired: a second carrier of one fact is how two roads come to size one image
# two ways.
#
# A DEFAULT, NEVER AN OVERRIDE. The consuming build scripts read the descriptor
# as a DEFAULT: an image that states `ZPICO_MAX_QUERYABLES` still wins, and the
# zenoh build refuses a stated value below the derived FLOOR rather than
# silently lowering the pool (`check_queryable_override`).

include_guard(GLOBAL)

include("${CMAKE_CURRENT_LIST_DIR}/NanoRosEntityFacts.cmake")
# phase-457 W0.b — `nros_sizing_descriptor_from_leaf`. Included at FILE scope and
# not inside the function: `CMAKE_CURRENT_LIST_DIR` inside a function frame is a
# footgun this tree has already paid for (a plain `set(_X_DIR
# ${CMAKE_CURRENT_LIST_DIR})` in a function broke every freertos workspace
# member's `configure_file`), and the sibling include one line up is the pattern
# that works. Both modules are `include_guard(GLOBAL)`, so this costs nothing.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosSizingDescriptor.cmake")
# Issue 2086 -- `nros_leaf_census_take`, shared with the Zephyr module.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosLeafCensus.cmake")

# Issue 2043 — the program a HOST census build of a leaf runs, written as
# `<build>/nros/census-program.txt` at generate time:
#
#     executable=<target>=<path of its binary>
#     source=<a directory of the project, nano-ros's own excluded>
#
# A standalone leaf names its program with `nano_ros_add_executable` in its own
# `CMakeLists.txt`, which `nros ws entity-census take --leaf` reads. A ported
# ament workspace does not: its executables are `ament_auto_add_executable` /
# `add_executable` calls in packages that sit outside the leaf directory (the
# workspace's `src/`). So the census build states them itself, and the package
# directories it lists are what the census is stale against.
#
# Armed only in the host census build (`NROS_LEAF_CENSUS_HOST`), which is the
# build the verb reads it from.
function(_nros_census_program_walk dir skip)
    get_property(_src DIRECTORY "${dir}" PROPERTY SOURCE_DIR)
    file(REAL_PATH "${_src}" _src)
    # nano-ros itself, and the nano-ros packages a port's `find_package`
    # brings in (`nros-diagnostic-updater`), are not the program: the binary
    # digest already covers what they compile into it. The leaf's own tree is,
    # always, even when the leaf sits inside a nano-ros checkout (an in-tree
    # fixture under `packages/testing`).
    file(REAL_PATH "${CMAKE_SOURCE_DIR}" _leaf)
    string(FIND "${_src}/" "${_leaf}/" _in_leaf)
    if(NOT _in_leaf EQUAL 0 AND (_src STREQUAL skip OR _src MATCHES "^${skip}/packages/"))
        return()
    endif()
    get_property(_tgts DIRECTORY "${dir}" PROPERTY BUILDSYSTEM_TARGETS)
    foreach(_t IN LISTS _tgts)
        get_target_property(_type ${_t} TYPE)
        if(_type STREQUAL "EXECUTABLE")
            set_property(GLOBAL APPEND PROPERTY _NROS_CENSUS_PROGRAM_LINES
                "executable=${_t}=$<TARGET_FILE:${_t}>")
        endif()
    endforeach()
    set_property(GLOBAL APPEND PROPERTY _NROS_CENSUS_PROGRAM_LINES "source=${_src}")
    get_property(_subs DIRECTORY "${dir}" PROPERTY SUBDIRECTORIES)
    foreach(_s IN LISTS _subs)
        _nros_census_program_walk("${_s}" "${skip}")
    endforeach()
endfunction()

function(_nros_census_program_write)
    get_property(_skip GLOBAL PROPERTY _NROS_CENSUS_PROGRAM_SKIP)
    set_property(GLOBAL PROPERTY _NROS_CENSUS_PROGRAM_LINES "")
    _nros_census_program_walk("${CMAKE_SOURCE_DIR}" "${_skip}")
    get_property(_lines GLOBAL PROPERTY _NROS_CENSUS_PROGRAM_LINES)
    list(JOIN _lines "\n" _text)
    file(GENERATE OUTPUT "${CMAKE_BINARY_DIR}/nros/census-program.txt"
        CONTENT "${_text}\n")
endfunction()

function(_nros_census_program_arm)
    get_property(_armed GLOBAL PROPERTY _NROS_CENSUS_PROGRAM_ARMED)
    if(_armed OR "$ENV{NROS_LEAF_CENSUS_HOST}" STREQUAL "")
        return()
    endif()
    set_property(GLOBAL PROPERTY _NROS_CENSUS_PROGRAM_ARMED TRUE)
    # nano-ros's own directory (and so every target under it) is not the
    # program: on the add_subdirectory road it is a subdirectory of the leaf.
    file(REAL_PATH "${NROS_REPO_DIR}" _skip)
    set_property(GLOBAL PROPERTY _NROS_CENSUS_PROGRAM_SKIP "${_skip}")
    cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}" CALL _nros_census_program_write)
endfunction()

# nros_record_leaf_entity_facts(<leaf-dir>)
#
# Write the sizing descriptor for what a standalone leaf's `system.toml`
# DECLARES. Call after `nano_ros_read_leaf_system()`, from the arm that already
# knows the leaf directory. (The name predates phase-486, when this also folded
# `NROS_DECLARED_*` facts.)
#
# SOFT ON EVERY ABSENCE: no `system.toml`, no CLI, a CLI that refuses -- each
# means "this configure has no leaf descriptor", the state every standalone
# leaf was already in. Except a census leaf whose census cannot answer, which
# has nothing else to size from and is FATAL (issue 1556).
#
# THE ONE THING IT IS LOUD ABOUT is a leaf that declares NOTHING, because that
# is the case a reader can act on and the case that otherwise looks identical
# to success. It names the file to edit and the key to add.
function(nros_record_leaf_entity_facts _dir)
    if(_dir STREQUAL "" OR NOT EXISTS "${_dir}/system.toml")
        return()
    endif()
    # `nros_resolve_cli` and NOT `_NANO_ROS_CODEGEN_TOOL` directly: this runs at
    # `find_package(nano_ros)` time, BEFORE the import that defines that cache
    # variable, so reading it here would find nothing on the very road this
    # module exists for. The shared resolver is the one that answers this early
    # (`nano_ros_read_leaf_system` is reached the same way, two lines up).
    if(NOT COMMAND nros_resolve_cli)
        return()
    endif()
    nros_resolve_cli(_nros OPTIONAL
        CONTEXT "nros_record_leaf_entity_facts (${_dir}/system.toml)")
    if(NOT _nros OR _nros STREQUAL "NOTFOUND" OR NOT EXISTS "${_nros}")
        return()
    endif()

    # Once per leaf directory. A configure can reach `find_package(nano_ros)`
    # several times (the verbs re-enter it), and folding the same declaration
    # twice is harmless only because the accumulator takes a MAX — relying on
    # that would be relying on an accident.
    string(MAKE_C_IDENTIFIER "NROS_LEAF_ENTITY_FACTS_MEMO__${_dir}" _memo)
    get_property(_seen GLOBAL PROPERTY ${_memo})
    if(_seen)
        return()
    endif()
    set_property(GLOBAL PROPERTY ${_memo} TRUE)

    # Issue 1556 -- `entities = "census"`: the leaf states that its entities
    # are what its program creates, read from its census. Read here from the
    # file itself, not inferred from the verb's message: whether a failure
    # below may degrade to the fallback budget is a property of the leaf.
    file(STRINGS "${_dir}/system.toml" _nros_leaf_census_optin
        REGEX "^[ \t]*entities[ \t]*=[ \t]*\"census\"")

    if(_nros_leaf_census_optin)
        nros_leaf_census_take("${_nros}" "${_dir}")
    endif()
    if(_nros_leaf_census_optin)
        _nros_census_program_arm()
        # Re-taking the census (after a `src/` edit) must reconfigure, because
        # this configure is what read it. ASKED for, never spelled: the CLI
        # owns where a leaf's census lives.
        execute_process(
            COMMAND "${_nros}" ws entity-census path --leaf "${_dir}"
            OUTPUT_VARIABLE _nros_leaf_census
            RESULT_VARIABLE _nros_leaf_census_rc
            OUTPUT_STRIP_TRAILING_WHITESPACE)
        if(_nros_leaf_census_rc EQUAL 0 AND EXISTS "${_nros_leaf_census}")
            set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS
                "${_nros_leaf_census}")
        endif()
    endif()

    # phase-486 -- the leaf's facts reach its consumers through its SIZING
    # DESCRIPTOR only (phase-457 W0.b gave this road one; issues 1407 / 1378).
    # `nros ws entity-facts --leaf` and the fold it fed carried the last
    # `NROS_DECLARED_*` facts of this road -- the node count (retired in W9) and
    # the service families (retired with the verb) -- and the descriptor states
    # both (`[image] model_nodes`, `infra_families`).
    #
    # A census leaf has no other statement to fall back on, so a census that
    # cannot answer is FATAL here (issue 1556), as it was on the verb.
    #
    # The top-level `project()` is the entry name: a standalone leaf's runs
    # before its `find_package(nano_ros)`, so this is the name every other
    # per-entry artifact in this build dir is keyed by. `CMAKE_PROJECT_NAME`
    # and not `PROJECT_NAME`, because on the add_subdirectory road this runs
    # inside nano-ros's own `project()` (issue 2043).
    set(_nros_leaf_required "")
    if(_nros_leaf_census_optin)
        set(_nros_leaf_required REQUIRED)
    endif()
    nros_sizing_descriptor_from_leaf(_nros_leaf_descriptor ${_nros_leaf_required}
        CLI       "${_nros}"
        LEAF      "${_dir}"
        ENTRY     "${CMAKE_PROJECT_NAME}"
        BUILD_DIR "${CMAKE_BINARY_DIR}")
    if(_nros_leaf_descriptor)
        message(STATUS
            "nano-ros: sizing descriptor written from ${_dir}/system.toml — "
            "${_nros_leaf_descriptor} (phase-457 W0.b, issue 1378)")
    elseif(_nros_leaf_census_optin)
        # This IS the host census build (`nros ws entity-census take --leaf`),
        # which is not sized by the census it produces.
        message(STATUS
            "nano-ros: ${_dir} -- host census build; pools at the hosted defaults "
            "(issue 1556)")
    else()
        # The leaf declares no entities. Say what the reader can do about it,
        # and say that the fallback is what decided (issue 0973's rule).
        message(STATUS
            "nano-ros: ${_dir}/system.toml declares no entities, so the queryable "
            "table keeps the backend's FALLBACK budget (issue 1142). To size it "
            "from the declaration, add `entities = [...]` to its `[[component]]` "
            "— e.g. entities = [\"publisher:std_msgs/msg/String:/chatter\", \"timer\"] "
            "(RFC-0098 D8); the runtime's own service families are "
            "`[system] features`.")
    endif()
    # A configure re-runs when the declaration changes. `nano_ros_read_leaf_system`
    # already registers this file, but that is its dependency and not ours: a
    # module that stops being included must take its own edge with it.
    set_property(DIRECTORY APPEND PROPERTY
        CMAKE_CONFIGURE_DEPENDS "${_dir}/system.toml")
endfunction()
