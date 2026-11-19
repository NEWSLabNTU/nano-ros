# Issue 2706 -- a Zephyr interrupt (a k_timer expiry, which runs in the system
# clock's interrupt handler) wakes a parked executor through
# `GuardCondition::trigger_from_isr`. Rust `rust_main` plus a small C helper
# that owns the timer and stamps its expiry.
cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(isr_wake_zephyr_entry)

if(NOT CONFIG_NROS_RMW_ZENOH)
    message(FATAL_ERROR
        "isr-wake-zephyr-native-sim is a zenoh image: build it with "
        "-DNROS_IMAGE=zephyr_zenoh (system.toml)")
endif()
set(EXTRA_CARGO_ARGS --no-default-features --features rmw-zenoh)

target_sources(app PRIVATE src/isr_timer.c)
rust_cargo_application()
