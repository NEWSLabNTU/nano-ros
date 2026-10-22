# phase-392 W5.c — deliver the ENTITY figures the RMW sizes its queryable table
# from to the cargo invocation this configure owns.
#
# Sibling of `NanoRosBoardFacts.cmake`, same carrier and the same reason: a
# workspace member's own `.cargo/config.toml` is never read, because Corrosion
# runs cargo from the workspace root (phase-349 W2.0), and `set(ENV{...})`
# reaches only the configure-time process, so a knob published that way lands in
# the C lane and not the cargo one (issue 0460). `corrosion_set_env_vars`
# attaches to the target's own build command.
#
# WHAT IS DIFFERENT FROM BOARD FACTS. Board facts answer a question about the
# BOARD, of which exactly one is active per configure. This answers a question
# about the resolved SystemModel, of which a workspace can hold SEVERAL — one
# per entry. There is only ONE runtime staticlib per configure and every entry
# links it, so the compile-time table must satisfy the largest declaration:
# entries ACCUMULATE here (union of the infrastructure flags, max of the
# application counts) and the union is applied once.
#
# Accumulation is safe in that order because `nros_synth_runtime_umbrella` runs
# AFTER the SUBDIRS loop that processes the entries (NanoRosWorkspace.cmake) —
# the same ordering `nros-metadata.json` already depends on.

include_guard(GLOBAL)

include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCorrosionEnv.cmake")

# phase-486 -- `nros_record_entity_facts` / `nros_fold_entity_facts` (and the
# `_nros_entity_fact` reader issue 1429 added for them) are RETIRED with the
# last `NROS_DECLARED_*` fact they carried, the runtime's service families. The
# sizing descriptor states it (`[image] infra_families`) on every image this
# configure names one for, beside the node count (`model_nodes`, W9).

# nros_entity_facts_env_deferred(<target>)
#
# Schedule `nros_entity_facts_env` for the END of the top-level scope, once per
# target. Use this from anywhere that imports a Corrosion crate; call the
# immediate form only if you can prove every `nano_ros_add_entry()` has already
# run, which almost nothing can.
#
# WHY DEFERRED (phase-392 W5.g). The facts were accumulated by
# `nros_record_entity_facts` (retired, phase-486), which ran inside
# `nano_ros_add_entry()` -- as do the sizing descriptor's inputs -- and an
# entry is declared LAST in a configure by design ("the first point guaranteed
# to be AFTER every nano_ros_node_register()"). Every caller that applies the env
# inline therefore reads an EMPTY accumulator. Traced on
# `examples/workspaces/mixed`: the consumer logged `seen=<empty>` before all
# seven producers, so the mechanism had never delivered anything anywhere.
#
# WHY EVERY CORROSION TARGET AND NOT JUST THE UMBRELLA (phase-392 W5.g follow-up).
# `zpico-sys` is compiled once per CARGO ROOT, and a workspace has two: the
# synthesised umbrella (`nros_ws_runtime`) and the repo root that
# `nros_cpp-static` / `nros_c-static` import from. Measured on `mixed`: 6
# `zpico-sys` units, and only the 1 under the umbrella could ever see the env.
# A pure-C/C++ workspace is worse — `nros_synth_runtime_umbrella` returns early
# for it, so the umbrella call site does not exist and NO unit was reachable.
#
# The DEFER target is the TOP-LEVEL scope, for the reason
# `_nano_ros_support_schedule_flush` states one module over: deferring to the
# CURRENT directory fires at the end of whichever package called first, which is
# the bug rather than a smaller version of it.
# The target list travels through a GLOBAL property and the deferred call takes
# NO arguments, which is the shape `_nano_ros_support_flush` uses one module
# over. That is not a style preference: passing the name as a deferred CALL
# argument was tried first and the callee received an EMPTY string, so the
# mapper resolved nothing and `corrosion_set_env_vars` was invoked with one
# argument ("incorrect arguments for function named"). A global carries the
# value across the scope boundary intact.
# Issue 1700 — the shared Corrosion cargo dir is keyed at `nros-c`'s configure
# (features, rmw, board, caps, profile, target, knobs), but the entity facts
# this flush hands cargo are inputs to the SAME build scripts: the sizes probe
# keys on `NROS_SIZING_DESCRIPTOR` and `NROS_DECLARED_*`, and measured the
# executor at 20664 bytes with a leaf's descriptor and 90648 without. Two leaves
# equal on the configure-time key and different here shared one directory, so
# whichever built last owned `nros-c-generated/`, and the other leaf's
# `nros-cpp` refused the build ("written by another crate with DIFFERENT probed
# sizes") — or, had the values agreed by luck, linked the other leaf's archive.
#
# The facts exist only now, so the key is completed now: re-point the build
# dir's `cargo` link at the directory keyed on base + facts. Re-pointing is the
# supported move (`nros_share_corrosion_cargo_dir` keeps the old directory for
# whoever still uses it), and the issue-0945 witness reads the directory from
# a target property at generate time, so it follows. No facts => no change.
function(_nros_entity_facts_rekey_shared_cargo_dir)
    get_property(_base GLOBAL PROPERTY NROS_SHARED_CARGO_BASE_KEY)
    if(NOT _base OR NOT COMMAND nros_share_corrosion_cargo_dir)
        return()
    endif()
    nros_entity_facts_env("" ENV_OUT _facts)
    # Issue 1858 -- features applied to the runtime crates AFTER `nros-c` keyed
    # the directory (`nros_apply_panic_policy`'s `panic-platform`). They change
    # what the build scripts write (the config header's VARIANT stamp) exactly
    # as the facts do, so they complete the key the same way. Measured: the
    # `features` and `managed` workspaces' metadata probes keyed equal, one with
    # `panic-platform` and one without, and each `nros sync` left the other's
    # mirrored header stale. Empty for an image with no late feature, so its key
    # does not move.
    get_property(_late GLOBAL PROPERTY NROS_SHARED_CARGO_LATE_FEATURES)
    if(_late)
        list(REMOVE_DUPLICATES _late)
        list(SORT _late)
        string(REPLACE ";" "," _late "${_late}")
        list(APPEND _facts "late_features=${_late}")
    endif()
    if(NOT _facts)
        return()
    endif()
    # The knob half is environment- or cache-derived (never directory-scoped),
    # so this is the same text `nros-c` keyed on.
    nros_knob_key_fields(_knob_fields)
    nros_share_corrosion_cargo_dir(KEY ${_base} ${_knob_fields} ${_facts})
    if(NROS_SHARED_CARGO_DIR)
        foreach(_t IN ITEMS nros_c-static nros_cpp-static)
            if(TARGET ${_t})
                get_target_property(_had ${_t} NROS_SHARED_CARGO_DIR)
                if(_had)
                    set_property(TARGET ${_t} PROPERTY NROS_SHARED_CARGO_DIR
                        "${NROS_SHARED_CARGO_DIR}")
                endif()
            endif()
        endforeach()
    endif()
