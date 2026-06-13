cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(caller)
# Sibling form: the bringup arg names the self-pkg dir, resolved relative to
# this caller — must still hit the M-F.3 self-pkg branch (no system.toml in
# alpha_pkg/).
nros_system_generate(alpha_pkg)
