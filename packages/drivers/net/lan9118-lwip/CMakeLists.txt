cmake_minimum_required(VERSION 3.22)
project(lan9118-lwip LANGUAGES C)

# LWIP_DIR is required — consumer must set it (or inherit from env)
# phase-484 W2c — `nros locate` first (RFC-0103 D4), when this is built inside
# a nano-ros configure that loaded it; a standalone configure keeps `-D`/env.
if(NOT DEFINED LWIP_DIR AND COMMAND nros_locate_var)
    nros_locate_var(LWIP_DIR)
endif()
if(NOT DEFINED LWIP_DIR)
    if(DEFINED ENV{LWIP_DIR})
        set(LWIP_DIR $ENV{LWIP_DIR})
    else()
        message(FATAL_ERROR
            "LWIP_DIR not set. Point it at the lwIP source directory.\n"
            "Run `./scripts/bootstrap.sh` (contributors: `just setup freertos`) to download, then:\n"
            "  cmake -DLWIP_DIR=<repo>/third-party/freertos/lwip ..")
    endif()
endif()

add_library(lan9118_lwip STATIC src/lan9118_lwip.c)

target_include_directories(lan9118_lwip
    PUBLIC  include                              # lan9118_lwip.h
    PRIVATE ${LWIP_DIR}/src/include              # lwIP core headers
)

# lwipopts.h and FreeRTOS headers are board-specific.
# Consumer must provide them, e.g.:
#   target_include_directories(lan9118_lwip PRIVATE ${CONFIG_DIR})
#   target_link_libraries(lan9118_lwip PRIVATE freertos_kernel)
