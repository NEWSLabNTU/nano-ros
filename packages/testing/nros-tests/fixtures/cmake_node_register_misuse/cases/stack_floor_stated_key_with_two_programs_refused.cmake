# Case `stack_floor_stated_key_with_two_programs_refused` — see ../CMakeLists.txt.
#
# phase-583 W4 (RFC-0071 D20): a stated `stack_floor_image` names one stack
# floor, so an entry that checks two programs refuses it rather than giving
# both programs one key.
set(VERDICT_PROJECT carrier_pkg)
macro(verdict_body)
    foreach(_x IN ITEMS prog_a prog_b)
        add_custom_target(carrier_pkg__${_x})
        set_target_properties(carrier_pkg__${_x} PROPERTIES
            NROS_STACK_FLOOR_KEY "talker/c/zenoh@b1"
            NROS_STACK_FLOOR_REPORT "${CMAKE_BINARY_DIR}/nros/stack-floor/report.json"
            NROS_STACK_FLOOR_ENTRY carrier_pkg
            NROS_STACK_FLOOR_PROGRAM ${_x}
            NROS_STACK_FLOOR_BOARD b1
            NROS_STACK_FLOOR_STATED "talker/c/zenoh@b1")
        set_property(GLOBAL APPEND PROPERTY NROS_STACK_FLOOR_PROGRAM_IMAGES carrier_pkg__${_x})
    endforeach()
    _nros_stack_floor_decide_program_keys()
    message(STATUS "VERDICT: the stated key was applied to both programs")
endmacro()
