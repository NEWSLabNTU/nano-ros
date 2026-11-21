# Case `stack_floor_two_programs_keyed_per_program` — see ../CMakeLists.txt.
#
# phase-583 W4 (RFC-0071 D20): an owned entry that checks TWO programs keys
# each `<pkg>/<exe>@<board>` and reports to `nros/stack-floor/<exe>/report.json`,
# the path `report_path` in scripts/nros-stack-floor.py spells.
set(VERDICT_PROJECT carrier_pkg)
macro(verdict_body)
    foreach(_x IN ITEMS prog_a prog_b)
        add_custom_target(carrier_pkg__${_x})
        set_target_properties(carrier_pkg__${_x} PROPERTIES
            NROS_STACK_FLOOR_KEY "carrier_pkg@b1"
            NROS_STACK_FLOOR_REPORT "${CMAKE_BINARY_DIR}/nros/stack-floor/report.json"
            NROS_STACK_FLOOR_ENTRY carrier_pkg
            NROS_STACK_FLOOR_PROGRAM ${_x}
            NROS_STACK_FLOOR_BOARD b1
            NROS_STACK_FLOOR_STATED "")
        set_property(GLOBAL APPEND PROPERTY NROS_STACK_FLOOR_PROGRAM_IMAGES carrier_pkg__${_x})
    endforeach()
    _nros_stack_floor_decide_program_keys()
    foreach(_x IN ITEMS prog_a prog_b)
        get_target_property(_k carrier_pkg__${_x} NROS_STACK_FLOOR_KEY)
        get_target_property(_r carrier_pkg__${_x} NROS_STACK_FLOOR_REPORT)
        file(RELATIVE_PATH _rel "${CMAKE_BINARY_DIR}" "${_r}")
        message(STATUS "VERDICT: ${_x} key=${_k} report=${_rel}")
    endforeach()
endmacro()
