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

# nros_record_entity_facts(<model-path>)
#
# Ask `nros ws entity-facts` about ONE entry's model and fold the answer into
# this configure's accumulated view.
#
# Deliberately soft on failure, exactly like `nros_resolve_board_facts`: a model
# that is not there yet, a CLI that has not been built, an entry addressed the
# `MODEL` way at a path that does not exist — all mean "this configure has no
# entity facts to carry", which is the state every build was in before this
# wave. Nothing here is a configuration error, so nothing here is fatal.
function(nros_record_entity_facts _model)
    if(_model STREQUAL "" OR NOT EXISTS "${_model}")
        return()
    endif()
    if(NOT DEFINED _NANO_ROS_CODEGEN_TOOL OR NOT EXISTS "${_NANO_ROS_CODEGEN_TOOL}")
        return()
    endif()

    # One run per distinct model — several entries share a bringup, and the
    # workspaces that do (workspaces/c has 7) would otherwise pay the verb once
    # per entry for the same answer.
    string(MAKE_C_IDENTIFIER "NROS_ENTITY_FACTS_MEMO__${_model}" _memo)
    get_property(_seen GLOBAL PROPERTY ${_memo})
    if(_seen)
        return()
    endif()
    set_property(GLOBAL PROPERTY ${_memo} TRUE)

    execute_process(
        COMMAND "${_NANO_ROS_CODEGEN_TOOL}" ws entity-facts --model "${_model}"
        OUTPUT_VARIABLE _out
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)
    if(NOT _rc EQUAL 0)
        string(REGEX REPLACE "\n+" " " _why "${_err}")
        string(SUBSTRING "${_why}" 0 200 _why)
        message(STATUS "nano-ros: entity facts NOT read from ${_model} — ${_why}")
        return()
    endif()

    set_property(GLOBAL PROPERTY NROS_ENTITY_FACTS_SEEN TRUE)

    string(REPLACE "\n" ";" _lines "${_out}")
    # An entry whose model describes no wiring says NOTHING about the
    # application count (the verb abstains rather than reporting a zero it
    # cannot support). One such entry makes the whole configure's application
    # count unknown: the shared staticlib has to hold the largest, and an
    # unknown is not smaller than anything.
    set(_saw_servers FALSE)
    foreach(_line IN LISTS _lines)
        if(_line MATCHES "^NROS_DECLARED_INFRA_QUERYABLES=(.*)$")
            set(_infra "${CMAKE_MATCH_1}")
            if(_infra MATCHES "param")
                set_property(GLOBAL PROPERTY NROS_ENTITY_INFRA_PARAM TRUE)
            endif()
            if(_infra MATCHES "lifecycle")
                set_property(GLOBAL PROPERTY NROS_ENTITY_INFRA_LIFECYCLE TRUE)
            endif()
        elseif(_line MATCHES "^NROS_DECLARED_SERVICE_SERVERS=([0-9]+)$")
            set(_saw_servers TRUE)
            get_property(_have GLOBAL PROPERTY NROS_ENTITY_SERVERS_MAX)
            if(NOT _have OR CMAKE_MATCH_1 GREATER _have)
                set_property(GLOBAL PROPERTY NROS_ENTITY_SERVERS_MAX "${CMAKE_MATCH_1}")
            endif()
        elseif(_line MATCHES "^NROS_DECLARED_NODES=([0-9]+)$")
            # phase-426 W3 -- the ROS parameter services are registered once
            # PER NODE, so this is a term in the queryable pool. MAX across the
            # configure's models for the same reason the server count is: the
            # shared staticlib holds the largest, and the pool is sized once.
            get_property(_have GLOBAL PROPERTY NROS_ENTITY_NODES_MAX)
            if(NOT _have OR CMAKE_MATCH_1 GREATER _have)
                set_property(GLOBAL PROPERTY NROS_ENTITY_NODES_MAX "${CMAKE_MATCH_1}")
            endif()
        endif()
    endforeach()
    if(NOT _saw_servers)
        set_property(GLOBAL PROPERTY NROS_ENTITY_SERVERS_UNKNOWN TRUE)
    endif()
