# Phase 23.2 — IDF project used by `scripts/arduino/build-libnanoros.sh`
# to cross-compile every nano-ros static archive needed inside the
# Arduino library (`libnros_c.a`, `libzpico.a`, `libnros_platform_esp_idf.a`,
# etc.). It is not user-facing — Arduino sketches consume the bundled
# `arduino/nros/src/<arch>/libnanoros.a` directly.
#
# Layout:
#   main/                          — minimal app stub (IDF requires a `main`)
#   ../../../integrations/esp-idf  — pulled as a component via
#                                    EXTRA_COMPONENT_DIRS
#   ../../../packages/core/nros-platform-esp-idf  — child component

cmake_minimum_required(VERSION 3.16)

# ESP-IDF derives each component's name from its containing directory.
# The Phase 139 integration shell lives at `integrations/esp-idf/` —
# which would make the component name "esp-idf" (confusing, conflicts
# with the framework). Stage a symlinked component named `nano-ros`
# under the builder's local `components/` dir before IDF's component
# scan runs, then point EXTRA_COMPONENT_DIRS at that staging dir.
file(MAKE_DIRECTORY "${CMAKE_CURRENT_LIST_DIR}/components")
file(CREATE_LINK
    "${CMAKE_CURRENT_LIST_DIR}/../../../integrations/esp-idf"
    "${CMAKE_CURRENT_LIST_DIR}/components/nano-ros"
    SYMBOLIC)

set(EXTRA_COMPONENT_DIRS
    "${CMAKE_CURRENT_LIST_DIR}/components"
    "${CMAKE_CURRENT_LIST_DIR}/../../../packages/core/nros-platform-esp-idf")

# Phase 21.6 — Corrosion needs the Rust target triple explicitly
# when CMake is in cross-compile mode (IDF). Map the IDF target
# chip to its Rust counterpart so `nros-c` / `nros-cpp` build for
# the right ABI.
if(NOT DEFINED Rust_CARGO_TARGET)
    if("$ENV{IDF_TARGET}" STREQUAL "esp32c3" OR IDF_TARGET STREQUAL "esp32c3")
        set(Rust_CARGO_TARGET "riscv32imc-unknown-none-elf" CACHE STRING "" FORCE)
    elseif("$ENV{IDF_TARGET}" STREQUAL "esp32s3" OR IDF_TARGET STREQUAL "esp32s3")
        set(Rust_CARGO_TARGET "xtensa-esp32s3-none-elf"     CACHE STRING "" FORCE)
    elseif("$ENV{IDF_TARGET}" STREQUAL "esp32" OR IDF_TARGET STREQUAL "esp32")
        set(Rust_CARGO_TARGET "xtensa-esp32-none-elf"       CACHE STRING "" FORCE)
    endif()
endif()

# Phase 21.6 — portable-atomic on no-CAS targets (riscv32imc,
# xtensa-esp32, xtensa-esp32s3) bails at compile time unless one of
# `critical-section`, `unsafe-assume-single-core`, or the matching cfg
# is set. ESP32-C3 / S3 / original are all single-core in their stock
# IDF configurations — flip the unsafe-assume-single-core cfg via
# RUSTFLAGS so portable-atomic compiles for the cross target.
set(ENV{RUSTFLAGS} "--cfg portable_atomic_unsafe_assume_single_core")

include($ENV{IDF_PATH}/tools/cmake/project.cmake)
project(nano_ros_arduino_lib_builder)
