# NrosRmwUorbSizing.cmake — phase-454 W6.d, RFC-0100 D5.
#
#   > Each backend's build reads the descriptor and computes its OWN knobs.
#   > Adding a fifth backend touches no shared code, and each backend's
#   > arithmetic stays where a maintainer of that backend reads it.
#
# uORB has exactly two size knobs and owns the sample storage itself, so there
# is no payload bound, no QoS depth, no MTU and no reliability to consume here.
# Counts only:
#
#   NROS_RMW_UORB_REGISTRY_CAPACITY   `Entry g_table[N]` in topic_registry.cpp,
#                                     a topic name -> `orb_metadata *` map. The
#                                     image's DISTINCT TOPIC count: its declared
#                                     publishers and subscriptions, deduplicated
#                                     BY TOPIC NAME, because two endpoints on one
#                                     topic share one registry entry.
#
#   NROS_RMW_UORB_PX4_MAX_CALLBACKS   `Slot g_pool[N]` in px4_callback_glue.cpp,
#                                     one per subscription wanting a push-wake
#                                     callback. `[image] subscriber_count` is
#                                     that count: the session's subscriber slots,
#                                     which is what the vtable opens.
#
# THE FLOOR IS HERE, AND THE DEMAND IS NOT FLOORED (RFC-0100 D7, issues
# 1015 + 1033). The descriptor publishes an image's demand unfloored — zero
# publishers and zero subscriptions IS zero distinct topics — and whether zero
# is a legal SIZE is a property of the storage. For these two it is not: both
# arrays are C++, ISO C++ has no zero-size array, and both TUs are built
# `-Wpedantic` inside a `-Werror` PX4 (issue 1131). So `_nros_c_array_pool_floor`
# raises each to 1 at THIS consumer, and the `#if < 1 / #error` beside each array
# is the backstop that binds a producer this file never reaches.
#
# NO DESCRIPTOR, NO DEFINES. An image nobody ran `nros sync` for gets an empty
# list back, no `-D` reaches either TU, and the `#ifndef ... 64` in each source
# stands — byte-identical to every uORB build before this wave.
#
# THE SECOND ROAD, AND WHY IT HAS NOTHING TO READ. `px4_add_module()` consumers
# (`packages/testing/nros-px4-register-check`, `examples/px4/cpp/*`) list these
# sources by path and pass defines through `COMPILE_FLAGS`, bypassing the
# package's own `CMakeLists.txt` entirely. They may call this function too — it
# emits a plain `<MACRO>=<value>` list, which is what both `COMPILE_FLAGS` and
# `target_compile_definitions` take. None does today, because none of them has a
# sizing descriptor: a PX4 firmware module is built inside a PX4 tree from
# sources with no `system.toml`, so there is no contract for `nros sync` to
# resolve. The abstention is written here rather than left to be rediscovered.

include_guard(GLOBAL)

# `_nros_c_array_pool_floor` — the ONE CMake spelling of the rule above, shared
# with the Zephyr lane's three zenoh pools. Reached by a path relative to this
# file, the same way the package's own `CMakeLists.txt` reaches the rmw-abi
# headers, so a standalone `add_subdirectory` of this package resolves it.
include("${CMAKE_CURRENT_LIST_DIR}/../../../../../cmake/NanoRosPoolFloor.cmake")

