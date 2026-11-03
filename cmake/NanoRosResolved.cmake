# NanoRosResolved.cmake -- phase-439 W2 (RFC-0094 D1/D2), phase-530 W4
# (RFC-0120 D2/D5): the HOST-CONFIGURE-SIDE READER for `nros configure`'s
# resolve phase.
#
# =============================================================================
# What the resolve phase is, from this side
# =============================================================================
#
# `nros configure` (C5, once `nros build`'s stage 3.5) reads DECLARATIONS --
# the descriptors, the image block, and the wiring the launch tree states
# through its contract sidecar -- runs `EntityInventory::derive` once for the
# whole image, and writes, BEFORE any host configure,
#
#   <resolve dir>/resolved.toml    the answer, with [provenance]
#   <resolve dir>/resolved.cmake   the same answer, include()able
#
# (plus the message-bound seed and the input records the producers compare).
#
# =============================================================================
# Where the answer is: located by IMAGE, never passed as a path
# =============================================================================
#
# phase-530 W4 (RFC-0120 D2) retired `-DNROS_RESOLVED_DIR`. The resolve phase
# is part of `nros configure` now, and configure records, in the image's stamp
# (`<project>/build/nros/configured/<image>/stamp.toml`), the directory it
# resolved into. A reader asks for it by the image's IDENTITY:
#
#   * the Zephyr module hook (`zephyr/cmake/nros_image_kconfig.cmake`) runs
#     `nros configure` for the image this host configure builds, asks
#     `nros locate configured` where its answer is, and publishes the GLOBAL
#     property `NROS_CONFIGURED_RESOLVED_DIR` for this configure only;
#   * a cmake road states the identity (`NROS_CONFIGURED_PROJECT` +
#     `NROS_CONFIGURED_IMAGE`, which `nros build` passes) and this file asks
#     `nros locate configured` itself, once per configure.
#
# Never GUESSED from `CMAKE_BINARY_DIR`: a reader that guessed where a resolve
# might live would silently pick up another image's answer (issue 0616's
# shape). And never a cached PATH: the old `-D` outlived the `nros build` that
# passed it, so a later reconfigure read a resolve nobody had refreshed.
#
# =============================================================================
# It is a SEED, held to by a PROMISE (RFC-0120 D5)
# =============================================================================
#
# The earliest readers in a configure (`nros_resolve_knobs()` inside
# `find_package(Zephyr)`, the payload-class join in `nros_find_interfaces()`)
# run BEFORE the producers that compose the same answer from what the host
# configure declared. The seed is copied in ahead of the first reader, so no
# reader ever sees a placeholder. The producers still compose, and
# `cmake/NanoRosPromise.cmake` holds their final answer to the seed: equal
# continues, different fails the configure naming both files -- or naming the
# declaration configure could not see. Nothing re-converges.
#
# A configure with no `nros configure` in front of it (a plain `cmake -S`,
# until phase-530 W5's cmake hook) locates nothing, writes the placeholder as
# before, and still converges through the old lever, inside the promise module.

include_guard(GLOBAL)

# `nros_resolved_entity_take` compares the board receive-cost arguments.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosBoardRxCost.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosPromise.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCli.cmake")

# nros_configured_locate(<project> <image>)
#
# Ask `nros locate configured` where configure put <image>'s answer, and
# publish it for this configure as the GLOBAL property
# `NROS_CONFIGURED_RESOLVED_DIR` (empty when configure resolved none). The one
# call both the Zephyr hook and a cmake road make.
#
# Only a CURRENT stamp's answer is published (Ruling R33). A cmake road keeps
# the identity `nros build` passed in its cache, so an edit to the project
# followed by `cmake --build` re-runs this configure with no `nros configure`
# in front of it: the stamp is stale, and a seed composed before the edit must
# not be promised. That configure takes the unseeded road and says what to
# run (phase-530 W5's cmake hook runs it first).
function(nros_configured_locate _project _image)
    set_property(GLOBAL PROPERTY NROS_CONFIGURED_COMMAND
        "nros configure ${_project} --image ${_image}")
    nros_resolve_cli(_nros OPTIONAL CONTEXT "locating ${_image}'s configured answer")
    if(NOT _nros OR NOT EXISTS "${_nros}")
        set_property(GLOBAL PROPERTY NROS_CONFIGURED_RESOLVED_DIR "")
        return()
    endif()
    execute_process(
        COMMAND "${_nros}" locate configured --project "${_project}" --image "${_image}"
                --format cmake
        OUTPUT_VARIABLE _out ERROR_VARIABLE _err RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)
    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR
            "nano-ros: `nros locate configured --project ${_project} --image ${_image}` "
            "failed:\n${_err}")
    endif()
    set(_dir "")
    if(_out MATCHES "set\\(NROS_LOCATE_RESOLVED \"([^\"]*)\"\\)")
        set(_dir "${CMAKE_MATCH_1}")
    endif()
    if(NOT _out MATCHES "set\\(NROS_LOCATE_CURRENT TRUE\\)")
        set(_why "")
        if(_out MATCHES "set\\(NROS_LOCATE_STALE \"([^\"]*)\"\\)")
            set(_why " (${CMAKE_MATCH_1})")
        endif()
        message(STATUS
            "nano-ros: image `${_image}`'s configure is stale${_why}: this configure "
            "reads no seed from it and composes its own answer; run "
            "`nros configure ${_project} --image ${_image}` (or `nros build`) to seed "
            "it again (RFC-0120 D5)")
        set(_dir "")
    endif()
    nros_configured_publish("${_dir}")
