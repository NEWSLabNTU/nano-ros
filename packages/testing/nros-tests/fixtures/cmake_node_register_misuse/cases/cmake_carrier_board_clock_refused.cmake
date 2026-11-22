# Case `cmake_carrier_board_clock_refused` — see ../CMakeLists.txt.
#
# Issue 2841: a native carrier executable in a configure whose image states
# `[knobs.clock] source = "board"` must FATAL_ERROR at configure, naming the
# knob and the executable, instead of failing later to link the image's board
# clock hook. The image's value reaches the configure as cargo would see it,
# here through the exported variable, the top rung.
set(VERDICT_PROJECT carrier_pkg)
macro(verdict_body)
    set(ENV{NROS_CLOCK_SOURCE} "board,platform")
    _nros_carrier_board_clock_refusal("carrier_pkg__talker")
    message(STATUS "VERDICT: the board clock carrier was not refused")
endmacro()
