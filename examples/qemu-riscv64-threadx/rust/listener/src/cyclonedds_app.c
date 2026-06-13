/* Empty translation unit for the CMake/CycloneDDS link path.
 * The real application entry is the Rust staticlib's app_main().
 */
void qemu_riscv64_threadx_listener_cyclonedds_link_anchor(void) {}

extern void register_Int32_0(void);

void nros_rmw_cyclonedds_register_app_descriptors(void) {
    register_Int32_0();
}
