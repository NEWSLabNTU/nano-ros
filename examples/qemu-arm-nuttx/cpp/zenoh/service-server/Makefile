# Phase 157 — NuttX external-app Makefile for the CPP service-server example.
#
# Delegates to apps/Application.mk for compile + link; the parent
# integrations/nuttx/Make.defs already put libnros_cpp.a on
# EXTRA_LIBS and the nano-ros CPP headers on INCDIR_PREFIX, so this
# file only declares the example's own SRCS + the built-in command
# registration (PROGNAME / PRIORITY / STACKSIZE / MODULE).
#
# Phase 157 — NROS_NUTTX_EXTERNAL_APP=1 flips main.cpp's
# NROS_APP_MAIN_REGISTER() auto-detect from REGISTER_VOID (used by
# the Phase 144.6 QEMU cmake bring-up via the nros-board-nuttx-
# qemu-arm Rust shim) to REGISTER_NUTTX which emits the
# `int main(int, char**)` symbol Application.mk renames to
# `<PROGNAME>_main` via `-Dmain=<PROGNAME>_main`.

include $(APPDIR)/Make.defs

PROGNAME  = nuttx_cpp_service_server
PRIORITY  = SCHED_PRIORITY_DEFAULT
STACKSIZE = 16384
MODULE    = $(CONFIG_NROS_EXAMPLE_SERVICE_SERVER_CPP)

CFLAGS   += -DNROS_NUTTX_EXTERNAL_APP=1
CXXFLAGS += -DNROS_NUTTX_EXTERNAL_APP=1

CXXSRCS = $(wildcard generated/*.cpp)
MAINSRC = src/main.cpp

include $(APPDIR)/Application.mk
