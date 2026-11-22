# SPDX-License-Identifier: Apache-2.0
# Issue 2738 -- a thread dies holding nros_log's dispatcher on a Zephyr SMP
# image. The fatal error handler says so once and hands the dispatcher back,
# so a later record is delivered; a second death, with the check skipped, is
# the negative control: its later record never arrives.

cmake_minimum_required(VERSION 3.20.0)

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(log_dead_holder_zephyr_smp)

target_sources(app PRIVATE src/main.c)
