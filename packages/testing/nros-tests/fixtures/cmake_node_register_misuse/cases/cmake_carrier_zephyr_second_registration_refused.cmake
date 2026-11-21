# Case `cmake_carrier_zephyr_second_registration_refused` — see ../CMakeLists.txt.
#
# phase-583 W1: a Zephyr build makes ONE program per package (one west image),
# so a second registration that names a DEPLOY is still refused there, saying
# to split the package.
set(VERDICT_PROJECT zephyr_pkg)
macro(verdict_body)
    set(_NRC_DEPLOY zephyr)
    set(_NRC_NAME first_exe)
    set(_NRC_CLASS zephyr_pkg::First)
    set(_go TRUE)
    _nros_carrier_claim(_go "zephyr" "zephyr_pkg_nros_zephyr_entry")
    add_custom_target(zephyr_pkg_nros_zephyr_entry)
    set(_NRC_NAME second_exe)
    set(_NRC_CLASS zephyr_pkg::Second)
    set(_go TRUE)
    _nros_carrier_claim(_go "zephyr" "zephyr_pkg_nros_zephyr_entry")
    message(STATUS "VERDICT: the second Zephyr registration was not refused")
endmacro()
