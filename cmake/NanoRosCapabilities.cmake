# NanoRosCapabilities.cmake — RFC-0042 D2 / phase-241 wave C (C.2).
#
# Lower a board's declared `[board.capabilities]` (the single source of truth in
# its `nros-board.toml`) into the matching `NROS_PLATFORM_HAS_*` compile defines,
# so the cmake-driven in-tree fixture builds derive them from board.toml instead
# of hand-setting them per overlay (the issue-0038 footgun: a heap-capable
# bare-metal board had to remember `-DNROS_PLATFORM_HAS_MALLOC`).
#
# Scope: the `-D`-mechanism platforms (bare-metal / ThreadX). Zephyr's heap/mutex
# stay Kconfig-derived (`CONFIG_HEAP_MEM_POOL_SIZE` / `CONFIG_MULTITHREADING`) and
# FreeRTOS's malloc stays `configSUPPORT_DYNAMIC_ALLOCATION`-derived — forcing a
# `-D` there would decouple the C view from the actual RTOS config. `HAS_ATOMICS`
# is universal today (header-defined); `threads` maps to `NROS_FEATURE_THREADS`,
# which is opt-in and not auto-emitted here.
#
# Reads the SSoT directly with `file(STRINGS)` — no generator, no committed
# fragment. A board with no `[board.capabilities]` block yields no defines (the
# conservative bare-metal default: no heap).
include_guard(GLOBAL)

# nros_board_capability_defines(<board_dir> <out_var>)
#   <board_dir> : dir holding the board's `nros-board.toml`
#   <out_var>   : set in the caller's scope to the list of capability defines
function(nros_board_capability_defines board_dir out_var)
    set(_toml "${board_dir}/nros-board.toml")
    set(_defs "")
    if(EXISTS "${_toml}")
        # `[board.capabilities]` `heap = true` → the board has an allocator, so
        # expose the canonical malloc/free (baremetal.h shims them over the CFFI
        # alloc/dealloc when NROS_PLATFORM_HAS_MALLOC is defined).
        file(STRINGS "${_toml}" _heap_true REGEX "^[ \t]*heap[ \t]*=[ \t]*true[ \t]*$")
        if(_heap_true)
            list(APPEND _defs NROS_PLATFORM_HAS_MALLOC)
        endif()
    endif()
    set(${out_var} "${_defs}" PARENT_SCOPE)
endfunction()