endfunction()

# nros_entity_facts_env_deferred(<target>)
#
# Schedule `nros_entity_facts_env` for the END of the top-level scope, once per
# target. Use this from anywhere that imports a Corrosion crate; call the
# immediate form only if you can prove every `nano_ros_add_entry()` has already
# run, which almost nothing can.
#
# WHY DEFERRED (phase-392 W5.g). The facts are accumulated by
# `nros_record_entity_facts`, which runs inside `nano_ros_add_entry()` — and an
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

function(_nros_entity_facts_flush)
    get_property(_targets GLOBAL PROPERTY NROS_ENTITY_FACTS_TARGETS)
    foreach(_t IN LISTS _targets)
        if(TARGET "${_t}")
            nros_entity_facts_env("${_t}")
        endif()
    endforeach()
endfunction()

# _nros_payload_facts_env(<out-var>)
#
# issue 1122 — carry the DERIVED large-payload class count across the lane
# boundary, as a DECLARED fact.
#
# `nros_derive_message_bound_knobs()` already computes this correctly on every
# lane and writes it to `${CMAKE_BINARY_DIR}/nros/message_bound_knobs.cmake`.
# Its only consumer in the tree is `_nros_resolve_derivable_knob` in
# `zephyr/cmake/nros_cargo_build.cmake`, reached only through
# `zephyr/CMakeLists.txt`, so on FreeRTOS / ThreadX / NuttX / posix the number
# was computed, written to disk, and discarded. Measured on the first
# out-of-tree consumer: `LARGE_PAYLOADS` was 131,072 B of bss on a node that
# never calls `declare_subscriber`, while the same build dir held
# `set(NROS_DERIVED_MAX_LARGE_SUBSCRIBERS 0)`.
#
# The FILE and not a variable: this runs at the deferred flush, in the
# top-level scope, where `nros_find_interfaces()`'s variables are not visible.
#
# TWO CONDITIONS, and they are the whole safety argument. `derived` says the
# join answered rather than refusing; `subscribed` says it answered over the
# subscriptions this image DECLARES. On the `closure` basis the count is a
# count of large TYPES in the linked closure, which under-counts an image with
# two subscriptions on one large type -- so we refuse there and leave the
# crate default alone. Under-sizing this pool is a `SubscriberCreationFailed`
# at `create_subscription`, and picking that up by accident is worse than the
# bytes.
#
# It travels as `NROS_DECLARED_*`, not `ZPICO_MAX_LARGE_SUBSCRIBERS`, so it is
# a DEFAULT the build script may override rather than a value set in the child
# environment. Setting the knob itself would silently break rung 1 of the
# ladder: a consumer who names `ZPICO_MAX_LARGE_SUBSCRIBERS` must still win.
#
# WHAT IS DELIBERATELY NOT HERE, issue 1255's per-type table
# (`NROS_DERIVED_SUBSCRIBED_TYPE_BOUNDS`). Its only consumer is the executor
# ARENA, and the arena's per-kind sum runs only where `NROS_ENTITY_COUNT_*`
# arrive -- which is the Zephyr resolver road alone
# (`zephyr/cmake/nros_cargo_build.cmake`). On this road and on the cargo-leaf
# sidecar the model is 0 and the sum is never reached, so a bound table
# delivered here would price nothing. THE COUNTS COME FIRST on these roads; the
# bounds follow them, in the same change, or they are a wire to a consumer that
# is not listening. Same shape as issue 1122, one fact over.
function(_nros_payload_facts_env _out_var)
    set(${_out_var} "" PARENT_SCOPE)
    if(NOT COMMAND nros_message_bounds_knobs_file)
        return()
    endif()
    nros_message_bounds_knobs_file(_knobs)
    if(NOT EXISTS "${_knobs}")
        return()
    endif()
    # Read into THIS function's scope; the file is a plain list of `set()`s.
    include("${_knobs}")
    if(NOT NROS_MESSAGE_BOUNDS_PAYLOAD_STATUS STREQUAL "derived")
        return()
    endif()
    if(NOT NROS_MESSAGE_BOUNDS_BASIS STREQUAL "subscribed")
        return()
    endif()
    # issue 1199 — the THREE payload keys, and the set is not ours to choose:
    # it mirrors `DERIVED_PAYLOAD_ENV_KEYS` in
    # `packages/cli/nros-cli-core/src/leaf_entity_env.rs`, which is the same
    # decision made for the cargo-LEAF road. Two roads delivering different key
    # sets is how an image's sizing depends on which lane built it.
    #
    # Each of the two SIZES is published by the derivation only under its own
    # condition, and this reads DEFINED rather than re-deriving them: a small
    # class of 0 means nothing received fits under the ceiling, and a large
    # SIZE for a class with no blocks would be inventing a number
    # (`_nros_bounds_publish_payload_classes`). Absent therefore means "no
    # answer" here exactly as it does there.
    set(_out "")
    if(DEFINED NROS_DERIVED_MAX_LARGE_SUBSCRIBERS)
        list(APPEND _out
            "NROS_DECLARED_LARGE_SUBSCRIBERS=${NROS_DERIVED_MAX_LARGE_SUBSCRIBERS}")
    endif()
    if(DEFINED NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE)
        list(APPEND _out
            "NROS_DECLARED_SUBSCRIBER_BUFFER_SIZE=${NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE}")
    endif()
    if(DEFINED NROS_DERIVED_SUBSCRIBER_LARGE_SIZE)
        list(APPEND _out
            "NROS_DECLARED_SUBSCRIBER_LARGE_SIZE=${NROS_DERIVED_SUBSCRIBER_LARGE_SIZE}")
    endif()
    set(${_out_var} "${_out}" PARENT_SCOPE)
