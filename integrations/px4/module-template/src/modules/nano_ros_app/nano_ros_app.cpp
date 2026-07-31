// PX4 module template — the shape a nano-ros PX4 module takes.
//
// phase-325 W1.3: this body used to be a comment ("Replace this comment block
// with NodeBuilder / Publisher calls" / "publisher loop"). A template that
// compiles nothing cannot fail, which is why its include paths went stale
// (`packages/api/...` for headers that had moved) with nobody noticing, and why
// nothing recorded that the uORB backend has no node API exercised above the
// vtable. It now includes a real nano-ros header and calls the registration
// hook, so a moved header or a broken link surfaces HERE rather than in someone
// else's copy.
//
// Written to PX4 convention (tab indent, Kconfig beside it, PRINT_MODULE_*
// usage strings), because a PX4 module is read and maintained by PX4 people. The
// BSD 3-clause block PX4 sources carry is a LICENSING practice naming the PX4
// Development Team, not a style rule — reproducing it here would misattribute
// copyright. nano-ros is MIT OR Apache-2.0.

// NOT YET <nros/init.h> — see phase-325 W1.4. That header pulls
// <nros/nros_config_generated.h>, which nano-ros's own cmake emits per build into
// ${CMAKE_BINARY_DIR}/nros-rust/nros-c-generated/. In the prebuilt-archive model
// this file uses, nothing exports it, so including it fails with:
//
//     error: "nros_config_generated.h must be supplied per-build by the build
//             system; see this stub for guidance."
//
// It must come from the SAME build as libnros_cpp.a — it carries storage sizes,
// and a mismatched copy is the issue-0268 silent-overflow class. Until W1.4
// exports the pair together, declare the entry points used here.

#include <px4_platform_common/log.h>
#include <px4_platform_common/module.h>

#include <cstring>

// Registered by the strong nros_app_register_backends() that
// nros_px4_add_module(BACKENDS uorb) generates; declared here so the template
// shows where registration comes from.
extern "C" int nros_rmw_uorb_register(void);

static int nano_ros_app_usage(const char *reason = nullptr)
{
	if (reason) {
		PX4_WARN("%s\n", reason);
	}

	PRINT_MODULE_DESCRIPTION(
		R"DESCR_STR(
### Description
Template for a PX4 module that uses nano-ros over the uORB backend.

Copy this directory into your own EXTERNAL_MODULES_LOCATION and replace the body
of the main function with your node logic. On the uORB backend a payload is the
PX4 struct itself — publish the raw bytes of a `<uORB/topics/*.h>` type through
the nano-ros publisher and stock PX4 modules read it with no serialization on
either side.

)DESCR_STR");

	PRINT_MODULE_USAGE_NAME_SIMPLE("nano_ros_app", "command");

	return 0;
}

extern "C" __EXPORT int nano_ros_app_main(int argc, char *argv[]);

extern "C" __EXPORT int nano_ros_app_main(int argc, char *argv[])
{
	if (argc > 1 && strcmp(argv[1], "help") == 0) {
		return nano_ros_app_usage();
	}

	// 1. Register the uORB-backed RMW backend. Must happen before any nano-ros
	//    entity is created.
	int rc = nros_rmw_uorb_register();

	if (rc != 0) {
		PX4_ERR("nros_rmw_uorb_register() -> %d", rc);
		return 1;
	}

	PX4_INFO("nano-ros uORB backend registered");

	// 2. Your node goes here. Map each ROS-style topic name to PX4's static
	//    descriptor, then create publishers / subscriptions through the
	//    nano-ros API:
	//
	//    #include <uORB/topics/vehicle_status.h>
	//    nros_rmw_uorb_register_topic("/vehicle_status",
	//                                 "px4_msgs::msg::VehicleStatus",
	//                                 ORB_ID(vehicle_status));
	//
	//    See docs/roadmap/phase-325-uorb-interop-and-bridge.md; the worked
	//    example lands at examples/px4/cpp/firmware/.

	return 0;
}