endfunction()

function(nros_entity_facts_env_deferred _target)
    get_property(_queued GLOBAL PROPERTY NROS_ENTITY_FACTS_TARGETS)
    if("${_target}" IN_LIST _queued)
        return()
    endif()
    set_property(GLOBAL APPEND PROPERTY NROS_ENTITY_FACTS_TARGETS "${_target}")
    get_property(_scheduled GLOBAL PROPERTY NROS_ENTITY_FACTS_FLUSH_SCHEDULED)
    if(_scheduled)
        return()
    endif()
    set_property(GLOBAL PROPERTY NROS_ENTITY_FACTS_FLUSH_SCHEDULED TRUE)
    cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}"
        CALL _nros_entity_facts_flush)
endfunction()

# phase-359 W10 stage C — "saying nothing gets `platform`" (ARCHITECTURE §2),
# for a configure that imports the runtime staticlib and declares no ending at
# all: a plain `find_package(nano_ros)` library or executable, no
# `nano_ros_entry()`, no board seam calling `nros_apply_panic_policy()`. On posix
# such an image used to get libstd's `#[panic_handler]` through `nros-c/std`;
# with `std` gone it would have none (`#[panic_handler] function required`), so
# the default ending is applied HERE, the way an entry applies it, at the end
# of the configure when every entry has had its say. Runs before the re-key
# below, so the late `panic-platform` is in the shared cargo key (issue 1858).
# A lane that applies the ending itself (Zephyr, NuttX) has declared it, so
# this stands aside.
function(_nros_apply_default_ending)
    get_property(_policy GLOBAL PROPERTY NROS_ENTRY_PANIC_POLICY)
    get_property(_applied GLOBAL PROPERTY NROS_ENTRY_PANIC_APPLIED)
    if(_policy OR _applied OR NOT COMMAND nros_apply_panic_policy)
        return()
    endif()
    foreach(_t IN ITEMS nros_c nros_cpp nros_c-static nros_cpp-static)
        if(TARGET ${_t})
            nros_apply_panic_policy(platform "nano-ros (no entry declared an ending)")
            return()
        endif()
    endforeach()