endfunction()

# nros_configured_publish(<dir>)
#
# Publish <dir> (possibly empty) as where this configure's image's answer is,
# for every reader of THIS configure. `nros_configured_locate` is its caller;
# a test stands in for the CLI through it.
function(nros_configured_publish _dir)
    set_property(GLOBAL PROPERTY NROS_CONFIGURED_RESOLVED_DIR "${_dir}")
    set_property(GLOBAL PROPERTY NROS_CONFIGURED_LOCATED TRUE)
endfunction()

# nros_resolved_dir(<out_var>)
#
# The directory `nros configure` resolved this image's answer into, or empty.
# See the header: located by identity, once per configure.
function(nros_resolved_dir _out_var)
    get_property(_located GLOBAL PROPERTY NROS_CONFIGURED_LOCATED)
    if(NOT _located
       AND DEFINED NROS_CONFIGURED_PROJECT AND NOT "${NROS_CONFIGURED_PROJECT}" STREQUAL ""
       AND DEFINED NROS_CONFIGURED_IMAGE AND NOT "${NROS_CONFIGURED_IMAGE}" STREQUAL "")
        nros_configured_locate("${NROS_CONFIGURED_PROJECT}" "${NROS_CONFIGURED_IMAGE}")
    endif()
    get_property(_dir GLOBAL PROPERTY NROS_CONFIGURED_RESOLVED_DIR)
    set(${_out_var} "${_dir}" PARENT_SCOPE)
endfunction()

# nros_resolved_entity_fragment(<out_var>)
#
# The include()able projection configure wrote, or empty when there is none.
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
# Seed the image's entity-inventory fragment from configure's answer, if there
# is one. Call it from a READER, before `nros_entity_inventory_seed_knobs_file`
# (a no-op on a file that now exists) and before `nros_promise_read`.
#
# Call it from a READER only. The three call sites inside
# `nros_derive_entity_inventory_knobs()` are a PRODUCER recording its own
# refusal ("I could not compose here"), and seeding those with a different
# composer's answer would publish a number the producer did not stand behind.
#
# Issue 1729 -- for the generated Rust Zephyr entry the seed is the fragment's
# ONLY producer, so a resolve that moved must reach it: the fragment is made
# equal to the projection on every configure (write-if-different), and the
# projection is a configure dependency.
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
# which configure writes beside `resolved.cmake`
# (`nros_cli_core::message_bound_knobs::write_west_seed`). Call it from the
# READER, before `nros_message_bounds_seed_knobs_file(<path>)`. The producer
# (`nros_find_interfaces()`) still runs, and its final answer is held to this
# one (RFC-0120 D5).
#
# No-op with no configure in front, and when configure declined to seed (it
# says why on its own output): the placeholder is then written as before.
function(nros_resolved_seed_message_bounds _path)
    nros_resolved_dir(_dir)
    if("${_dir}" STREQUAL "" OR NOT EXISTS "${_dir}/resolved-message-bounds.cmake")
        return()
    endif()
    _nros_resolved_seed("${_dir}/resolved-message-bounds.cmake" "${_path}"
        "message-bound sizes" _used)
    # The producer's account of WHY it moved, from the input record configure
    # wrote beside the seed. Queued before the reader queues the promise check,
    # so it runs first (cmake's deferred calls are FIFO).
    if(EXISTS "${_dir}/resolved-message-bounds-inputs.cmake")
        get_property(_queued GLOBAL PROPERTY NROS_RESOLVED_BOUNDS_PROMISE_FRAG SET)
        set_property(GLOBAL PROPERTY NROS_RESOLVED_BOUNDS_PROMISE_FRAG "${_path}")
        if(NOT _queued)
            cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}"
                CALL _nros_resolved_bounds_why)
        endif()
    endif()
