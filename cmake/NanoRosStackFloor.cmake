# NanoRosStackFloor.cmake — issue 2400 W2: the cmake road's stack-floor CHECK.
#
# Every build measures the thread stack floor of the image it links
# (`scripts/nros-stack-floor.py`: the image's own frames, from stable
# `-C save-temps` bitcode through the pinned llc for Rust and GCC's
# `-fcallgraph-info=su` `.ci` files (frames by assembler name, so C++ matches
# its mangled symbols; `-fstack-usage` `.su` beside them as the fallback).
# On this road the only stack the image states is the
# FreeRTOS carrier's `.app_stack_bytes`, so the check is
#
#     app_stack_bytes >= floor(OPEN from app_task_entry)
#
# and what the build does with the answer is ruled (issue 2400, 2026-11-10,
# "CHECK by default; auto-raise is opt-in", plus ruling (A)):
#
#   * an image with an [[allowance]] row in config/stack-floors.toml is
#     MEASURED, and a floor above its stack FAILS the build, naming the knob;
#   * an image with no row is UNMEASURED: its floor is printed with a warning
#     and never enforced;
#   * nothing here lowers a stack. Raising one automatically is opt-in and is a
#     separate wave (W2b), because it needs the floor BEFORE the carrier
#     compiles, which is a pre-link edge this POST_BUILD check deliberately
#     does not add.
#
# The image key is the image's stated `stack_floor_image` (the `nros new`
# template it came from), else `<bringup or entry name>@<board>` (issue 2400,
# key (c)). It is decided in ONE place, `_nros_stack_floor_image_key`.
#
# `NROS_STACK_FLOOR=OFF` skips the whole thing (no save-temps, no .su, no walk).

include_guard(GLOBAL)

include("${CMAKE_CURRENT_LIST_DIR}/NanoRosCorrosionEnv.cmake")
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosPlatformFamily.cmake")

set(NROS_STACK_FLOOR "CHECK" CACHE STRING
    "Stack-floor check on the image's stated stack: CHECK (default) or OFF (issue 2400)")
set_property(CACHE NROS_STACK_FLOOR PROPERTY STRINGS CHECK OFF)

set(_NROS_STACK_FLOOR_DIR "${CMAKE_CURRENT_LIST_DIR}" CACHE INTERNAL "")

function(_nros_stack_floor_enabled _out)
    string(TOUPPER "${NROS_STACK_FLOOR}" _mode)
    if(_mode STREQUAL "OFF")
        set(${_out} FALSE PARENT_SCOPE)
    elseif(_mode STREQUAL "CHECK")
        set(${_out} TRUE PARENT_SCOPE)
    else()
        message(FATAL_ERROR "nano-ros: NROS_STACK_FLOOR='${NROS_STACK_FLOOR}' -- expected CHECK "
            "or OFF. Raising a stack to the floor is opt-in and not built yet (issue 2400 W2b).")
    endif()
endfunction()

# The image key (issue 2400, key (c), ruled 2026-11-12): the image's own
# `[image.<id>] stack_floor_image` when it states one -- `nros new` writes the
# template's `<template>/<lang>/<rmw>@<board>` there, so a renamed project keeps
# its row -- else the bringup's name (the entry's, with none) `@<board>`. The
# one place it is decided; the template spelling's one place is the CLI's
# `stack_floor_template_key`.
#
#   _nros_stack_floor_image_key(<out> <entry> <bringup dir> <board> [<stated>])
function(_nros_stack_floor_image_key _out _entry _bringup_dir _board)
    if(ARGC GREATER 4 AND NOT "${ARGV4}" STREQUAL "")
        set(${_out} "${ARGV4}" PARENT_SCOPE)
        return()
    endif()
    if(_bringup_dir)
        get_filename_component(_name "${_bringup_dir}" NAME)
    else()
        set(_name "${_entry}")
    endif()
    set(${_out} "${_name}@${_board}" PARENT_SCOPE)
endfunction()

