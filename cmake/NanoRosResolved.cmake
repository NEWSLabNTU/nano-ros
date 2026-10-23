# NanoRosResolved.cmake -- phase-439 W2 (RFC-0094 D1/D2): the CONFIGURE-SIDE
# READER for `nros build`'s resolve phase.
#
# =============================================================================
# What stage 3.5 is, from this side
# =============================================================================
#
# `nros build` gains a phase between "is the toolchain present" (stage 3) and
# "emit a root build file" (stage 4). It reads DECLARATIONS -- the descriptors,
# the image block, and the wiring the launch tree states through its contract
# sidecar -- runs `EntityInventory::derive` once for the whole image, and writes
#
#   <NROS_RESOLVED_DIR>/resolved.toml    the answer, with [provenance]
#   <NROS_RESOLVED_DIR>/resolved.cmake   the same answer, include()able
#
# BEFORE any configure. `nros build` passes the directory as
# `-DNROS_RESOLVED_DIR=<dir>` on the cmake and west handoffs.
#
# =============================================================================
# Why a reader wants it: the producer-after-reader lag
# =============================================================================
#
# `${CMAKE_BINARY_DIR}/nros/entity_inventory.cmake` is written by
# `nano_ros_entry()`, which must run after every `nano_ros_node_register()` --
# and is READ by `nros_resolve_knobs()` inside `find_package(Zephyr)` and by
# `nros_find_interfaces()`, both of which run earlier. On a clean build dir the
# earliest reader therefore finds a PLACEHOLDER, and issue 0991's future-mtime
# arm exists to buy a second configure in which it finds the answer.
#
# A file written before the configure has no such lag. Seeding the fragment from
# it means the earliest reader of the FIRST pass sees a real number, which is
# the pass that arm was buying.
#
# =============================================================================
# It is a SEED, not an OVERRIDE -- and that is the safety property
# =============================================================================
#
# `nros_resolved_seed_entity_inventory` writes ONLY where
# `nros_entity_inventory_seed_knobs_file` would have written a placeholder: a
# fragment that does not exist yet. The mid-configure producer still runs, still
# composes over `nros-metadata.json` AND the model, and still arms a
# re-configure through `nros_reconfigure_on_change` if its answer differs.
#
# That ordering is deliberate and it is what makes this safe to land before the
# fixed point is deleted. The two composers do not have the same INPUTS:
#
#   * this phase reads the model alone, since `nros-metadata.json` is written
#     DURING a configure and reading it here would recreate the lag;
#   * the mid-configure producer also reads that metadata, whose component set
#     is what makes `derive()` REFUSE when a registered component is absent from
#     the launch declaration.
#
# So the seed can only be equal to, or more optimistic than, the producer's
# answer -- and when it is more optimistic the producer overwrites it and arms,
# exactly as it does today over a placeholder. Nothing here can make a build
# quieter than it is now.
#
# =============================================================================
# A lane that ran no resolve phase is UNCHANGED
# =============================================================================
#
# `NROS_RESOLVED_DIR` unset -- a bare `west build`, `just zephyr
# build-fixtures`, a user's own `cmake -S` -- means every function here is a
# no-op and the placeholder is written as before. That is the whole reason the
# seam is a passed variable rather than a search: a reader that GUESSED where a
# resolve might live would silently pick up another image's answer, which is
# issue 0616's shape one directory over.

include_guard(GLOBAL)

# `nros_resolved_entity_take` compares the board receive-cost arguments.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosBoardRxCost.cmake")

# nros_resolved_dir(<out_var>)
#
# The directory stage 3.5 wrote this image's answer into, or empty.
#
# Read from the cache/normal variable first and the ENVIRONMENT second, because
# the two carriers reach different lanes: `-D` reaches a configure `nros build`
# spawned directly, and the environment reaches one it spawned through a tool
# that owns its own command line. Never GUESSED from `CMAKE_BINARY_DIR` -- see
# the header.
function(nros_resolved_dir _out_var)
    if(DEFINED NROS_RESOLVED_DIR AND NOT "${NROS_RESOLVED_DIR}" STREQUAL "")
        set(${_out_var} "${NROS_RESOLVED_DIR}" PARENT_SCOPE)
        return()
    endif()
    if(DEFINED ENV{NROS_RESOLVED_DIR} AND NOT "$ENV{NROS_RESOLVED_DIR}" STREQUAL "")
        set(${_out_var} "$ENV{NROS_RESOLVED_DIR}" PARENT_SCOPE)
        return()
    endif()
    set(${_out_var} "" PARENT_SCOPE)
