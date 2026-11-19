# Case `cmake_board_facts_board_outranks_leaf_board` — see ../CMakeLists.txt.
#
# Issue 2751: the census fallback to `NROS_LEAF_BOARD` applies only when no
# board is named. A named `NROS_BOARD` still wins: here it names a board no
# descriptor declares, so the configure must FAIL naming THAT board, never
# quietly answer for `linux`.
set(VERDICT_PROJECT census_pkg)
macro(verdict_body)
    include("${_NROS_NODE_REGISTER_DIR}/NanoRosBoardFacts.cmake")
    set(NROS_BOARD no_such_board_2751)
    set(NROS_LEAF_BOARD linux)
    set(NROS_DEPLOY linux)
    nros_resolve_board_facts()
    message(STATUS "VERDICT: board facts resolved")
endmacro()
