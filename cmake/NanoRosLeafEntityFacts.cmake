# NanoRosLeafEntityFacts.cmake — issue 1142 (phase-412 W5): a STANDALONE CMake
# leaf sizes its pools from its own declaration instead of a guess.
#
# THE GAP THIS CLOSES. Three declaration channels existed and a standalone
# copy-out project was in between all three:
#
#   * CMake WORKSPACE — a resolved SystemModel, `nros ws entity-facts --model`,
#     folded by `nros_record_entity_facts` (NanoRosEntityFacts.cmake). A leaf
#     with no bringup has no model, so that function returns early by design.
#   * ZEPHYR — `CONFIG_NROS_MAX_QUERYABLES` at the `-1` derive sentinel. Not a
#     Zephyr image, so it does not apply.
#   * CARGO LEAF — `nros sync`'s `[env]` sidecar. Not a cargo leaf, so the
#     sidecar never reaches a `cmake`-driven build.
#
# So `NROS_DECLARED_SERVICE_SERVERS` and `NROS_DECLARED_INFRA_QUERYABLES` never
# arrived, and `queryable_default_from` took its last arm — `if hosted { 32 }
# else { 8 }`, a guess by construction. Issue 1028 measured what a wrong guess
# on a sibling path costs: 142,336 B of `.bss` on an image with zero queryables.
#
# THE ANSWER IS ALREADY DECIDED, by RFC-0098 D3/D8: every leaf states its board
# in `system.toml`, and a component the host cannot probe DECLARES its entities
# there — `entities = [...]`, the `EntityDecl::parse` grammar, the same grammar
# `nano_ros_node_register(... ENTITIES ...)` uses. This module is only the
# CMake side of reading it, which is what RFC-0098's "What is NOT decided" left
# open.
#
# NOTHING IS DERIVED HERE. `nros ws entity-facts --leaf` applies the model
# road's own counting rule (a service server is one queryable, an action server
# is three, either client is none) and prints the SAME three `KEY=VALUE` facts;
# `nros_fold_entity_facts` folds them into the SAME accumulator; and
# `nros_entity_facts_env` — already deferred onto `nros_cpp-static` /
# `nros_c-static` — delivers them through the SAME `corrosion_set_env_vars`
# carrier. A second derivation is how two roads come to size one image two ways.
#
# A DEFAULT, NEVER AN OVERRIDE. The facts travel as `NROS_DECLARED_*`, which the
# consuming build script reads as a DEFAULT: an image that states
# `ZPICO_MAX_QUERYABLES` still wins, and the build script refuses a stated value
# below the derived FLOOR rather than silently lowering the pool
# (`check_queryable_override`). That is phase-412's ladder, unchanged.

include_guard(GLOBAL)

include("${CMAKE_CURRENT_LIST_DIR}/NanoRosEntityFacts.cmake")

# nros_record_leaf_entity_facts(<leaf-dir>)
#
# Fold what a standalone leaf's `system.toml` DECLARES into this configure's
# entity facts. Call after `nano_ros_read_leaf_system()`, from the arm that
# already knows the leaf directory; the delivery happens later, at the deferred
# flush, so ordering here is only "before the end of the top-level scope".
#
# SOFT ON EVERY ABSENCE, exactly like `nros_record_entity_facts`. No
# `system.toml`, no CLI, a CLI that refuses — each means "this configure has no
# leaf facts to carry", which is the state every standalone leaf was already in.
# None of them is a configuration error, so none of them is fatal.
#
# THE ONE THING IT IS LOUD ABOUT is a leaf that declares NOTHING, because that
# is the case a reader can act on and the case that otherwise looks identical to
# success. `nros_entity_facts_env` stays silent when no facts were seen — by
# design, since "a configure whose models are not resolved yet has always sized
# the table from the backend's own default" — so without this line the fallback
# deciding is indistinguishable from the declaration deciding. It names the file
# to edit and the key to add.
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

    execute_process(
        COMMAND "${_nros}" ws entity-facts --leaf "${_dir}"
        OUTPUT_VARIABLE _out
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)
    if(NOT _rc EQUAL 0)
        string(REGEX REPLACE "\n+" " " _why "${_err}")
        string(SUBSTRING "${_why}" 0 200 _why)
        message(STATUS
            "nano-ros: leaf entity facts NOT read from ${_dir}/system.toml — ${_why}")
        return()
    endif()

    if(_out STREQUAL "")
        # The verb ABSTAINED: the leaf declares no entities. Say what the
        # reader can do about it, and say that the fallback is what decided
        # (issue 0973's rule — "nobody stated it" is true and unactionable on
        # its own).
        message(STATUS
            "nano-ros: ${_dir}/system.toml declares no entities, so the queryable "
            "table keeps the backend's FALLBACK budget (issue 1142). To size it "
            "from the declaration, add `entities = [...]` to its `[[component]]` "
            "— e.g. entities = [\"publisher:std_msgs/msg/String:/chatter\", \"timer\"] "
            "(RFC-0098 D8); the runtime's own service families are "
            "`[system] features`.")
        return()
    endif()

    nros_fold_entity_facts("${_out}")
    # A configure re-runs when the declaration changes. `nano_ros_read_leaf_system`
    # already registers this file, but that is its dependency and not ours: a
    # module that stops being included must take its own edge with it.
    set_property(DIRECTORY APPEND PROPERTY
        CMAKE_CONFIGURE_DEPENDS "${_dir}/system.toml")
endfunction()
