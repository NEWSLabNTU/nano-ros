# Case `cmake_zephyr_one_program_claimed_twice` — see ../CMakeLists.txt.
#
# Issue 2746, the negative control: the SAME program reaching `app` twice (a
# re-run, or one program by two roads) is one program, and passes.
set(VERDICT_PROJECT zephyr_pkg)
macro(verdict_body)
    _nros_zephyr_claim_app_program(talker "nano_ros_entry(talker)")
    _nros_zephyr_claim_app_program(talker "nano_ros_node_register(talker) in package `zephyr_pkg`")
    message(STATUS "VERDICT: one Zephyr program claimed twice passes")
endmacro()
