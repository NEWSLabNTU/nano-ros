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

# _nros_sizing_target_args(<out_var>) — phase-457 W4
#
# The `[target]` arguments for a `ws sizing-descriptor` invocation: either
# `--host-build`, or `--target-triple <t>`, or nothing.
#
# ONE SPELLING, called by both producers. `from_model` and `from_leaf` each built
# `--host-build` themselves, identically, and each omitted the triple — so a
# CROSS cmake image got a REFUSED `pointer_bytes`/`max_align` on both roads, and
# `storage_bytes` with them (a receive region is sized from the pointer width).
# Fixing one would have left the other, which is issue 1513's shape and #282's
# before it: a second copy of a rule is how a class fix stops being one.
#
# `--host-build` answers only the NATIVE case — the target IS this process, which
# is RFC-0100 D1's single exemption to "build scripts run for the host"
# (phase-118-E). For a cross configure the triple is resolvable right here, and
# `_nros_resolve_rust_target()` is the one way to ask: never `Rust_CARGO_TARGET`,
# a normal var that does not survive `add_subdirectory()` (phase-155's wrong-arch
# link).
#
# Emits NOTHING when cross-compiling and the resolver has no answer, so the
# writer REFUSES rather than guessing. A guessed pointer width under-sizes a
# ring's length array, and the CLI's own `size_of` would answer for the host.
function(_nros_sizing_target_args _out_var)
    set(_args "")
    if(NOT CMAKE_CROSSCOMPILING)
        set(_args --host-build)
    elseif(COMMAND _nros_resolve_rust_target)
        _nros_resolve_rust_target(_nsta_triple)
        if(_nsta_triple)
            set(_args --target-triple "${_nsta_triple}")
        endif()
    endif()
    # SAY which branch was taken. A silent no-op here is the "green that never
    # ran" shape: the `COMMAND` guard above means a configure that never included
    # `NanoRosCodegenCore` would quietly keep refusing `[target]` and look exactly
    # like one that resolved it. The printed line is the only evidence, the same
    # reason the Corrosion resolver prints its origin (issue 0500).
    if(_args)
        message(STATUS "nano-ros: sizing descriptor [target] via ${_args}")
    else()
        message(STATUS
            "nano-ros: sizing descriptor [target] REFUSED -- cross-compiling and "
            "no rustc triple resolvable here, so `pointer_bytes` is not guessed")
    endif()
    set(${_out_var} "${_args}" PARENT_SCOPE)
endfunction()

