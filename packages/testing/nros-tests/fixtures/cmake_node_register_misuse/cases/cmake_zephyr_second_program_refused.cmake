# Case `cmake_zephyr_second_program_refused` — see ../CMakeLists.txt.
#
# Issue 2746: a west build is one Zephyr image with one `main`, so a second,
# different program put into `app` is refused at configure, naming both,
# rather than failing at link with `multiple definition of main`.
set(VERDICT_PROJECT zephyr_pkg)
macro(verdict_body)
    _nros_zephyr_claim_app_program(talker "nano_ros_entry(talker)")
    _nros_zephyr_claim_app_program(talker_b "nano_ros_entry(talker_b)")
    message(STATUS "VERDICT: the second Zephyr program was not refused")
endmacro()
