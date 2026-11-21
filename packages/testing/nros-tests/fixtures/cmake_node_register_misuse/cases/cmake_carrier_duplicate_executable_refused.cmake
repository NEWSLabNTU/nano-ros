# Case `cmake_carrier_duplicate_executable_refused` — see ../CMakeLists.txt.
#
# phase-583 W1: each EXECUTABLE is one program, so two components registered
# under the SAME EXECUTABLE with a DEPLOY must FATAL_ERROR, naming both, rather
# than build one of them into nothing.
set(VERDICT_PROJECT carrier_pkg)
macro(verdict_body)
    set(_NRC_DEPLOY threadx)
    set(_NRC_NAME talker)
    set(_NRC_CLASS carrier_pkg::First)
    set(_go TRUE)
    _nros_carrier_claim(_go "threadx" "carrier_pkg__talker")
    add_custom_target(carrier_pkg__talker)
    set(_NRC_CLASS carrier_pkg::Second)
    set(_go TRUE)
    _nros_carrier_claim(_go "threadx" "carrier_pkg__talker")
    message(STATUS "VERDICT: the duplicate EXECUTABLE was not refused")
endmacro()