# Merge `_flag` into the `_var=` entry of a Corrosion env target's environment,
# or add one. Another helper (riscv64's `-Ctarget-feature=+d`) may already have
# set RUSTFLAGS / CFLAGS there, and a second `VAR=` entry would replace the
# first rather than add to it.
function(_nros_stack_floor_merge_env _env_target _var _flag)
    # Corrosion's own name for the property (it differs between versions).
    if(NOT _CORR_PROP_ENV_VARS)
        message(FATAL_ERROR "nros_stack_floor_cargo_env: Corrosion is not loaded")
    endif()
    get_target_property(_envs ${_env_target} ${_CORR_PROP_ENV_VARS})
    if(NOT _envs)
        set(_envs "")
    endif()
    set(_out "")
    set(_done FALSE)
    foreach(_e IN LISTS _envs)
        if(_e MATCHES "^${_var}=(.*)$")
            set(_e "${_var}=${CMAKE_MATCH_1} ${_flag}")
            set(_done TRUE)
        endif()
        list(APPEND _out "${_e}")
    endforeach()
    if(NOT _done)
        list(APPEND _out "${_var}=${_flag}")
    endif()
    set_target_properties(${_env_target} PROPERTIES ${_CORR_PROP_ENV_VARS} "${_out}")
endfunction()

# The cargo half: every crate of a Corrosion import keeps its post-opt bitcode
# (`-C save-temps`), and the C that cc-rs compiles inside it writes `.su` files.
# Call it AFTER any other helper that sets RUSTFLAGS / CFLAGS on the same import.
function(nros_stack_floor_cargo_env _target)
    _nros_stack_floor_enabled(_on)
    if(NOT _on)
        return()
    endif()
    # W2 checks the FreeRTOS carrier only; every other platform's import keeps
    # its plain flags (and its sccache hits) until its own wave wires a check.
    nros_platform_family(_fam "${NROS_PLATFORM}")
    if(NOT _fam STREQUAL "freertos")
        return()
    endif()
    nros_corrosion_env_target("${_target}" _env_target)
    if(NOT TARGET ${_env_target})
        message(FATAL_ERROR "nros_stack_floor_cargo_env(${_target}): no Corrosion env target "
            "`${_env_target}` -- call it after the crate is imported")
    endif()
    _nros_stack_floor_merge_env(${_env_target} RUSTFLAGS "-Csave-temps")
    _nros_stack_floor_merge_env(${_env_target} CFLAGS "-fstack-usage -fcallgraph-info=su")
    _nros_stack_floor_merge_env(${_env_target} CXXFLAGS "-fstack-usage -fcallgraph-info=su")
endfunction()

# The same flags as `cmake -E env` rows, for a cargo command Corrosion does not
# run (the generated message FFI crates, NanoRosGenerateInterfaces.cmake).
# Empty when off or not FreeRTOS. Such a command sets no RUSTFLAGS of its own.
#
# RUSTFLAGS in the environment REPLACES the config files' rustflags. For the
# FreeRTOS triples the only one is the root `[build] rustflags =
# symbol-mangling-version=v0`, which is the toolchain default since 1.99
# (measured: a plain rustc names `_RNv...`), so nothing is lost; a FreeRTOS
# triple that gains a `[target.<t>] rustflags` row needs this revisited.
function(nros_stack_floor_cargo_env_list _out)
    set(${_out} "" PARENT_SCOPE)
    _nros_stack_floor_enabled(_on)
    if(NOT _on)
        return()
    endif()
    _nros_stack_floor_zephyr_applies(_zephyr)
    if(_zephyr)
        nros_stack_floor_zephyr_cargo_env(_env)
        set(${_out} "${_env}" PARENT_SCOPE)
        return()
    endif()
    nros_platform_family(_fam "${NROS_PLATFORM}")
    if(NOT _fam STREQUAL "freertos")
        return()
    endif()
    set(${_out}
        "RUSTFLAGS=-Csave-temps"
        "CFLAGS=-fstack-usage -fcallgraph-info=su"
        "CXXFLAGS=-fstack-usage -fcallgraph-info=su"
        PARENT_SCOPE)
endfunction()

