# SPDX-License-Identifier: Apache-2.0
# Phase 88.15.e — Zephyr native_sim nros-log smoke fixture

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(logging_smoke_zephyr_native_sim)

# Pulls the nros module — libnros_c.a + the Phase-88 Zephyr
# `PlatformLog` impl (routes records through LOG_INF / LOG_ERR /
# etc. under the hood).
target_sources(app PRIVATE src/main.c)
