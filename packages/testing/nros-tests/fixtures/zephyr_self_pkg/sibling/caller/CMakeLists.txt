cmake_minimum_required(VERSION 3.20.0)
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(caller)
# Sibling form: the bringup arg names the self-pkg dir, resolved relative to
# this caller — must still hit the M-F.3 self-pkg branch. That branch is
# `system.toml` BESIDE a package marker since phase-445 W3b (RFC-0098 D3); the
# parenthetical here said "no system.toml in alpha_pkg/", which described the
# retired `package.xml`-tuple shape and has been false since phase-445 W5.
nros_system_generate(alpha_pkg)
