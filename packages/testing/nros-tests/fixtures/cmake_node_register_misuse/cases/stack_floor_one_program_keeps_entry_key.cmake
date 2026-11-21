# Case `stack_floor_one_program_keeps_entry_key` — see ../CMakeLists.txt.
#
# phase-583 W4 (RFC-0071 D20): an owned entry that checks ONE program keeps
# the entry's key `<pkg>@<board>` and the build dir's one report, byte for
# byte what it was before carriers became one per executable.
set(VERDICT_PROJECT carrier_pkg)
macro(verdict_body)
    add_custom_target(carrier_pkg__only_exe)
    set_target_properties(carrier_pkg__only_exe PROPERTIES
        NROS_STACK_FLOOR_KEY "carrier_pkg@b1"
        NROS_STACK_FLOOR_REPORT "${CMAKE_BINARY_DIR}/nros/stack-floor/report.json"
        NROS_STACK_FLOOR_ENTRY carrier_pkg
        NROS_STACK_FLOOR_PROGRAM only_exe
        NROS_STACK_FLOOR_BOARD b1
        NROS_STACK_FLOOR_STATED "")
    set_property(GLOBAL APPEND PROPERTY NROS_STACK_FLOOR_PROGRAM_IMAGES carrier_pkg__only_exe)
    _nros_stack_floor_decide_program_keys()
    get_target_property(_k carrier_pkg__only_exe NROS_STACK_FLOOR_KEY)
    get_target_property(_r carrier_pkg__only_exe NROS_STACK_FLOOR_REPORT)
    message(STATUS "VERDICT: key=${_k}")
    file(RELATIVE_PATH _rel "${CMAKE_BINARY_DIR}" "${_r}")
    message(STATUS "VERDICT: report=${_rel}")
endmacro()