endfunction()

function(_nros_entity_facts_flush)
    _nros_apply_default_ending()
    _nros_entity_facts_rekey_shared_cargo_dir()
    # Issue 1746 -- the shared cargo key is FINAL now; the configure-time
    # sizes-header heals waited for exactly this (see nros_config_header_heal).
    set_property(GLOBAL PROPERTY NROS_ENTITY_FACTS_FLUSHED TRUE)
    _nros_config_header_heal_flush()
    get_property(_targets GLOBAL PROPERTY NROS_ENTITY_FACTS_TARGETS)
    foreach(_t IN LISTS _targets)
        if(TARGET "${_t}")
            nros_entity_facts_env("${_t}")
        endif()
    endforeach()
    # Issues 1653/1661 -- the C++ half of the same runtime gets the Cyclone facts
    # from the same descriptor cargo was just handed, once, now that every entry
    # has registered (RFC-0100 D12).
    if(COMMAND nros_sizing_descriptor_apply_cyclonedds)
        nros_sizing_descriptor_apply_cyclonedds()
    endif()
endfunction()

# _nros_payload_facts_env -- RETIRED (issue 1649).
#
# It carried the payload trio (`NROS_DECLARED_SUBSCRIBER_BUFFER_SIZE`,
# `_LARGE_SUBSCRIBERS`, `_SUBSCRIBER_LARGE_SIZE`, issue 1122) and the
# transient-local retention slot (`NROS_DECLARED_TL_RETAIN_BYTES`, issue 1498).
# Every road that delivered them names a sizing descriptor (RFC-0100 D12 for a
# multi-entry configure), `nros-rmw-zenoh` reads the same facts off it first
# (`subscriber_payload_classes`, `transient_local_retain_bytes`), and the knob
# diff with them dropped was ZERO: `examples/workspaces/cpp` native (five
# entries) and freertos (mps2-an385, zenoh), both with a non-empty large class
# and a transient-local publisher (measured under a lowered ceiling and a
# temporary latched `/chatter`, issue 1649). The Zephyr resolver carries the
# same derivations as the KNOBS themselves (`ZPICO_*`), not as these carriers.
#
# STILL DELIBERATELY NOT ON THIS ROAD, issue 1255's per-type table
# (`NROS_DERIVED_SUBSCRIBED_TYPE_BOUNDS`). Its only consumer is the executor
# ARENA, whose per-kind sum runs wherever a sizing descriptor states `[image]
# *_entities` (issue 1577) or the Zephyr resolver forwards
# `NROS_ENTITY_COUNT_*`; and where the descriptor states `[[endpoint]]` rows,
# their `claimed_slot_bytes` already price each receive region, so a bound
# table delivered here would price nothing the rows do not.

# _nros_take_buffer_env -- RETIRED (phase-486 W9).
#
# It carried `NROS_DECLARED_SUBSCRIPTION_BUFFER_SIZE` (issue 1233): the take
# buffer's closure maximum, guarded on `NROS_MESSAGE_BOUNDS_STATUS derived`. The
# sizing descriptor's `[types] max_wire_bound_bytes` is the same maximum over
# the same registered tables (issue 1595), and since W9 an image whose model
# describes no wiring states it too, so every image this carrier reached names
# a descriptor that states it.

