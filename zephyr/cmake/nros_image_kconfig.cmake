# nros_image_kconfig.cmake — phase-481 W1 (RFC-0098 D10/D11): an image's
# configuration, stated once in `system.toml`, rendered into the Zephyr build.
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Reached from `zephyr/modules/modules.cmake` (the module's `module_ext_root`
# hook), inside `find_package(Zephyr)`, before Kconfig runs. Three deliveries:
#
# 1. KCONFIG. `nros ws leaf-system --kconfig-out` renders the image's RMW
#    choice, the package's language API, the deploy endpoint and every
#    `[image.<id>] env` row that has a Kconfig symbol into
#    `<build>/nros/<image>.conf`, and this puts that file LAST in the CACHE value
#    of `EXTRA_CONF_FILE`.
#
#    Why the cache, and why last. Zephyr reads the variable with
#    `zephyr_get(EXTRA_CONF_FILE … MERGE REVERSE)`, which concatenates the
#    scopes local, ENV, snippets, cache (then sysbuild) and keeps the LAST copy
#    of a duplicate. A value appended to the LOCAL variable therefore merges
#    BEFORE a command-line `-DEXTRA_CONF_FILE` — phase-481 W0 measured exactly
#    that on the workspace road, where `nros build` passes the image's own
#    `prj-zenoh.conf` that way, and it merged after the fragment. Last in the
#    cache is last of every conf FILE, so the fragment beats the leaf's
#    `prj.conf`/`prj-<rmw>.conf`, the image's `os_config` files and a snippet, and
#    still loses to the per-run `-DCONFIG_*` values a harness passes (those
#    merge after every file). The fragment sits in a SUBDIRECTORY because a
#    `*.conf` at the top of the build dir merges after `-DCONFIG_*` too.
#
#    The user's own cache entries are kept, in order; only an earlier rendering
#    of ours (anything under `<build>/nros/` ending `.conf`) is dropped first, so
#    a re-configure, an image switch, or a leaf that stopped stating anything
#    leaves no stale entry. The LOCAL variable is untouched: this runs in a
#    function, where `set(CACHE … FORCE)` cannot unbind the caller's copy.
#
# 2. CARGO. Rows no Kconfig symbol carries go to issue 1712's unforced
#    `--config` `[env]` file (`<build>/nros/<image>-env.toml`), recorded as the
#    GLOBAL property `NROS_IMAGE_ENV_CONFIG` that `NanoRosImageEnv.cmake` reads
#    (so `nros_cargo_build()` passes it) and as the cache variable
#    `NROS_IMAGE_ENV_CARGO_FLAGS`, which the patched zephyr-lang-rust
#    `rust_cargo_application` puts on the Rust lane's cargo command
#    (`scripts/zephyr/cargo-features-patch.sh`).
#
# 3. THE KNOB LADDER. A `cmake -E env K=V` row OVERWRITES an inherited value, so
#    a knob the Zephyr cmake lane forwards itself (a derived inventory value,
#    say) would outrank the unforced `[env]` row. Each such row is therefore
#    also recorded as `NROS_IMAGE_ENV_ROW_<KEY>` (keys in `NROS_IMAGE_ENV_ROWS`)
#    and `_nros_resolve_knob` resolves it as rung `image`: below an exported
#    variable, above a derived or default value — the same number cargo and the
#    C defines read (issue 0135).
#
# 4. THE IMAGE'S OWN FRAGMENTS (phase-481 W3). A single-package leaf's image
#    names the Zephyr-native fragment its RMW needs (`conf = ["prj-xrce.conf"]`:
#    TCP, POSIX, heap sizing -- facts about Zephyr, not nano-ros). They join the
#    cache value AHEAD of the caller's own `EXTRA_CONF_FILE` entries and the
#    rendering, so a plain `west build -- -DNROS_IMAGE=zephyr_xrce` gets them
#    with no `CONF_FILE` to assemble, a board tail passed as an extra still
#    overrides them, and a fragment the caller already names is left where it
#    is. A bringup's `os_config` is `nros build`'s to pass (RFC-0085 D4), never added
#    here.
#
# 5. THE KNOBS (phase-484 W5b, RFC-0103 D2a/D2b). The module's knob section
#    defines every registry knob WITHOUT a prompt and `osource`s
#    `<build>/nros/Kconfig.from-system-toml` first; this hook always writes it.
#    For a project with a `system.toml`, `--kconfig-defaults-out` renders the
#    image's knob values (and every `-DNROS_OVERRIDE_<ID>=<v>`, the transient
#    channel) as prompt-less `default`s, and those rows leave the fragment.
#    With no prompt anywhere a `prj.conf` knob line or a `-DCONFIG_<knob>` is a
#    Kconfig error -- refused here first, because Kconfig's own message names
#    only the module's definition (M3), never system.toml. For a GUEST (no
#    system.toml) the file is a copy of `zephyr/Kconfig.knob-prompts`, which
#    gives every knob back its prompt: prj.conf, menuconfig and `-DCONFIG_*`
#    work exactly as before.
#
# For an application with no `system.toml` the hook writes the guest prompts
# and drops a stale rendering of its own from the cache; nothing else.
#
# WHICH system.toml, and which image:
#   * a single-package leaf: `${APPLICATION_SOURCE_DIR}/system.toml`;
#     `-DNROS_IMAGE=<id>` picks one of several images (the leaf's own rule —
#     one image, or `[system] default_images` — applies otherwise);
#   * a generated workspace application: it states `NROS_IMAGE_SYSTEM_DIR` (the
#     bringup), `NROS_IMAGE` and `NROS_IMAGE_LANGUAGE` before
#     `find_package(Zephyr)` (`nros build`, `builder/west_app.rs`).

include("${CMAKE_CURRENT_LIST_DIR}/../../cmake/NanoRosCli.cmake")
# Issue 2200 -- reads the CLI's rows without passing them through a list.
include("${CMAKE_CURRENT_LIST_DIR}/../../cmake/NanoRosCliRows.cmake")
# phase-484 W5b -- the Kconfig knob ids and symbols (`NROS_KNOB_KCONFIG_*`).
include("${CMAKE_CURRENT_LIST_DIR}/../../cmake/NanoRosKnobRegistry.cmake")
# Issue 2086 -- `nros_leaf_census_take`.
include("${CMAKE_CURRENT_LIST_DIR}/../../cmake/NanoRosLeafCensus.cmake")

# The module root, captured at FILE scope (inside a function
# `CMAKE_CURRENT_LIST_DIR` names the caller's file).
get_filename_component(_NROS_IMAGE_KCONFIG_ROOT "${CMAKE_CURRENT_LIST_DIR}/../.." ABSOLUTE)
set(_NROS_IMAGE_KCONFIG_ROOT "${_NROS_IMAGE_KCONFIG_ROOT}" CACHE INTERNAL
    "the nano-ros module root the module_ext_root hook renders against (phase-481 W1)")

# phase-484 W8 — the spelling a project writes a knob in, for a refusal to name:
# `[image.<image>.knobs.<table>] <name> = <value>`. A Kconfig bool's `y`/`n` is
# TOML's `true`/`false`; anything else is carried as written.
function(_nros_image_knob_spelling _out _image _id _val)
    set(_v "${_val}")
    if(_v STREQUAL "y")
        set(_v "true")
    elseif(_v STREQUAL "n")
        set(_v "false")
    endif()
    set(${_out}
        "`[image.${_image}.knobs.${NROS_KNOB_IMAGE_TABLE_${_id}}] ${NROS_KNOB_IMAGE_NAME_${_id}} = ${_v}`"
        PARENT_SCOPE)
endfunction()

function(nros_image_kconfig_hook)
    if(DEFINED APPLICATION_BINARY_DIR)
        set(_bin "${APPLICATION_BINARY_DIR}")
    else()
        set(_bin "${CMAKE_BINARY_DIR}")
    endif()
    set(_nros_dir "${_bin}/nros")

    # phase-484 W5b -- the knob file the module's Kconfig `osource`s, at the
    # path Kconfig spells it: `$(KCONFIG_BINARY_DIR)/../nros/…`, where
    # `kconfig.cmake` defaults KCONFIG_BINARY_DIR to `<this binary dir>/Kconfig`
    # on 3.7 and 4.4 (it has not been included yet; a caller may preset it).
    if(DEFINED KCONFIG_BINARY_DIR AND NOT KCONFIG_BINARY_DIR STREQUAL "")
        set(_kbin "${KCONFIG_BINARY_DIR}")
    else()
        set(_kbin "${CMAKE_CURRENT_BINARY_DIR}/Kconfig")
    endif()
    get_filename_component(_kgen "${_kbin}/../nros/Kconfig.from-system-toml" ABSOLUTE)
    # Every `-DNROS_OVERRIDE_<ID>=<v>` on the configure line (cache entries).
    get_cmake_property(_cache_vars CACHE_VARIABLES)
    set(_overrides "")
    foreach(_v IN LISTS _cache_vars)
        if(_v MATCHES "^NROS_OVERRIDE_(.+)$")
            list(APPEND _overrides "${CMAKE_MATCH_1}")
        endif()
    endforeach()

    # Drop an earlier rendering of ours from the cache value, whatever happens
    # below. A path that no longer exists would otherwise fail Kconfig.
    # phase-481 W3 -- and the image's own `os_config` fragments an earlier configure
    # inserted (recorded in NROS_IMAGE_CONF_FILES), so an image switch leaves
    # none of the previous image's behind.
    set(_user "")
    set(_had_ours FALSE)
    if(DEFINED CACHE{EXTRA_CONF_FILE})
        set(_cached "$CACHE{EXTRA_CONF_FILE}")
        foreach(_f IN LISTS _cached)
            get_filename_component(_fdir "${_f}" DIRECTORY)
            if(_fdir STREQUAL _nros_dir AND _f MATCHES "\\.conf$")
                set(_had_ours TRUE)
            elseif(_f IN_LIST NROS_IMAGE_CONF_FILES)
                set(_had_ours TRUE)
            else()
                list(APPEND _user "${_f}")
            endif()
        endforeach()
    endif()

    # Clear what an earlier configure recorded for the ladder and the cargo lanes.
    foreach(_k IN LISTS NROS_IMAGE_ENV_ROWS)
        unset(NROS_IMAGE_ENV_ROW_${_k} CACHE)
    endforeach()
    set(NROS_IMAGE_ENV_ROWS "" CACHE INTERNAL
        "phase-481 W1: [image.<id>] env rows with no Kconfig symbol")
    set(NROS_IMAGE_ENV_CARGO_FLAGS "" CACHE INTERNAL
        "phase-481 W1: the image env --config flag for the Rust lane's cargo")
    set_property(GLOBAL PROPERTY NROS_IMAGE_ENV_CONFIG "")

    set(_args "")
    if(DEFINED NROS_IMAGE_SYSTEM_DIR AND NOT NROS_IMAGE_SYSTEM_DIR STREQUAL "")
        get_filename_component(_dir "${NROS_IMAGE_SYSTEM_DIR}" ABSOLUTE
            BASE_DIR "${APPLICATION_SOURCE_DIR}")
        if(NOT DEFINED NROS_IMAGE OR NROS_IMAGE STREQUAL "")
            message(FATAL_ERROR
                "nano-ros: NROS_IMAGE_SYSTEM_DIR=${NROS_IMAGE_SYSTEM_DIR} names a bringup "
                "but no NROS_IMAGE — a generated application states both (phase-481 W1)")
        endif()
    elseif(EXISTS "${APPLICATION_SOURCE_DIR}/system.toml")
        set(_dir "${APPLICATION_SOURCE_DIR}")
    else()
        set(NROS_IMAGE_CONF_FILES "" CACHE INTERNAL
            "phase-481 W3: the image conf fragments the module hook added to EXTRA_CONF_FILE")
        if(_had_ours)
            _nros_image_kconfig_set_cache("${_user}")
        endif()
        # phase-484 W5b -- a GUEST: every knob gets its prompt back, so its conf
        # files and `-DCONFIG_*` stay its one source. The override channel is a
        # system.toml project's; a guest already has -DCONFIG_*.
        if(_overrides)
            list(TRANSFORM _overrides PREPEND "-DNROS_OVERRIDE_")
            string(REPLACE ";" " " _overrides "${_overrides}")
            message(FATAL_ERROR
                "nano-ros: ${_overrides}: this application has no system.toml, so its knobs "
                "keep their Kconfig prompts -- set them in prj.conf or with -DCONFIG_<symbol> "
                "(-DNROS_OVERRIDE_<ID> overrides a system.toml image's value; phase-484 W5b)")
        endif()
        configure_file("${_NROS_IMAGE_KCONFIG_ROOT}/zephyr/Kconfig.knob-prompts" "${_kgen}"
            COPYONLY)
        # Issue 2157 -- a hand-written entry is a guest here, and its board's
        # snippets arrive as `-DNROS_BOARD_SNIPPETS` from `nros build`.
        _nros_image_kconfig_merge_snippets("" "")
        return()
    endif()
    if(NOT EXISTS "${_dir}/system.toml")
        message(FATAL_ERROR
            "nano-ros: NROS_IMAGE_SYSTEM_DIR=${_dir} holds no system.toml (phase-481 W1)")
    endif()
    if(DEFINED NROS_IMAGE_LANGUAGE AND NOT NROS_IMAGE_LANGUAGE STREQUAL "")
        list(APPEND _args --language "${NROS_IMAGE_LANGUAGE}")
    endif()

    nros_resolve_cli(_nros CONTEXT
        "nano-ros Zephyr module: rendering ${_dir}/system.toml (phase-481 W1)")

    # The image id names the files; ask for it when the caller did not choose.
    set(_image "")
    if(DEFINED NROS_IMAGE AND NOT NROS_IMAGE STREQUAL "")
        set(_image "${NROS_IMAGE}")
    else()
        execute_process(
            COMMAND "${_nros}" ws leaf-system "${_dir}"
                    --nano-ros-path "${_NROS_IMAGE_KCONFIG_ROOT}"
            OUTPUT_VARIABLE _out ERROR_VARIABLE _err RESULT_VARIABLE _rc
            OUTPUT_STRIP_TRAILING_WHITESPACE)
        if(NOT _rc EQUAL 0)
            message(FATAL_ERROR "nano-ros: ${_dir}/system.toml: ${_err}\n"
                "(a leaf with several images builds the one `-DNROS_IMAGE=<id>` names)")
        endif()
        if(_out MATCHES "(^|\n)NROS_LEAF_IMAGE=([^\n]*)")
            set(_image "${CMAKE_MATCH_2}")
        endif()
    endif()
    if(_image STREQUAL "")
        message(FATAL_ERROR "nano-ros: ${_dir}/system.toml: no image to render")
    endif()
    list(PREPEND _args --image "${_image}")
    # phase-524 W3 -- the board facts (`nros_resolve_board_facts`, which runs
    # later in this configure from zephyr/CMakeLists.txt) must ask for THIS
    # image's row. It reads `NROS_LEAF_IMAGE_ID`, which nothing on the Zephyr
    # leaf road set, so it asked with no `--deploy`: right while every image of
    # a leaf names one board, and an "images resolve DIFFERENTLY" configure
    # error the moment one names another (measured: a C talker leaf with a
    # `nucleo-f767zi-zephyr` image beside its `zephyr` ones, even with
    # `-DNROS_IMAGE` given). Re-set on every configure, so it never outlives
    # the image choice that made it.
    set(NROS_LEAF_IMAGE_ID "${_image}" CACHE INTERNAL
        "the image this Zephyr leaf configure renders (phase-524 W3)")

    # phase-484 W5b -- a `-DCONFIG_<knob>` would be Kconfig's "not directly
    # user-configurable" error, naming neither system.toml nor the override
    # channel (M3). Zephyr caches it as `CONFIG_<SYM>` on the configure that
    # passes it and as `CLI_CONFIG_<SYM>` on every re-configure after.
    set(_cli_knobs "")
    foreach(_id IN LISTS NROS_KNOB_KCONFIG_IDS)
        set(_sym "${NROS_KNOB_KCONFIG_SYMBOL_${_id}}")
        if(DEFINED CACHE{CONFIG_${_sym}} OR DEFINED CACHE{CLI_CONFIG_${_sym}})
            if(DEFINED CACHE{CONFIG_${_sym}})
                set(_val "$CACHE{CONFIG_${_sym}}")
            else()
                set(_val "$CACHE{CLI_CONFIG_${_sym}}")
            endif()
            _nros_image_knob_spelling(_spell "${_image}" "${_id}" "${_val}")
            string(APPEND _cli_knobs
                "\n  -DCONFIG_${_sym}=${_val}  ->  -DNROS_OVERRIDE_${_id}=${_val} for this "
                "build, or ${_spell} in system.toml")
        endif()
    endforeach()
    if(NOT _cli_knobs STREQUAL "")
        message(FATAL_ERROR
            "nano-ros: ${_dir}/system.toml is this image's one source of its nano-ros "
            "knobs, and those Kconfig symbols have no prompt (RFC-0103 D2a), so the "
            "command line cannot set them as CONFIG_*:${_cli_knobs}\n"
            "Zephyr re-applies a cached -DCONFIG_* on every later configure: remove it "
            "with -UCONFIG_<SYM> -UCLI_CONFIG_<SYM> (or a pristine build). (phase-484 W5b)")
    endif()
    set(_override_args "")
    foreach(_id IN LISTS _overrides)
        list(APPEND _override_args --override "${_id}=$CACHE{NROS_OVERRIDE_${_id}}")
    endforeach()
    if(_overrides)
        string(REPLACE ";" ", " _ov "${_overrides}")
        message(STATUS "nano-ros: image `${_image}` knob override(s) from the command line: "
            "${_ov} (transient; RFC-0103 D2)")
    endif()

    # Issue 2086 -- a leaf that takes its entities from its census takes it
    # HERE, before Kconfig: the rendering below states the parameter store's
    # axis from it (and so the nros heap's default), and the module's
    # `west-leaf-sizing` reads the same census for the pools after Kconfig.
    if(_dir STREQUAL APPLICATION_SOURCE_DIR)
        nros_leaf_census_take("${_nros}" "${_dir}")
    endif()

    # Issue 2179 -- the Zephyr board this build was given, so the image's
    # board package contributes its snippets and conf files only when it
    # describes that board. `boards.cmake` has not run yet (it follows the
    # module hooks), so read BOARD the way its `zephyr_check_cache(BOARD)`
    # will: the variable (local or cache), the environment, then the value a
    # previous configure cached.
    set(_west_board "")
    if(DEFINED BOARD AND NOT "${BOARD}" STREQUAL "")
        set(_west_board "${BOARD}")
    elseif(DEFINED ENV{BOARD} AND NOT "$ENV{BOARD}" STREQUAL "")
        set(_west_board "$ENV{BOARD}")
    elseif(DEFINED CACHE{CACHED_BOARD})
        set(_west_board "$CACHE{CACHED_BOARD}")
    endif()
    if(NOT _west_board STREQUAL "")
        list(APPEND _args --west-board "${_west_board}")
    endif()

    set(_frag "${_nros_dir}/${_image}.conf")
    set(_env "${_nros_dir}/${_image}-env.toml")
    execute_process(
        COMMAND "${_nros}" ws leaf-system "${_dir}"
                --nano-ros-path "${_NROS_IMAGE_KCONFIG_ROOT}"
                ${_args} --kconfig-out "${_frag}" --image-env-out "${_env}"
                --kconfig-defaults-out "${_kgen}" ${_override_args}
        OUTPUT_VARIABLE _out ERROR_VARIABLE _err RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)
    # A configure re-runs when the file changes, and when the CLI that rendered
    # it is rebuilt (issue 1018) — the fragment is baked into `.config`.
    set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_dir}/system.toml")
    nros_codegen_tool_reconfigure("${_nros}")
    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR "nano-ros: ${_dir}/system.toml [image.${_image}]: ${_err}")
    endif()

    # Issue 2200 -- the rows, read by one function the gate can call
    # (`check cmake-cli-rows`), since a configure that drops one says nothing.
    _nros_image_kconfig_leaf_rows("${_out}" "${_dir}" "${_image}" "${_west_board}")
    if(NOT _kconfig STREQUAL "" AND EXISTS "${_kconfig}")
        _nros_image_kconfig_drop_cli_overridden("${_kconfig}")
    endif()

    # phase-481 W3 -- the image's Zephyr-native fragments (`[image.<id>] os_config`
    # of a single-package leaf; the CLI answers nothing for a bringup, whose
    # `os_config` `nros build` passes itself). They go FIRST in the cache value:
    # before the user's own entries -- a board tail a harness passes as
    # `-DEXTRA_CONF_FILE` must still override them, exactly as it did when the
    # same file sat in `CONF_FILE` ahead of it -- and before the rendering. A
    # fragment the build already names (in `CONF_FILE`, or among the user's
    # extras) is left where the caller put it, so a harness that lists it
    # explicitly gets byte-for-byte the merge order it asked for.
    # phase-481 W5 -- a 4.x RMW snippet (`-S nros-<rmw>`) also chooses the RMW,
    # and merges BEFORE this rendering, so one naming a different RMW than the
    # image used to lose without a word. Refused instead, naming both.
    if(NOT _kconfig STREQUAL "" AND EXISTS "${_kconfig}")
        file(STRINGS "${_kconfig}" _rmw_line REGEX "^CONFIG_NROS_RMW_[A-Z]+=y$")
        if(_rmw_line MATCHES "^CONFIG_NROS_RMW_([A-Z]+)=y$")
            set(_snippets "")
            if(DEFINED CACHE{SNIPPET})
                list(APPEND _snippets $CACHE{SNIPPET})
            endif()
            if(DEFINED SNIPPET)
                list(APPEND _snippets ${SNIPPET})
            endif()
            if(DEFINED ENV{SNIPPET})
                list(APPEND _snippets $ENV{SNIPPET})
            endif()
            _nros_image_kconfig_check_snippets(
                "${CMAKE_MATCH_1}" "${_snippets}" "${_dir}/system.toml" "${_image}")
        endif()
    endif()

    _nros_image_kconfig_merge_snippets("${_board_snippets}" "${_image}")
    _nros_image_kconfig_merge_overlays("${_board_overlays}" "${_image}")

    _nros_image_kconfig_named_confs(_named)
    # phase-484 W5b -- a knob assigned in a conf file this image merges is a
    # Kconfig error too (no prompt); refuse it here, naming the file, the line
    # and where the value belongs. The files the build names, the image's own
    # `os_config`, and Zephyr's default `prj.conf` when no CONF_FILE is given; a
    # board fragment or a snippet still meets Kconfig's error as the backstop.
    set(_scan ${_named} ${_user} ${_image_conf})
    if(NOT DEFINED CACHE{CONF_FILE} AND NOT DEFINED CONF_FILE)
        if(DEFINED APPLICATION_CONFIG_DIR AND NOT APPLICATION_CONFIG_DIR STREQUAL "")
            list(APPEND _scan "${APPLICATION_CONFIG_DIR}/prj.conf")
        else()
            list(APPEND _scan "${APPLICATION_SOURCE_DIR}/prj.conf")
        endif()
    endif()
    _nros_image_kconfig_refuse_conf_knobs("${_scan}" "${_dir}/system.toml" "${_image}")
    set(_inserted "")
    foreach(_c IN LISTS _image_conf)
        if(NOT _c IN_LIST _named AND NOT _c IN_LIST _user)
            list(APPEND _inserted "${_c}")
        endif()
    endforeach()
    set(NROS_IMAGE_CONF_FILES "${_inserted}" CACHE INTERNAL
        "phase-481 W3: the image conf fragments the module hook added to EXTRA_CONF_FILE")
    if(NOT _inserted STREQUAL "")
        message(STATUS
            "nano-ros: image `${_image}` conf -> ${_inserted} "
            "(ahead of the caller's EXTRA_CONF_FILE; phase-481 W3)")
    endif()
    set(_new ${_inserted} ${_user})
    if(NOT _kconfig STREQUAL "")
        list(APPEND _new "${_kconfig}")
        message(STATUS
            "nano-ros: image `${_image}` from ${_dir}/system.toml -> ${_kconfig} "
            "(last of EXTRA_CONF_FILE; RFC-0098 D11)")
    endif()
    if(_had_ours OR NOT _kconfig STREQUAL "" OR NOT _inserted STREQUAL "")
        _nros_image_kconfig_set_cache("${_new}")
    endif()

    if(NOT _envcfg STREQUAL "")
        set_property(GLOBAL PROPERTY NROS_IMAGE_ENV_CONFIG "${_envcfg}")
        set(NROS_IMAGE_ENV_CARGO_FLAGS "--config=${_envcfg}" CACHE INTERNAL
            "phase-481 W1: the image env --config flag for the Rust lane's cargo")
        message(STATUS
            "nano-ros: image `${_image}` env rows with no Kconfig symbol (${_keys}) -> "
            "every cargo command gets --config ${_envcfg} (issue 1712)")
    endif()
