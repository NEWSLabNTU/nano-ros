/* CycloneDDS app-descriptor registration for the C++ typed talker.
 *
 * The CycloneDDS RMW calls the weak `nros_rmw_cyclonedds_register_app_descriptors`
 * at init (packages/dds/nros-rmw-cyclonedds/src/vtable.cpp). The typed
 * `Publisher<Int32>` needs the std_msgs/Int32 type descriptor registered with
 * CycloneDDS before publish; `register_Int32_0` (from the idlc-generated
 * std_msgs__cyclonedds_ts, linked via nano_ros_link_rmw cyclonedds) does that.
 * Without this strong override, the weak no-op leaves the type unregistered and
 * publish fails silently. Mirrors the C talker's cyclonedds_app.c — the C++ typed
 * path has no auto-registration on bare-metal, so it provides the hook too.
 */
extern void register_Int32_0(void);

void nros_rmw_cyclonedds_register_app_descriptors(void) {
    register_Int32_0();
}
