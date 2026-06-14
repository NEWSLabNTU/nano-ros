/*
 * Phase 249 P4a (issue 0050 W3.1) — the weak default of
 * `nros_app_register_backends` was REMOVED for nros-cpp too.
 *
 * `nros_cpp_init` calls `nros_app_register_backends` before opening the CFFI RMW
 * session; the symbol is now the single generated STRONG def from
 * `nano_ros_link_rmw()` (universal per `nros_platform_link_app`, phase-249 P2b),
 * which calls each linked backend's `nros_rmw_<x>_register`. With no weak
 * fallback, a missing strong def is a LINK ERROR, not a silent no-op (the
 * #48-class hazard). This TU is intentionally empty (kept as a build source so
 * the nros-cpp `cc` build inputs do not change).
 */