endfunction()

# _nros_image_kconfig_leaf_rows(<cli-stdout> <dir> <image> <west-board>)
#
# Issue 2200 -- read `nros ws leaf-system`'s rows into the hook's
# `_kconfig`, `_envcfg`, `_image_conf`, `_board_snippets`, `_board_overlays` and
# `_keys` (caller scope),
# recording the image env rows in the cache as before. Split out of the hook
# so `packages/testing/nros-tests/tests/cmake_cli_rows.sh` reads a real
# board-plus-leaf answer through THIS code: the defect it pins (a board
# `prj.conf` plus an `os_config` fragment, `;`-joined, cut to the board's
# alone) produced a configure that succeeded and an image with no zenoh link.
function(_nros_image_kconfig_leaf_rows _out _dir _image _west_board)
    set(_kconfig "")
    set(_envcfg "")
    set(_keys "")
    set(_image_conf "")
    set(_board_snippets "")
    set(_board_overlays "")
    # Line by line, never through a cmake list: NROS_LEAF_CONF and
    # NROS_LEAF_SNIPPET are `;`-joined, and the list spelling cut each at its
    # first `;`, so a board `prj.conf` cost the leaf its `os_config`.
    nros_cli_rows(_row "${_out}" REQUIRE "^NROS_LEAF_[A-Z0-9_]+="
        CONTEXT "`nros ws leaf-system` for ${_dir}/system.toml [image.${_image}]")
    foreach(_i IN LISTS _row_INDICES)
        set(_l "${_row_${_i}}")
        if(_l MATCHES "^NROS_LEAF_KCONFIG=(.*)$")
            set(_kconfig "${CMAKE_MATCH_1}")
        elseif(_l MATCHES "^NROS_LEAF_IMAGE_ENV=(.*)$")
            set(_envcfg "${CMAKE_MATCH_1}")
        elseif(_l MATCHES "^NROS_LEAF_CONF=(.*)$")
            set(_image_conf "${CMAKE_MATCH_1}")
        elseif(_l MATCHES "^NROS_LEAF_SNIPPET=(.*)$")
            set(_board_snippets "${CMAKE_MATCH_1}")
        elseif(_l MATCHES "^NROS_LEAF_DTC_OVERLAY=(.*)$")
            set(_board_overlays "${CMAKE_MATCH_1}")
        elseif(_l MATCHES "^NROS_LEAF_BOARD_NOT_APPLIED=(.+)$")
            message(STATUS
                "nano-ros: image `${_image}` names a board package for "
                "${CMAKE_MATCH_1}; this build is for ${_west_board}, so its "
                "snippets and conf files are not applied")
        elseif(_l MATCHES "^NROS_LEAF_ROS_ARGS_INPUT=(.*)$")
            # Issue 2073 -- a parameter file whose text the image env bakes.
            set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${CMAKE_MATCH_1}")
        elseif(_l MATCHES "^NROS_LEAF_IMAGE_ENV_ROW=([A-Za-z0-9_]+)=(.*)$")
            list(APPEND _keys "${CMAKE_MATCH_1}")
            set(NROS_IMAGE_ENV_ROW_${CMAKE_MATCH_1} "${CMAKE_MATCH_2}" CACHE INTERNAL
                "phase-481 W1: [image.${_image}] env row with no Kconfig symbol")
        endif()
    endforeach()
    set(NROS_IMAGE_ENV_ROWS "${_keys}" CACHE INTERNAL
        "phase-481 W1: [image.<id>] env rows with no Kconfig symbol")
    # Issue 2200 -- every element must be a whole path to a file. The CLI
    # checked each one exists; a fragment of a path here means the list was
    # mangled on the way, and a mangled list is a conf file silently missing
    # from the image.
    foreach(_c IN LISTS _image_conf)
        if(NOT IS_ABSOLUTE "${_c}" OR NOT EXISTS "${_c}" OR IS_DIRECTORY "${_c}")
            message(FATAL_ERROR
                "nano-ros: ${_dir}/system.toml [image.${_image}]: NROS_LEAF_CONF names "
                "`${_c}`, which is not an existing conf file (full value: ${_image_conf}). "
                "(issue 2200)")
        endif()
    endforeach()
    set(_kconfig "${_kconfig}" PARENT_SCOPE)
    set(_envcfg "${_envcfg}" PARENT_SCOPE)
    set(_image_conf "${_image_conf}" PARENT_SCOPE)
    set(_board_snippets "${_board_snippets}" PARENT_SCOPE)
    set(_board_overlays "${_board_overlays}" PARENT_SCOPE)
    set(_keys "${_keys}" PARENT_SCOPE)
