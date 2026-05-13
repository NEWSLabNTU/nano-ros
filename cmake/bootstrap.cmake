#[=======================================================================[.rst:
nano-ros bootstrap
==================

Phase 123.A.5 — auto-run ``tools/setup.sh`` when the user invokes
``cmake -B build`` directly without having sourced setup first.
Closes the gap between two valid entry flows:

* `tools/setup.sh --target=posix-zenoh && cmake -B build` (explicit)
* `cmake -B build -DNANO_ROS_RMW=zenoh -DNANO_ROS_PLATFORM=posix`
  (direct — bootstrap.cmake catches this and runs setup.sh)

Idempotent: if the submodules required by ``(NANO_ROS_PLATFORM,
NANO_ROS_RMW)`` are already populated, no-op. Opt out with
``-DNANO_ROS_SKIP_BOOTSTRAP=ON`` (CI / container builds that
manage submodules separately).

Included exactly once from the top-level ``CMakeLists.txt`` before
any ``add_subdirectory`` call.
#]=======================================================================]

if(NANO_ROS_SKIP_BOOTSTRAP)
    return()
endif()

# Already-bootstrapped guard so a parent project that includes
# nano-ros via add_subdirectory doesn't re-trigger.
if(DEFINED CACHE{_NANO_ROS_BOOTSTRAP_DONE})
    return()
endif()

set(_repo_root "${CMAKE_CURRENT_LIST_DIR}/..")
get_filename_component(_repo_root "${_repo_root}" ABSOLUTE)
set(_setup_sh "${_repo_root}/tools/setup.sh")
set(_manifest "${_repo_root}/config/submodule-deps.toml")

if(NOT EXISTS "${_setup_sh}" OR NOT EXISTS "${_manifest}")
    # Not in a nano-ros source tree (likely consumed via find_package
    # from an install prefix). Nothing to bootstrap.
    set(_NANO_ROS_BOOTSTRAP_DONE TRUE CACHE INTERNAL "")
    return()
endif()

# Map NANO_ROS_PLATFORM CMake value → setup.sh platform tag. The
# CMake values include a sub-arch suffix (`freertos_armcm3` vs
# `freertos`); the setup.sh manifest keys on the family name.
set(_plat_tag "${NANO_ROS_PLATFORM}")
if(_plat_tag MATCHES "^freertos")
    set(_plat_tag "freertos")
elseif(_plat_tag MATCHES "^threadx")
    set(_plat_tag "threadx")
elseif(_plat_tag MATCHES "^nuttx")
    set(_plat_tag "nuttx")
endif()

# Quick check: are the submodules already populated? Run a `git
# submodule status` query and fall through to setup.sh only when
# something is missing.
execute_process(
    COMMAND git -C "${_repo_root}" submodule status
    OUTPUT_VARIABLE _submod_status
    OUTPUT_STRIP_TRAILING_WHITESPACE
    RESULT_VARIABLE _submod_rc
)

set(_need_bootstrap FALSE)
if(_submod_rc EQUAL 0)
    # Lines starting with `-` indicate uninitialised submodules.
    # If any required path matches, trigger bootstrap.
    string(REPLACE "\n" ";" _submod_lines "${_submod_status}")
    foreach(_line IN LISTS _submod_lines)
        if(_line MATCHES "^-")
            set(_need_bootstrap TRUE)
            break()
        endif()
    endforeach()
else()
    # `git submodule status` failed (maybe not a git checkout) — no
    # bootstrap possible.
    set(_NANO_ROS_BOOTSTRAP_DONE TRUE CACHE INTERNAL "")
    return()
endif()

if(_need_bootstrap)
    message(STATUS "nano-ros: bootstrapping submodules for "
        "(platform=${_plat_tag}, rmw=${NANO_ROS_RMW}) via tools/setup.sh")
    execute_process(
        COMMAND bash "${_setup_sh}"
                --target=${_plat_tag}-${NANO_ROS_RMW}
                --skip-rustup --skip-apt-check
        WORKING_DIRECTORY "${_repo_root}"
        RESULT_VARIABLE _setup_rc
    )
    if(NOT _setup_rc EQUAL 0)
        message(FATAL_ERROR
            "nano-ros bootstrap failed (exit ${_setup_rc}).\n"
            "Re-run manually:\n"
            "  ${_setup_sh} --target=${_plat_tag}-${NANO_ROS_RMW}\n"
            "Or skip bootstrap with -DNANO_ROS_SKIP_BOOTSTRAP=ON if "
            "you've already populated the submodules another way.")
    endif()
else()
    message(STATUS "nano-ros: submodules already populated; bootstrap skipped")
endif()

set(_NANO_ROS_BOOTSTRAP_DONE TRUE CACHE INTERNAL "")
