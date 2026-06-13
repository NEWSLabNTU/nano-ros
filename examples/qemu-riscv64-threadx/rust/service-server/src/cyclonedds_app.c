/* Empty translation unit for the CMake/CycloneDDS link path.
 * The real application entry is the Rust staticlib's app_main().
 */
void qemu_riscv64_threadx_service_server_cyclonedds_link_anchor(void) {}

/* `example_interfaces/AddTwoInts.srv` lowers to one IDL stem (`AddTwoInts`)
 * carrying two types — the Request (idx 0) and Response (idx 1). idlc + the
 * NrosRmwCycloneddsTypeSupport.cmake per-type register TUs emit
 * `register_<stem>_<idx>`. */
extern void register_AddTwoInts_0(void);
extern void register_AddTwoInts_1(void);

void nros_rmw_cyclonedds_register_app_descriptors(void) {
    register_AddTwoInts_0();
    register_AddTwoInts_1();
}
