cmake_minimum_required(VERSION 3.20)

# This component is intended to be consumed as an ESP-IDF component.
# Place it under `components/nros_platform_esp_idf/` in your IDF
# project (or register it via `EXTRA_COMPONENT_DIRS`); IDF's
# `idf_component_register` provides the kernel + driver headers
# automatically. The block below is gated so a standalone
# `cmake -B build` outside an IDF project still produces a sensible
# error message instead of silent garbage.

if(COMMAND idf_component_register)
    idf_component_register(
        SRCS src/platform.c src/net.c src/timer.c
        INCLUDE_DIRS "${CMAKE_CURRENT_LIST_DIR}/../nros-platform-api/include"
        REQUIRES freertos esp_timer esp_hw_support esp_system lwip)
    return()
endif()

project(nros-platform-esp-idf
    VERSION 0.1.0
    DESCRIPTION "Native C implementation of the nros canonical platform ABI for Espressif ESP-IDF"
    LANGUAGES C)

message(FATAL_ERROR
    "nros-platform-esp-idf must be built inside an ESP-IDF project. "
    "Add this directory to EXTRA_COMPONENT_DIRS (or symlink under "
    "components/) and rebuild your IDF application.")
