# nano-ros uORB → RMW bridge (phase-325 W3, issue 0362).
#
# Two things this needs beyond the W2 demo, both consequences of publishing on a
# WIRE rather than into uORB:
#
#   1. the generated `px4_msgs` C++ headers — `nros generate-px4-msgs --lang cpp`
#      (their RIHS01 type hash is what lets a ROS 2 peer match the topic), and
#   2. the FFI staticlib holding those headers' Rust bodies. A normal CMake
#      consumer gets it from `nros_generate_interfaces(LANGUAGE CPP)`, which a PX4
#      module never runs, so the bridge builds it explicitly (`ffi/`).
#
# Both are produced by `just px4 build-bridge-example`, which passes their paths
# in. Building this module directly without that recipe fails loudly below rather
# than emitting an image whose publisher silently never matches a subscriber.

include("$ENV{NROS_REPO_DIR}/integrations/px4/NanoRosPx4Module.cmake")

# Where `nros generate-px4-msgs --lang cpp` wrote `px4_msgs/msg/*.hpp`.
if(DEFINED NROS_PX4_BRIDGE_GEN AND NOT "${NROS_PX4_BRIDGE_GEN}" STREQUAL "")
    set(_gen "${NROS_PX4_BRIDGE_GEN}")
elseif(NOT "$ENV{NROS_PX4_BRIDGE_GEN}" STREQUAL "")
    set(_gen "$ENV{NROS_PX4_BRIDGE_GEN}")
else()
    set(_gen "$ENV{NROS_REPO_DIR}/build/px4-bridge/generated")
endif()

if(NOT EXISTS "${_gen}/px4_msgs/msg")
    message(FATAL_ERROR
        "nros_uorb_bridge: generated px4_msgs C++ headers not found at\n"
        "    ${_gen}/px4_msgs/msg\n"
        "Generate them (issue 0362):\n"
        "    nros generate-px4-msgs --lang cpp --topics debug_key_value \\\n"
        "        --ros-edition jazzy -o ${_gen}\n"
        "or just run:  just px4 build-bridge-example")
endif()

# The FFI staticlib carrying those headers' Rust bodies
# (`nros_cpp_{publish,serialize,deserialize}_px4_msgs_msg_*`).
if(DEFINED NROS_PX4_BRIDGE_FFI_ARCHIVE AND NOT "${NROS_PX4_BRIDGE_FFI_ARCHIVE}" STREQUAL "")
    set(_ffi_a "${NROS_PX4_BRIDGE_FFI_ARCHIVE}")
elseif(NOT "$ENV{NROS_PX4_BRIDGE_FFI_ARCHIVE}" STREQUAL "")
    set(_ffi_a "$ENV{NROS_PX4_BRIDGE_FFI_ARCHIVE}")
else()
    set(_ffi_a "${CMAKE_CURRENT_LIST_DIR}/../../../ffi/target/release/libnros_px4_bridge_ffi.a")
endif()

if(NOT EXISTS "${_ffi_a}")
    message(FATAL_ERROR
        "nros_uorb_bridge: the generated-message FFI archive is missing:\n"
        "    ${_ffi_a}\n"
        "Build it (its build.rs reads NROS_PX4_BRIDGE_GEN):\n"
        "    NROS_PX4_BRIDGE_GEN=${_gen} cargo build --release \\\n"
        "        --manifest-path examples/px4/cpp/bridge/ffi/Cargo.toml\n"
        "or just run:  just px4 build-bridge-example")
endif()

nros_px4_add_module(
	MODULE   modules__nros_uorb_bridge
	MAIN     nros_uorb_bridge
	# uorb = inward (in-firmware, zero-copy). zenoh = outward; a networked
	# backend contributes no sources here — it is compiled INTO libnros_cpp.a by
	# `--features rmw-zenoh-cffi`, and this list only drives the register call.
	BACKENDS uorb zenoh
	INCLUDES ${_gen}
	SRCS
		${CMAKE_CURRENT_LIST_DIR}/NrosUorbBridge.cpp
)

# The generated headers' Rust bodies. PUBLIC so it propagates to the final PX4
# link, matching how the helper attaches the nano-ros archives.
if(TARGET modules__nros_uorb_bridge)
    target_link_libraries(modules__nros_uorb_bridge PUBLIC "${_ffi_a}")
endif()
