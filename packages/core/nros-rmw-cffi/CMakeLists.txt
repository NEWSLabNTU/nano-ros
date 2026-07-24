cmake_minimum_required(VERSION 3.16)
project(nros_rmw_cffi VERSION 0.1.0 LANGUAGES C)

# RFC-0054 (phase-299): the C ABI headers moved to the pure header
# package `packages/core/nros-rmw-abi/` (the SSoT). This package is now
# Rust-only (the committed-bindgen consumer). Pull in the header targets
# (incl. the back-compat `nros_rmw_cffi_headers` alias) when this
# directory is added standalone.
if(NOT TARGET nros_rmw_abi_headers)
    add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../nros-rmw-abi"
                     "${CMAKE_CURRENT_BINARY_DIR}/nros-rmw-abi")
endif()
