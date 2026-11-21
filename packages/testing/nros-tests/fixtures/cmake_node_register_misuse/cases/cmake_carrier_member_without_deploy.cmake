# Case `cmake_carrier_member_without_deploy` — see ../CMakeLists.txt.
#
# Issue 2744 / phase-583 W1: a registration with NO DEPLOY is a workspace
# member's component, which a generated entry composes from its library. A
# second one under the same EXECUTABLE is NOT refused: it builds no second
# carrier, and the configure succeeds.
set(VERDICT_PROJECT member_pkg)
macro(verdict_body)
    set(_NRC_DEPLOY "")
    set(_NRC_NAME member_exe)
    set(_NRC_CLASS member_pkg::First)
    set(_go TRUE)
    _nros_carrier_claim(_go "native" "member_pkg__member_exe")
    add_custom_target(member_pkg__member_exe)
    set(_NRC_CLASS member_pkg::Second)
    set(_go TRUE)
    _nros_carrier_claim(_go "native" "member_pkg__member_exe")
    if(_go)
        message(FATAL_ERROR "VERDICT: the second member registration claimed a second carrier")
    endif()
    message(STATUS "VERDICT: second member registration skipped")
endmacro()