endfunction()

# nros_resolved_entity_fragment(<out_var>)
#
# The include()able projection stage 3.5 wrote, or empty when there is none.
#
# EMPTY rather than a path-that-does-not-exist, so a caller cannot accidentally
# `include()` a missing file and get cmake's own error about a name that means
# nothing to the reader. The file name is spelled ONCE, here, matching
# `nros_cli_core::resolve::RESOLVED_CMAKE_NAME`.
function(nros_resolved_entity_fragment _out_var)
    nros_resolved_dir(_dir)
    if("${_dir}" STREQUAL "")
        set(${_out_var} "" PARENT_SCOPE)
        return()
    endif()
    set(_frag "${_dir}/resolved.cmake")
    if(NOT EXISTS "${_frag}")
        # A resolve that REFUSED writes `resolved.toml` and no projection, on
        # purpose: an empty projection would publish "every count is zero",
        # which is a substituted default wearing an answer's clothes.
        set(${_out_var} "" PARENT_SCOPE)
        return()
    endif()
    set(${_out_var} "${_frag}" PARENT_SCOPE)
endfunction()

# nros_resolved_seed_entity_inventory(<path> [<out_used>])
#
# Seed the image's entity-inventory fragment from the resolve phase's answer,
# if there is one and the fragment does not exist yet.
#
# Call it IMMEDIATELY BEFORE `nros_entity_inventory_seed_knobs_file(<path>)`,
# which is a no-op on a file that now exists. Both are then correct in either
# order of arrival, and a lane with no resolve keeps today's placeholder.
#
# Call it from a READER only. The three call sites inside
# `nros_derive_entity_inventory_knobs()` are a PRODUCER recording its own
# refusal ("I could not compose here"), and seeding those with a different
# composer's answer would publish a number the producer did not stand behind.
# Two roles, two behaviours -- the sites are not siblings even though they call
# one function.
#
# Issue 1729 -- and RE-seed it when the resolve phase's answer has MOVED since
# the last seed. "Only where no fragment exists" assumed a mid-configure
# producer that recomposes the fragment on every configure. The generated Rust
# Zephyr entry (`rust_cargo_application()`, no `nano_ros_entry()`) has none:
# the seed IS its only producer. So the first configure's answer was the
# image's answer forever -- `resolved.cmake` was not even a configure
# dependency -- and `realtime-rust`'s derived-tier image kept a three-slot
# publisher pool after `nros build` had re-resolved four, failing
# `PublisherCreationFailed` at boot. Measured: the build dir configured at
# 09:34, the resolve rewritten at 09:56, ninja ran no configure.
#
# The record `<path>.resolved-seed` holds the content hash of the projection the
# fragment was last seeded from. A seed happens when the fragment is absent OR
# that hash moved; an unchanged resolve never re-seeds, so a road WITH a
# producer still converges (the producer overwrites, arms once, and the next
# configure finds the record current). The projection is a configure
# dependency, and `nros build` writes it write-if-changed, so an unchanged
# resolve costs no configure.
function(nros_resolved_seed_entity_inventory _path)
    nros_resolved_entity_fragment(_frag)
    _nros_resolved_seed("${_frag}" "${_path}" "entity inventory" _used)
    if(ARGC GREATER 1)
        set(${ARGV1} ${_used} PARENT_SCOPE)
    endif()
endfunction()

