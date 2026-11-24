# SPDX-License-Identifier: Apache-2.0
#
# Issue 2878 -- a Zephyr image selects a tracing backend it registers itself.
#
# A plain Zephyr application, no nano-ros module: what it proves is a property
# of the Zephyr tree nano-ros's patch set leaves behind, so nothing else is
# linked. `src/main.c` registers `nros_test_ring` with TRACING_BACKEND_DEFINE()
# and `prj.conf` selects it by CONFIG_TRACING_BACKEND_CUSTOM(_NAME), the two
# symbols `scripts/zephyr/tracing-custom-backend-patch.sh` adds.
#
# Driver: just zephyr test-tracing-custom-backend (builds, runs, checks).

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_tracing_custom_backend)

target_sources(app PRIVATE src/main.c)
