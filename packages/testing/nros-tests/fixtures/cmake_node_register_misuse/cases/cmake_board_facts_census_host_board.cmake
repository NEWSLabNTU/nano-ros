# Case `cmake_board_facts_census_host_board` — see ../CMakeLists.txt.
#
# Issue 2751: the host census build of an OWNED entry configures the package
# alone, in a dir with no system.toml, with `-DNROS_LEAF_BOARD=linux`, and the
# entry verb's host deploy (`linux`) in `NROS_DEPLOY`. Board facts must answer
# from the board the configure states, not ask for an image `linux` that no
# file declares (which FATAL_ERRORs at configure).
set(VERDICT_PROJECT census_pkg)
macro(verdict_body)
    include("${_NROS_NODE_REGISTER_DIR}/NanoRosBoardFacts.cmake")
    set(NROS_LEAF_BOARD linux)
    set(NROS_DEPLOY linux)
    nros_resolve_board_facts()
    message(STATUS "VERDICT: board facts resolved from the stated board")
endmacro()