# nros_resolved_seed_message_bounds(<path>) — phase-439 W6 (issue 1252)
#
# The same seed for the image's MESSAGE-BOUND fragment
# (`nros/message_bound_knobs.cmake`), from `resolved-message-bounds.cmake`,
# which `nros build` writes beside `resolved.cmake` for a west image
# (`nros_cli_core::message_bound_knobs::write_west_seed`). Call it from the
# READER, before `nros_message_bounds_seed_knobs_file(<path>)` -- the same
# order and the same reason as the entity seed above. The producer
# (`nros_find_interfaces()`) still runs and still wins: a seed that differs
# from its answer arms the pass it would have armed anyway.
#
# No-op with no resolve phase, and when `nros build` declined to seed (it says
# why on its own output): the placeholder is then written as before.
function(nros_resolved_seed_message_bounds _path)
    nros_resolved_dir(_dir)
    if("${_dir}" STREQUAL "" OR NOT EXISTS "${_dir}/resolved-message-bounds.cmake")
        return()
    endif()
    _nros_resolved_seed("${_dir}/resolved-message-bounds.cmake" "${_path}"
        "message-bound sizes" _used)
    # phase-439 W8 -- and hold the producer to it (see the check below).
    if(EXISTS "${_dir}/resolved-message-bounds-inputs.cmake")
        get_property(_queued GLOBAL PROPERTY NROS_RESOLVED_BOUNDS_PROMISE_FRAG SET)
        set_property(GLOBAL PROPERTY NROS_RESOLVED_BOUNDS_PROMISE_FRAG "${_path}")
        if(NOT _queued)
            cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}"
                CALL _nros_resolved_bounds_promise_check)
        endif()
    endif()
endfunction()

# _nros_resolved_bounds_promise_check() -- phase-439 W8 (RFC-0094 A3, amended)
#
# A configure `nros build` started takes ONE pass for the message-bound sizes:
# its readers read the seed stage 3.5 composed, and the producer
# (`nros_derive_message_bound_knobs`, once per interface-declaring package)
# composes the same answer again. This holds the producer to it, at the end of
# the top-level configure -- after the LAST producer call, because an earlier
# call sees fewer tables and legitimately differs.
#
# Three outcomes, decided by what the readers consumed and by the inputs:
#
#   * readers did not read the seed, or the final fragment equals what they
#     read: nothing to say (the fixed point in NanoRosReconfigure.cmake covers
#     the first);
#   * the answer moved AND this configure's inputs are not the ones the seed was
#     composed over (a table regenerated, another entity inventory, another
#     ceiling -- a tree edited since `nros build` resolved it): a resolve older
#     than the tree. The producer's arm buys the pass, as it always has, and
#     this says why the configure was not single-pass;
#   * the answer moved over the SAME inputs: the two composers of one answer
#     disagree. That is a bug in one of them (the parity case of
#     `tests/cmake-message-bounds-tests.sh` is what should have caught it), and
#     a second pass would only build with whichever answer came last -- so the
#     configure stops, naming both files.
#
# Runs one hop late: the first call re-queues itself, so it follows every
# deferred call queued before it (an entry's standalone flushes among them).
function(_nros_resolved_bounds_promise_check)
    get_property(_hop GLOBAL PROPERTY NROS_RESOLVED_BOUNDS_PROMISE_HOP)
    if(NOT _hop)
        set_property(GLOBAL PROPERTY NROS_RESOLVED_BOUNDS_PROMISE_HOP TRUE)
        cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}"
            CALL _nros_resolved_bounds_promise_check)
        return()
    endif()
    if(NOT COMMAND _nros_reconfigure_pass_props OR NOT COMMAND nros_message_bounds_fragments
       OR NOT COMMAND nros_entity_inventory_knobs_file)
        return()
    endif()
    get_property(_frag GLOBAL PROPERTY NROS_RESOLVED_BOUNDS_PROMISE_FRAG)
    nros_resolved_dir(_dir)
    set(_seed "${_dir}/resolved-message-bounds.cmake")
    set(_rec "${_dir}/resolved-message-bounds-inputs.cmake")
    if("${_frag}" STREQUAL "" OR NOT EXISTS "${_seed}" OR NOT EXISTS "${_rec}"
       OR NOT EXISTS "${_frag}")
        return()
    endif()
    _nros_reconfigure_pass_props("${_frag}" _basis_prop _armed_prop)
    get_property(_have GLOBAL PROPERTY ${_basis_prop} SET)
    if(NOT _have)
        return()
    endif()
    get_property(_basis GLOBAL PROPERTY ${_basis_prop})
    file(SHA256 "${_seed}" _seed_sha)
    file(SHA256 "${_frag}" _final_sha)
    if(NOT _basis STREQUAL _seed_sha OR _final_sha STREQUAL _basis)
        return()
    endif()

    unset(NROS_RESOLVED_BOUNDS_INPUTS_SCHEMA)
    include("${_rec}")
    _nros_resolved_bounds_inputs(_tables _entity_sha)
    set(_why "")
    if(NOT "${NROS_RESOLVED_BOUNDS_INPUTS_SCHEMA}" STREQUAL "1")
        set(_why "its input record has a schema this reader does not know")
    elseif(NOT "${_tables}" STREQUAL "${NROS_RESOLVED_BOUNDS_TABLES}")
        set(_why "its bound tables are [${NROS_RESOLVED_BOUNDS_TABLES}], this configure's [${_tables}]")
    elseif(NOT "${_entity_sha}" STREQUAL "${NROS_RESOLVED_BOUNDS_ENTITY_SHA256}")
        set(_why "this configure's entity inventory is not the one it was composed with")
    elseif(NOT "${NROS_MESSAGE_BOUNDS_DEFAULT_SMALL_CEILING}" STREQUAL
           "${NROS_RESOLVED_BOUNDS_CEILING}")
        set(_why "its small-class ceiling is ${NROS_RESOLVED_BOUNDS_CEILING}, this "
                 "configure's ${NROS_MESSAGE_BOUNDS_DEFAULT_SMALL_CEILING}")
        string(CONCAT _why ${_why})
    endif()
    if(NOT _why STREQUAL "")
        message(STATUS
            "nros: message-bound sizes: the resolve phase's seed (${_seed}) was "
            "composed over other inputs -- ${_why}. This configure converges by "
            "one more pass; re-run `nros build` to keep it to one.")
        return()
    endif()
    message(FATAL_ERROR
        "nros: this image's message-bound sizes differ from the answer `nros build` "
        "resolved before this configure, over the SAME inputs.
"
        "  seed:      ${_seed}
"
        "  configure: ${_frag}
"
        "  The two composers of one answer disagree, which is a bug in one of them, "
        "not a lag to converge: a configure `nros build` started takes one pass "
        "(RFC-0094 A3, phase-439 W8), and a second pass would build with whichever "
        "answer came last. `diff` the two files and report it.")
