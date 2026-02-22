# NuttX application Makefile for nros C listener example

include $(APPDIR)/Make.defs

PROGNAME  = nuttx_c_listener
PRIORITY  = SCHED_PRIORITY_DEFAULT
STACKSIZE = $(CONFIG_DEFAULT_TASK_STACKSIZE)
MODULE    = CONFIG_NROS_NUTTX_C_LISTENER

MAINSRC = src/main.c

CFLAGS += -I$(NROS_INSTALL_DIR)/include

include $(APPDIR)/Application.mk
