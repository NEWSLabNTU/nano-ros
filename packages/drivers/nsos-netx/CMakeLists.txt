cmake_minimum_required(VERSION 3.16)
project(nsos-netx C)

# NSOS-style NetX BSD compatibility shim — forwards nx_bsd_* calls to
# the host POSIX socket API. No NetX Duo or ThreadX dependency.

add_library(nsos-netx STATIC src/nsos_netx.c)

target_include_directories(nsos-netx PUBLIC include)

# Standalone — no external headers needed (uses POSIX <sys/socket.h>).
