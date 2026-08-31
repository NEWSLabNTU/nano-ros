# NanoRosMessageBounds.cmake -- phase-403 W8 (issue 0940): the READER for the
# bound inventory W6 exports.
#
# W6 gave every generated interface package a derived per-type serialized-size
# bound and three transports to carry it out of codegen. Nothing read any of
# them, so every size downstream was still a number a human typed -- and on the
# one bring-up that measured it, every one of those numbers was wrong at least
# once. `NROS_MAX_LARGE_SUBSCRIBERS` and `NROS_SUBSCRIBER_LARGE_SIZE` were read
# off generated C++ headers BY EYE, and the headers they were read off state an
# ESTIMATE rather than a bound (W6's own finding).
#
# This module composes the fragments and derives the FOUR knobs a bound
# inventory can actually answer.
#
# =============================================================================
# What a bound inventory can and cannot answer
# =============================================================================
#
# It knows EVERY TYPE'S SIZE. It does not know WHICH ENTITIES AN IMAGE CREATES.
# W4 established that: 0 of the 115 resolved SystemModels in the tree carry any
# topic wiring, and the RFC-0043 C++ components register in their constructors
# at runtime. So:
#
#   DERIVABLE HERE -- a question about sizes:
#     NROS_SUBSCRIBER_BUFFER_SIZE     the small payload class
#     NROS_SUBSCRIBER_LARGE_SIZE      the large payload class
#     NROS_MAX_LARGE_SUBSCRIBERS      how many types exceed the small ceiling
#     NROS_SUBSCRIPTION_BUFFER_SIZE   the take buffer for a caller with no type
#
#   NOT DERIVABLE HERE -- a question about entity COUNTS, which needs a second
#   source (an entity inventory), not this one:
#     NROS_EXECUTOR_MAX_CBS, NROS_EXECUTOR_ARENA_SIZE,
#     NROS_MAX_SUBSCRIBERS, NROS_MAX_PUBLISHERS
#
#   A package's TYPE count is not an image's ENTITY count. Deriving those from
#   this inventory would produce exactly the plausible-wrong-number this
#   campaign exists to remove.
#
# =============================================================================
# The derived numbers are an UPPER BOUND on what the image needs
# =============================================================================
#
# The inventory holds every type in the LINKED INTERFACE CLOSURE, not just the
# subscribed ones. A package is linked because something in the image mentions
# one of its types; the other 90 come along. So a derived class size is the
# largest type the image COULD receive, not the largest it DOES.
#
# That errs in the safe direction -- too big, never too small -- and it is the
# same direction the hand-set numbers were supposed to err in and did not. It
# is stated here, in the generated output, and in the Kconfig help, because a
# number a user cannot account for is a number they will eventually "fix".
#
# Narrowing it needs the same entity inventory the out-of-scope knobs need.
#
# =============================================================================
# An unbounded type is not silently dropped
# =============================================================================
#
# If ANY type in the closure is `unbounded` or `unresolved`, no class size is
# derived at all. The alternative -- deriving over the bounded subset -- would
# publish a maximum that a real sample can exceed, which is the silent
# BufferTooSmall drop this whole phase exists to stop. The refusal is LOUD, it
# names the types and the member that costs each one its bound, and every knob
# falls back to its configured value.
#
# =============================================================================
# Usage
# =============================================================================
#
#   include(NanoRosMessageBounds.cmake)
#   nros_derive_message_bound_knobs(
#       FRAGMENTS <nros_message_bounds.cmake>...   # or omit: uses the cache list
#       [SMALL_CLASS_CEILING 2048]                 # policy, see below
#       [OUTPUT_FILE <path>]                       # write the answer + why
#       [QUIET])
#
# Sets in the CALLER's scope:
#
#   NROS_MESSAGE_BOUNDS_STATUS        derived | refused
#   NROS_MESSAGE_BOUNDS_REASON        prose, when refused
#   NROS_MESSAGE_BOUNDS_PACKAGES      the packages composed
#   NROS_MESSAGE_BOUNDS_TYPE_COUNT    types seen
#   NROS_MESSAGE_BOUNDS_BOUNDED_COUNT types with a derived bound
#   NROS_MESSAGE_BOUNDS_OPEN_TYPES    the unbounded/unresolved ones
#   NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE     \
#   NROS_DERIVED_SUBSCRIBER_LARGE_SIZE       |  unset when not derivable --
#   NROS_DERIVED_MAX_LARGE_SUBSCRIBERS       |  ABSENT means "no answer",
#   NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE   /   never a substituted default
#   NROS_DERIVED_LARGEST_TYPE / _LARGEST_RX  provenance for the two above
#   NROS_DERIVED_LARGE_TYPES                 which types drove MAX_LARGE
#
# A derived value is a DEFAULT. Every consumer applies it only where nothing
# else stated a number -- see `_nros_resolve_derivable_knob` in
# `zephyr/cmake/nros_cargo_build.cmake` for the precedence ladder.