endfunction()

# _nros_take_buffer_env(<out-var>)
#
# issue 1233 — the TAKE BUFFER, and it needed its own carrier rather than a row
# in `_nros_payload_facts_env` above. That is almost certainly why it was left
# behind when 1122 swept the payload trio onto this road: it does not fit that
# function's guard, and the guard is not incidental.
#
# The payload classes require `BASIS subscribed`, because on the `closure` basis
# they count large TYPES in the linked closure and under-count an image with two
# subscriptions on one large type. This knob is the opposite. Its producer says
# so where it publishes it:
#
#     Buffer 1, the runtime-owned take buffer: ONE global size for every ENTITY
#     in the image (`RX_BUF` is a const generic and the C/C++ path is
#     type-erased), so it must hold the largest type the image could receive --
#     and, because `DEFAULT_TX_BUF` aliases it, the largest it could publish.
#     BASIS `closure`, always. Narrowing this one is the under-derivation.
#
# So the correct guard here is the WHOLE-BOUNDS status, not the payload status
# and not the basis: `NROS_MESSAGE_BOUNDS_STATUS derived` means every type in
# the closure carried a bound, which is exactly the condition under which the
# maximum over them is an upper bound. Applying the payload guard would have
# refused every image whose join answered on the closure basis -- which for this
# fact is all of them.
#
# What a leafless CMake image got before this: the crate default of 1024 for a
# buffer its own configure had already measured, and `DEFAULT_TX_BUF` aliases
# it, so the publish side took the same default. Issue 1122's shape, on the
# fourth size knob.
function(_nros_take_buffer_env _out_var)
    set(${_out_var} "" PARENT_SCOPE)
    if(NOT COMMAND nros_message_bounds_knobs_file)
        return()
    endif()
    nros_message_bounds_knobs_file(_knobs)
    if(NOT EXISTS "${_knobs}")
        return()
    endif()
    include("${_knobs}")
    if(NOT NROS_MESSAGE_BOUNDS_STATUS STREQUAL "derived")
        return()
    endif()
    if(NOT DEFINED NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE)
        return()
    endif()
    set(${_out_var}
        "NROS_DECLARED_SUBSCRIPTION_BUFFER_SIZE=${NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE}"
        PARENT_SCOPE)
