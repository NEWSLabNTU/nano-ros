/* Empty translation unit for the CMake/CycloneDDS link path.
 * The real application entry is the Rust staticlib's app_main().
 */
void qemu_riscv64_threadx_listener_cyclonedds_link_anchor(void) {}

/* CycloneDDS app-descriptor registration (phase-245 W2 cyclone tail).
 * Strong-overrides the weak nros_rmw_cyclonedds_register_app_descriptors
 * (packages/dds/nros-rmw-cyclonedds/src/vtable.cpp) so the Int32 type is
 * registered with CycloneDDS before publish/subscribe. register_* come from the
 * idlc-generated *__cyclonedds_ts (linked via nano_ros_link_rmw cyclonedds). */
extern void register_String_0(void);

void nros_rmw_cyclonedds_register_app_descriptors(void) {
    register_String_0();
}
