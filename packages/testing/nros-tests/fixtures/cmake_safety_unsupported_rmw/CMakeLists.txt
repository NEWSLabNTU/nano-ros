# Issue 2487 — the configure-VERDICT fixture behind
# `cmake_platform_matrix::safety_on_an_rmw_without_crc_is_refused_at_configure`
# (row `cmake_safety_unsupported_rmw_refused`).
#
# `safety` declared on an RMW that carries no E2E integrity path (Cyclone DDS
# declares no `safety` in its nros-rmw.toml `[rmw.capabilities]`) must
# FATAL_ERROR at configure time, naming the RMW. It used to WARN and build on,
# so the image had the validated-subscription API with no check behind it and
# every sample read as valid. The build stage configures this
# (`builder = "cmake-configure-verdict"`) and records the verdict; the test
# asserts it.
cmake_minimum_required(VERSION 3.22)
project(safety_unsupported_rmw C)

set(NROS_PLATFORM posix)
set(NROS_RMW      cyclonedds)
set(NROS_FEATURES safety)

# The FATAL_ERROR fires inside this call, so nothing below it would run. No
# executable is declared on purpose: the configure must never get that far.
add_subdirectory("@NROS_REPO_DIR@" nano_ros)