endfunction()

# _nros_resolved_bounds_why() -- phase-439 W8, phase-530 W4
#
# When this configure's final message-bound answer moved from the seed, say
# which INPUT moved, by comparing this configure's inputs with the record
# configure composed the seed over: its bound tables (a `find_package(<msg>)`
# configure did not see adds one), the entity fragment the last composition
# read, and the small-class ceiling. Equal inputs leave no account, and the
# promise check then names the two composers' disagreement as the bug it is.
#
# One hop late, like the promise check it feeds.
function(_nros_resolved_bounds_why)
    get_property(_hop GLOBAL PROPERTY NROS_RESOLVED_BOUNDS_PROMISE_HOP)
    if(NOT _hop)
        set_property(GLOBAL PROPERTY NROS_RESOLVED_BOUNDS_PROMISE_HOP TRUE)
        cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}"
            CALL _nros_resolved_bounds_why)
        return()
    endif()
    if(NOT COMMAND nros_message_bounds_fragments
       OR NOT COMMAND nros_entity_inventory_knobs_file)
        return()
    endif()
    get_property(_frag GLOBAL PROPERTY NROS_RESOLVED_BOUNDS_PROMISE_FRAG)
    nros_resolved_dir(_dir)
    set(_rec "${_dir}/resolved-message-bounds-inputs.cmake")
    if("${_frag}" STREQUAL "" OR NOT EXISTS "${_rec}")
        return()
    endif()
    unset(NROS_RESOLVED_BOUNDS_INPUTS_SCHEMA)
    include("${_rec}")
    _nros_resolved_bounds_inputs(_tables _entity_sha)
    get_property(_ceiling GLOBAL PROPERTY NROS_MESSAGE_BOUNDS_COMPOSED_CEILING)
    set(_why "")
    if(NOT "${NROS_RESOLVED_BOUNDS_INPUTS_SCHEMA}" STREQUAL "1")
        set(_why "its input record (${_rec}) has a schema this reader does not know")
    elseif(NOT "${_tables}" STREQUAL "${NROS_RESOLVED_BOUNDS_TABLES}")
        # CONCAT of QUOTED arguments: the table lists keep their `;`, which
        # an unquoted `${_why}` re-split would not promise.
        string(CONCAT _why
            "configure composed over the bound tables [${NROS_RESOLVED_BOUNDS_TABLES}], "
            "this configure registers [${_tables}] -- an interface package the "
            "host configure finds that `package.xml` does not name")
    elseif(NOT "${_entity_sha}" STREQUAL "${NROS_RESOLVED_BOUNDS_ENTITY_SHA256}")
        string(CONCAT _why
            "this configure's entity inventory is not the one configure composed "
            "the sizes with -- a registration configure did not count")
    elseif(NOT "${_ceiling}" STREQUAL "${NROS_RESOLVED_BOUNDS_CEILING}")
        string(CONCAT _why
            "configure's small-class ceiling is ${NROS_RESOLVED_BOUNDS_CEILING}, this "
            "configure's ${_ceiling}")
    endif()
    if(NOT _why STREQUAL "")
        nros_promise_why("${_frag}" "${_why}")
    endif()
endfunction()

# _nros_resolved_bounds_inputs(<out_tables> <out_entity_sha>)
#
# This configure's half of the record `write_west_seed` writes: `<pkg>=<sha256>`
# of each registered bound table's JSON (its fragment's sibling), sorted by
# package, and the digest of the entity fragment the last composition read. A
# package registered with two DIFFERENT tables is reported as both rows, which
# matches no record.
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
    # The entity fragment the LAST bounds composition read, never the fragment
    # as it stands now: the entity producer may rewrite it afterwards.
    get_property(_esha GLOBAL PROPERTY NROS_MESSAGE_BOUNDS_COMPOSED_ENTITY_SHA256)
    set(${_out_entity} "${_esha}" PARENT_SCOPE)
endfunction()