# nros_rmw_uorb_sizing_defines(<out_var>)
#
# Compute the two `-D` rows from the `NROS_SIZING_*` variables a prior
# `nros_sizing_descriptor_read()` defined, and return them as a
# `<MACRO>=<value>` list. EMPTY when this image declared nothing.
#
# It reads the variables rather than the file: CMake does not parse TOML and
# must not learn to (the schema has ONE reader), and the projection is already
# the shape a `.cmake` consumer can use. A REFUSED fact has no value variable at
# all — only a `_REFUSED` reason beside it — so `if(DEFINED ...)` below is the
# only road to a number, which is RFC-0100 D6 in CMake's vocabulary.
function(nros_rmw_uorb_sizing_defines out_var)
    set(_defs "")

    # --- the registry: distinct topics over publishers and subscriptions -----
    #
    # The endpoint table arrives as parallel lists of NROS_SIZING_ENDPOINT_COUNT
    # elements. A cell that is the literal `REFUSED` or `ABSENT` is not a value;
    # `kind` and `topic` are the descriptor's IDENTITY columns and are never
    # either, which is what makes this count derivable from rows at all.
    #
    # `NROS_SIZING_UNDECLARED_ENDPOINTS` is the precondition, and it is not
    # decoration. An `ENDPOINT_COUNT` of 0 has two causes that read identically
    # here: an image that declares no endpoints, and an image whose entity
    # inventory DID NOT COMPOSE. The descriptor keeps them apart -- the second
    # refuses `undeclared_endpoints` with "absence is not zero" and so publishes
    # no value for it -- and the difference is the whole ballgame, because a
    # registry derived from rows nobody could see is SHORT, and a short registry
    # is topics that fail to register at runtime. Unlike every other number in
    # this file, that is the direction with no safe fallback, so the row-derived
    # capacity is withheld rather than floored.
    if(DEFINED NROS_SIZING_ENDPOINT_COUNT AND DEFINED NROS_SIZING_UNDECLARED_ENDPOINTS)
        set(_topics "")
        math(EXPR _last "${NROS_SIZING_ENDPOINT_COUNT} - 1")
        if(_last GREATER_EQUAL 0)
            foreach(_i RANGE ${_last})
                list(GET NROS_SIZING_ENDPOINT_KIND ${_i} _kind)
                list(GET NROS_SIZING_ENDPOINT_TOPIC ${_i} _topic)
                if(_kind STREQUAL "publisher" OR _kind STREQUAL "subscription")
                    list(APPEND _topics "${_topic}")
                endif()
            endforeach()
        endif()
        # Deduplicate BY TOPIC NAME: a publisher and a subscription on one topic
        # are one registry entry, because the table is keyed by name.
        if(_topics)
            list(REMOVE_DUPLICATES _topics)
        endif()
        list(LENGTH _topics _capacity)
        _nros_c_array_pool_floor(_capacity "${_capacity}"
            NROS_RMW_UORB_REGISTRY_CAPACITY)
        list(APPEND _defs "NROS_RMW_UORB_REGISTRY_CAPACITY=${_capacity}")
    elseif(DEFINED NROS_SIZING_ENDPOINT_COUNT)
        message(STATUS
            "nano-ros: the sizing descriptor states no `undeclared_endpoints`, so its "
            "endpoint rows are not a complete account of this image; "
            "NROS_RMW_UORB_REGISTRY_CAPACITY keeps its built-in 64 rather than a count "
            "that could be short")
    endif()

    # --- the push-wake pool: one slot per subscriber ------------------------
    if(DEFINED NROS_SIZING_IMAGE_SUBSCRIBER_COUNT)
        _nros_c_array_pool_floor(_callbacks "${NROS_SIZING_IMAGE_SUBSCRIBER_COUNT}"
            NROS_RMW_UORB_PX4_MAX_CALLBACKS)
        list(APPEND _defs "NROS_RMW_UORB_PX4_MAX_CALLBACKS=${_callbacks}")
    elseif(DEFINED NROS_SIZING_IMAGE_SUBSCRIBER_COUNT_REFUSED)
        # LOUD, and the safe direction (D6). The builtin 64 over-states this
        # image; a derived number under it would be a pool that runs out.
        message(STATUS
            "nano-ros: sizing descriptor refused `subscriber_count` "
            "(${NROS_SIZING_IMAGE_SUBSCRIBER_COUNT_REFUSED}); "
            "NROS_RMW_UORB_PX4_MAX_CALLBACKS keeps its built-in 64")
    endif()

    set(${out_var} "${_defs}" PARENT_SCOPE)
endfunction()