endfunction()

# phase-484 W5b -- refuse a registry knob assigned in any of `files` (a project
# with a system.toml). The symbol has no prompt, so Kconfig would refuse the
# line anyway, but its message names the module's Kconfig, not where the value
# belongs.
# Issue 2157 -- add the BOARD's snippets to SNIPPET: `board` (what
# `nros ws leaf-system` printed for a rendered image) plus
# `-DNROS_BOARD_SNIPPETS`, which `nros build` passes for every west image so a
# hand-written entry (a guest here) gets them too. e.g. `nros-nsos`, native_sim's
# host-socket network.
#
# `snippets.cmake` runs after this hook, so a name added here is applied exactly
# as `-S <name>` is, with or without CONF_FILE -- which a board conf file is not
# (it is discovered only when CONF_FILE is unset). Merged with every snippet the
# caller gave, never replacing one. The value goes to the cache AND the
# application's scope, so `zephyr_check_cache(SNIPPET)` reads the same list
# whichever it consults, and a re-configure computes the same list again. The
# names added last time are dropped first, so a board that stops naming one
# stops getting it, with no pristine build: Zephyr's own `CACHED_SNIPPET` is
# moved for the names the hook owns (issue 2179). A MACRO, so `PARENT_SCOPE` is the hook's caller: the
# application's directory scope.
macro(_nros_image_kconfig_merge_snippets _ms_board _ms_image)
    set(_ms_want ${_ms_board})
    if(DEFINED CACHE{NROS_BOARD_SNIPPETS})
        list(APPEND _ms_want $CACHE{NROS_BOARD_SNIPPETS})
    endif()
    set(_ms_caller "")
    foreach(_ms_src IN ITEMS LOCAL CACHE ENV)
        set(_ms_v "")
        set(_ms_have FALSE)
        if(_ms_src STREQUAL "LOCAL" AND DEFINED SNIPPET)
            set(_ms_v "${SNIPPET}")
            set(_ms_have TRUE)
        elseif(_ms_src STREQUAL "CACHE" AND DEFINED CACHE{SNIPPET})
            set(_ms_v "$CACHE{SNIPPET}")
            set(_ms_have TRUE)
        elseif(_ms_src STREQUAL "ENV" AND DEFINED ENV{SNIPPET})
            set(_ms_v "$ENV{SNIPPET}")
            set(_ms_have TRUE)
        endif()
        if(_ms_have)
            string(REPLACE " " ";" _ms_v "${_ms_v}")
            foreach(_ms_s IN LISTS _ms_v)
                if(NOT _ms_s STREQUAL "" AND NOT _ms_s IN_LIST NROS_IMAGE_SNIPPETS
                   AND NOT _ms_s IN_LIST _ms_caller)
                    list(APPEND _ms_caller "${_ms_s}")
                endif()
            endforeach()
        endif()
    endforeach()
    set(_ms_added "")
    foreach(_ms_s IN LISTS _ms_want)
        if(NOT _ms_s IN_LIST _ms_caller AND NOT _ms_s IN_LIST _ms_added)
            list(APPEND _ms_added "${_ms_s}")
        endif()
    endforeach()
    set(_ms_prev "${NROS_IMAGE_SNIPPETS}")
    set(NROS_IMAGE_SNIPPETS "${_ms_added}" CACHE INTERNAL
        "issue 2157: the board snippets the module hook added to SNIPPET")
    if(NOT _ms_added STREQUAL "" OR NOT _ms_prev STREQUAL "")
        set(_ms_all ${_ms_caller} ${_ms_added})
        if(_ms_all STREQUAL "")
            unset(SNIPPET CACHE)
            unset(SNIPPET PARENT_SCOPE)
        else()
            set(SNIPPET "${_ms_all}" CACHE STRING "Selected snippet" FORCE)
            set(SNIPPET "${_ms_all}" PARENT_SCOPE)
        endif()
        # Issue 2179 -- `snippets.cmake`'s `zephyr_check_cache(SNIPPET)` takes
        # `CACHED_SNIPPET` (the previous configure's list) over SNIPPET, so a
        # name this hook added once stayed in the build dir after the board
        # stopped naming it -- the configure that dropped it re-read it from
        # there, and a dir that once failed on a snippet kept failing until a
        # pristine build. The names the hook owns move in that list too: last
        # time's dropped, this time's added; the caller's own stay as Zephyr
        # cached them.
        if(DEFINED CACHE{CACHED_SNIPPET})
            set(_ms_cached "")
            foreach(_ms_s IN LISTS CACHED_SNIPPET)
                if(NOT _ms_s IN_LIST _ms_prev AND NOT _ms_s IN_LIST _ms_cached)
                    list(APPEND _ms_cached "${_ms_s}")
                endif()
            endforeach()
            foreach(_ms_s IN LISTS _ms_added)
                if(NOT _ms_s IN_LIST _ms_cached)
                    list(APPEND _ms_cached "${_ms_s}")
                endif()
            endforeach()
            if(_ms_cached STREQUAL "")
                unset(CACHED_SNIPPET CACHE)
            else()
                set(CACHED_SNIPPET "${_ms_cached}" CACHE STRING "Selected snippet" FORCE)
            endif()
        endif()
    endif()
    if(NOT _ms_added STREQUAL "")
        if("${_ms_image}" STREQUAL "")
            message(STATUS "nano-ros: board snippet(s) ${_ms_added} "
                "(the board's `[board.zephyr] snippets`)")
        else()
            message(STATUS "nano-ros: image `${_ms_image}` -> board snippet(s) ${_ms_added} "
                "(the board's `[board.zephyr] snippets`)")
        endif()
    endif()
