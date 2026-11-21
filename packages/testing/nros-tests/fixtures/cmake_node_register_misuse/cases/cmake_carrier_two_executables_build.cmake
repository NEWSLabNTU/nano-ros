# Case `cmake_carrier_two_executables_build` — see ../CMakeLists.txt.
#
# phase-583 W1 (RFC-0071 D20): one carrier per EXECUTABLE. A package that
# registers two components with two EXECUTABLE names builds two carriers,
# `<pkg>__<exe>` each; neither is refused.
set(VERDICT_PROJECT carrier_pkg)
macro(verdict_body)
    set(_NRC_DEPLOY threadx)
    set(_NRC_NAME first_exe)
    set(_NRC_CLASS carrier_pkg::First)
    set(_go TRUE)
    _nros_carrier_claim(_go "threadx" "carrier_pkg__first_exe")
    if(NOT _go)
        message(FATAL_ERROR "VERDICT: the first registration did not claim its carrier")
    endif()
    add_custom_target(carrier_pkg__first_exe)
    set(_NRC_NAME second_exe)
    set(_NRC_CLASS carrier_pkg::Second)
    set(_go TRUE)
    _nros_carrier_claim(_go "threadx" "carrier_pkg__second_exe")
    if(NOT _go)
        message(FATAL_ERROR "VERDICT: the second EXECUTABLE did not claim its own carrier")
    endif()
    message(STATUS "VERDICT: both executables claimed a carrier")
endmacro()