# _nros_sizing_bound_args(<out_var>) — phase-457-payload W2
#
# The `--bound-inventory` arguments for a `ws sizing-descriptor` invocation: one
# per bound table this configure's interface closure REGISTERED.
#
# EXPORTED, not re-derived. Codegen already walked every type and wrote
# `nros_message_bounds.json` beside each `nros_message_bounds.cmake` fragment,
# and `nros_message_bounds_register_fragment` already collects those fragments
# image-wide for the message-bound aggregator. This reads that SAME list, so the
# descriptor and the aggregator cannot disagree about which packages are in the
# closure. The JSON sibling is named by `nros_message_bounds_files()`, the one
# function that owns both file names, never spelled here.
#
# ONE SPELLING, called by both producers, for the reason
# `_nros_sizing_target_args` gives above.
#
# EVERY registered table is passed, present or not. On the non-Zephyr cmake lane
# a table is a BUILD-time output, so on a clean tree it is absent at the first
# configure; the CLI then REFUSES the bound fields naming that table's package,
# which is the aggregator's rule for the same list ("a promise rather than a
# fact"). Only a table that EXISTS is added to `CMAKE_CONFIGURE_DEPENDS`: a ninja
# input with no rule to make it is a hard `missing and no known rule` at LOAD.
function(_nros_sizing_bound_args _out_var)
    set(_args "")
    set(_registered 0)
    set(_present 0)
    if(COMMAND nros_message_bounds_fragments AND COMMAND nros_message_bounds_files)
        nros_message_bounds_fragments(_nsba_frags)
        foreach(_nsba_frag IN LISTS _nsba_frags)
            get_filename_component(_nsba_dir "${_nsba_frag}" DIRECTORY)
            nros_message_bounds_files("${_nsba_dir}" _nsba_json _nsba_unused)
            list(APPEND _args --bound-inventory "${_nsba_json}")
            math(EXPR _registered "${_registered} + 1")
            if(EXISTS "${_nsba_json}")
                math(EXPR _present "${_present} + 1")
                set_property(DIRECTORY APPEND PROPERTY
                    CMAKE_CONFIGURE_DEPENDS "${_nsba_json}")
            endif()
        endforeach()
    endif()
    # SAY which branch was taken -- the same reason as `[target]` above. A
    # configure that registered no table and one whose tables were all read
    # otherwise look identical, and only one of them states a bound.
    if(_registered EQUAL 0)
        message(STATUS
            "nano-ros: sizing descriptor bounds REFUSED -- this configure registered "
            "no message-bound table, so `wire_bound_bytes` and `[types]` are not stated")
    elseif(_present LESS _registered)
        message(STATUS
            "nano-ros: sizing descriptor bounds REFUSED -- ${_present} of ${_registered} "
            "registered bound table(s) exist; the rest are built by the first build and "
            "read from the next configure")
    else()
        message(STATUS
            "nano-ros: sizing descriptor bounds from ${_registered} registered table(s)")
    endif()
    set(${_out_var} "${_args}" PARENT_SCOPE)
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
# What the model-only producer may CLAIM is settled by RFC-0100 D6: STATE what
# the inputs support, REFUSE per field on an input that is missing.
# phase-454 W14 refused `wire_bound_bytes`, `storage_bytes`, `[types]`'s three
# maxima and `registration_path` outright on this road (issue 1393). That issue
# is CLOSED: phase-457-payload W2 hands this producer the bound tables the
# closure registered (`_nros_sizing_bound_args`), phase-457 W4 the triple
# (`_nros_sizing_target_args`), and the producer composes each field with the
# leaf road's own code. What still refuses is per input: a table not built yet
# (named), and an in-place backend's subscription path wherever no FRESH probe
# sidecar in `WORKSPACE` could be attributed to it by the contract join
# (issue 1594). `Fact::stated()` is the only accessor
# that yields a value, so a consumer cannot read a refusal as a default.
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
# * It does not GUESS a target. `_nros_sizing_target_args` passes
#   `--host-build` for a native configure and the resolved rustc triple for a
#   cross one (phase-457 W4); a cross configure with no resolvable triple gets a
#   REFUSED `[target]`, naming the board rule (RFC-0100 D1), and says so.
function(nros_sizing_descriptor_from_model _out_var)
    cmake_parse_arguments(_nsw "" "CLI;MODEL;ENTRY;BUILD_DIR;RMW;METADATA;WORKSPACE" "" ${ARGN})
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

    _nros_sizing_target_args(_host_arg)
    set(_rmw_arg "")
    if(_nsw_RMW)
        set(_rmw_arg --rmw "${_nsw_RMW}")
    endif()

    # phase-457 W0 (issue 1407) — the COMPONENT METADATA, composed with the
    # contract by the same `merged_per_kind_max` `nros ws entity-inventory`
    # uses. Without it this producer derived over the contract's nodes alone,
    # while the `NROS_DECLARED_*` carriers beside it derived over every
    # REGISTERED component — two numbers for one image, the descriptor's the
    # smaller, and a short `NROS_EXECUTOR_MAX_NODES` is `NodeTableFull` at boot.
    #
    # The path comes from the function that owns it, never spelled here: the
    # emitter (`_nros_metadata_emit`) and every reader must agree on one
    # location. ABSENT is a normal state — a configure where nothing called
    # `nano_ros_node_register()` has no such file, and the contract's set is
    # then the whole truth — so the flag is only passed when the file exists.
    # Passing a path that is not there would make the CLI fail the configure for
    # the most ordinary shape in the tree.
    set(_meta_arg "")
    set(_meta "${_nsw_METADATA}")
    if(NOT _meta AND COMMAND nros_entity_inventory_metadata_file)
        nros_entity_inventory_metadata_file(_meta)
    endif()
    if(_meta AND EXISTS "${_meta}")
        set(_meta_arg --metadata "${_meta}")
        # Issue 1018 again: a configure-time emitter's inputs reduce to "does a
        # configure happen", and this is now one of them.
        set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_meta}")
    endif()

    _nros_sizing_bound_args(_bound_args)

    # Issue 1594 -- the WORKSPACE whose metadata-probe sidecars carry each
    # subscription's REGISTRATION observation (`in_place`). Without it every
    # subscription on an in-place backend (zenoh, XRCE) refuses its
    # `registration_path` and keeps a receive region. The CLI joins a FRESH
    # sidecar's observation onto the model's rows by the contract join's own rule
    # and prints every sidecar it read as an `input <path>` line, registered below
    # as a configure dependency (issue 1018). A root that is not a workspace is
    # reported by the CLI and costs nothing but the observation.
    set(_ws_arg "")
    if(_nsw_WORKSPACE AND IS_DIRECTORY "${_nsw_WORKSPACE}/src")
        set(_ws_arg --workspace "${_nsw_WORKSPACE}")
    endif()

    execute_process(
        COMMAND "${_nsw_CLI}" ws sizing-descriptor
                --from-model "${_nsw_MODEL}"
                --build-dir "${_build_dir}"
                --entry "${_nsw_ENTRY}"
                --road "a cmake entry"
                ${_host_arg} ${_rmw_arg} ${_meta_arg} ${_ws_arg} ${_bound_args}
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
    # Line 1 is the descriptor; each later `input <path>` line is a probe sidecar
    # the observation join read (issue 1594).
    string(REPLACE "\n" ";" _lines "${_out}")
    list(GET _lines 0 _out)
    foreach(_line IN LISTS _lines)
        if(_line MATCHES "^input (.+)$")
            set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${CMAKE_MATCH_1}")
        endif()
    endforeach()
    set(${_out_var} "${_out}" PARENT_SCOPE)
    set_property(GLOBAL APPEND PROPERTY NROS_SIZING_DESCRIPTOR_PATHS "${_out}")
endfunction()

# nros_sizing_descriptor_from_leaf(<out_var>) — phase-457 W0.b, issues 1407/1378
#
# WRITE a descriptor for a STANDALONE LEAF, from its own `system.toml`
# `[[component]] entities`.
#
# ## The road this exists for
#
# A copy-out cmake project has no bringup, no launch file and no SystemModel, so
# `nros_sizing_descriptor_from_model` above can never reach it — and it is the
# road issue 1378 measured FAILING. `examples/qemu-armv7a-nuttx/{c,cpp}/action-server`
# declare one action server; the queryable table was sized to its three services
# and the action server's own TRANSIENT_LOCAL `/status` publisher was the FOURTH
# slot, so `nros_executor_add_action_server` returned -1 at boot. The cargo-leaf
# road never saw it because `nros sync` writes that road a descriptor whose
# `action_server` row is counted.
#
# So this is the third producer, and it states what a DECLARATION can: the
# counts, the endpoint table, and every QoS policy the declaration spells. The
# payload class stays REFUSED — a C or C++ leaf has no `generated/` bound table
# either — and the refusal names the leaf's own `system.toml` rather than a
# SystemModel this road does not have.
#
# ## Soft on every absence, exactly like `nros_record_leaf_entity_facts`
#
# No `system.toml`, no CLI, a leaf that declares nothing, a leaf that is a CARGO
# leaf (whose descriptor is `nros sync`'s, with strictly more inputs) — each
# means "this configure has no leaf descriptor to carry", which is the state
# every standalone leaf was already in. None is a configuration error, so none
# is fatal.
function(nros_sizing_descriptor_from_leaf _out_var)
    cmake_parse_arguments(_nsl "" "CLI;LEAF;ENTRY;BUILD_DIR" "" ${ARGN})
    set(${_out_var} "" PARENT_SCOPE)

    if(NOT _nsl_LEAF OR NOT EXISTS "${_nsl_LEAF}/system.toml")
        return()
    endif()
    if(NOT _nsl_ENTRY OR NOT _nsl_CLI OR NOT EXISTS "${_nsl_CLI}")
        return()
    endif()
    set(_build_dir "${_nsl_BUILD_DIR}")
    if(NOT _build_dir)
        set(_build_dir "${CMAKE_BINARY_DIR}")
    endif()

    # Issue 1018 — a configure-time emitter has no `DEPENDS`, so the freshness of
    # what it writes reduces to "does a configure happen". The declaration is the
    # input; the CLI half is registered by `nros_sizing_descriptor_read()`.
    set_property(DIRECTORY APPEND PROPERTY
        CMAKE_CONFIGURE_DEPENDS "${_nsl_LEAF}/system.toml")

    _nros_sizing_target_args(_host_arg)
    _nros_sizing_bound_args(_bound_args)

    execute_process(
        COMMAND "${_nsl_CLI}" ws sizing-descriptor
                --from-leaf "${_nsl_LEAF}"
                --build-dir "${_build_dir}"
                --entry "${_nsl_ENTRY}"
                --road "a standalone cmake leaf"
                ${_host_arg} ${_bound_args}
        OUTPUT_VARIABLE _out
        ERROR_VARIABLE _err
        RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)
    if(NOT _rc EQUAL 0)
        string(REGEX REPLACE "\n+" " " _why "${_err}")
        string(SUBSTRING "${_why}" 0 200 _why)
        message(STATUS
            "nano-ros: no sizing descriptor written for leaf `${_nsl_LEAF}` -- ${_why}. "
            "Every consumer keeps its own default sizes (RFC-0100 D6).")
        return()
    endif()
    if(_out STREQUAL "")
        # The leaf declares nothing, or is a cargo leaf. The CLI said which.
        return()
    endif()
    set(${_out_var} "${_out}" PARENT_SCOPE)
    # A property of its OWN, not `NROS_SIZING_DESCRIPTOR_PATHS`. The entry road's
    # "exactly one or none" rule is a DECISION about a shared staticlib
    # (phase-457 W0.c re-affirmed it), and appending a leaf path to that list
    # would turn a leaf that also declares an entry into "two descriptors,
    # therefore none" — silently withdrawing a fact the carrier still delivers.
    # Measured 2026-09-27: no `nano_ros_entry(` call site in the tree has a
    # `system.toml` at all, so the collision does not occur today; keeping the
    # lists apart makes that a property of the code rather than of the survey.
    set_property(GLOBAL APPEND PROPERTY NROS_SIZING_DESCRIPTOR_LEAF_PATHS "${_out}")
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
# image and call it derived. The entity facts reduce across models for the
# same collision -- the `ws entity-facts` accumulator takes a MAX, and the
# entity-inventory fragment folds every entry's model into the union the shared
# runtime must hold (issue 1600; until then it was last-entry-wins, which this
# comment claimed it was not). A descriptor is a whole per-endpoint table and
# has no such reduction, so this refuses instead and says so.
function(nros_sizing_descriptor_cargo_env _out_var)
    set(${_out_var} "" PARENT_SCOPE)
    get_property(_paths GLOBAL PROPERTY NROS_SIZING_DESCRIPTOR_PATHS)
    if(NOT _paths)
        # phase-457 W0.b — the STANDALONE LEAF road, BELOW the entry road and
        # never beside it. An entry's descriptor is derived from the resolved
        # model of the image about to be built; a leaf declaration is what
        # answers when there is no entry at all. Ranked rather than merged, so
        # the entry road's one-or-none rule keeps deciding on its own terms.
        get_property(_paths GLOBAL PROPERTY NROS_SIZING_DESCRIPTOR_LEAF_PATHS)
    endif()
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

# nros_sizing_descriptor_west_fragment(<out_var>) — issue 1407
#
# Where a Zephyr west configure records the descriptor it names to cargo, so
# that the module's knob resolver — which runs during `find_package(Zephyr)`,
# long before `nano_ros_entry()` writes the descriptor — can read it. One
# spelling for the writer and the reader.
function(nros_sizing_descriptor_west_fragment _out_var)
    set(${_out_var} "${CMAKE_BINARY_DIR}/nros/sizing/west-cargo-descriptor.cmake" PARENT_SCOPE)
endfunction()

# nros_sizing_descriptor_record_for_west() — issue 1407
#
# The Zephyr WEST road's half of `nros_sizing_descriptor_cargo_env()` above.
#
# That function hands the descriptor to cargo as a row on the emitted Corrosion
# command, and a west build emits no such command: its cargo builds are created
# by `zephyr/cmake/nros_cargo_build.cmake`, whose `nros_resolve_knobs()` has
# already run by the time any entry is configured. So the west road had a
# descriptor in its build dir and named none to cargo, while forwarding the
# `NROS_DECLARED_*` carriers the descriptor was meant to replace.
#
# The same producer-after-reader shape the entity inventory has (issue 0991),
# closed the same way: write the ONE-OR-NONE decision (the same rule, the same
# function) to a fragment, and arm a re-configure when it changes, so the
# resolver of the next pass — inside the same `west build` — puts
# `NROS_SIZING_DESCRIPTOR` into `NROS_RESOLVED_KNOBS`. From there it rides the
# C lane's command exactly like every other resolved knob (issue 0460: never a
# `set(ENV{})` on its own). It is a PATH, and nothing watches the variable's
# text (issue 0491): `load_for_build_script` puts the edge on the file.
#
# Called on every entry; the last call of a pass sees every entry's descriptor,
# so a multi-entry configure lands on "none" exactly as the cmake road does.
function(nros_sizing_descriptor_record_for_west)
    nros_sizing_descriptor_west_fragment(_frag)
    nros_sizing_descriptor_cargo_env(_row)
    string(REGEX REPLACE "^NROS_SIZING_DESCRIPTOR=" "" _path "${_row}")
    set(_body
        "# GENERATED by nano_ros_entry() (issue 1407) -- do not edit.\n"
        "# The sizing descriptor this west configure names to cargo (empty = none).\n"
        "set(NROS_SIZING_DESCRIPTOR_FOR_CARGO \"${_path}\")\n")
    string(CONCAT _body ${_body})
    set(_old "")
    if(EXISTS "${_frag}")
        file(READ "${_frag}" _old)
    endif()
    if(COMMAND nros_reconfigure_snapshot)
        nros_reconfigure_snapshot("${_frag}" _before)
    endif()
    if(NOT _old STREQUAL _body)
        file(WRITE "${_frag}" "${_body}")
    endif()
    if(COMMAND nros_reconfigure_on_change)
        nros_reconfigure_on_change("${_frag}" "${_before}"
            LABEL "the sizing descriptor this image names to cargo")
    endif()
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
