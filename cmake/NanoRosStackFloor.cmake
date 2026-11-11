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
# The image key is PROVISIONAL: `<bringup or entry name>@<board>` (issue 2400,
# "the image key"). It is computed in ONE place, `_nros_stack_floor_image_key`,
# so swapping it for a key the user states is a one-function change.
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

# The PROVISIONAL image key (issue 2400): the bringup's name when the entry has
# one, else the entry's, then `@<board>`. The one place it is spelled.
function(_nros_stack_floor_image_key _out _entry _bringup_dir _board)
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
    nros_platform_family(_fam "${NROS_PLATFORM}")
    if(NOT _on OR NOT _fam STREQUAL "freertos")
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
    if(NOT CMAKE_C_COMPILER_ID MATCHES "GNU|Clang")
        message(STATUS "nano-ros: stack floor for ${_exe} not checked -- "
            "${CMAKE_C_COMPILER_ID} has no -fstack-usage")
        return()
    endif()
    set(_script "${_NROS_STACK_FLOOR_DIR}/../scripts/nros-stack-floor.py")
    set(_table "${_NROS_STACK_FLOOR_DIR}/../config/stack-floors.toml")
    if(NOT EXISTS "${_script}" OR NOT EXISTS "${_table}")
        message(STATUS "nano-ros: stack floor for ${_exe} not checked -- this nano-ros "
            "has no scripts/nros-stack-floor.py or config/stack-floors.toml")
        return()
    endif()
    find_package(Python3 COMPONENTS Interpreter QUIET)
    if(NOT Python3_Interpreter_FOUND)
        message(STATUS "nano-ros: stack floor for ${_exe} not checked -- no python3")
        return()
    endif()

    # C frames: every C/C++ TU this image links writes its `.su`.
    _nros_stack_floor_closure(${_exe} _closure)
    foreach(_t IN LISTS _closure)
        get_target_property(_imp "${_t}" IMPORTED)
        get_target_property(_type "${_t}" TYPE)
        if(NOT _imp AND _type MATCHES "^(EXECUTABLE|STATIC_LIBRARY|OBJECT_LIBRARY)$")
            target_compile_options("${_t}" PRIVATE
                "$<$<COMPILE_LANGUAGE:C,CXX>:-fstack-usage>"
                "$<$<AND:$<COMPILE_LANGUAGE:C,CXX>,$<C_COMPILER_ID:GNU>>:-fcallgraph-info=su>")
        endif()
    endforeach()

    # The link's own record of what it loaded, whatever the generator.
    set(_dir "${CMAKE_BINARY_DIR}/nros/stack-floor")
    set(_map "${_dir}/${_exe}.map")
    file(MAKE_DIRECTORY "${_dir}")
    target_link_options(${_exe} PRIVATE "-Wl,-Map=${_map}")

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

    _nros_stack_floor_image_key(_key "${_exe}" "${_SF_BRINGUP}" "${_SF_BOARD}")
    get_target_property(_bindir ${_exe} BINARY_DIR)
    add_custom_command(TARGET ${_exe} POST_BUILD
        COMMAND "${Python3_EXECUTABLE}" "${_script}"
            --table "${_table}"
            --rmw "${_SF_RMW}" --target "${_SF_TARGET}" --image "${_key}"
            --role "OPEN=${_SF_ROOT}"
            --check-stack "OPEN=${_SF_STACK}" --stack-knob "${_SF_KNOB}"
            --link-map "${_map}" --link-dir "${_bindir}" --link-dir "${CMAKE_BINARY_DIR}"
            --su "${CMAKE_BINARY_DIR}"
            --bitcode-dir "${CMAKE_BINARY_DIR}/cargo/*/${_SF_TARGET}/${_SF_PROFILE}/deps"
            --bitcode-dir "${CMAKE_BINARY_DIR}/**/nano_ros_*_ffi_*/target/${_SF_TARGET}/*/deps"
            --sysroot
            --llc-cache "${_dir}/llc"
            ${_cxxfilt_arg}
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