include_guard(GLOBAL)

# Both constants below are `CACHE INTERNAL` and not plain variables, for the
# `_NROS_ENTRY_DIR` reason (AGENTS.md, CMake Pitfalls) sharpened by
# `include_guard(GLOBAL)`. This file is reached through
# `NanoRosCodegenCore.cmake`, and at least one caller
# (`NanoRosWorkspace.cmake`'s `nros_resolve_cli` branch) includes that from
# INSIDE a function. If that include happens first, a file-scope `set()` here
# lands in that frame and is gone when it pops -- while the guard makes every
# later include a no-op, so the constant never comes back. The FUNCTIONS
# survive (they are global) and only the variables vanish, which is the shape
# that fails far from its cause: an empty schema constant turns the version
# check into a FATAL_ERROR on every well-formed fragment.

# The inventory schema this reader understands. `bounds.rs`'s
# `INVENTORY_SCHEMA_VERSION`. A fragment that states anything else is REFUSED,
# never read field-by-field on the hope that nothing moved.
set(NROS_MESSAGE_BOUNDS_SCHEMA_SUPPORTED 1 CACHE INTERNAL
    "phase-403 W8: the nros_message_bounds fragment schema this tree reads")

# The split between the small and the large payload class, in bytes.
#
# POLICY, not a derived fact: it is `ZPICO_SUBSCRIBER_SIZE_THRESHOLD`'s shipped
# default, and the shim's own ceiling is `min(threshold, SUBSCRIBER_BUFFER_SIZE)`
# (`shim/subscriber.rs::SMALL_CLASS_CEILING`). Because the derived small size is
# by construction the largest bound AT OR UNDER this number, that `min` picks
# the derived size and the two agree. Pass the resolved threshold instead when a
# consumer has set one, so the classification here and the routing at runtime
# cannot disagree.
set(NROS_MESSAGE_BOUNDS_DEFAULT_SMALL_CEILING 2048 CACHE INTERNAL
    "phase-403 W8: the small/large payload class split used when none is passed")

# nros_message_bounds_register_fragment(<path>)
#
# Record one package's `nros_message_bounds.cmake` in the image-wide list the
# composer reads when it is given no explicit FRAGMENTS. Called by both
# generator lanes (`cmake/NanoRosGenerateInterfaces.cmake` and
# `zephyr/cmake/nros_generate_interfaces.cmake`) so there is ONE place to look
# and not one per lane.
#
# A GLOBAL PROPERTY and not a CACHE variable, deliberately. The list has to
# cross function frames and `add_subdirectory()` boundaries, which both rule out
# a normal variable -- but it must NOT survive to the next configure, which
# rules out the cache. A cached list keeps a package that has been REMOVED from
# the closure, and its stale fragment is usually still sitting in the build dir,
# so the derivation would go on pricing a type nothing links. A global property
# is reset at the start of every configure, which is exactly the lifetime the
# closure has.
function(nros_message_bounds_register_fragment _path)
    get_property(_list GLOBAL PROPERTY NROS_MESSAGE_BOUNDS_FRAGMENTS)
    list(APPEND _list "${_path}")
    list(REMOVE_DUPLICATES _list)
    list(SORT _list)
    set_property(GLOBAL PROPERTY NROS_MESSAGE_BOUNDS_FRAGMENTS "${_list}")
