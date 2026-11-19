# Case `cmake_carrier_second_registration_refused` — see ../CMakeLists.txt.
#
# Issue 2744: a package builds ONE carrier image per road, so a second
# registration that names a DEPLOY must FATAL_ERROR, naming both, rather than
# be built into nothing while the image ships with the first component only.
set(VERDICT_PROJECT carrier_pkg)
macro(verdict_body)
    set(_NRC_DEPLOY threadx)
    set(_NRC_NAME first_exe)
    set(_NRC_CLASS carrier_pkg::First)
    set(_go TRUE)
    _nros_carrier_claim(_go "threadx" "carrier_pkg_carrier")
    if(NOT _go)
        message(FATAL_ERROR "VERDICT: the first registration did not claim the carrier")
    endif()
    add_custom_target(carrier_pkg_carrier)
    set(_NRC_NAME second_exe)
    set(_NRC_CLASS carrier_pkg::Second)
    set(_go TRUE)
    _nros_carrier_claim(_go "threadx" "carrier_pkg_carrier")
    message(STATUS "VERDICT: the second registration was not refused")
endmacro()