endfunction()

# _nros_resolved_bounds_inputs(<out_tables> <out_entity_sha>)
#
# This configure's half of the record `write_west_seed` writes: `<pkg>=<sha256>`
# of each registered bound table's JSON (its fragment's sibling), sorted by
# package, and the entity fragment's digest. A package registered with two
# DIFFERENT tables is reported as both rows, which matches no record.
function(_nros_resolved_bounds_inputs _out_tables _out_entity)
    nros_message_bounds_fragments(_frags)
    set(_rows "")
    foreach(_f IN LISTS _frags)
        get_filename_component(_d "${_f}" DIRECTORY)
        get_filename_component(_pkg "${_d}" NAME)
        set(_json "${_d}/nros_message_bounds.json")
        if(EXISTS "${_json}")
            file(SHA256 "${_json}" _h)
        else()
            set(_h "missing")
        endif()
        list(APPEND _rows "${_pkg}=${_h}")
    endforeach()
    if(_rows)
        list(REMOVE_DUPLICATES _rows)
        list(SORT _rows)
    endif()
    set(${_out_tables} "${_rows}" PARENT_SCOPE)
    nros_entity_inventory_knobs_file(_e)
    set(_esha "")
    if(EXISTS "${_e}")
        file(SHA256 "${_e}" _esha)
    endif()
    set(${_out_entity} "${_esha}" PARENT_SCOPE)
endfunction()

