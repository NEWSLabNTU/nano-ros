# Case `cmake_carrier_board_clock_default_builds` — see ../CMakeLists.txt.
#
# Issue 2841's control: with any other source (here the default), the same
# carrier configures. Only `board` is refused.
set(VERDICT_PROJECT carrier_pkg)
macro(verdict_body)
    set(ENV{NROS_CLOCK_SOURCE} "platform")
    _nros_carrier_board_clock_refusal("carrier_pkg__talker")
    message(STATUS "VERDICT: the carrier configured")
endmacro()