# Every non-imported library the target links, transitively.
function(_nros_stack_floor_closure _target _out)
    set(_seen "")
    set(_queue "${_target}")
    while(_queue)
        list(POP_FRONT _queue _t)
        if(NOT TARGET "${_t}" OR "${_t}" IN_LIST _seen)
            continue()
        endif()
        get_target_property(_alias "${_t}" ALIASED_TARGET)
        if(_alias)
            set(_t "${_alias}")
        endif()
        list(APPEND _seen "${_t}")
        foreach(_p LINK_LIBRARIES INTERFACE_LINK_LIBRARIES)
            get_target_property(_libs "${_t}" ${_p})
            if(_libs)
                foreach(_l IN LISTS _libs)
                    if(NOT _l MATCHES "^\\$<" AND TARGET "${_l}")
                        list(APPEND _queue "${_l}")
                    endif()
                endforeach()
            endif()
        endforeach()
    endwhile()
    set(${_out} "${_seen}" PARENT_SCOPE)
endfunction()

# The image half. ROOT is the thread entry the stated stack belongs to, STACK
# its size in bytes, KNOB what the user sets to raise it, with `{bytes}` / `{kb}` for the floor.
#
#   nros_stack_floor_check(<exe> ROOT <fn> STACK <bytes> KNOB <text>
#                          BOARD <board> TARGET <triple> RMW <rmw>
#                          PROFILE <cargo profile> [BRINGUP <dir>])
# The walker, the table and python3, or a STATUS line saying which is missing.
function(_nros_stack_floor_tools _what _ok)
    set(${_ok} FALSE PARENT_SCOPE)
    if(NOT CMAKE_C_COMPILER_ID MATCHES "GNU|Clang")
        message(STATUS "nano-ros: stack floor for ${_what} not checked -- "
            "${CMAKE_C_COMPILER_ID} has no -fstack-usage")
        return()
    endif()
    set(_script "${_NROS_STACK_FLOOR_DIR}/../scripts/nros-stack-floor.py")
    # The build reads the table's JSON twin: stdlib `json` loads it on every
    # Python the build supports, where `tomllib` needs 3.11. The TOML is what
    # people edit; `--validate-table` refuses a twin that no longer matches it.
    set(_table "${_NROS_STACK_FLOOR_DIR}/../config/stack-floors.json")
    if(NOT EXISTS "${_script}" OR NOT EXISTS "${_table}")
        message(STATUS "nano-ros: stack floor for ${_what} not checked -- this nano-ros "
            "has no scripts/nros-stack-floor.py or config/stack-floors.json")
        return()
    endif()
    find_package(Python3 COMPONENTS Interpreter QUIET)
    if(NOT Python3_Interpreter_FOUND)
        message(STATUS "nano-ros: stack floor for ${_what} not checked -- no python3")
        return()
    endif()
    set(${_ok} TRUE PARENT_SCOPE)
endfunction()

# `-fstack-usage` on every target an image links, applied once the whole
# configure has run. The images travel by GLOBAL property and the deferred
# call takes no arguments: `cmake_language(DEFER … CALL fn "${_x}")` expands
# its arguments when it runs, in a scope where a function-local `_x` is gone.
function(_nros_stack_floor_schedule_closure _exe)
    set_property(GLOBAL APPEND PROPERTY NROS_STACK_FLOOR_IMAGES "${_exe}")
    get_property(_scheduled GLOBAL PROPERTY NROS_STACK_FLOOR_CLOSURE_SCHEDULED)
    if(_scheduled)
        return()
    endif()
    set_property(GLOBAL PROPERTY NROS_STACK_FLOOR_CLOSURE_SCHEDULED TRUE)
    cmake_language(DEFER DIRECTORY "${CMAKE_SOURCE_DIR}" CALL _nros_stack_floor_apply_closures)
endfunction()

function(_nros_stack_floor_apply_closures)
    get_property(_images GLOBAL PROPERTY NROS_STACK_FLOOR_IMAGES)
    set(_done "")
    foreach(_exe IN LISTS _images)
        _nros_stack_floor_closure(${_exe} _closure)
        foreach(_t IN LISTS _closure)
            if("${_t}" IN_LIST _done)
                continue()
            endif()
            list(APPEND _done "${_t}")
            get_target_property(_imp "${_t}" IMPORTED)
            get_target_property(_type "${_t}" TYPE)
            if(NOT _imp AND _type MATCHES "^(EXECUTABLE|STATIC_LIBRARY|OBJECT_LIBRARY|SHARED_LIBRARY|MODULE_LIBRARY)$")
                target_compile_options("${_t}" PRIVATE
                    "$<$<COMPILE_LANGUAGE:C,CXX>:-fstack-usage>"
                    "$<$<AND:$<COMPILE_LANGUAGE:C,CXX>,$<C_COMPILER_ID:GNU>>:-fcallgraph-info=su>")
            endif()
        endforeach()
    endforeach()
