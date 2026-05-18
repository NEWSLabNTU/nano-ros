# Phase 23.5d — host transport-glue smoke test.
#
# Builds `arduino/nros/src/nros_arduino.cpp` against the mock
# Arduino + WiFi headers under `../mock_wifi/` and links it into a
# native binary. The test driver
# (`test_transport_host.cpp`) calls `set_nanoros_wifi_transports`
# + `nanoros_ping` and verifies the mock WiFi reports connected.
#
# Not built as part of the umbrella `add_subdirectory(<nano-ros>)`
# consumption shape — invoked via `just test-arduino-transport`
# from a host-platform configure, so the same `cmake` that
# `just qemu test` uses (no cross-compile flags).

cmake_minimum_required(VERSION 3.20)
project(nros_arduino_transport_host LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)  # inline-variable mocks for WiFi / Serial
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Resolve repo root from this directory (tests/arduino/test-transport-host/).
get_filename_component(_repo "${CMAKE_CURRENT_LIST_DIR}/../../.." ABSOLUTE)

add_executable(test_transport_host
    test_transport_host.cpp
    "${_repo}/arduino/nros/src/nros_arduino.cpp"
)

target_include_directories(test_transport_host PRIVATE
    "${CMAKE_CURRENT_LIST_DIR}/../mock_wifi"   # Arduino.h / WiFi.h stubs
    "${_repo}/arduino/nros/src"                # nros_arduino.h
    "${_repo}/packages/core/nros-c/include"    # nros/init.h (signature only)
)

enable_testing()
add_test(NAME transport_host_smoke COMMAND test_transport_host)