# _nros_entity_budget_env(<out-var>)
#
# issue 1199 — the ENTITY budget half of the DECLARED road, sibling of
# `_nros_payload_facts_env` above.
#
# The key set is NOT a choice made here: it mirrors `DERIVED_ENV_KEYS` in
# `packages/cli/nros-cli-core/src/leaf_entity_env.rs`, which is the same
# decision already made for the cargo-LEAF road. Two roads delivering different
# key sets is how an image's sizing comes to depend on which lane built it, and
# `check-declared-fact-carriers` holds the two lists together.
#
# What that set deliberately EXCLUDES, and why the exclusions are not oversights:
#
#   * `ZPICO_MAX_QUERYABLES` -- `max_queryables` counts the param and lifecycle
#     service families only when the inventory was built from a model that
#     declares them (issue 1270); the leaf road's inventory comes from
#     metadata, which cannot. A short queryable table is a registration
#     failure at boot, not a smaller pool (issues 1061, 0460).
#     The CMake road completes it from the sizing descriptor's `[image]
#     infra_families` instead (it was the `NROS_DECLARED_INFRA_QUERYABLES`
#     carrier until phase-486).
#
# Two exclusions this comment used to list are GONE (issue 1233): both
# `NROS_EXECUTOR_MAX_NODES` and `NROS_SUBSCRIPTION_BUFFER_SIZE` now travel all
# three roads. Phase-412 withheld the node table on the ground that
# under-counting HALTS the board -- but the ceiling failure is a named
# `NodeError::NodeTableFull`, which is a property of the FAILURE and not of the
# road the number arrived on, and the same count already reached the resolver
# road. The take buffer moved on its own carrier (`_nros_take_buffer_env`,
# retired by phase-486 W9) because its guard is the MESSAGE-BOUND status, not
# this one.
#
# ADDED by issue 1198 (phase-448 W6): `NROS_EXECUTOR_MAX_SC`, the executor's
# other fixed table, which travelled on NO road and was not even published as a
# fact. Same test as its sibling: its one undeclared source is application code
# calling `create_sched_context` by hand, and every such path now NAMES the knob
# (`NodeError::NoSchedContextSlot`) instead of returning a bare `RET_FULL`.
#
# INCLUDED since issue 1198 (phase-448 W6), and the reason the exclusion was
# dropped rather than re-argued: `NROS_EXECUTOR_MAX_NODES` was withheld by
# phase-412 W1 "on the ground that under-counting HALTS the board". That ground
# is now measured rather than assumed. Under-counting nodes has exactly ONE
# source -- `rclc_pubsub_bridge_create`, whose two nodes are runtime strings
# declared nowhere -- and that path NAMES this knob when the table fills, as do
# the executor's own `NodeTableFull` and the zenoh session's per-node liveliness
# table. Every other node is a component the inventory counted. The same test
# applied to `NROS_EXECUTOR_MAX_SC` (on NO road before W6): its one undeclared
# source is application code calling `create_sched_context` by hand, and all
# three FFI wrappers now name the knob instead of returning a bare `RET_FULL`.
# Withholding them cost 12,416 B of every FreeRTOS executor backing, identical
# to the byte across leaves whose declarations differ.
#
# The guard is a single status. Unlike message bounds there is no BASIS here:
# `derived` means every `NROS_DERIVED_*` in the fragment is present, and
# `refused` means none is (`NanoRosEntityInventory.cmake`).
#
# NO FLOOR IS APPLIED HERE, on purpose. The derivation publishes DEMAND and the
# floor belongs to the consumer that names the knob (issues 1015, 1033) -- the
# two `ZPICO_*` counts size fixed C arrays where zero is not a smaller pool,
# while the same numbers reach pools where zero IS the answer. On this road the
# consumer is a build script, so it floors what it takes; the leaf sidecar
# floors at its own boundary for the same reason, one layer over.
function(_nros_entity_budget_env _out_var)
    set(${_out_var} "" PARENT_SCOPE)
    if(NOT COMMAND nros_entity_inventory_knobs_file)
        return()
    endif()
    nros_entity_inventory_knobs_file(_inv)
    if(NOT EXISTS "${_inv}")
        return()
    endif()
    include("${_inv}")
    if(NOT NROS_ENTITY_INVENTORY_STATUS STREQUAL "derived")
        return()
    endif()
    # Both names are written IN FULL, and the delivered name is never built by
    # interpolation. phase-412's second delivery failure was exactly that: a
    # `foreach` composing `NROS_DERIVED_${_pool}` produced a name that matches
    # nothing, and CMake yields EMPTY for an unknown variable rather than
    # failing. A constructed name is also invisible to `grep`, which is how
    # `check-declared-fact-carriers` reads this file -- so a fact spelled only
    # in pieces would be delivered and still report as unproduced.
    #
    # `NROS_DERIVED_MAX_LIVELINESS` is NOT carried on this road (phase-412 W2).
    # It counts a token per parameter and lifecycle server, which the fragment
    # knows only from the model it was composed with -- on a multi-entry
    # configure, ONE entry's model. The consumer completes it instead, as it
    # does the queryable table: `nros-zpico-build` adds the node and
    # infrastructure tokens from the descriptor's `[image] infra_families` and
    # `model_nodes` to its `[image] entity_liveliness_tokens` (issue 1713; it kept the zpico default of 16
    # until then, short for a param + lifecycle image).
    set(_out "")
    # Issue 1649 (RFC-0100 D12) -- NINE entity counts were retired onto the
    # sizing descriptor: EXECUTOR_MAX_CBS, _MAX_SC, _ACTION_CLIENTS, _MAX_NODES,
    # _MAX_MONITORS, _MAX_AGE_MONITORS, MAX_PUBLISHERS, MAX_SUBSCRIBERS and
    # RMW_SUBSCRIBER_SLOTS. Every road this list reached names a descriptor now
    # (a multi-entry configure its shared runtime's), the descriptor states each
    # as an `[image]` field from the SAME `derive` (issue 1655), its consumer
    # reads it first, and the per-carrier knob diff was ZERO on both measured
    # images. A carrier delivering the same number a second way is the
    # 0460/0491 shape the descriptor exists to remove.
    #
    # Issue 1655 -- and the per-kind cell registry capacity (issue 1130's
    # RUNTIME_MAX_CELL_ENTITIES) followed them: `[image] cell_entities`, read
    # first by `nros/build.rs`, zero-diff with this carrier dropped on
    # `examples/workspaces/cpp` native, freertos_posix and freertos (mps2,
    # zenoh).
    #
    # Issue 1649 -- and the last two, the service-inbox families' slot sizes
    # (SERVICE_INBOX_BYTES, ACTION_INBOX_BYTES): `nros-rmw-zenoh` reads the
    # descriptor's per-family request bound first, and the drop was zero-diff
    # on the cmake road (native, five entries with service and action servers)
    # and on the Zephyr WEST road (a service-serving `native_sim` image), while
    # with no descriptor named the same drop moves SERVICE_INBOX_BYTES 24 ->
    # 1024. So this list is EMPTY: every entity count this function carried is
    # a descriptor field now. The function stays as the one place a future
    # carrier would be produced, and returns nothing.
    #
    # The zenoh read-task budget (phase-462 W6b) is not carried on this road
    # either, and was never added to it: the sizing descriptor states it
    # (`[image] rx_task_budget_frames` / `_rest_us`, from the SAME `derive`)
    # and `nros-zpico-build` reads it there, below every stated rung. A
    # carrier delivering it beside the descriptor would be the 0460/0491 shape
    # this list was emptied to end.
    set(${_out_var} "${_out}" PARENT_SCOPE)