endfunction()

# The walker's command line, shared by every road that CHECKs.
#
#   _nros_stack_floor_command(<out> KEY <image key> ROOT <fn> STACK <bytes>
#       KNOB <text> TARGET <triple> RMW <rmw> MAP <link map> DIR <work dir>
#       LINK_DIRS <dir>... BITCODE <glob>... [SHAPE <KNOB=count>...] [ELF <image>])
#
# ELF is the linked image, whose `.debug_frame` gives a frame to a function no
# input states one for (a prebuilt toolchain library: libgcc, compiler_builtins
# shipped without bitcode). The cargo road passes it too (`--cfi-elf`).
function(_nros_stack_floor_command _out)
    cmake_parse_arguments(_C "" "KEY;ROOT;STACK;KNOB;TARGET;RMW;MAP;DIR;ELF" "LINK_DIRS;BITCODE;SHAPE" ${ARGN})
    # The rustup llvm-tools ship no llvm-cxxfilt, so name the cross
    # toolchain's own demangler (`arm-none-eabi-g++` -> `arm-none-eabi-c++filt`)
    # when it sits beside the compiler; the walker falls back to the host's.
    set(_cxxfilt_arg "")
    if(CMAKE_CXX_COMPILER MATCHES "g\\+\\+(\\.exe)?$")
        string(REGEX REPLACE "g\\+\\+((\\.exe)?)$" "c++filt\\1" _cxxfilt "${CMAKE_CXX_COMPILER}")
        if(EXISTS "${_cxxfilt}")
            set(_cxxfilt_arg --cxxfilt "${_cxxfilt}")
        endif()
    endif()
    set(_cmd "${Python3_EXECUTABLE}" "${_NROS_STACK_FLOOR_DIR}/../scripts/nros-stack-floor.py"
        --table "${_NROS_STACK_FLOOR_DIR}/../config/stack-floors.json"
        --rmw "${_C_RMW}" --target "${_C_TARGET}" --image "${_C_KEY}"
        --role "OPEN=${_C_ROOT}"
        --check-stack "OPEN=${_C_STACK}" --stack-knob "${_C_KNOB}"
        --link-map "${_C_MAP}")
    if(_C_ELF)
        list(APPEND _cmd --cfi-elf "${_C_ELF}")
    endif()
    foreach(_d IN LISTS _C_LINK_DIRS)
        list(APPEND _cmd --link-dir "${_d}")
    endforeach()
    list(APPEND _cmd --su "${CMAKE_BINARY_DIR}")
    foreach(_b IN LISTS _C_BITCODE)
        list(APPEND _cmd --bitcode-dir "${_b}")
    endforeach()
    foreach(_s IN LISTS _C_SHAPE)
        list(APPEND _cmd --shape "${_s}")
    endforeach()
    # report.json: what this build measured, for the twin lane
    # (scripts/stack-floor-twin.py), which compares a twin's peak with it.
    list(APPEND _cmd --sysroot --llc-cache "${_C_DIR}/llc" --report "${_C_DIR}/report.json"
        ${_cxxfilt_arg})
    set(${_out} "${_cmd}" PARENT_SCOPE)
endfunction()

