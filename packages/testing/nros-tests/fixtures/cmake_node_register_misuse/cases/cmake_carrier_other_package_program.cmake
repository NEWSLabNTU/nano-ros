# Case `cmake_carrier_other_package_program` — see ../CMakeLists.txt.
#
# phase-583 W1: target names are global, so a program called `talker` defined
# by ANOTHER package must not switch this package's `talker` carrier off. Only
# the package's own program of that name does.
set(VERDICT_PROJECT carrier_pkg)
macro(verdict_body)
    add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/src/other_pkg_program" other_pkg_program)
    set(_NRC_DEPLOY threadx)
    set(_NRC_NAME talker)
    set(_NRC_CLASS carrier_pkg::Talker)
    set(_go TRUE)
    _nros_carrier_claim(_go "threadx" "carrier_pkg__talker")
    if(NOT _go)
        message(FATAL_ERROR "VERDICT: another package's `talker` switched this carrier off")
    endif()
    message(STATUS "VERDICT: the carrier is claimed beside another package's talker")
endmacro()
