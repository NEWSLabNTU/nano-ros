# SPDX-License-Identifier: Apache-2.0
# Phase 117.14 — nros pub/sub demo on the ARM FVP Base_RevC AEMv8-R
# Cortex-A SMP target under Zephyr. Copy-out template per CLAUDE.md
# examples convention — no walk-up paths, no shared example-only
# helpers. The nros module supplies the C++ API + codegen.

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(nros_zephyr_aemv8r_cyclonedds_talker LANGUAGES CXX)

# Generate C++ message bindings for the Int32 type used by the talker.
# `nros_generate_interfaces()` is provided by the nros Zephyr module
# (CONFIG_NROS=y, CONFIG_NROS_CPP_API=y).
nros_generate_interfaces(std_msgs LANGUAGE CPP)

target_sources(app PRIVATE src/main.cpp)
