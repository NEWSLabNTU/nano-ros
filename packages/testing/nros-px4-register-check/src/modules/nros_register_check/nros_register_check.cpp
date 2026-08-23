// nano-ros uORB RMW register-check — a PX4 one-shot command.
//
// Phase 115.K.4.5: the BUILD is the validation. Compiling this module links the
// whole nros-rmw-uorb backend inline inside a real px4_add_module() context,
// against real <uORB/uORB.h>, <uORB/SubscriptionCallback.hpp> and
// <px4_boardconfig.h>; calling nros_rmw_uorb_register() forces the linker to
// resolve every entry point the cffi adapter dispatches to. The runtime output
// only matters when somebody pokes it from the pxh shell.
//
// phase-244 D5 — the SITL-only weak `nros_rmw_cffi_register` link stub moved
// out to the `sitl_register_stub.c` build scaffold (the registry symbol is
// build wiring, not application logic). This TU carries only the check.
//
// phase-325 W0.2 — brought to PX4 convention: tab indent, a Kconfig beside it,
// and PRINT_MODULE_* usage strings, so `nros_register_check help` works and
// PX4's module-reference scraper can see it. Modelled on `src/systemcmds/gpio`
// — a one-shot COMMAND — and deliberately NOT on ModuleBase<T>: this does not
// daemonize, so it has no start/stop/status to offer, and the old invocation
// documented in this header (`nros_register_check start`) never existed.
//
// One PX4 convention deliberately not adopted: PX4 sources open with a BSD
// 3-clause block naming the PX4 Development Team. That is a LICENSING practice,
// not a style rule, and reproducing it here would misattribute copyright — this
// file is nano-ros's, under MIT OR Apache-2.0.

#include "nros_rmw_uorb.h"

#include <px4_platform_common/log.h>
#include <px4_platform_common/module.h>

#include <cstring>

static int nros_register_check_usage(const char *reason = nullptr)
{
	if (reason) {
		PX4_WARN("%s\n", reason);
	}

	PRINT_MODULE_DESCRIPTION(
		R"DESCR_STR(
### Description
Link/registration check for the nano-ros uORB RMW backend.

Calls `nros_rmw_uorb_register()` and reports the result. The interesting work
happens at BUILD time: this module compiles the nros-rmw-uorb backend inline
inside a real `px4_add_module()` context, so a link error here is a real
regression in the backend's PX4-facing surface. There is no "did not run"
outcome — building and linking it is the test.

)DESCR_STR");

	PRINT_MODULE_USAGE_NAME_SIMPLE("nros_register_check", "command");

	return 0;
}

extern "C" {

__EXPORT int nros_register_check_main(int argc, char *argv[]);

__EXPORT int nros_register_check_main(int argc, char *argv[])
{
	if (argc > 1 && (strcmp(argv[1], "help") == 0 || strcmp(argv[1], "-h") == 0)) {
		return nros_register_check_usage();
	}

	rmw_ret_t rc = nros_rmw_uorb_register();

	if (rc == NROS_RMW_RET_OK) {
		PX4_INFO("nros_rmw_uorb_register() -> OK");
		return 0;
	}

	PX4_ERR("nros_rmw_uorb_register() -> %d", static_cast<int>(rc));
	return 1;
}

} // extern "C"