# _nros_resolved_seed(<projection> <path> <what> <out_used>)
#
# The one copy both seeds make: VERBATIM, re-seeded only when the projection's
# content moved (issue 1729's record), and the projection made a configure
# dependency. The rationale for each half is the comment below.
function(_nros_resolved_seed _frag _path _what _out_used)
    set(${_out_used} FALSE PARENT_SCOPE)
    if("${_frag}" STREQUAL "")
        return()
    endif()
    set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_frag}")
    file(SHA256 "${_frag}" _seed_hash)
    set(_record "${_path}.resolved-seed")
    if(EXISTS "${_path}" AND EXISTS "${_record}")
        file(READ "${_record}" _last_hash)
        string(STRIP "${_last_hash}" _last_hash)
        if(_last_hash STREQUAL _seed_hash)
            return()
        endif()
    endif()
    # Absent fragment, a moved resolve, or a fragment with no record (one this
    # rule predates, which is exactly the build dir issue 1729 measured). On a
    # road WITH a producer the producer still runs after this, overwrites, and
    # arms one re-configure if it disagrees -- case C -- and the record then
    # keeps it from being re-seeded.
    get_filename_component(_dir "${_path}" DIRECTORY)
    file(MAKE_DIRECTORY "${_dir}")
    # VERBATIM. Not a copy with a "seeded from ..." banner on top, which is what
    # this did first and which MEASURED as buying nothing: `nros_reconfigure_
    # snapshot` hashes CONTENT, so a seed that differs from the producer's
    # answer by one comment line arms exactly the re-configure the seed exists
    # to remove. Case B of `tests/cmake-resolved-seed-tests.sh` is that
    # measurement, and it reported `re-runs=1` until the banner came off.
    #
    # The same rule cost the pass a SECOND time, from INSIDE the fragment
    # rather than from this copy (issue 1228, measured on `demo_bringup:zephyr`
    # / native_sim/native/64): `to_cmake` used to set
    # `NROS_ENTITY_INVENTORY_SOURCE` and to print the per-component provenance
    # as `<pkg>::<component>`, and both depend on WHICH COMPOSER ran rather
    # than on the image -- the model names nodes (`/talker`), the merge names
    # ament packages (`talker_pkg`). Every NUMBER agreed and those two
    # renderings were the entire byte difference, so the producer re-armed over
    # provenance every time. Both are out of the fragment now.
    #
    # The provenance is not lost, it is in the places a byte comparison cannot
    # reach: the STATUS line below, `nros/entity_inventory.json`, and
    # `resolved.toml`'s `[provenance]` table beside the file this reads.
    file(READ "${_frag}" _body)
    file(WRITE "${_path}" "${_body}")
    file(WRITE "${_record}" "${_seed_hash}\n")
    set(${_out_used} TRUE PARENT_SCOPE)
    message(STATUS
        "nros: ${_what} seeded from the resolve phase (${_frag}); this "
        "configure's first reader sees a derived answer rather than a placeholder")
endfunction()