endmacro()

function(_nros_image_kconfig_refuse_conf_knobs files system image)
    set(_syms "")
    foreach(_id IN LISTS NROS_KNOB_KCONFIG_IDS)
        list(APPEND _syms "${NROS_KNOB_KCONFIG_SYMBOL_${_id}}")
        set(_id_of_${NROS_KNOB_KCONFIG_SYMBOL_${_id}} "${_id}")
    endforeach()
    set(_found "")
    list(REMOVE_DUPLICATES files)
    foreach(_f IN LISTS files)
        if(NOT EXISTS "${_f}" OR IS_DIRECTORY "${_f}")
            continue()
        endif()
        file(STRINGS "${_f}" _lines REGEX "^[ \t]*CONFIG_[A-Za-z0-9_]+[ \t]*=")
        foreach(_l IN LISTS _lines)
            if(_l MATCHES "^[ \t]*CONFIG_([A-Za-z0-9_]+)[ \t]*=[ \t]*(.*)$")
                set(_sym "${CMAKE_MATCH_1}")
                set(_val "${CMAKE_MATCH_2}")
                if(_sym IN_LIST _syms)
                    set(_id "${_id_of_${_sym}}")
                    _nros_image_knob_spelling(_spell "${image}" "${_id}" "${_val}")
                    string(APPEND _found
                        "\n  ${_f}: CONFIG_${_sym}=${_val}  ->  ${_spell} in system.toml, "
                        "or -DNROS_OVERRIDE_${_id}=${_val} for one build")
                endif()
            endif()
        endforeach()
    endforeach()
    if(NOT _found STREQUAL "")
        message(FATAL_ERROR
            "nano-ros: ${system} is this image's one source of its nano-ros knobs "
            "(RFC-0103 D2a); those Kconfig symbols have no prompt, so a conf file "
            "cannot set them:${_found}\n(phase-484 W5b)")
    endif()