endfunction()

# _nros_qos_depth_env -- RETIRED (issue 1655).
#
# It carried `NROS_DECLARED_MAX_QOS_DEPTH`, the largest declared subscription
# depth, guarded on every subscription having declared one (phase-412 W3). The
# sizing descriptor states every row's depth and every road names one, so the
# same guarded MAX is now ONE function in the descriptor crate
# (`nros_sizing_descriptor::max_subscription_depth`), read first by
# `nros-node/build.rs` -- and the knob diff with this carrier dropped was ZERO
# on `examples/workspaces/cpp` native, freertos_posix and freertos (mps2,
# zenoh). Two spellings of one reduction is what the descriptor exists to end.

# _nros_declared_depth_table_env(<out-var>)
#
# phase-454 W10 -- carry the per-endpoint declared depths, not their maximum, on
# the lanes with no Kconfig.
#
# The SAME carrier the Zephyr lane already forwards
# (`nros_cargo_build.cmake`'s `NROS_ENTITY_DECLARED_DEPTHS`), and the same
# `type|topic=depth` spelling, because a second name for one fact is how a
# producer and a consumer quietly stop meeting. `nros-node/build.rs` reads it
# twice, for two different jobs.
#
# ONE GUARD, where the retired `_nros_qos_depth_env` had two, and the difference is
# the point. That function SIZES: the arena charges every pub/sub slot the same
# price, so a table over the endpoints that happened to be declared would size
# an image from a subset of itself, and it must refuse unless the UNDECLARED
# count is zero. This function CHECKS, per endpoint: whether `/chatter`'s
# declared depth matches the depth registered on `/chatter` is a question about
# `/chatter`, and a silent neighbour makes it no less answerable. Requiring the
# whole image to declare before any of it is checked would turn one unannotated
# subscription into no checking at all -- and every in-tree contract leaves one,
# which is exactly the measurement recorded above.
#
# The value is `,`-joined: cmake would split a `;`-separated list inside a
# `NAME=VALUE` payload. `build.rs` accepts either separator, as the Zephyr lane
# also hands it a comma-joined string.
function(_nros_declared_depth_table_env _out_var)
    set(${_out_var} "" PARENT_SCOPE)
    if(NOT COMMAND nros_entity_inventory_knobs_file)
        return()
    endif()
    nros_entity_inventory_knobs_file(_inv)
    if(NOT EXISTS "${_inv}")
        return()
    endif()
    include("${_inv}")
    # A table this module did not compose is a table nobody may read: a
    # `refused` inventory has no rows, and a partial one would check call sites
    # against declarations that were never resolved.
    if(NOT NROS_ENTITY_DECLARED_DEPTH_STATUS STREQUAL "resolved")
        return()
    endif()
    if(NOT DEFINED NROS_ENTITY_DECLARED_DEPTHS)
        return()
    endif()
    string(REPLACE ";" "," _nros_depth_table "${NROS_ENTITY_DECLARED_DEPTHS}")
    if(_nros_depth_table STREQUAL "")
        return()
    endif()
    set(${_out_var} "NROS_ENTITY_DECLARED_DEPTHS=${_nros_depth_table}" PARENT_SCOPE)
