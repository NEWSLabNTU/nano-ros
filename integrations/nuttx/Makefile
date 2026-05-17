############################################################################
# Phase 139.4 — NuttX integration shell (Makefile).
#
# Boilerplate NuttX app Makefile. The actual nano-ros static
# libraries are produced by the Rust / CMake build outside NuttX's
# tree; this Makefile only declares the consumer-app shape so
# `make menuconfig` surfaces nano-ros under External Modules.
############################################################################

include $(APPDIR)/Make.defs

# No C sources of our own — the symbol-level integration goes
# through `libnros_c.a` / `libnros_cpp.a` that the user's NuttX
# config links via $(LIBNROS).
CSRCS =

include $(APPDIR)/Application.mk