endfunction()

# phase-481 W5 -- refuse an `nros-<rmw>` snippet that names a different RMW
# than the image. `rmw` is the rendered symbol's suffix (`ZENOH`, `XRCE`,
# `CYCLONEDDS`); `snippets` is every snippet the build was given. A snippet
# that AGREES is redundant and allowed; an app with no system.toml never gets
# here, so the snippets still choose the RMW for it.
function(_nros_image_kconfig_check_snippets rmw snippets system image)
    foreach(_s IN LISTS snippets)
        # Issue 2157 -- only an RMW's snippet chooses the RMW. `nros-nsos` (a
        # board's network) is an `nros-` snippet too, and was refused as "selects
        # NSOS" when this matched every one.
        if(_s MATCHES "^nros-(zenoh|xrce|cyclonedds)$")
            string(TOUPPER "${CMAKE_MATCH_1}" _snip_rmw)
            if(NOT _snip_rmw STREQUAL rmw)
                string(TOLOWER "${rmw}" _img_rmw)
                message(FATAL_ERROR
                    "nano-ros: `-S ${_s}` selects ${_snip_rmw}, but ${system} "
                    "[image.${image}] has rmw = \"${_img_rmw}\", and the image wins "
                    "(it merges after the snippet). Drop the snippet, or build the image "
                    "for that RMW (`-DNROS_IMAGE=<id>`). (phase-481 W5)")
            endif()
        endif()
    endforeach()