endfunction()

# _nros_param_store_env(<out-var>)
#
# phase-446 W4 -- carry the PARAMETER STORE sizing the contract's `params:`
# derived to nros-params' build script, on the lanes with no Kconfig.
#
# Read from the entity-inventory fragment, which is the one derivation
# (`nros_cli_core::entity_inventory::render_param_store`); this function only
# forwards what it wrote. Guarded by its own status and not the entity
# inventory's: the store is sized from the MODEL, and an image whose entity
# count refuses can still have declared every parameter.
#
# The numbers travel as `NROS_DECLARED_*` DEFAULTS, below every stated rung, so
# an `NROS_MAX_*` in the environment or a board's `[knobs.params]` still wins.
# A capacity a declared type NEEDS carries no number at all -- a capacity is a
# board fact -- only the parameter that needs it, and the build script refuses
# when no rung states one.
#
# The cargo-LEAF road carries none of this: a leaf's sidecar is derived from a
# metadata probe that sees no SystemModel, so there is no declaration there to
# forward and its store keeps the crate defaults (`check-declared-fact-carriers`
# records why: FACT_DISPOSITION for the five capacities, ROAD_UNPAIRED for the
# three `PARAM_NEEDS_*` inputs).
function(_nros_param_store_env _out_var)
    set(${_out_var} "" PARENT_SCOPE)
    if(NOT COMMAND nros_entity_inventory_knobs_file)
        return()
    endif()
    nros_entity_inventory_knobs_file(_inv)
    if(NOT EXISTS "${_inv}")
        return()
    endif()
    include("${_inv}")
    # phase-460 W2 (issue 1421) -- a PARTIAL declaration is heard, not sized
    # around. The inventory refuses when some nodes declare `params:` and the
    # rest do not (phase-446's every-node-or-none rule, which is right: a store
    # sized from the nodes that declared gives the others no slots), and it
    # writes WHICH nodes. This function used to `return()` on anything but
    # `declared`, so that refusal reached nros-params' build script as no
    # declaration at all and the store took the crate defaults. Measured on the
    # island (brief D, E6a): one node's block deleted, 25/35/0/0/0 became
    # 32/64/256/32/256 with no build-time line, the `set_parameters` request
    # shape grew with the capacities (issue 1352's class), and that one node
    # lost its DECLARED_PARAM_MISMATCH check. The producer said no and the next
    # layer read it as "nothing to say"; the configure stops here instead.
    #
    # `absent` keeps its meaning -- no node declares, and an image with no
    # contract is sized by its board -- and so does a fragment that predates
    # the field. Only the inventory's own `refused` is fatal.
    if(NROS_PARAM_DECLARATION_STATUS STREQUAL "refused")
        message(FATAL_ERROR
            "nros: the parameter store cannot be sized: the entity inventory "
            "REFUSED the contract's `params:` declaration "
            "(NROS_PARAM_DECLARATION_STATUS \"refused\" in ${_inv}):\n"
            "  ${NROS_PARAM_DECLARATION_REASON}\n"
            "Declare `params:` on every node in the image, or on none. The "
            "configure stops here rather than build the image on nros-params' "
            "crate defaults (phase-460 W2, issue 1421).")
    endif()
    if(NOT NROS_PARAM_DECLARATION_STATUS STREQUAL "declared")
        return()
    endif()
    # Both names written IN FULL, for the reason `_nros_entity_budget_env`
    # gives: an interpolated name resolves EMPTY and is invisible to grep.
    #
    # Issue 1649 -- MAX_PARAMETERS and MAX_PARAM_NAME_LEN left this list: the
    # descriptor's `[params]` states both, `nros-params` reads it first, the
    # knob diff with them dropped was zero on a parameter-declaring
    # `examples/workspaces/cpp` native configure, and no other road carries
    # them under these names. Then the three NEEDS rows and the service shape
    # followed: the descriptor's `[params]` states each need and the
    # per-node shapes, `nros-params` / `nros-node` / `nros-rmw-zenoh` read
    # them first, and the drop was zero-diff on the cmake road AND the Zephyr
    # WEST road (a parameter-declaring, service-serving `native_sim` image),
    # while with no descriptor the same drop turns the declared shape to
    # `None`. What stays are the three CAPACITIES: a board fact, never in the
    # descriptor (RFC-0100 D1).
    set(_out "")
    foreach(_pair
            "NROS_DECLARED_MAX_STRING_VALUE_LEN;NROS_DERIVED_MAX_STRING_VALUE_LEN"
            "NROS_DECLARED_MAX_ARRAY_LEN;NROS_DERIVED_MAX_ARRAY_LEN"
            "NROS_DECLARED_MAX_BYTE_ARRAY_LEN;NROS_DERIVED_MAX_BYTE_ARRAY_LEN")
        list(GET _pair 0 _name)
        list(GET _pair 1 _src)
        if(DEFINED ${_src})
            list(APPEND _out "${_name}=${${_src}}")
        endif()
    endforeach()
    set(${_out_var} "${_out}" PARENT_SCOPE)
