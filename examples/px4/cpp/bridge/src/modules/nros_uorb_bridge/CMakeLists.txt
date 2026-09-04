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
    set(_ffi_a "${CMAKE_CURRENT_LIST_DIR}/../../../ffi/target-bridge-ffi/release/libnros_px4_bridge_ffi.a")
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

# phase-325 W3.3 — the OUTWARD backend, selectable.
#
# W3.2's rule is that the outward backend is a build-time CHOICE, not a
# directory axis. It was neither until now: `NrosUorbBridge.cpp` guards
# `NROS_BRIDGE_RMW` with `#ifndef` and defaults it to "zenoh", and NOTHING in
# cmake ever defined it — so the choice existed in the source and could not be
# made from outside. One backend does not demonstrate selection; it
# demonstrates a hardcoded bridge with extra ceremony (W3.3).
#
# Two things must agree and are driven from the SAME value:
#   * `BACKENDS uorb <rmw>` — drives the generated `nros_app_register_backends()`
#     call, so the backend is REGISTERED.
#   * `-DNROS_BRIDGE_RMW="<rmw>"` — the name the module hands to
#     `NrosCppSessionSpec`, so the right session is SELECTED by name rather
#     than by registration order.
#
# They are separate mechanisms, and the two halves fail differently:
#   * BACKENDS vs the ARCHIVE is already guarded — `nros_px4_add_module` runs an
#     `llvm-nm --defined-only` precheck for `nros_rmw_<name>_register` and
#     FATAL_ERRORs with the exact cargo command (NanoRosPx4Module.cmake:329).
#     When llvm-nm is absent it says so and the link catches it ~10 min later.
#   * BACKENDS vs the NAME in the session spec is NOT guarded by anything.
#     Registering zenoh while asking for "xrce" by name opens nothing and
#     surfaces as a transport failure, which reads as a dead network rather
#     than a wiring mistake. Driving both from one value is what removes it.
#
# The cargo half is NOT here. A networked backend is compiled into
# libnros_cpp.a by `--features rmw-<rmw>-cffi`, which `just px4
# build-bridge-example rmw=<rmw>` passes; this file only names it. That split is
# deliberate (the archive is built before cmake runs) and is why the recipe
# exports the same value it builds with.
if(DEFINED NROS_PX4_BRIDGE_RMW AND NOT "${NROS_PX4_BRIDGE_RMW}" STREQUAL "")
    set(_rmw "${NROS_PX4_BRIDGE_RMW}")
elseif(NOT "$ENV{NROS_PX4_BRIDGE_RMW}" STREQUAL "")
    set(_rmw "$ENV{NROS_PX4_BRIDGE_RMW}")
else()
    set(_rmw "zenoh")
endif()

# `nros_px4_add_module` already rejects an unknown BACKENDS entry
# (NanoRosPx4Module.cmake:226). This fires EARLIER and names the variable the
# caller actually set, so a typo in `rmw=` reads as a typo in `rmw=` rather than
# as an unknown backend several frames down.
if(NOT _rmw MATCHES "^(zenoh|xrce|cyclonedds)$")
    message(FATAL_ERROR
        "nros_uorb_bridge: NROS_PX4_BRIDGE_RMW is '${_rmw}', which is not a\n"
        "networked backend this module can select. Use one of: zenoh xrce cyclonedds\n"
        "(uorb is the INWARD side and is always registered).\n"
        "Set it with:  just px4 build-bridge-example rmw=<name>")
endif()
message(STATUS "nros_uorb_bridge: outward backend = ${_rmw}")

nros_px4_add_module(
	MODULE   modules__nros_uorb_bridge
	MAIN     nros_uorb_bridge
	# zenoh-pico's z_open allocates sizeable buffers + spawns its read/lease
	# tasks; the default module stack is not sized for it (#0436 probe).
	STACK_MAIN 32768
	# uorb = inward (in-firmware, zero-copy). zenoh = outward; a networked
	# backend contributes no sources here — it is compiled INTO libnros_cpp.a by
	# `--features rmw-zenoh-cffi`, and this list only drives the register call.
	BACKENDS uorb ${_rmw}
	# `<nros/bridge.hpp>` (MultiExecutor) is gated on NROS_CPP_STD rather than
	# __STDC_HOSTED__ on purpose (issue 0332: a hosted compiler can be run
	# -nostdinc++ against a minimal libcpp with no <string>/<vector>). PX4 SITL is
	# real posix with full libstdc++, so opt in explicitly.
	COMPILE_FLAGS -DNROS_CPP_STD=1
	INCLUDES ${_gen}
	SRCS
		${CMAKE_CURRENT_LIST_DIR}/NrosUorbBridge.cpp
)

# The generated headers' Rust bodies. PUBLIC so it propagates to the final PX4
# link, matching how the helper attaches the nano-ros archives.
if(TARGET modules__nros_uorb_bridge)
    # phase-325 W3.3 — the outward backend's NAME, as a proper definition.
    #
    # NOT via `COMPILE_FLAGS`: that property is a raw string appended to the
    # command line, so the quotes needed around a string macro have to survive
    # cmake, the generator and the shell. Passing
    # `-DNROS_BRIDGE_RMW="\"${_rmw}\""` there produced `""zenoh""` and g++
    # rejected it with
    #     error: unable to find string literal operator 'operator""zenoh'
    # ten minutes into a SITL build. `target_compile_definitions` quotes the
    # value itself, which is what it is for.
    target_compile_definitions(modules__nros_uorb_bridge PRIVATE
        NROS_BRIDGE_RMW="${_rmw}")
    target_link_libraries(modules__nros_uorb_bridge PUBLIC "${_ffi_a}")
endif()
