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
# phase-500 W5 — `posix` is refused the same way: it is the PLATFORM
# (`NROS_PLATFORM=posix`), and a DEPLOY / BOARD token `posix` once fell through
# `_nros_deploy_to_platform`'s identity arm to the host platform, i.e. an alias
# for the host board.
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
                "${context}: board `native` does not name a board: write "
                "`linux`. `native` names every general-purpose host, and the "
                "host board builds on Linux only (phase-500).")
        elseif(_t STREQUAL "posix")
            message(FATAL_ERROR
                "${context}: board `posix` does not name a board: write "
                "`linux`. `posix` is the platform (`nros-platform-posix`), not "
                "a board (phase-500).")
        endif()
    endforeach()
endfunction()
