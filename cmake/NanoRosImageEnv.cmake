# NanoRosImageEnv.cmake — issue 1712: a C/C++ leaf's `[image.<id>] env`
# (RFC-0049's APP rung) on every cargo command the CMAKE road spawns.
#
# The cargo road writes the image's rows into `nros-cargo.toml` `[env]`, with no
# `force`, and that is the whole precedence story there: cargo puts an `[env]`
# row into a build script's environment only when the calling process does not
# already carry the variable, so an exported variable (the ladder's top rung)
# still wins. The cmake road had no carrier at all — the rows were parsed and
# dropped, and a C image could override its board only through whatever shell
# ran `cmake --build`.
#
# WHY A CARGO CONFIG, NOT `corrosion_set_env_vars`. That call renders
# `cmake -E env K=V cargo …`, and `cmake -E env` OVERWRITES an inherited value —
# measured: `K=shell cmake -E env K=cmake sh -c 'echo $K'` prints `cmake`. An
# image row on that carrier would outrank the exported variable, inverting the
# top two rungs. `--config <file>` with an `[env]` table is the cargo road's own
# mechanism, so the two roads agree by construction rather than by a second
# precedence rule written in cmake. Measured with cargo 1.99 on a probe crate
# whose build script declares `rerun-if-env-changed`: the file's value reaches
# the build script; an exported value wins over it; and editing or deleting the
# row RE-RUNS the build script on the next build (cargo fingerprints `[env]`
# values it hands a build script), so the edge from `system.toml` to the build
# script is cargo's own and needs no `rm -rf`.
#
# NOT `set(ENV{...})` (issue 0460): it touches only the configure-time process.
# And no path is fingerprinted (issue 0491): the file path rides the command
# line as a flag, and what cargo tracks is each row's VALUE.
#
# The file is written by `nros ws leaf-system --image-env-out`
# (`nano_ros_read_leaf_system`, NanoRosPackageXml.cmake) — the one reader of
# `system.toml` — and only when the image states a row, so an image that states
# none keeps a byte-identical cargo command.

include_guard(GLOBAL)

# issue 0657 — `nros_corrosion_env_target`.
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCorrosionEnv.cmake")

# nros_image_env_config(<out_var>)
#
# The image-env cargo config this configure recorded, or "".
function(nros_image_env_config out_var)
    get_property(_cfg GLOBAL PROPERTY NROS_IMAGE_ENV_CONFIG)
    if(NOT "${_cfg}" STREQUAL "" AND NOT EXISTS "${_cfg}")
        message(FATAL_ERROR
            "nano-ros: the image env config ${_cfg} was recorded by "
            "nano_ros_read_leaf_system() but does not exist (issue 1712)")
    endif()
    set(${out_var} "${_cfg}" PARENT_SCOPE)
endfunction()

# nros_image_env_cargo_flags(<out_var>)
#
# The cargo flag(s) that hand the image's rows to one cargo command — for a
# lane that builds its own command (the NuttX FFI driver). Empty when the image
# states nothing. Corrosion targets get the same flag from
# `nros_image_env_attach`, reached through `nros_board_facts_env`.
function(nros_image_env_cargo_flags out_var)
    nros_image_env_config(_cfg)
    if("${_cfg}" STREQUAL "")
        set(${out_var} "" PARENT_SCOPE)
    else()
        set(${out_var} "--config=${_cfg}" PARENT_SCOPE)
    endif()
endfunction()

# nros_image_env_attach(<corrosion-target>)
#
# Put the flag on a Corrosion target's cargo command, once. Corrosion runs that
# command from an always-out-of-date custom target, so cargo itself decides
# what the row change re-runs.
function(nros_image_env_attach _target)
    nros_image_env_cargo_flags(_flags)
    if("${_flags}" STREQUAL "")
        return()
    endif()
    if(NOT COMMAND corrosion_set_cargo_flags)
        message(FATAL_ERROR "nros_image_env_attach(${_target}): Corrosion not loaded")
    endif()
    nros_corrosion_env_target("${_target}" _target)
    get_target_property(_done ${_target} NROS_IMAGE_ENV_ATTACHED)
    if(_done)
        return()
    endif()
    corrosion_set_cargo_flags(${_target} ${_flags})
    set_property(TARGET ${_target} PROPERTY NROS_IMAGE_ENV_ATTACHED TRUE)
endfunction()

# nros_image_env_key_fields(<out_var>)
#
# The image env's share of a shared-cargo-directory KEY (issue 0616's rule):
# the rows change what the build scripts compile, so two images differing only
# there must not share a directory and its unhashed uplifted archive. One field,
# the content hash, and none when the image states nothing — a warm directory is
# not invalidated for an image that did not move.
function(nros_image_env_key_fields out_var)
    get_property(_cfg GLOBAL PROPERTY NROS_IMAGE_ENV_CONFIG)
    if("${_cfg}" STREQUAL "" OR NOT EXISTS "${_cfg}")
        set(${out_var} "" PARENT_SCOPE)
        return()
    endif()
    file(SHA256 "${_cfg}" _h)
    string(SUBSTRING "${_h}" 0 16 _h)
    set(${out_var} "NROS_IMAGE_ENV=${_h}" PARENT_SCOPE)
endfunction()
