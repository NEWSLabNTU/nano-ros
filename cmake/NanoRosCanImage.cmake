# cmake/NanoRosCanImage.cmake
#
# RFC-0132 D4 (phase-560 W9.e) -- the cmake road's half of the CAN frame
# device's delivery, for a C or C++ entry on a host-stack board (`linux`,
# `freertos-posix`, `threadx-linux`: Linux processes, so SocketCAN).
#
#   nros_entry_can_image(<target> <board>)
#
# Called by `nano_ros_entry` inside the entry's scope, where the image's own
# `NROS_IMAGE_NET` shadows the configure's. When the image names a CAN frame
# device (`[image.<id>.can] device`), it:
#
#   1. asks the CLI for the device (`nros_board_net_c ... CAN_DEVICE_VAR`), so
#      the `can` row is decoded by the one codec, never by a regex here;
#   2. links `nros_can_socketcan`: the `socketcan` backend
#      (`nros-board-linux/c/can_socketcan.c`, the one source every Linux-process
#      board compiles) and the device store
#      (`nros-board-common/c/nros_board_can_image.c`);
#   3. compiles a per-entry hand-off TU whose constructor stores the device
#      before `main`, so the application's `open(NULL)` opens it.
#
# In a census host build (`NROS_LEAF_CENSUS_HOST`) it links the census mock
# instead (RFC-0132 D6) and hands nothing over.
#
# Per entry, not per configure: two images of one configure may name two
# devices, and the hand-off is in each image's own executable. An image that
# names no device gets neither the library nor the TU, so its link is
# unchanged. No cache variable and no environment variable carries the
# device: it is read from the image network in scope.
#
#   nros_entry_can_defaults(<target> <board>)
#
# On a board with no CAN frame backend of its own (any stack but "host"; a
# Zephyr entry never reaches here, its backend is `nros-platform-zephyr`), it
# compiles the weak fail-loud defaults
# (`nros-board-common/c/nros_board_can_unsupported.c`) into the entry, naming
# the board (RFC-0132 D3, issue 2788). An object of the image, so a board
# package's or a consumer's strong driver in any other object wins; an entry
# that never calls the driver carries five small unused weak functions.
#
#   nros_link_can_census_mock(<target>)
#
# Links the census mock into a host target that is not an entry: the
# metadata probe's one-node executables (issue 2787), which construct the same
# node the census counts and so call the same driver.

include_guard(GLOBAL)
include("${CMAKE_CURRENT_LIST_DIR}/NanoRosBoardNet.cmake")

set(_NROS_CAN_IMAGE_ROOT "${CMAKE_CURRENT_LIST_DIR}/.." CACHE INTERNAL "")

function(_nros_can_socketcan_library)
    if(TARGET nros_can_socketcan)
        return()
    endif()
    get_filename_component(_root "${_NROS_CAN_IMAGE_ROOT}" ABSOLUTE)
    add_library(nros_can_socketcan STATIC
        "${_root}/packages/boards/nros-board-linux/c/can_socketcan.c"
        "${_root}/packages/boards/nros-board-common/c/nros_board_can_image.c")
    target_include_directories(nros_can_socketcan
        PUBLIC "${_root}/packages/boards/nros-board-common/c"
        PRIVATE "${_root}/packages/platform/nros-platform-api/include")
    # `clock_gettime`, `IFNAMSIZ` and the socket ioctls under a strict -std.
    target_compile_definitions(nros_can_socketcan PRIVATE _DEFAULT_SOURCE)
    set_target_properties(nros_can_socketcan PROPERTIES LINKER_LANGUAGE C)
endfunction()

# RFC-0132 D6 -- the census host build's CAN backend: the mock's C twin
# (`nros-can-mock/c/census_mock.c`) and the device store, as archive members,
# so an entry that never calls the driver links none of them.
function(_nros_can_census_mock_library)
    if(TARGET nros_can_census_mock)
        return()
    endif()
    get_filename_component(_root "${_NROS_CAN_IMAGE_ROOT}" ABSOLUTE)
    # The device store too: its reader is board-internal, but a census build
    # links the same program the image does, store reference included.
    add_library(nros_can_census_mock STATIC
        "${_root}/packages/drivers/can/nros-can-mock/c/census_mock.c"
        "${_root}/packages/boards/nros-board-common/c/nros_board_can_image.c")
    target_include_directories(nros_can_census_mock
        PUBLIC "${_root}/packages/boards/nros-board-common/c"
        PRIVATE "${_root}/packages/platform/nros-platform-api/include")
    set_target_properties(nros_can_census_mock PROPERTIES LINKER_LANGUAGE C)
endfunction()

# RFC-0132 D6 -- the mock, for a host target that calls the driver and never
# reaches a bus: an entry of a census host build, or a metadata probe
# (issue 2787). An archive, so a target that never calls the driver links none
# of it.
function(nros_link_can_census_mock target)
    get_property(_langs GLOBAL PROPERTY ENABLED_LANGUAGES)
    if(NOT "C" IN_LIST _langs)
        enable_language(C)
    endif()
    _nros_can_census_mock_library()
    target_link_libraries(${target} PRIVATE nros_can_census_mock)