endfunction()

# nros_entity_facts_env(<target> [ENV_OUT <var>])
#
# Attach this configure's accumulated entity facts to a Corrosion target's cargo
# invocation. Called once, after every entry has been processed.
#
# ENV_OUT <var> (issue 1142) — set <var> in the caller's scope to the same
# `KEY=VALUE` list instead of attaching it to a Corrosion target, and ignore
# <target>. For a lane whose cargo invocation is NOT a Corrosion target: the
# NuttX board links its image with an env-wrapped `cargo build` of its own
# (`packages/api/nros-c/cmake/nros-nuttx.cmake`), so `corrosion_set_env_vars`
# reaches nothing there and every NuttX image sized its pools from the
# backend's literal. Only the CARRIER differs — the composition above and the
# status lines below are the same ones, deliberately, because two compositions
# is how two lanes come to size one image two ways.
#
# (`check-board-facts-delivery` detects such a lane by SOURCE TEXT; since
# issue 1541 it strips full-line comments first, so prose here that names the
# lane's command no longer makes this file read as one.)
function(nros_entity_facts_env _target)
    cmake_parse_arguments(_NEF "" "ENV_OUT" "" ${ARGN})
    if(_NEF_ENV_OUT)
        set(${_NEF_ENV_OUT} "" PARENT_SCOPE)
    endif()
    # issue 1122 — the accumulator starts with the payload facts, which are
    # INDEPENDENT of the entity facts below; the trio and the retention slot
    # retired onto the sizing descriptor (issue 1649), and the take buffer
    # stays (below).
    set(_payload_env "")
    _nros_entity_budget_env(_budget_env)
    if(_budget_env)
        list(APPEND _payload_env ${_budget_env})
    endif()

    # phase-454 W10 -- the per-endpoint table, for the REGISTRATION check.
    # Distinct from the maximum above, which sizes; see the two functions'
    # guards.
    _nros_declared_depth_table_env(_depth_table_env)
    if(_depth_table_env)
        list(APPEND _payload_env "${_depth_table_env}")
    endif()

    # phase-454 W14 (RFC-0100 D4, issue 0460) -- the SIZING DESCRIPTOR's path.
    #
    # Not a derived number but the file every derived number now travels in, and
    # it rides this carrier for the reason 0460 records: `set(ENV{...})` touches
    # only the configure-time process, so a knob published that way reaches the
    # C lane and not the Rust one. Putting it on the emitted command is the fix,
    # and this is the emitted command.
    #
    # It is a PATH, and nothing watches the VARIABLE (issue 0491) -- the rebuild
    # edge is on the file's CONTENT, in
    # `nros_sizing_descriptor::load_for_build_script`.
    if(COMMAND nros_sizing_descriptor_cargo_env)
        nros_sizing_descriptor_cargo_env(_sizing_env)
        if(_sizing_env)
            list(APPEND _payload_env "${_sizing_env}")
        endif()
    endif()

    # phase-446 W4 -- the parameter store, from the contract's `params:`.
    _nros_param_store_env(_param_env)
    if(_param_env)
        list(APPEND _payload_env ${_param_env})
        message(STATUS
            "nano-ros: parameter store sized from the contract -- "
            "${_param_env} (phase-446 W4)")
    endif()

    # phase-486 -- what remains to deliver is the descriptor's path and the
    # payload / budget / parameter-store env above. The queryable table's
    # inputs all come from the descriptor (issue 1649 retired the carriers of
    # the application counts from the sizing descriptor's fields), and the node
    # count and service families too (phase-486).
    if(NOT _payload_env)
        return()
    endif()
    if(_NEF_ENV_OUT)
        set(${_NEF_ENV_OUT} "${_payload_env}" PARENT_SCOPE)
    else()
        if(NOT COMMAND corrosion_set_env_vars)
            message(FATAL_ERROR "nros_entity_facts_env(${_target}): Corrosion not loaded")
        endif()
        # issue 0657 — attach to the target the cargo command actually READS.
        nros_corrosion_env_target("${_target}" _target)
        corrosion_set_env_vars(${_target} ${_payload_env})
    endif()
    message(STATUS
        "nano-ros: sizing facts delivered from the declaration — ${_payload_env}")