# The image half. ROOT is the thread entry the stated stack belongs to, STACK
# its size in bytes, KNOB what the user sets to raise it, with `{bytes}` / `{kb}` for the floor.
#
#   nros_stack_floor_check(<exe> ROOT <fn> STACK <bytes> KNOB <text>
#                          BOARD <board> TARGET <triple> RMW <rmw>
#                          PROFILE <cargo profile> [BRINGUP <dir>])
function(nros_stack_floor_check _exe)
    cmake_parse_arguments(_SF "" "ROOT;STACK;KNOB;BOARD;TARGET;RMW;PROFILE;BRINGUP" "" ${ARGN})
    _nros_stack_floor_enabled(_on)
    if(NOT _on)
        return()
    endif()
    foreach(_req ROOT STACK KNOB BOARD TARGET RMW PROFILE)
        if("${_SF_${_req}}" STREQUAL "")
            message(FATAL_ERROR "nros_stack_floor_check(${_exe}): ${_req} is required")
        endif()
    endforeach()
    _nros_stack_floor_tools(${_exe} _ok)
    if(NOT _ok)
        return()
    endif()

    # C frames: every C/C++ TU this image links writes its `.su`. The closure
    # is walked at the END of the configure, not here: this runs when the
    # image is created, and a project links its own libraries after that
    # (`add_executable(app …)` then `target_link_libraries(app my_component)`),
    # so a closure taken now missed them -- their TUs wrote no `.su` and the
    # walk refused over frameless functions of the image's own code.
    _nros_stack_floor_schedule_closure(${_exe})

    # The link's own record of what it loaded, whatever the generator.
    set(_dir "${CMAKE_BINARY_DIR}/nros/stack-floor")
    set(_map "${_dir}/${_exe}.map")
    file(MAKE_DIRECTORY "${_dir}")
    target_link_options(${_exe} PRIVATE "-Wl,-Map=${_map}")

    # The stated key (`[image.<id>] stack_floor_image`) and the opt-in to a
    # raise reach this configure from `nros build` as -D values it passes on
    # every configure, empty included, so neither outlives an edit to
    # system.toml (issue 2461; issue 2400 W2b).
    _nros_stack_floor_image_key(_key "${_exe}" "${_SF_BRINGUP}" "${_SF_BOARD}"
        "${NROS_IMAGE_STACK_FLOOR_IMAGE}")
    get_target_property(_bindir ${_exe} BINARY_DIR)
    _nros_stack_floor_command(_cmd KEY "${_key}" ROOT "${_SF_ROOT}" STACK "${_SF_STACK}"
        KNOB "${_SF_KNOB}" TARGET "${_SF_TARGET}" RMW "${_SF_RMW}" MAP "${_map}" DIR "${_dir}"
        ELF "$<TARGET_FILE:${_exe}>"
        LINK_DIRS "${_bindir}" "${CMAKE_BINARY_DIR}"
        BITCODE "${CMAKE_BINARY_DIR}/cargo/*/${_SF_TARGET}/${_SF_PROFILE}/deps"
                "${CMAKE_BINARY_DIR}/**/nano_ros_*_ffi_*/target/${_SF_TARGET}/*/deps")
    if(NROS_IMAGE_STACK_FLOOR_MODE STREQUAL "raise")
        # A measured floor above the stack is recorded in the report as a
        # raise instead of failing; `nros build` rebuilds once with it.
        list(APPEND _cmd --raise)
    endif()
    add_custom_command(TARGET ${_exe} POST_BUILD
        COMMAND ${_cmd}
        COMMENT "nano-ros: stack floor of ${_key}"
        VERBATIM)
    message(STATUS "nano-ros: stack floor of ${_exe} checked at build as ${_key} "
        "(OPEN from ${_SF_ROOT} against ${_SF_STACK} B; NROS_STACK_FLOOR=OFF skips)")
endfunction()

# The FreeRTOS carrier: the app task (`app_task_entry`, freertos_c_entry.c)
# runs registration, Executor::open and the spin loop on `.app_stack_bytes`.
# Both carrier producers (`nano_ros_add_entry`, `nano_ros_node_register`'s
# carrier image) call this with the value they baked into the config TU.
function(nros_stack_floor_freertos_check _exe _stack_bytes _bringup_dir)
    _nros_resolve_rust_target(_triple)
    set(_board "${NROS_BOARD}")
    if(_board STREQUAL "")
        set(_board "${NROS_PLATFORM}")
    endif()
    nros_stack_floor_check(${_exe}
        ROOT    app_task_entry
        STACK   "${_stack_bytes}"
        KNOB    "NROS_FREERTOS_APP_STACK_KB={kb}"
        BOARD   "${_board}"
        TARGET  "${_triple}"
        RMW     "${NROS_RMW}"
        PROFILE "${NROS_CARGO_PROFILE_DIR}"
        BRINGUP "${_bringup_dir}")