endfunction()

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
#     The CMake road completes it through `NROS_DECLARED_INFRA_QUERYABLES`
#     instead, which is why that fact exists.
#
# Two exclusions this comment used to list are GONE (issue 1233): both
# `NROS_EXECUTOR_MAX_NODES` and `NROS_SUBSCRIPTION_BUFFER_SIZE` now travel all
# three roads. Phase-412 withheld the node table on the ground that
# under-counting HALTS the board -- but the ceiling failure is a named
# `NodeError::NodeTableFull`, which is a property of the FAILURE and not of the
# road the number arrived on, and the same count already reached the resolver
# road. The take buffer moves on its own carrier below
# (`_nros_take_buffer_env`) because its guard is the MESSAGE-BOUND status, not
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
# source -- `nros_pubsub_bridge_create`, whose two nodes are runtime strings
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
    # configure, ONE entry's model. The queryable sizing on this road completes
    # that infrastructure term at the consumer from per-entry facts; the
    # liveliness pool has no such completion, so it keeps the zpico default.
    set(_out "")
    foreach(_pair
            # issue 1130 -- the per-kind cell registry capacity for a class that
            # states no ENTITY_BOUNDS. Composed across entries by MAX with no
            # accumulator here: the fragment is ONE per configure, derived over
            # every component this configure registered, so its number already
            # is the largest component's -- and it is absent (the whole road
            # abstains) when any component declared nothing.
            "NROS_DECLARED_RUNTIME_MAX_CELL_ENTITIES;NROS_DERIVED_RUNTIME_MAX_CELL_ENTITIES"
            "NROS_DECLARED_EXECUTOR_ACTION_CLIENTS;NROS_DERIVED_EXECUTOR_ACTION_CLIENTS"
            "NROS_DECLARED_EXECUTOR_MAX_CBS;NROS_DERIVED_EXECUTOR_MAX_CBS"
            "NROS_DECLARED_EXECUTOR_MAX_NODES;NROS_DERIVED_EXECUTOR_MAX_NODES"
            "NROS_DECLARED_EXECUTOR_MAX_SC;NROS_DERIVED_EXECUTOR_MAX_SC"
            "NROS_DECLARED_RMW_SUBSCRIBER_SLOTS;NROS_DERIVED_RMW_SUBSCRIBER_SLOTS"
            "NROS_DECLARED_MAX_PUBLISHERS;NROS_DERIVED_MAX_PUBLISHERS"
            "NROS_DECLARED_MAX_SUBSCRIBERS;NROS_DERIVED_MAX_SUBSCRIBERS"
            # issue 1233 — the node table. Withheld from phase-412 W1 because
            # under-counting HALTS the board, and admitted to the resolver road
            # on 2026-09-03 once `NodeError::NodeTableFull` was made to name the
            # knob. That precondition is a property of the FAILURE, not of the
            # road, so it holds here too; nothing in the tree ever said why the
            # other two roads were left out.
            "NROS_DECLARED_EXECUTOR_MAX_NODES;NROS_DERIVED_EXECUTOR_MAX_NODES")
        list(GET _pair 0 _name)
        list(GET _pair 1 _src)
        if(DEFINED ${_src})
            list(APPEND _out "${_name}=${${_src}}")
        endif()
    endforeach()
    set(${_out_var} "${_out}" PARENT_SCOPE)
endfunction()