endfunction()

# nros_message_bounds_fragments(<out_var>) -- the list, in the caller's scope.
function(nros_message_bounds_fragments _out_var)
    get_property(_list GLOBAL PROPERTY NROS_MESSAGE_BOUNDS_FRAGMENTS)
    set(${_out_var} "${_list}" PARENT_SCOPE)
endfunction()

# _nros_bounds_publish(<name> <value>)
#
# Set a result BOTH locally and in the caller's scope.
#
# A `set(X ... PARENT_SCOPE)` writes ONLY the parent, so the deriving function
# cannot read back what it just published -- and `_nros_message_bounds_write_output`
# reads exactly those names through the scope chain. Publishing to one of the
# two scopes wrote a file full of empty values while every returned variable was
# correct, which is the shape that reads as working.
#
# A macro and not a function: a macro runs in the CALLER's scope, so its
# `PARENT_SCOPE` is the caller's parent, which is what the name promises.
macro(_nros_bounds_publish _name _value)
    set(${_name} "${_value}")
    set(${_name} "${_value}" PARENT_SCOPE)
endmacro()

# nros_message_bounds_knobs_file(<out_var>)
#
# Where the composed, image-wide answer is written, and where a consumer reads
# it. ONE path, because the writer (`nros_find_interfaces`) and the reader
# (`nros_resolve_knobs`, in the Zephyr lane) are in different files, run at
# different points of one configure, and a second spelling is how a derived
# value silently stops arriving.
#
# `CMAKE_BINARY_DIR` and not a per-package dir: the answer is a property of the
# IMAGE, composed over every interface package it links.
function(nros_message_bounds_knobs_file _out_var)
    set(${_out_var} "${CMAKE_BINARY_DIR}/nros/message_bound_knobs.cmake" PARENT_SCOPE)
endfunction()

# nros_message_bounds_seed_knobs_file(<path>)
#
# Write a "nothing composed yet" knobs file, for a reader that runs BEFORE the
# interface lane in the same configure.
#
# It exists for one mechanical reason. A consumer registers this path with
# `CMAKE_CONFIGURE_DEPENDS` so ninja re-runs cmake when the interfaces lane
# writes a new answer into it -- and a ninja input that does not exist and has
# no rule producing it is a hard `missing and no known rule to make it` at
# LOAD, before any rule runs. Seeding makes the dependency well-formed on the
# very first configure; the real answer overwrites it later in that same
# configure and the next build picks it up.
#
# Does NOT overwrite an existing file: the whole point is that the file may
# already hold a derived answer.
function(nros_message_bounds_seed_knobs_file _path)
    if(EXISTS "${_path}")
        return()
    endif()
    get_filename_component(_dir "${_path}" DIRECTORY)
    file(MAKE_DIRECTORY "${_dir}")
    file(WRITE "${_path}"
        "# GENERATED by nros (phase-403 W8, issue 0940). Do not edit.\n"
        "#\n"
        "# Placeholder: no message-bound inventory had been composed when this\n"
        "# configure first needed one. It is rewritten with the real answer by\n"
        "# nros_find_interfaces(), and its rewrite re-runs cmake.\n"
        "set(NROS_MESSAGE_BOUNDS_STATUS \"refused\")\n"
        "set(NROS_MESSAGE_BOUNDS_REASON \"no inventory composed yet\")\n")
endfunction()