endfunction()

# --- Zephyr (issue 2400 W4) -------------------------------------------------
#
# The main thread runs registration and Executor::open (OPEN), so the check is
#
#     CONFIG_MAIN_STACK_SIZE >= floor(OPEN from rust_main | main)
#
# run after `zephyr_final` links, on its own `zephyr.map`. Not on native_sim
# (CONFIG_ARCH_POSIX): a Zephyr thread there runs on a host pthread's stack,
# and MAIN_STACK_SIZE bounds nothing the frames use. The configure-time
# `NROS_RUST_MAIN_STACK_FLOOR` (issue 2359) stays: an UNMEASURED image is
# never refused here (ruling (A)), and that floor is what keeps an unmeasured
# Rust image off the silent overflow until rows cover the boards.

function(_nros_stack_floor_zephyr_applies _out)
    _nros_stack_floor_enabled(_on)
    if(_on AND DEFINED CONFIG_BOARD AND NOT CONFIG_ARCH_POSIX
       AND DEFINED CONFIG_MAIN_STACK_SIZE)
        set(${_out} TRUE PARENT_SCOPE)
    else()
        set(${_out} FALSE PARENT_SCOPE)
    endif()
endfunction()

# The cargo half, as `cmake -E env` rows for BOTH Zephyr lanes (the C lane's
# `nros_cargo_build` command and zephyr-lang-rust's, through
# NROS_BOARD_FACTS_ENV). `CARGO_TARGET_<TRIPLE>_RUSTFLAGS` is target-scoped,
# so build scripts and proc-macros keep their flags, and cargo JOINS it with a
# config file's `[target.<triple>] rustflags` instead of replacing them, as a
# bare RUSTFLAGS would. RUSTC_WRAPPER is emptied: a sccache hit restores the
# rlib and not the save-temps bitcode (issue 2400, 11 of 27 crates).
function(nros_stack_floor_zephyr_cargo_env _out)
    set(${_out} "" PARENT_SCOPE)
    _nros_stack_floor_zephyr_applies(_on)
    if(NOT _on OR NOT NROS_RUST_TARGET)
        return()
    endif()
    string(TOUPPER "${NROS_RUST_TARGET}" _t)
    string(REPLACE "-" "_" _t "${_t}")
    string(REPLACE "." "_" _t "${_t}")
    set(${_out} "CARGO_TARGET_${_t}_RUSTFLAGS=-Csave-temps" "RUSTC_WRAPPER=" PARENT_SCOPE)
endfunction()

