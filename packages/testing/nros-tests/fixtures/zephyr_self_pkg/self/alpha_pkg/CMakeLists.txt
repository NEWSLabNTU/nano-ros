cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(alpha_pkg)
# Issue 1536 — Zephyr's `app` must have a source or GENERATE fails. Never
# compiled: this fixture is `west-configure` and asserts the bake below.
target_sources(app PRIVATE src/main.c)
# Self-pkg form: the bringup arg is `.` — the app pkg IS the self-pkg.
nros_system_generate(.)
