# Which Corrosion target carries environment to cargo — issue 0657.
#
# Its own module because BOTH `NanoRosCargoProfile.cmake` and
# `NanoRosBoardFacts.cmake` need the answer and neither includes the other.

include_guard(GLOBAL)

# nros_corrosion_env_target(<name> <out>) — issue 0657
#
# WHICH target carries env to cargo. Corrosion 0.6 creates TWO for one crate:
#
#   `<crate>`          an INTERFACE target whose properties the cargo build
#                      command reads through a generator expression
#                      (`$<TARGET_PROPERTY:<crate>,CORROSION_ENVIRONMENT_VARIABLES>`);
#   `<crate>-static`   an IMPORTED library naming the produced `.a`.
#
# `corrosion_set_env_vars` is `set_property(TARGET …)` — it succeeds on either,
# and only the first is ever read. Every caller in this repo passed the
# `-static` spelling, so `nros_cargo_profile_env`, `nros_board_facts_env` and
# the riscv64 rustflags all set a property nothing consumes. Measured on a
# configured example: `build.ninja` contains no `CARGO_PROFILE_*`, no
# `NROS_BOARD*` and no `RUSTFLAGS` — the phase-351 W5 board rung reached cargo
# on exactly zero targets under this Corrosion.
#
# Nothing failed loudly because every consumer has a default: the profile falls
# back to Corrosion's build-type mapping, the board rung to the build script's
# defaults (issue 0529's shape), and the float ABI to the toolchain's.
#
# So: normalise here, once. Strip a trailing `-static`/`-shared` when the base
# target exists, and keep the name otherwise.
function(nros_corrosion_env_target _name _out)
    foreach(_suffix "-static" "-shared")
        string(LENGTH "${_suffix}" _n)
        string(LENGTH "${_name}" _len)
        if(_len GREATER _n)
            math(EXPR _start "${_len} - ${_n}")
            string(SUBSTRING "${_name}" ${_start} -1 _tail)
            if(_tail STREQUAL _suffix)
                string(SUBSTRING "${_name}" 0 ${_start} _base)
                if(TARGET "${_base}")
                    set(${_out} "${_base}" PARENT_SCOPE)
                    return()
                endif()
            endif()
        endif()
    endforeach()
    set(${_out} "${_name}" PARENT_SCOPE)
endfunction()