endfunction()

# A rendered row whose symbol the cmake/west command line also sets
# (`-DCONFIG_<SYM>=…`) is DEAD: Zephyr merges the command-line values after
# every conf file, this rendering included, so the row can never be what the
# image bakes. Left in, it is an image stating one thing and running another,
# which issue 1550's domain check refuses by name -- measured: the tier-2 west
# fixtures re-domain every Cyclone image (`-DCONFIG_NROS_DOMAIN_ID=33`) while
# the bringup's `system.toml` says `domain_id = 0`, and once issue 1721 rendered
# that row `build-ws-cpp-entry-cyclonedds` stopped configuring. So the row is
# commented out with the reason, and the command line stays the one statement.
#
# Zephyr's own spelling of "set on the command line" (`kconfig.cmake`): the
# cache entry `CONFIG_<SYM>` on the configure that passes it, `CLI_CONFIG_<SYM>`
# on every re-configure after.
function(_nros_image_kconfig_drop_cli_overridden frag)
    file(STRINGS "${frag}" _lines)
    set(_out "")
    set(_dropped "")
    foreach(_l IN LISTS _lines)
        if(_l MATCHES "^(CONFIG_[A-Za-z0-9_]+)=")
            set(_sym "${CMAKE_MATCH_1}")
            if(DEFINED CACHE{${_sym}} OR DEFINED CACHE{CLI_${_sym}})
                string(APPEND _out
                    "# ${_l}\n"
                    "#   ^ not rendered: -D${_sym} on the command line states it, "
                    "and a command-line value merges after every conf file\n")
                list(APPEND _dropped "${_sym}")
                continue()
            endif()
        endif()
        string(APPEND _out "${_l}\n")
    endforeach()
    if(_dropped)
        file(WRITE "${frag}" "${_out}")
        string(REPLACE ";" ", " _dropped "${_dropped}")
        message(STATUS
            "nano-ros: ${frag}: ${_dropped} left to the command line's -D "
            "(a rendered row there would be overridden, i.e. dead)")
    endif()