# nros_derive_message_bound_knobs(...)  -- see the header comment.
function(nros_derive_message_bound_knobs)
    cmake_parse_arguments(_B "QUIET" "SMALL_CLASS_CEILING;OUTPUT_FILE" "FRAGMENTS" ${ARGN})

    set(_fragments "${_B_FRAGMENTS}")
    if(NOT _fragments)
        nros_message_bounds_fragments(_fragments)
    endif()
    set(_ceiling "${_B_SMALL_CLASS_CEILING}")
    if(NOT _ceiling)
        set(_ceiling "${NROS_MESSAGE_BOUNDS_DEFAULT_SMALL_CEILING}")
    endif()

    # ---- Nothing derived until proven otherwise -------------------------
    # Every out-variable starts UNSET. A knob that cannot be derived must be
    # absent, so a consumer either reads a number this function computed or
    # reads nothing -- the same rule the inventory itself holds to for a type
    # with no bound.
    _nros_bounds_publish(NROS_MESSAGE_BOUNDS_STATUS "refused")
    _nros_bounds_publish(NROS_MESSAGE_BOUNDS_PACKAGES "")
    _nros_bounds_publish(NROS_MESSAGE_BOUNDS_TYPE_COUNT 0)
    _nros_bounds_publish(NROS_MESSAGE_BOUNDS_BOUNDED_COUNT 0)
    _nros_bounds_publish(NROS_MESSAGE_BOUNDS_OPEN_TYPES "")
    # Cleared in BOTH scopes. The parent's copy so a second call cannot leave a
    # stale answer standing; this frame's copy because a function inherits the
    # caller's variables through the scope chain, and
    # `_nros_message_bounds_write_output` reads these names that way -- so a
    # refusal after a successful call would otherwise write the PREVIOUS numbers
    # into a file whose status says "refused".
    foreach(_v
        NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE
        NROS_DERIVED_SUBSCRIBER_LARGE_SIZE
        NROS_DERIVED_MAX_LARGE_SUBSCRIBERS
        NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE
        NROS_DERIVED_LARGEST_TYPE
        NROS_DERIVED_LARGEST_RX
        NROS_DERIVED_LARGE_TYPES)
        unset(${_v})
        unset(${_v} PARENT_SCOPE)
    endforeach()

    if(NOT _fragments)
        _nros_bounds_publish(NROS_MESSAGE_BOUNDS_REASON "no message-bound inventory was produced by this configure")
        _nros_message_bounds_write_output("${_B_OUTPUT_FILE}" "refused"
            "no message-bound inventory was produced by this configure" "${_ceiling}")
        return()
    endif()

    # ---- Compose ---------------------------------------------------------
    # `include()` inside a function keeps the fragment's `set()`s local to this
    # frame, which is what makes composing several packages safe: each fragment
    # APPENDs to the two lists and de-duplicates them itself.
    #
    # The schema version is checked PER FRAGMENT and BEFORE anything is read
    # from it. A mixed-version tree (one package regenerated, one not) is the
    # case that would otherwise read a moved field as if it had not moved.
    #
    # A MISSING fragment is a refusal, not a fatal: on the canonical lane
    # codegen is a build-time custom command, so on a clean tree the file is a
    # promise rather than a fact. A fragment that EXISTS and is malformed or
    # from another schema IS fatal -- that is a broken producer, not a lane
    # that has not run yet.
    set(_pending "")
    foreach(_frag IN LISTS _fragments)
        if(NOT EXISTS "${_frag}")
            list(APPEND _pending "${_frag}")
            continue()
        endif()
        unset(NROS_MESSAGE_BOUNDS_SCHEMA_VERSION)
        include("${_frag}")
        if(NOT DEFINED NROS_MESSAGE_BOUNDS_SCHEMA_VERSION)
            message(FATAL_ERROR
                "nros: ${_frag} sets no NROS_MESSAGE_BOUNDS_SCHEMA_VERSION.\n"
                "  Either it is not a message-bound fragment, or it predates "
                "the schema. Regenerate it with `nros codegen`.")
        endif()
        if(NOT NROS_MESSAGE_BOUNDS_SCHEMA_VERSION EQUAL
           NROS_MESSAGE_BOUNDS_SCHEMA_SUPPORTED)
            message(FATAL_ERROR
                "nros: ${_frag} states message-bound schema version "
                "${NROS_MESSAGE_BOUNDS_SCHEMA_VERSION}; this reader understands "
                "${NROS_MESSAGE_BOUNDS_SCHEMA_SUPPORTED}.\n"
                "  Refusing rather than reading fields that may have moved.\n"
                "  Rebuild the `nros` CLI so the producer and the reader come "
                "from one tree: `./scripts/bootstrap.sh` (contributors: "
                "`just setup-cli`).")
        endif()
    endforeach()

    if(_pending)
        list(LENGTH _pending _pending_count)
        list(LENGTH _fragments _frag_count)
        string(REPLACE ";" "\n    " _pending_block "${_pending}")
        set(_why
            "${_pending_count} of ${_frag_count} message-bound fragments have not been written yet:\n    ${_pending_block}")
        _nros_bounds_publish(NROS_MESSAGE_BOUNDS_REASON "${_why}")
        if(NOT _B_QUIET)
            message(STATUS
                "nros: message-bound sizing not available this configure -- "
                "${_pending_count} of ${_frag_count} fragments are still a "
                "build-time output. Every size knob keeps its configured value; "
                "the numbers apply from the next configure.")
        endif()
        _nros_message_bounds_write_output("${_B_OUTPUT_FILE}" "refused" "${_why}" "${_ceiling}")
        return()
    endif()

    set(_packages "${NROS_MESSAGE_BOUND_PACKAGES}")
    set(_types "${NROS_MESSAGE_BOUND_TYPES}")
    list(LENGTH _types _type_count)
    _nros_bounds_publish(NROS_MESSAGE_BOUNDS_PACKAGES "${_packages}")
    _nros_bounds_publish(NROS_MESSAGE_BOUNDS_TYPE_COUNT "${_type_count}")

    if(_type_count EQUAL 0)
        set(_why "the composed inventory holds no message types")
        _nros_bounds_publish(NROS_MESSAGE_BOUNDS_REASON "${_why}")
        _nros_message_bounds_write_output("${_B_OUTPUT_FILE}" "refused" "${_why}" "${_ceiling}")
        return()
    endif()

    # ---- Read every type, and refuse on the first open one ---------------
    set(_open "")
    set(_open_detail "")
    set(_bounded 0)
    set(_max_rx 0)
    set(_max_type "")
    set(_small 0)
    set(_large_types "")
    set(_large_max 0)
    foreach(_t IN LISTS _types)
        string(REGEX REPLACE "[^A-Za-z0-9]" "_" _key "${_t}")
        set(_state "${NROS_MESSAGE_BOUND_${_key}_STATE}")
        if(NOT _state STREQUAL "bounded")
            list(APPEND _open "${_t}")
            set(_reason "${NROS_MESSAGE_BOUND_${_key}_REASON}")
            if(NOT _reason)
                set(_reason "no reason recorded")
            endif()
            list(APPEND _open_detail "    ${_t} (${_state}): ${_reason}")
            continue()
        endif()
        set(_rx "${NROS_MESSAGE_BOUND_${_key}_RX}")
        if(NOT _rx MATCHES "^[0-9]+$")
            # `bounded` with no `_RX` cannot happen from a fragment this reader
            # accepts -- but a hand-edited or half-written one would, and a
            # non-numeric compared with LESS silently reads as 0.
            message(FATAL_ERROR
                "nros: ${_t} is `bounded` in the inventory and carries no "
                "numeric _RX (`${_rx}`). The fragment is malformed; regenerate "
                "it with `nros codegen`.")
        endif()
        math(EXPR _bounded "${_bounded} + 1")
        if(_rx GREATER _max_rx)
            set(_max_rx "${_rx}")
            set(_max_type "${_t}")
        endif()
        if(_rx GREATER _ceiling)
            list(APPEND _large_types "${_t}=${_rx}")
            if(_rx GREATER _large_max)
                set(_large_max "${_rx}")
            endif()
        elseif(_rx GREATER _small)
            set(_small "${_rx}")
        endif()
    endforeach()
    _nros_bounds_publish(NROS_MESSAGE_BOUNDS_BOUNDED_COUNT "${_bounded}")
    _nros_bounds_publish(NROS_MESSAGE_BOUNDS_OPEN_TYPES "${_open}")

    if(_open)
        list(LENGTH _open _open_count)
        string(REPLACE ";" "\n" _open_block "${_open_detail}")
        set(_why
            "${_open_count} of ${_type_count} types in the linked interface closure have no derived bound, so no class size can be trusted:\n${_open_block}")
        _nros_bounds_publish(NROS_MESSAGE_BOUNDS_REASON "${_why}")
        if(NOT _B_QUIET)
            message(WARNING
                "nros: message-bound sizing REFUSED -- every size knob keeps its "
                "configured value.\n"
                "  ${_open_count} of ${_type_count} types in the linked "
                "interface closure carry no bound:\n${_open_block}\n"
                "  Deriving a class size over only the bounded types would "
                "publish a maximum a real sample can exceed, which is a SILENT "
                "BufferTooSmall drop on the C/C++ arena dispatch path.\n"
                "  Remedy: bound the member in its `.msg` (`string<=64`), or cap "
                "it `inline` in the package's `nros-codegen.toml` -- `inline` is "
                "the only mode that bounds (RFC-0033). One cap on a DECLARING "
                "type is transitive: `\"std_msgs/Header.frame_id\" = { cap = 64, "
                "mode = \"inline\" }` bounds every message that nests a Header.")
        endif()
        _nros_message_bounds_write_output("${_B_OUTPUT_FILE}" "refused" "${_why}" "${_ceiling}")
        return()
    endif()

    # ---- Derive ----------------------------------------------------------
    #
    # Buffer 1, the runtime-owned take buffer: ONE global size for every
    # subscription in the image (`RX_BUF` is a const generic and the C/C++ path
    # is type-erased), so it must hold the largest type the image could
    # receive.
    _nros_bounds_publish(NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE "${_max_rx}")
    _nros_bounds_publish(NROS_DERIVED_LARGEST_TYPE "${_max_type}")
    _nros_bounds_publish(NROS_DERIVED_LARGEST_RX "${_max_rx}")

    # Buffer 2, the backend's staging pools: two classes, split at the policy
    # ceiling. `_small` is the largest bound AT OR UNDER the ceiling, so the
    # shim's own `min(threshold, SUBSCRIBER_BUFFER_SIZE)` picks it and the
    # classification here is the routing at runtime.
    #
    # `_small == 0` means no type fits under the ceiling. The small class is
    # still USED -- a caller that states no hint (`rx_buffer_hint == 0`, which
    # is still every C/C++ subscription until W3/W5) is served from it -- so
    # there is nothing to derive from the type set and the configured value
    # stands.
    if(_small GREATER 0)
        _nros_bounds_publish(NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE "${_small}")
    endif()

    list(LENGTH _large_types _large_count)
    _nros_bounds_publish(NROS_DERIVED_MAX_LARGE_SUBSCRIBERS "${_large_count}")
    _nros_bounds_publish(NROS_DERIVED_LARGE_TYPES "${_large_types}")
    if(_large_count GREATER 0)
        _nros_bounds_publish(NROS_DERIVED_SUBSCRIBER_LARGE_SIZE "${_large_max}")
    endif()
    # A count of ZERO is an ANSWER, not an abstention -- W4 made
    # `ZPICO_MAX_LARGE_SUBSCRIBERS = 0` legal precisely so an image whose types
    # all fit the small class can say so and stop reserving
    # RING_DEPTH x LARGE_SIZE for a class it never routes into. The large SIZE
    # is deliberately left underived in that case: with zero blocks the pool is
    # zero bytes whatever the size says, and naming a size for a class that does
    # not exist would be inventing a number.

    _nros_bounds_publish(NROS_MESSAGE_BOUNDS_STATUS "derived")
    _nros_bounds_publish(NROS_MESSAGE_BOUNDS_REASON "")

    if(NOT _B_QUIET)
        list(LENGTH _packages _pkg_count)
        message(STATUS
            "nros: message-bound sizing DERIVED from ${_type_count} types in "
            "${_pkg_count} interface packages (all bounded)")
        message(STATUS
            "nros:   largest type ${_max_type} at ${_max_rx} B -> "
            "NROS_SUBSCRIPTION_BUFFER_SIZE")
        if(_small GREATER 0)
            message(STATUS
                "nros:   small payload class ${_small} B -> "
                "NROS_SUBSCRIBER_BUFFER_SIZE")
        endif()
        message(STATUS
            "nros:   ${_large_count} types over the ${_ceiling} B ceiling -> "
            "NROS_MAX_LARGE_SUBSCRIBERS")
        if(_large_count GREATER 0)
            message(STATUS
                "nros:   large payload class ${_large_max} B -> "
                "NROS_SUBSCRIBER_LARGE_SIZE")
        endif()
        message(STATUS
            "nros:   these are UPPER BOUNDS -- the inventory holds the whole "
            "linked closure, not only the subscribed types")
    endif()

    _nros_message_bounds_write_output("${_B_OUTPUT_FILE}" "derived" "" "${_ceiling}")
