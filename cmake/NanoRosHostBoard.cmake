# cmake/NanoRosHostBoard.cmake — phase-500 W1
#
# The host board's DEPLOY token, and the refusal of the one it replaced.
#
# The host board (`packages/boards/linux/nros-board.toml`) builds on Linux
# only, so its one name is `linux`. `native` named it until phase-500 W1; under
# the project's definitions `native` is the whole class of general-purpose hosts
# (Linux, macOS, Windows), which claimed more than the board delivers. A cmake
# DEPLOY / BOARD token is the same word a `system.toml` image writes, so it
# follows the same rule: `linux`, and `native` is refused naming its
# replacement.
#
# The Rust side spells the same table as `nros_entry_lower::RETIRED_BOARD_KEYS`
# (cmake cannot read it); the refusal text below is that function's text.

include_guard(GLOBAL)

# The DEPLOY token that selects the host board (maps to `NROS_PLATFORM=posix`).
set(NROS_HOST_DEPLOY "linux")

# nros_refuse_retired_deploy(<context> <token>...)
#
# FATAL_ERROR when any <token> is a retired board key, naming its replacement.
function(nros_refuse_retired_deploy context)
    foreach(_t IN LISTS ARGN)
        if(_t STREQUAL "native")
            message(FATAL_ERROR
                "${context}: board `native` no longer names a board: write "
                "`linux`. The host board builds on Linux only, so `native` "
                "(every general-purpose host) claimed more than it delivers "
                "(phase-500).")
        endif()
    endforeach()
endfunction()
