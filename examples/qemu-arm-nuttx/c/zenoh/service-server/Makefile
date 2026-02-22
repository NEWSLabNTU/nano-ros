# NuttX application Makefile for nros C service server example

include $(APPDIR)/Make.defs

PROGNAME  = nuttx_c_service_server
PRIORITY  = SCHED_PRIORITY_DEFAULT
STACKSIZE = $(CONFIG_DEFAULT_TASK_STACKSIZE)
MODULE    = CONFIG_NROS_NUTTX_C_SERVICE_SERVER

MAINSRC = src/main.c

CFLAGS += -I$(NROS_INSTALL_DIR)/include

include $(APPDIR)/Application.mk