# _nros_qos_depth_env(<out-var>)
#
# phase-412 W3 / phase-403 step 2 — carry the declared QoS DEPTH to the arena.
#
# `nros-node/build.rs` bills every pub/sub callback slot at `PUBSUB_QOS_DEPTH`,
# a CONSTANT 10, and says why: "Carrying the declared depths here instead is
# phase-403 step 2's remaining wiring: `NROS_ENTITY_DECLARED_DEPTHS` and
# `NROS_ENTITY_UNDECLARED_DEPTH_COUNT` reach cmake and stop there, so this lane
# has nothing better to read yet." This is that wiring, on the road issue 1122
# built.
#
# The over-billing is structural rather than marginal. `buffered_region` gives a
# `depth <= 1` subscription a TripleBuffer of 3 slots and anything deeper an
# `SpscRing` of `depth + 1`, so an image whose subscriptions all declare depth 1
# is charged 11 slots for 3 -- on every lane, before this.
#
# WHAT TRAVELS IS THE MAXIMUM, not the table. The arena charges every pub/sub
# slot the same `pubsub_entry`, so one number is what the consumer can use, and
# the max is the only reduction that cannot under-size it. Shipping the triples
# would hand the build script a table it has no way to attribute to slots.
#
# TWO GUARDS, and the second is the producer's own instruction. The status must
# be `resolved`, and the UNDECLARED count must be ZERO --
# `NanoRosEntityInventory.cmake` calls it "what a consumer must refuse on",
# because a table over the endpoints that happened to be annotated sizes an
# image from a subset of itself. One unannotated subscription and the max is a
# lower bound rather than a bound, which is the under-size direction.
#
# phase-454 W2 -- and the count is the SUBSCRIPTION-scoped one, because the list
# it guards is the subscription list. Two reasons, and the second is the one
# that made this move now:
#
#   * it is issue 1227's ruling applied to this function's sibling. The broad
#     count spans every depth-carrying kind, so on the reference island it is 18
#     against 11 while all eleven subscriptions declare, and refusing on it keeps
#     that image on the worst case forever for endpoints this number does not
#     price. `subs_arena` was moved off the broad count for exactly that; this
#     was the one consumer left on it.
#   * a publisher can now DECLARE a depth (phase-454 W2), so the broad count is
#     something a publisher's contract can move. Guarding a subscription number
#     on it would mean a publisher declaration flips a subscription lane -- a
#     coupling in the one direction this wave exists to rule out, since nothing
#     prices a publisher's depth yet.
#
# Measured: no in-tree image moves. Every in-tree contract leaves at least one
# subscription silent (`demo_bringup`'s says `sub: { chatter: {} }`), so both
# spellings of the guard return early on all of them. And where the new guard
# DOES pass, `subs_arena`'s own guard passes with it -- it is the same predicate
# -- so `NROS_DECLARED_MAX_QOS_DEPTH` only reaches `pubsub_entry`, which is that
# function's FALLBACK price and unused on the branch that took it.
function(_nros_qos_depth_env _out_var)
    set(${_out_var} "" PARENT_SCOPE)
    if(NOT COMMAND nros_entity_inventory_knobs_file)
        return()
    endif()
    nros_entity_inventory_knobs_file(_inv)
    if(NOT EXISTS "${_inv}")
        return()
    endif()
    include("${_inv}")
    if(NOT NROS_ENTITY_DECLARED_DEPTH_STATUS STREQUAL "resolved")
        return()
    endif()
    if(NOT DEFINED NROS_ENTITY_UNDECLARED_DEPTH_COUNT_SUBSCRIPTION
            OR NOT NROS_ENTITY_UNDECLARED_DEPTH_COUNT_SUBSCRIPTION EQUAL 0)
        return()
    endif()
    if(NOT DEFINED NROS_ENTITY_DECLARED_DEPTHS)
        return()
    endif()
    # `type|topic=depth` triples. The depth is what follows the LAST `=`, so a
    # topic containing one does not shift the field.
    set(_max 0)
    foreach(_triple IN LISTS NROS_ENTITY_DECLARED_DEPTHS)
        string(REGEX MATCH "=([0-9]+)$" _m "${_triple}")
        if(_m)
            if(CMAKE_MATCH_1 GREATER _max)
                set(_max "${CMAKE_MATCH_1}")
            endif()
        endif()
    endforeach()
    if(_max GREATER 0)
        set(${_out_var} "NROS_DECLARED_MAX_QOS_DEPTH=${_max}" PARENT_SCOPE)
    endif()
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
    if(NOT NROS_PARAM_DECLARATION_STATUS STREQUAL "declared")
        return()
    endif()
    # Both names written IN FULL, for the reason `_nros_entity_budget_env`
    # gives: an interpolated name resolves EMPTY and is invisible to grep.
    set(_out "")
    foreach(_pair
            "NROS_DECLARED_MAX_PARAMETERS;NROS_DERIVED_MAX_PARAMETERS"
            "NROS_DECLARED_MAX_PARAM_NAME_LEN;NROS_DERIVED_MAX_PARAM_NAME_LEN"
            "NROS_DECLARED_MAX_STRING_VALUE_LEN;NROS_DERIVED_MAX_STRING_VALUE_LEN"
            "NROS_DECLARED_MAX_ARRAY_LEN;NROS_DERIVED_MAX_ARRAY_LEN"
            "NROS_DECLARED_MAX_BYTE_ARRAY_LEN;NROS_DERIVED_MAX_BYTE_ARRAY_LEN"
            "NROS_DECLARED_PARAM_NEEDS_MAX_STRING_VALUE_LEN;NROS_PARAM_NEEDS_MAX_STRING_VALUE_LEN"
            "NROS_DECLARED_PARAM_NEEDS_MAX_ARRAY_LEN;NROS_PARAM_NEEDS_MAX_ARRAY_LEN"
            "NROS_DECLARED_PARAM_NEEDS_MAX_BYTE_ARRAY_LEN;NROS_PARAM_NEEDS_MAX_BYTE_ARRAY_LEN"
            # phase-446 F3 -- the parameter services' shape; nros-node's
            # build script bounds the service buffer from it.
            "NROS_DECLARED_PARAM_SERVICE_SHAPE;NROS_PARAM_SERVICE_SHAPE")
        list(GET _pair 0 _name)
        list(GET _pair 1 _src)
        if(DEFINED ${_src})
            list(APPEND _out "${_name}=${${_src}}")
        endif()
    endforeach()
    set(${_out_var} "${_out}" PARENT_SCOPE)
