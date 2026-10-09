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
#    `prj.conf`/`prj-<rmw>.conf`, the image's `conf` files and a snippet, and
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
#    is. A bringup's `conf` is `nros build`'s to pass (RFC-0085 D4), never added
#    here.
#
# The hook is a no-op for an application with no `system.toml`, except that it
# drops a stale rendering of its own from the cache.
#
# WHICH system.toml, and which image:
#   * a single-package leaf: `${APPLICATION_SOURCE_DIR}/system.toml`;
#     `-DNROS_IMAGE=<id>` picks one of several images (the leaf's own rule —
#     one image, or `[system] default_images` — applies otherwise);
#   * a generated workspace application: it states `NROS_IMAGE_SYSTEM_DIR` (the
#     bringup), `NROS_IMAGE` and `NROS_IMAGE_LANGUAGE` before
#     `find_package(Zephyr)` (`nros build`, `builder/west_app.rs`).

include("${CMAKE_CURRENT_LIST_DIR}/../../cmake/NanoRosCli.cmake")

# The module root, captured at FILE scope (inside a function
# `CMAKE_CURRENT_LIST_DIR` names the caller's file).
get_filename_component(_NROS_IMAGE_KCONFIG_ROOT "${CMAKE_CURRENT_LIST_DIR}/../.." ABSOLUTE)
set(_NROS_IMAGE_KCONFIG_ROOT "${_NROS_IMAGE_KCONFIG_ROOT}" CACHE INTERNAL
    "the nano-ros module root the module_ext_root hook renders against (phase-481 W1)")

function(nros_image_kconfig_hook)
    if(DEFINED APPLICATION_BINARY_DIR)
        set(_bin "${APPLICATION_BINARY_DIR}")
    else()
        set(_bin "${CMAKE_BINARY_DIR}")
    endif()
    set(_nros_dir "${_bin}/nros")

    # Drop an earlier rendering of ours from the cache value, whatever happens
    # below. A path that no longer exists would otherwise fail Kconfig.
    # phase-481 W3 -- and the image's own `conf` fragments an earlier configure
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

    set(_frag "${_nros_dir}/${_image}.conf")
    set(_env "${_nros_dir}/${_image}-env.toml")
    execute_process(
        COMMAND "${_nros}" ws leaf-system "${_dir}"
                --nano-ros-path "${_NROS_IMAGE_KCONFIG_ROOT}"
                ${_args} --kconfig-out "${_frag}" --image-env-out "${_env}"
        OUTPUT_VARIABLE _out ERROR_VARIABLE _err RESULT_VARIABLE _rc
        OUTPUT_STRIP_TRAILING_WHITESPACE)
    # A configure re-runs when the file changes, and when the CLI that rendered
    # it is rebuilt (issue 1018) — the fragment is baked into `.config`.
    set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS "${_dir}/system.toml")
    nros_codegen_tool_reconfigure("${_nros}")
    if(NOT _rc EQUAL 0)
        message(FATAL_ERROR "nano-ros: ${_dir}/system.toml [image.${_image}]: ${_err}")
    endif()

    set(_kconfig "")
    set(_envcfg "")
    set(_keys "")
    set(_image_conf "")
    string(REPLACE "\n" ";" _lines "${_out}")
    foreach(_l IN LISTS _lines)
        if(_l MATCHES "^NROS_LEAF_KCONFIG=(.*)$")
            set(_kconfig "${CMAKE_MATCH_1}")
        elseif(_l MATCHES "^NROS_LEAF_IMAGE_ENV=(.*)$")
            set(_envcfg "${CMAKE_MATCH_1}")
        elseif(_l MATCHES "^NROS_LEAF_CONF=(.*)$")
            set(_image_conf "${CMAKE_MATCH_1}")
        elseif(_l MATCHES "^NROS_LEAF_IMAGE_ENV_ROW=([A-Za-z0-9_]+)=(.*)$")
            list(APPEND _keys "${CMAKE_MATCH_1}")
            set(NROS_IMAGE_ENV_ROW_${CMAKE_MATCH_1} "${CMAKE_MATCH_2}" CACHE INTERNAL
                "phase-481 W1: [image.${_image}] env row with no Kconfig symbol")
        endif()
    endforeach()
    set(NROS_IMAGE_ENV_ROWS "${_keys}" CACHE INTERNAL
        "phase-481 W1: [image.<id>] env rows with no Kconfig symbol")

    # phase-481 W3 -- the image's Zephyr-native fragments (`[image.<id>] conf`
    # of a single-package leaf; the CLI answers nothing for a bringup, whose
    # `conf` `nros build` passes itself). They go FIRST in the cache value:
    # before the user's own entries -- a board tail a harness passes as
    # `-DEXTRA_CONF_FILE` must still override them, exactly as it did when the
    # same file sat in `CONF_FILE` ahead of it -- and before the rendering. A
    # fragment the build already names (in `CONF_FILE`, or among the user's
    # extras) is left where the caller put it, so a harness that lists it
    # explicitly gets byte-for-byte the merge order it asked for.
    _nros_image_kconfig_named_confs(_named)
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