# _nros_resolved_seed(<projection> <path> <what> <out_used>)
#
# The one copy both seeds make: VERBATIM, whenever the fragment's content is
# not the projection's (write-if-different, so an unchanged answer moves no
# mtime), the projection made a configure dependency, and the fragment
# recorded as SEEDED for the promise check (RFC-0120 D5). <out_used> is TRUE
# whenever there is a projection, copied or already equal.
#
# phase-530 W4 -- re-seeded on CONTENT, not on a record of the last seed
# (issue 1729's `<path>.resolved-seed`): with the fixed point gone, a producer
# that moved the fragment fails the configure rather than converging, and the
# fragment it left behind must not be what the next configure's readers see.
function(_nros_resolved_seed _frag _path _what _out_used)
    set(${_out_used} FALSE PARENT_SCOPE)
    if("${_frag}" STREQUAL "")
        return()
    endif()
    set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_frag}")
    set(${_out_used} TRUE PARENT_SCOPE)
    # ONCE per configure: a reader called once per package (the payload-class
    # join in `nros_find_interfaces()`) must not put the seed back over what a
    # producer wrote in between -- only the FINAL answer is held to it.
    nros_promise_is_seeded("${_path}" _already)
    if(_already)
        return()
    endif()
    nros_promise_seeded("${_path}" "${_frag}")
    file(SHA256 "${_frag}" _seed_hash)
    if(EXISTS "${_path}")
        file(SHA256 "${_path}" _have_hash)
        if(_have_hash STREQUAL _seed_hash)
            return()
        endif()
    endif()
    get_filename_component(_dir "${_path}" DIRECTORY)
    file(MAKE_DIRECTORY "${_dir}")
    # VERBATIM. Not a copy with a "seeded from ..." banner on top, which is what
    # this did first and which MEASURED as buying nothing: the comparison is of
    # CONTENT, so a seed that differs from the producer's answer by one comment
    # line is a different answer (case B of `tests/cmake-resolved-seed-tests.sh`).
    #
    # The same rule cost a pass a SECOND time, from INSIDE the fragment rather
    # than from this copy (issue 1228): `to_cmake` used to set
    # `NROS_ENTITY_INVENTORY_SOURCE` and to print the per-component provenance
    # as `<pkg>::<component>`, and both depend on WHICH COMPOSER ran rather
    # than on the image. Both are out of the fragment now; the provenance is in
    # the STATUS line below, `nros/entity_inventory.json`, and `resolved.toml`'s
    # `[provenance]` table beside the file this reads.
    file(READ "${_frag}" _body)
    file(WRITE "${_path}" "${_body}")
    # The record issue 1729 kept is retired (see above); drop one a previous
    # tree left, so nothing reads it.
    file(REMOVE "${_path}.resolved-seed")
    message(STATUS
        "nros: ${_what} seeded from `nros configure` (${_frag}); this "
        "configure's first reader sees a derived answer rather than a placeholder")
endfunction()

# nros_resolved_entity_take(<metadata> <output> <json> <out_taken>) -- phase-439 W7
#
# The mid-configure entity producer READS the resolve instead of recomposing,
# when the resolve was composed over this configure's own inputs (issue 1228).
#
# `nros build` composes a C/C++ west image at stage 3.5 by the same function
# `nros dev image entity-inventory` runs (`cmd::entity_inventory::compose_image`), over
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
#               resolve read them from `nros configure`'s metadata probe; a component
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
# leaves `<out_taken>` FALSE and says why once: the caller composes, and the
# promise check (RFC-0120 D5) quotes the reason if that composition differs
# from the seed. A road with no record -- every non-west image -- never reaches
# the comparison.
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

    # Only over a CURRENT configure. The record covers the registered rows, the
    # model and the workspace, not the probe sidecars that floor the counts, so
    # a record a stale configure left must not be read. Current means either:
    #   * phase-530 W4 -- the Zephyr module hook ran `nros configure` for this
    #     image in THIS host configure (`NROS_CONFIGURED_CURRENT`), so every
    #     input configure reads is current now; or
    #   * the `nros build` that wrote the record started this configure -- its
    #     digest rides the environment, which ninja's own re-runs inherit.
    file(SHA256 "${_rec}" _rec_sha)
    get_property(_current GLOBAL PROPERTY NROS_CONFIGURED_CURRENT)
    if(NOT _current AND NOT "$ENV{NROS_RESOLVED_ENTITY_RECORD_SHA256}" STREQUAL "${_rec_sha}")
        message(STATUS "nros: entity inventory composed at configure, not read from the "
                       "resolve phase: no current `nros configure` wrote ${_rec}")
        return()
    endif()

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
                     "configure registers [${_keys}] -- re-run `nros configure`")
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
        # RFC-0120 D5 -- if this configure's composition then differs from the
        # seed its readers used, this is why.
        nros_promise_why("${_output}" "${_why}")
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
            # A newline cannot appear in either name, so `a::b`+`c` and
            # `a`+`b::c` stay two rows, as they are in `EntityInventory::insert`.
            string(MD5 _id "${_pkg}\n${_name}")
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