endfunction()

# nros_entity_facts_env(<target>)
#
# Attach this configure's accumulated entity facts to a Corrosion target's cargo
# invocation. Called once, after every entry has been processed.
function(nros_entity_facts_env _target)
    # issue 1122 — the payload fact is INDEPENDENT of the entity facts below.
    # An image with no LAUNCH entry still links interface packages and still
    # gets a message-bound derivation, so this is computed before the
    # queryable-table early return rather than after it.
    _nros_payload_facts_env(_payload_env)
    _nros_entity_budget_env(_budget_env)
    _nros_take_buffer_env(_take_buf_env)
    if(_take_buf_env)
        list(APPEND _payload_env "${_take_buf_env}")
    endif()
    if(_budget_env)
        list(APPEND _payload_env ${_budget_env})
    endif()

    _nros_qos_depth_env(_depth_env)
    if(_depth_env)
        list(APPEND _payload_env "${_depth_env}")
    endif()

    # phase-446 W4 -- the parameter store, from the contract's `params:`.
    _nros_param_store_env(_param_env)
    if(_param_env)
        list(APPEND _payload_env ${_param_env})
        message(STATUS
            "nano-ros: parameter store sized from the contract -- "
            "${_param_env} (phase-446 W4)")
    endif()

    get_property(_seen GLOBAL PROPERTY NROS_ENTITY_FACTS_SEEN)
    if(NOT _seen)
        # Not a warning: a pure-C workspace with no LAUNCH entry, or a
        # configure whose models are not resolved yet, has always sized the
        # table from the backend's own default and still does. The payload
        # fact still travels, when there is one.
        if(_payload_env)
            if(NOT COMMAND corrosion_set_env_vars)
                message(FATAL_ERROR
                    "nros_entity_facts_env(${_target}): Corrosion not loaded")
            endif()
            nros_corrosion_env_target("${_target}" _target)
            corrosion_set_env_vars(${_target} ${_payload_env})
            message(STATUS
                "nano-ros: large-payload class sized from the declaration — "
                "${_payload_env} (issue 1122)")
        endif()
        return()
    endif()

    get_property(_param GLOBAL PROPERTY NROS_ENTITY_INFRA_PARAM)
    get_property(_lc GLOBAL PROPERTY NROS_ENTITY_INFRA_LIFECYCLE)
    if(_param AND _lc)
        set(_infra "param+lifecycle")
    elseif(_param)
        set(_infra "param")
    elseif(_lc)
        set(_infra "lifecycle")
    else()
        set(_infra "none")
    endif()
    set(_env "NROS_DECLARED_INFRA_QUERYABLES=${_infra}")

    # phase-426 W3 -- how many nodes claim a set of parameter services. Sent
    # unconditionally when known, even with `_infra` "none": the consumer owns
    # what a node COSTS (`PARAM_SERVICE_QUERYABLES`, beside the code that
    # creates the servers), and this side only states the count. Absent means
    # undeclared, which the consumer reads as one -- the pre-W3 number.
    get_property(_nodes GLOBAL PROPERTY NROS_ENTITY_NODES_MAX)
    if(NOT _nodes STREQUAL "")
        list(APPEND _env "NROS_DECLARED_NODES=${_nodes}")
    endif()

    get_property(_unknown GLOBAL PROPERTY NROS_ENTITY_SERVERS_UNKNOWN)
    get_property(_max GLOBAL PROPERTY NROS_ENTITY_SERVERS_MAX)
    if(NOT _unknown AND NOT _max STREQUAL "")
        list(APPEND _env "NROS_DECLARED_SERVICE_SERVERS=${_max}")
        set(_app "${_max} declared service server(s)")
    else()
        # issue 0973 — say what a reader can DO about it. "No model here
        # describes wiring" is true and unactionable: it reads as a resolver
        # fault, and three consumers were written against it on that reading.
        # Endpoint wiring is AUTHORED, so the line names the file that would
        # answer the question. One spelling: this is the existing status line
        # extended, not a second diagnostic beside it.
        set(_app "application count undeclared — no model here describes wiring")
        string(APPEND _app "; to declare it, author")
        string(APPEND _app " <bringup>/launch/<stem>.contract.yaml beside")
        string(APPEND _app " <stem>.launch.xml (RFC-0060)")
    endif()

    if(NOT COMMAND corrosion_set_env_vars)
        message(FATAL_ERROR "nros_entity_facts_env(${_target}): Corrosion not loaded")
    endif()
    # issue 0657 — attach to the target the cargo command actually READS.
    nros_corrosion_env_target("${_target}" _target)
    if(_payload_env)
        list(APPEND _env "${_payload_env}")
    endif()
    corrosion_set_env_vars(${_target} ${_env})
    message(STATUS
        "nano-ros: queryable table sized from the declaration — "
        "infrastructure ${_infra}, ${_app} (phase-392 W5)")
    if(_payload_env)
        message(STATUS
            "nano-ros: large-payload class sized from the declaration — "
            "${_payload_env} (issue 1122)")
    endif()
endfunction()