# The image half: `-fstack-usage` on every Zephyr library and the app, and the
# walk as a post-build command of `zephyr_final`.
#
#   nros_stack_floor_zephyr_check(BOARD <nros board> RMW <rmw>
#                                 ROOT <rust_main|main> [BRINGUP <dir>])
function(nros_stack_floor_zephyr_check)
    cmake_parse_arguments(_Z "" "BOARD;RMW;ROOT;BRINGUP" "" ${ARGN})
    _nros_stack_floor_zephyr_applies(_on)
    if(NOT _on)
        return()
    endif()
    if(_Z_BOARD STREQUAL "" OR _Z_RMW STREQUAL "" OR NOT NROS_RUST_TARGET)
        message(STATUS "nano-ros: stack floor not checked -- no nano-ros board, RMW or "
            "Rust target for this Zephyr image")
        return()
    endif()
    _nros_stack_floor_tools(zephyr _ok)
    if(NOT _ok)
        return()
    endif()
    zephyr_compile_options(
        "$<$<COMPILE_LANGUAGE:C,CXX>:-fstack-usage>"
        "$<$<AND:$<COMPILE_LANGUAGE:C,CXX>,$<C_COMPILER_ID:GNU>>:-fcallgraph-info=su>")

    set(_dir "${CMAKE_BINARY_DIR}/nros/stack-floor")
    file(MAKE_DIRECTORY "${_dir}")
    # The stated key rides the image facts `nros dev image facts` wrote for
    # this configure (NanoRosImageAgreement.cmake); a hand-written application
    # outside a workspace has none and keys by its bringup.
    set(NROS_IMAGE_STACK_FLOOR_IMAGE "")
    get_property(_facts GLOBAL PROPERTY NROS_IMAGE_FACTS_FILE)
    if(_facts AND EXISTS "${_facts}")
        include("${_facts}")
    endif()
    _nros_stack_floor_image_key(_key zephyr "${_Z_BRINGUP}" "${_Z_BOARD}"
        "${NROS_IMAGE_STACK_FLOOR_IMAGE}")
    # The image's derived entity counts, which the walker compares with the
    # shape its row was measured on (a mismatch is a loud warning).
    set(_shape "")
    foreach(_fact IN LISTS NROS_BOARD_FACTS_ENV)
        if(_fact MATCHES "^(NROS_EXECUTOR_MAX_NODES|NROS_MAX_PUBLISHERS|NROS_MAX_SUBSCRIBERS|NROS_MAX_QUERYABLES|NROS_EXECUTOR_MAX_CBS|NROS_EXECUTOR_ACTION_CLIENTS)=(-?[0-9]+)$")
            list(APPEND _shape "${CMAKE_MATCH_1}=${CMAKE_MATCH_2}")
        endif()
    endforeach()
    set(_t "${NROS_RUST_TARGET}")
    _nros_stack_floor_command(_cmd KEY "${_key}" ROOT "${_Z_ROOT}" SHAPE ${_shape}
        STACK "${CONFIG_MAIN_STACK_SIZE}" KNOB "CONFIG_MAIN_STACK_SIZE={bytes}"
        TARGET "${_t}" RMW "${_Z_RMW}"
        MAP "${CMAKE_BINARY_DIR}/zephyr/zephyr.map" DIR "${_dir}"
        ELF "${CMAKE_BINARY_DIR}/zephyr/zephyr.elf"
        LINK_DIRS "${CMAKE_BINARY_DIR}"
        BITCODE "${CMAKE_BINARY_DIR}/rust/target/${_t}/*/deps"
                "${CMAKE_BINARY_DIR}/nros-rust*/${_t}/*/deps"
                "${CMAKE_BINARY_DIR}/**/nano_ros_*_ffi_*/target/${_t}/*/deps")
    # Zephyr runs its post-build commands without VERBATIM, through the shell,
    # which would expand the bitcode globs; the command lives in a script,
    # each argument a bracket argument, and the shell sees one path.
    set(_args "")
    foreach(_a IN LISTS _cmd)
        string(APPEND _args "\n    [==[${_a}]==]")
    endforeach()
    file(WRITE "${_dir}/check.cmake"
        "# GENERATED by nros_stack_floor_zephyr_check (issue 2400 W4).\n"
        "execute_process(COMMAND${_args}\n    RESULT_VARIABLE _rc)\n"
        "if(NOT _rc EQUAL 0)\n"
        "    message(FATAL_ERROR \"nano-ros: stack floor of ${_key}: see above\")\n"
        "endif()\n")
    set_property(GLOBAL APPEND PROPERTY extra_post_build_commands
        COMMAND ${CMAKE_COMMAND} -P "${_dir}/check.cmake")
    if(NROS_IMAGE_STACK_FLOOR_MODE STREQUAL "raise")
        # The raise reaches a configure through `NROS_STACK_FLOOR_RAISE`, which
        # the FreeRTOS carrier reads; CONFIG_MAIN_STACK_SIZE is Kconfig, which
        # it does not reach, so a Zephyr image keeps the CHECK.
        message(STATUS "nano-ros: `stack_floor = \"raise\"` is not available on Zephyr; "
            "the stack floor is CHECKED (raise CONFIG_MAIN_STACK_SIZE in os_config)")
    endif()
    message(STATUS "nano-ros: stack floor of ${_key} checked at build (OPEN from ${_Z_ROOT} "
        "against CONFIG_MAIN_STACK_SIZE=${CONFIG_MAIN_STACK_SIZE}; NROS_STACK_FLOOR=OFF skips)")
endfunction()