endfunction()

# The conf files the caller already names, as absolute paths: every
# `CONF_FILE` entry (cache, else local -- the scopes Zephyr's own
# `zephyr_get(CONF_FILE SYSBUILD LOCAL)` reads next), a relative one resolved
# against the application config dir as Zephyr resolves it.
function(_nros_image_kconfig_named_confs _out)
    if(DEFINED CACHE{CONF_FILE})
        set(_cf "$CACHE{CONF_FILE}")
    elseif(DEFINED CONF_FILE)
        set(_cf "${CONF_FILE}")
    else()
        set(_cf "")
    endif()
    if(DEFINED APPLICATION_CONFIG_DIR AND NOT APPLICATION_CONFIG_DIR STREQUAL "")
        set(_base "${APPLICATION_CONFIG_DIR}")
    else()
        set(_base "${APPLICATION_SOURCE_DIR}")
    endif()
    string(REPLACE " " ";" _cf "${_cf}")
    set(_abs "")
    foreach(_f IN LISTS _cf)
        if(NOT _f STREQUAL "")
            get_filename_component(_a "${_f}" ABSOLUTE BASE_DIR "${_base}")
            list(APPEND _abs "${_a}")
        endif()
    endforeach()
    set(${_out} "${_abs}" PARENT_SCOPE)
