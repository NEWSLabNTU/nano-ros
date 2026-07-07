# SPDX-License-Identifier: Apache-2.0
# phase-282 W2 (#145) — Zephyr streaming tx benchmark.
#
# A tight-loop zpico publisher (stress-zenoh talker semantics) on
# native_sim, used to measure the Zephyr zsock tx ceiling across the
# ZPICO_TX_BATCH / Z_FEATURE_TX_SPLIT_LOCK knobs. The native
# `nros-bench/stress-zenoh` listener is the counting/validating sink.
# See README.md for the build + measurement procedure.

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(stress_zenoh_zephyr)

# Benchmark shape knobs (cache vars so `west build -- -DSTRESS_*` works).
set(STRESS_COUNT 5000 CACHE STRING "number of publishes")
set(STRESS_SIZE 64 CACHE STRING "payload bytes (>=16)")
set(STRESS_EXPRESS 0 CACHE STRING "1 = declare the publisher express (bypass batching)")
set(STRESS_INTERVAL_MS 0 CACHE STRING "per-publish pacing, ms (0 = tight loop)")

target_sources(app PRIVATE src/main.c)
target_compile_definitions(app PRIVATE
    STRESS_COUNT=${STRESS_COUNT}
    STRESS_SIZE=${STRESS_SIZE}
    STRESS_EXPRESS=${STRESS_EXPRESS}
    STRESS_INTERVAL_MS=${STRESS_INTERVAL_MS})
