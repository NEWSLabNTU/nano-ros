/* Empty translation unit for the CMake/CycloneDDS link path.
 * The real application entry is the Rust staticlib's app_main().
 *
 * Issue #205 step 1 — the hand-written strong
 * `nros_rmw_cyclonedds_register_app_descriptors` override that used to live
 * here is RETIRED: since #195 the board walks `.init_array`
 * (board_threadx_qemu_riscv64.c + the link.lds bounds), so the idlc-generated
 * `register_*` constructor TUs register every descriptor themselves and the
 * weak no-op default in nros-rmw-cyclonedds/src/vtable.cpp is the correct
 * resolution. */
void qemu_riscv64_threadx_service_server_cyclonedds_link_anchor(void) {}