endfunction()

# phase-524 W3 -- add the BOARD package's devicetree overlay (what
# `nros ws leaf-system` printed as `NROS_LEAF_DTC_OVERLAY`) to
# EXTRA_DTC_OVERLAY_FILE, ahead of the caller's own extras so theirs still win.
# EXTRA_, not DTC_OVERLAY_FILE: setting the latter would switch off Zephyr's
# discovery of the application's own `boards/<board>.overlay`. The entries the
# hook added last time are dropped first (`NROS_IMAGE_DTC_OVERLAYS`), so a
# board that stops carrying one, or an image on another board, stops getting
# it with no pristine build -- the rule the snippet merge follows (issue 2179).
function(_nros_image_kconfig_merge_overlays _mo_board _mo_image)
    set(_mo_prev "")
    if(DEFINED CACHE{NROS_IMAGE_DTC_OVERLAYS})
        set(_mo_prev "$CACHE{NROS_IMAGE_DTC_OVERLAYS}")
    endif()
    set(_mo_user "")
    if(DEFINED CACHE{EXTRA_DTC_OVERLAY_FILE})
        foreach(_mo_f IN LISTS CACHE{EXTRA_DTC_OVERLAY_FILE})
            if(NOT _mo_f IN_LIST _mo_prev)
                list(APPEND _mo_user "${_mo_f}")
            endif()
        endforeach()
    endif()
    set(_mo_new ${_mo_board} ${_mo_user})
    set(NROS_IMAGE_DTC_OVERLAYS "${_mo_board}" CACHE INTERNAL
        "phase-524 W3: the board overlays the module hook added to EXTRA_DTC_OVERLAY_FILE")
    if(_mo_prev STREQUAL "" AND _mo_board STREQUAL "")
        return()
    endif()
    if(_mo_new STREQUAL "")
        unset(EXTRA_DTC_OVERLAY_FILE CACHE)
    else()
        set(EXTRA_DTC_OVERLAY_FILE "${_mo_new}" CACHE STRING
            "Extra devicetree overlays; nano-ros adds the image's board package overlay first"
            FORCE)
    endif()
    if(NOT _mo_board STREQUAL "")
        message(STATUS
            "nano-ros: image `${_mo_image}` board overlay -> ${_mo_board} "
            "(head of EXTRA_DTC_OVERLAY_FILE)")
    endif()
endfunction()

# Write the cache value of EXTRA_CONF_FILE, or remove it when nothing is left.
function(_nros_image_kconfig_set_cache _value)
    if(_value STREQUAL "")
        unset(EXTRA_CONF_FILE CACHE)
    else()
        set(EXTRA_CONF_FILE "${_value}" CACHE STRING
            "Extra Kconfig fragments; nano-ros appends the image's rendering last (RFC-0098 D11)"
            FORCE)
    endif()
endfunction()
