# NuttX application Makefile for nros C talker example
#
# This Makefile is designed for NuttX's external application build system.
# To integrate, add this directory to CONFIGURED_APPS in NuttX or symlink
# into nuttx-apps/examples/.

include $(APPDIR)/Make.defs

PROGNAME  = nuttx_c_talker
PRIORITY  = SCHED_PRIORITY_DEFAULT
STACKSIZE = $(CONFIG_DEFAULT_TASK_STACKSIZE)
MODULE    = CONFIG_NROS_NUTTX_C_TALKER

MAINSRC = src/main.c

# nros-c headers and generated message bindings
CFLAGS += -I$(NROS_INSTALL_DIR)/include

include $(APPDIR)/Application.mk
