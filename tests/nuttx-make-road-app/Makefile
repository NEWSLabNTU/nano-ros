# phase-550 W1 — see Kconfig.
include $(APPDIR)/Make.defs

PROGNAME  = nros_make_road_smoke
PRIORITY  = SCHED_PRIORITY_DEFAULT
STACKSIZE = $(CONFIG_DEFAULT_TASK_STACKSIZE)
MODULE    = $(CONFIG_NROS_MAKE_ROAD_SMOKE)

MAINSRC = nros_make_road_smoke_main.c

include $(APPDIR)/Application.mk