endfunction()

# _nros_message_bounds_write_output(<path> <status> <reason> <ceiling>)
#
# The composed answer, as an `include()`able fragment AND as a readable record
# of where every number came from. Written only when the caller asked for a
# path.
#
# WRITE-IF-CHANGED, and that is load-bearing rather than tidy: a consumer
# registers this file with `CMAKE_CONFIGURE_DEPENDS`, so rewriting it with
# identical bytes on every configure would re-arm a re-configure forever.
function(_nros_message_bounds_write_output _path _status _reason _ceiling)
    if(NOT _path)
        return()
    endif()
    set(_c "# GENERATED by nros (phase-403 W8, issue 0940). Do not edit.\n")
    string(APPEND _c "#\n")
    string(APPEND _c "# The size knobs DERIVED from this image's message-bound inventory\n")
    string(APPEND _c "# (`nros_message_bounds.cmake`, one per generated interface package).\n")
    string(APPEND _c "#\n")
    string(APPEND _c "# Every number here is a DEFAULT. A board `.conf`, a Kconfig value or an\n")
    string(APPEND _c "# environment override states a number and WINS; this file only fills in\n")
    string(APPEND _c "# what nobody stated.\n")
    string(APPEND _c "#\n")
    string(APPEND _c "# Each is an UPPER BOUND on what the image needs: the inventory holds every\n")
    string(APPEND _c "# type in the LINKED interface closure, not only the subscribed ones. Too\n")
    string(APPEND _c "# big, never too small. Narrowing it needs an ENTITY inventory, which no\n")
    string(APPEND _c "# resolved SystemModel carries today (phase-403 W4).\n")
    string(APPEND _c "#\n")
    string(APPEND _c "# Derivation: nros_serdes::size::max_serialized_size, the same rule the\n")
    string(APPEND _c "# runtime's M::MAX_SERIALIZED_SIZE_XCDR* uses. NOT the C++ pack's\n")
    string(APPEND _c "# SERIALIZED_SIZE_MAX, which is an estimate (phase-403 W6).\n")
    string(APPEND _c "#\n")
    string(APPEND _c "# small/large class split: ${_ceiling} B (policy -- ZPICO_SUBSCRIBER_SIZE_THRESHOLD)\n")
    string(APPEND _c "\n")
    string(APPEND _c "set(NROS_MESSAGE_BOUNDS_STATUS \"${_status}\")\n")
    if(_status STREQUAL "refused")
        string(REPLACE "\\" "\\\\" _r "${_reason}")
        string(REPLACE "\"" "\\\"" _r "${_r}")
        string(REPLACE "\n" "\\n" _r "${_r}")
        string(APPEND _c "set(NROS_MESSAGE_BOUNDS_REASON \"${_r}\")\n")
        string(APPEND _c "# No knob is derived. Every one keeps its configured value.\n")
    else()
        string(APPEND _c "set(NROS_MESSAGE_BOUNDS_PACKAGES \"${NROS_MESSAGE_BOUNDS_PACKAGES}\")\n")
        string(APPEND _c "set(NROS_MESSAGE_BOUNDS_TYPE_COUNT ${NROS_MESSAGE_BOUNDS_TYPE_COUNT})\n")
        string(APPEND _c
            "# ${NROS_DERIVED_LARGEST_TYPE} is the largest type in the closure.\n")
        string(APPEND _c
            "set(NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE ${NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE})\n")
        if(DEFINED NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE)
            string(APPEND _c
                "# The largest type at or under the class split.\n")
            string(APPEND _c
                "set(NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE ${NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE})\n")
        else()
            string(APPEND _c
                "# No type fits under the ${_ceiling} B split, so the small class size is\n"
                "# not derivable from the type set -- but the class is still used, by any\n"
                "# caller that states no hint. NROS_SUBSCRIBER_BUFFER_SIZE keeps its\n"
                "# configured value.\n")
        endif()
        string(APPEND _c
            "# Types over the split: ${NROS_DERIVED_LARGE_TYPES}\n")
        string(APPEND _c
            "set(NROS_DERIVED_MAX_LARGE_SUBSCRIBERS ${NROS_DERIVED_MAX_LARGE_SUBSCRIBERS})\n")
        if(DEFINED NROS_DERIVED_SUBSCRIBER_LARGE_SIZE)
            string(APPEND _c
                "set(NROS_DERIVED_SUBSCRIBER_LARGE_SIZE ${NROS_DERIVED_SUBSCRIBER_LARGE_SIZE})\n")
        else()
            string(APPEND _c
                "# Zero large-class blocks, so the pool is zero bytes whatever size it\n"
                "# would name -- NROS_SUBSCRIBER_LARGE_SIZE is deliberately not derived.\n")
        endif()
    endif()
    set(_write TRUE)
    if(EXISTS "${_path}")
        file(READ "${_path}" _existing)
        if(_existing STREQUAL _c)
            set(_write FALSE)
        endif()
    endif()
    if(_write)
        get_filename_component(_dir "${_path}" DIRECTORY)
        file(MAKE_DIRECTORY "${_dir}")
        file(WRITE "${_path}" "${_c}")
    endif()