endfunction()

# RFC-0132 D3 (issue 2788) -- the weak fail-loud defaults, as a per-entry TU
# that names the board and includes the one shared source.
function(_nros_can_unsupported_defaults target board)
    get_property(_langs GLOBAL PROPERTY ENABLED_LANGUAGES)
    if(NOT "C" IN_LIST _langs)
        enable_language(C)
    endif()
    get_filename_component(_root "${_NROS_CAN_IMAGE_ROOT}" ABSOLUTE)
    set(_src "${_root}/packages/boards/nros-board-common/c/nros_board_can_unsupported.c")
    set(_tu "${CMAKE_CURRENT_BINARY_DIR}/${target}_nros_can_unsupported.c")
    string(CONCAT _content
"/* Generated by nros_entry_can_image (cmake/NanoRosCanImage.cmake), RFC-0132 D3:\n"
" * board `${board}` has no CAN frame backend, so the driver's weak fail-loud\n"
" * defaults are an object of this image; a strong driver in any other object wins. */\n"
"#define NROS_BOARD_CAN_BOARD_NAME \"${board}\"\n"
"#include \"${_src}\"\n")
    if(EXISTS "${_tu}")
        file(READ "${_tu}" _old)
    else()
        set(_old "")
    endif()
    if(NOT _old STREQUAL _content)
        file(WRITE "${_tu}" "${_content}")
    endif()
    target_sources(${target} PRIVATE "${_tu}")
    set_source_files_properties("${_tu}" TARGET_DIRECTORY ${target} PROPERTIES
        INCLUDE_DIRECTORIES "${_root}/packages/platform/nros-platform-api/include"
        OBJECT_DEPENDS "${_src}")
endfunction()

# nros_entry_can_defaults(<target> <board>)
#
# Issue 2788 (RFC-0132 D3) -- called by `nano_ros_entry` for an entry on a
# cross board (not posix, not Zephyr). A board whose stack is not "host" has
# no CAN frame backend in nano-ros, so the entry gets the weak fail-loud
# defaults: the same application links on every board and fails loud at
# `open`, naming the board. A census host build gets the mock instead
# (`nros_entry_can_image`), so this is a no-op there.
function(nros_entry_can_defaults target board)
    if(NOT "$ENV{NROS_LEAF_CENSUS_HOST}" STREQUAL "")
        return()
    endif()
    nros_board_net_c("${board}" _unused STACK_VAR _stack)
    if(NOT _stack STREQUAL "host")
        _nros_can_unsupported_defaults(${target} "${board}")
    endif()
endfunction()

function(nros_entry_can_image target board)
    # RFC-0132 D6 -- the census builds its host image with the mock: it counts
    # the program, it never reaches a bus, and the image's own device (a
    # SocketCAN interface, a devicetree node, a board's `[canN]`) is not the
    # host's. Every entry of a census host build gets it, whatever the image
    # names, so an entry that calls the driver links on the census's road too.
    if(NOT "$ENV{NROS_LEAF_CENSUS_HOST}" STREQUAL "")
        nros_link_can_census_mock(${target})
        return()
    endif()
    nros_image_net(_net)
    if("${_net}" STREQUAL "")
        return()
    endif()
    nros_board_net_c("${board}" _unused CAN_DEVICE_VAR _dev STACK_VAR _stack)
    # A device rides only a host-stack board's text (`nros configure` refuses
    # it elsewhere); a "board" stack's `[canN]` name is the board package's.
    if("${_dev}" STREQUAL "" OR NOT _stack STREQUAL "host")
        return()
    endif()
    get_property(_langs GLOBAL PROPERTY ENABLED_LANGUAGES)
    if(NOT "C" IN_LIST _langs)
        enable_language(C)
    endif()
    _nros_can_socketcan_library()
    set(_tu "${CMAKE_CURRENT_BINARY_DIR}/${target}_nros_can_image.c")
    string(LENGTH "${_dev}" _len)
    string(CONCAT _content
"/* Generated by nros_entry_can_image (cmake/NanoRosCanImage.cmake), RFC-0132 D4:\n"
" * the image's CAN frame device, handed to the backend's open(NULL) before main. */\n"
"#include <stdio.h>\n"
"#include <stdlib.h>\n"
"#include \"nros_board_can_image.h\"\n"
"\n"
"__attribute__((constructor)) static void nros_entry_can_image(void) {\n"
"    if (nros_board_can_set_image_device(\"${_dev}\", ${_len}u) != 0) {\n"
"        fprintf(stderr, \"NROS_IMAGE_NET: CAN device `${_dev}` does not fit the board's store\\n\");\n"
"        abort();\n"
"    }\n"
"}\n")
    if(EXISTS "${_tu}")
        file(READ "${_tu}" _old)
    else()
        set(_old "")
    endif()
    if(NOT _old STREQUAL _content)
        file(WRITE "${_tu}" "${_content}")
    endif()
    target_sources(${target} PRIVATE "${_tu}")
    target_link_libraries(${target} PRIVATE nros_can_socketcan)
endfunction()