endfunction()


# nros_config_header_heal(<mirror-sh> <leaf-src> <build-dir> <gen-subdir> <name> <dest>)
#
# Issue 1746 -- the configure-time HEAL of a mirrored sizes header (issue 0268 /
# 0985), run only once the shared Corrosion cargo key is FINAL.
#
# `<build>/cargo` is a symlink whose target is chosen by KEY, and the key is
# completed in two steps: `nros-c`'s configure points it at base + knobs, and
# the deferred `_nros_entity_facts_flush` re-points it at base + knobs + entity
# facts (issue 1700). On a RE-configure that is a round trip -- away from the
# directory that built this leaf's archive and back -- and the heal ran in the
# middle, at `nros-cpp`'s own scope. It therefore copied the facts-LESS key's
# header (`NROS_EXECUTOR_SIZE 90576`, no descriptor) over the right one
# (20592), with a NEW mtime. The build-time mirror's only input is the archive,
# now OLDER than its output, so ninja skipped the one rule that would have put
# the right bytes back, and the leaf's first build linked
# `undefined reference to nros_config_variant_sz_*`. A second `ninja` settled
# it only because something else rebuilt the archive.
#
# So the request is QUEUED here and executed at the end of the top-level
# configure: by `_nros_entity_facts_flush` after its re-key when one is
# scheduled, otherwise by its own deferred call. Any configure-time reader of
# `<build>/cargo/*` must take the same road; the mirror script is the only one
# (`check-config-header-single-writer` holds every writer to that script).
function(nros_config_header_heal _sh _leaf _build _gendir _name _dest)
    string(JOIN "|" _req "${_sh}" "${_leaf}" "${_build}" "${_gendir}" "${_name}" "${_dest}")
    set_property(GLOBAL APPEND PROPERTY NROS_CONFIG_HEADER_HEALS "${_req}")
    get_property(_scheduled GLOBAL PROPERTY NROS_CONFIG_HEADER_HEAL_SCHEDULED)
    if(NOT _scheduled)
        set_property(GLOBAL PROPERTY NROS_CONFIG_HEADER_HEAL_SCHEDULED TRUE)
        cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}"
            CALL _nros_config_header_heal_flush)
    endif()
endfunction()

function(_nros_config_header_heal_flush)
    # Every deferral is registered during the ordinary configure, so by the time
    # any deferred call runs the facts flush is either queued or never will be.
    # If it is queued and has not run, it calls this after its re-key.
    get_property(_facts_scheduled GLOBAL PROPERTY NROS_ENTITY_FACTS_FLUSH_SCHEDULED)
    get_property(_facts_flushed GLOBAL PROPERTY NROS_ENTITY_FACTS_FLUSHED)
    if(_facts_scheduled AND NOT _facts_flushed)
        return()
    endif()
    get_property(_heals GLOBAL PROPERTY NROS_CONFIG_HEADER_HEALS)
    set_property(GLOBAL PROPERTY NROS_CONFIG_HEADER_HEALS "")
    foreach(_req IN LISTS _heals)
        string(REPLACE "|" ";" _a "${_req}")
        list(GET _a 0 _sh)
        list(GET _a 1 _leaf)
        list(GET _a 2 _build)
        list(GET _a 3 _gendir)
        list(GET _a 4 _name)
        list(GET _a 5 _dest)
        # Best-effort, as the heal always was: a tree that never built has
        # neither candidate, and the build-time edge is the real mechanism.
        execute_process(
            COMMAND bash "${_sh}" "${_leaf}" "${_build}" "${_gendir}" "${_name}" "${_dest}"
            RESULT_VARIABLE _rc
            OUTPUT_QUIET ERROR_QUIET)
    endforeach()
endfunction()
