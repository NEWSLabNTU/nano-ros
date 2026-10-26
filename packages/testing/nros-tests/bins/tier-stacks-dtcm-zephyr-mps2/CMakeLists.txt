# Issue 2084 -- a link-layout witness: one level spawned on the shim's tier
# pool and one on a declared stack (the generated C/C++ entry's spelling),
# with CONFIG_NROS_ZEPHYR_STACKS_IN_DTCM on a devicetree that has a "DTCM"
# region (app.overlay carves one out of mps2_an385's SRAM). The test
# (tests/zephyr_link_layout.rs) reads the ELF: `nros_tier_stacks`,
# `__nros_tier_stack_1` and `nros_thread_stacks` must all sit in the DTCM
# output section.
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(tier_stacks_dtcm_zephyr_mps2)
target_sources(app PRIVATE src/main.c)