endfunction()

# -----------------------------------------------------------------------------
# `cmake -P` entry point.
#
# Running the derivation without configuring a project is what makes it
# TESTABLE, and it is how W6's prototype was measured in the first place. Under
# `cmake -P` the script mode is on and `CMAKE_ARGC`/`CMAKE_ARGV*` carry the
# arguments:
#
#   cmake -DNROS_BOUNDS_FRAGMENTS="a.cmake;b.cmake" \
#         [-DNROS_BOUNDS_CEILING=2048] [-DNROS_BOUNDS_OUTPUT=out.cmake] \
#         -P cmake/NanoRosMessageBounds.cmake
# -----------------------------------------------------------------------------
if(CMAKE_SCRIPT_MODE_FILE AND
   CMAKE_SCRIPT_MODE_FILE STREQUAL CMAKE_CURRENT_LIST_FILE)
    if(NOT DEFINED NROS_BOUNDS_FRAGMENTS)
        message(FATAL_ERROR
            "usage: cmake -DNROS_BOUNDS_FRAGMENTS=\"a.cmake;b.cmake\" "
            "[-DNROS_BOUNDS_CEILING=N] [-DNROS_BOUNDS_OUTPUT=path] "
            "-P cmake/NanoRosMessageBounds.cmake")
    endif()
    set(_args FRAGMENTS ${NROS_BOUNDS_FRAGMENTS})
    if(DEFINED NROS_BOUNDS_CEILING)
        list(APPEND _args SMALL_CLASS_CEILING "${NROS_BOUNDS_CEILING}")
    endif()
    if(DEFINED NROS_BOUNDS_OUTPUT)
        list(APPEND _args OUTPUT_FILE "${NROS_BOUNDS_OUTPUT}")
    endif()
    nros_derive_message_bound_knobs(${_args})
    message(STATUS "NROS_MESSAGE_BOUNDS_STATUS=${NROS_MESSAGE_BOUNDS_STATUS}")
    foreach(_v
        NROS_MESSAGE_BOUNDS_TYPE_COUNT
        NROS_MESSAGE_BOUNDS_BOUNDED_COUNT
        NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE
        NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE
        NROS_DERIVED_MAX_LARGE_SUBSCRIBERS
        NROS_DERIVED_SUBSCRIBER_LARGE_SIZE
        NROS_DERIVED_LARGEST_TYPE
        NROS_DERIVED_LARGE_TYPES)
        if(DEFINED ${_v})
            message(STATUS "${_v}=${${_v}}")
        endif()
    endforeach()
endif()