# nros_resolved_entity_take(<metadata> <output> <json> <out_taken>) -- phase-439 W7
#
# The mid-configure entity producer READS the resolve instead of recomposing,
# when the resolve was composed over this configure's own inputs (issue 1228).
#
# `nros build` composes a C/C++ west image at stage 3.5 by the same function
# `nros ws entity-inventory` runs (`cmd::entity_inventory::compose_image`), over
# the population the image's packages register, and records what it composed
# over in `resolved-entity-inputs.cmake`. Equal inputs into one function are
# equal outputs, so on a match this copies `resolved.cmake` to `<output>` and
# `resolved-entity-inventory.json` to `<json>` and the caller runs no CLI: one
# composer, and nothing for the reconfigure arm to find.
#
# The inputs compared are every one the producer passes the verb:
#
#   registered  `<pkg>::<component>::<class>` per row of <metadata>, the rows
#               THIS configure's `nano_ros_node_register()` calls wrote. The
#               resolve read them from `nros sync`'s metadata probe; a component
#               added since the last sync differs here.
#   model       the one model this configure accumulated (issue 1600's list),
#               by path AND content.
#               Several models are a shared runtime, which the resolve -- one
#               image -- does not compose.
#   workspace   the colcon workspace whose probe sidecars floor the contract.
#   rx          the board's `--board-rx-*` arguments.
#
# Any difference, an absent or unreadable record, or a <metadata> row carrying
# the retired `entities` key (the verb refuses it, and only the verb says so)
# leaves `<out_taken>` FALSE and says why once: the caller composes exactly as
# it did before this function existed. A road with no record -- every
# configure `nros build` did not start, and every non-west image -- never
# reaches the comparison.
function(nros_resolved_entity_take _metadata _output _json _out_taken)
    set(${_out_taken} FALSE PARENT_SCOPE)
    nros_resolved_entity_fragment(_frag)
    if("${_frag}" STREQUAL "")
        return()
    endif()
    nros_resolved_dir(_dir)
    set(_rec "${_dir}/resolved-entity-inputs.cmake")
    set(_rjson "${_dir}/resolved-entity-inventory.json")
    if(NOT EXISTS "${_rec}" OR NOT EXISTS "${_rjson}")
        return()
    endif()
    set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_rec}")

    foreach(_v NROS_RESOLVED_ENTITY_INPUTS_SCHEMA NROS_RESOLVED_ENTITY_REGISTERED
               NROS_RESOLVED_ENTITY_MODEL NROS_RESOLVED_ENTITY_MODEL_SHA256
               NROS_RESOLVED_ENTITY_WORKSPACE
               NROS_RESOLVED_ENTITY_RX_ARGS)
        unset(${_v})
    endforeach()
    include("${_rec}")
    set(_why "")
    if(NOT "${NROS_RESOLVED_ENTITY_INPUTS_SCHEMA}" STREQUAL "1")
        set(_why "its input record (${_rec}) has a schema this reader does not know")
    endif()

    if(_why STREQUAL "")
        _nros_resolved_registered_keys("${_metadata}" _keys _why)
        if(_why STREQUAL "" AND NOT "${_keys}" STREQUAL "${NROS_RESOLVED_ENTITY_REGISTERED}")
            set(_why "it registered [${NROS_RESOLVED_ENTITY_REGISTERED}] and this "
                     "configure registers [${_keys}] -- re-run `nros sync`")
            string(CONCAT _why ${_why})
        endif()
    endif()

    if(_why STREQUAL "")
        get_property(_models GLOBAL PROPERTY NROS_ENTITY_INVENTORY_MODELS)
        set(_existing "")
        foreach(_m IN LISTS _models)
            if(EXISTS "${_m}")
                get_filename_component(_m "${_m}" REALPATH)
                list(APPEND _existing "${_m}")
            endif()
        endforeach()
        if(_existing)
            list(REMOVE_DUPLICATES _existing)
        endif()
        list(LENGTH _existing _n_models)
        if(NOT _n_models EQUAL 1)
            set(_why "this configure composes over ${_n_models} models, the resolve over one")
        else()
            get_filename_component(_want "${NROS_RESOLVED_ENTITY_MODEL}" REALPATH)
            if(NOT "${_existing}" STREQUAL "${_want}")
                set(_why "it was composed over ${_want}, this configure over ${_existing}")
            else()
                # The CONTENT, not the path: a configure `nros build` did not
                # start re-resolves the model in place, and the resolve then
                # describes the model it replaced -- re-run `nros build`.
                file(SHA256 "${_existing}" _model_sha)
                if(NOT "${_model_sha}" STREQUAL "${NROS_RESOLVED_ENTITY_MODEL_SHA256}")
                    set(_why "the model ${_existing} changed since `nros build` resolved it")
                endif()
            endif()
        endif()
    endif()

    if(_why STREQUAL "")
        get_property(_ws GLOBAL PROPERTY NROS_ENTITY_INVENTORY_WORKSPACE)
        set(_ws_real "")
        if(NOT "${_ws}" STREQUAL "")
            get_filename_component(_ws_real "${_ws}" REALPATH)
        endif()
        get_filename_component(_ws_want "${NROS_RESOLVED_ENTITY_WORKSPACE}" REALPATH)
        if(NOT "${_ws_real}" STREQUAL "${_ws_want}")
            set(_why "its workspace is ${_ws_want}, this configure's is `${_ws_real}`")
        endif()
    endif()

    if(_why STREQUAL "")
        _nros_board_rx_cost_args(_rx)
        if(NOT "${_rx}" STREQUAL "${NROS_RESOLVED_ENTITY_RX_ARGS}")
            set(_why "its board receive cost is [${NROS_RESOLVED_ENTITY_RX_ARGS}], "
                     "this configure's [${_rx}]")
            string(CONCAT _why ${_why})
        endif()
    endif()

    if(NOT _why STREQUAL "")
        message(STATUS "nros: entity inventory composed at configure, not read from the "
                       "resolve phase: ${_why}")
        return()
    endif()

    # COPYONLY is write-if-different, so an unchanged answer moves no mtime and
    # the copy is a configure dependency on its source -- both what the verb's
    # own write-if-changed gave the reader.
    configure_file("${_frag}" "${_output}" COPYONLY)
    configure_file("${_rjson}" "${_json}" COPYONLY)
    set(${_out_taken} TRUE PARENT_SCOPE)
    message(STATUS "nros: entity inventory read from the resolve phase (${_frag}); "
                   "this configure composed nothing")
