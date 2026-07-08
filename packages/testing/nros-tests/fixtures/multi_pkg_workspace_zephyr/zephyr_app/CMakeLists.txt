# Phase 212.H.1 fixture — Zephyr app that drives the adapter shim.
#
# Build:
#   west build -b native_sim/native/64 -d build \
#     packages/testing/nros-tests/fixtures/multi_pkg_workspace_zephyr/zephyr_app \
#     -- -DCONF_FILE="prj.conf;prj-zenoh.conf"
#
# The shim (`nros_system_generate(<bringup>)`) is exported by the
# nano-ros Zephyr module (`zephyr/cmake/nros_system_generate.cmake`,
# auto-included). Resolves the sibling `demo_bringup/` (Path A bringup
# pkg) and bakes its system.toml into ${CMAKE_BINARY_DIR}/nros-system/.

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_phase212_h1_fixture)

# Single-line consumer surface. The bringup pkg lives one level up
# (sibling to zephyr_app/). The shim resolves "demo_bringup" against
# ${CMAKE_CURRENT_SOURCE_DIR}/.. automatically.
nros_system_generate(demo_bringup)

# Issue 0154 — post-258 the shim bakes config only (no generated TU); Zephyr
# requires `app` to own a source, and this stub main consumes the baked header.
target_sources(app PRIVATE src/main.c)