endfunction()

# _nros_resolved_registered_keys(<metadata> <out_keys> <out_why>)
#
# `<pkg>::<component>::<class>` for every row of a `nros-metadata.json`,
# deduplicated on `(pkg, component)` with the LAST row winning and sorted --
# `EntityInventory::insert`'s rule and `resolve::EntityInputs::new`'s order. The
# package falls back to the class's leading `::` segment, as
# `ComponentMeta::pkg_or_class_prefix` does.
function(_nros_resolved_registered_keys _metadata _out_keys _out_why)
    set(${_out_keys} "" PARENT_SCOPE)
    set(${_out_why} "" PARENT_SCOPE)
    if(NOT EXISTS "${_metadata}")
        set(${_out_why} "this configure has no ${_metadata}" PARENT_SCOPE)
        return()
    endif()
    file(READ "${_metadata}" _doc)
    string(JSON _n ERROR_VARIABLE _err LENGTH "${_doc}" components)
    if(_err)
        set(_n 0)
    endif()
    set(_ids "")
    if(_n GREATER 0)
        math(EXPR _last "${_n} - 1")
        foreach(_i RANGE ${_last})
            string(JSON _name ERROR_VARIABLE _e1 GET "${_doc}" components ${_i} name)
            string(JSON _class ERROR_VARIABLE _e2 GET "${_doc}" components ${_i} class)
            if(_e1 OR _e2)
                set(${_out_why} "${_metadata} row ${_i} has no name or class" PARENT_SCOPE)
                return()
            endif()
            string(JSON _t ERROR_VARIABLE _e3 TYPE "${_doc}" components ${_i} entities)
            if(NOT _e3)
                set(${_out_why} "${_metadata} row ${_i} carries the retired `entities` key"
                    PARENT_SCOPE)
                return()
            endif()
            string(JSON _t ERROR_VARIABLE _e4 TYPE "${_doc}" components ${_i} pkg)
            if(_e4 OR _t STREQUAL "NULL")
                string(FIND "${_class}" "::" _at)
                if(_at EQUAL -1)
                    set(_pkg "${_class}")
                else()
                    string(SUBSTRING "${_class}" 0 ${_at} _pkg)
                endif()
            else()
                string(JSON _pkg GET "${_doc}" components ${_i} pkg)
            endif()
            string(MD5 _id "${_pkg}::${_name}")
            set(_row_${_id} "${_pkg}::${_name}::${_class}")
            list(APPEND _ids "${_id}")
        endforeach()
    endif()
    set(_keys "")
    if(_ids)
        list(REMOVE_DUPLICATES _ids)
        foreach(_id IN LISTS _ids)
            list(APPEND _keys "${_row_${_id}}")
        endforeach()
        list(SORT _keys)
    endif()
    set(${_out_keys} "${_keys}" PARENT_SCOPE)
endfunction()
