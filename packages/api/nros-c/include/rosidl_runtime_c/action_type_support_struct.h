/**
 * @file action_type_support_struct.h
 * @brief `ROSIDL_GET_ACTION_TYPE_SUPPORT`, adopted as a pass-through
 *        (phase-492 W8, RFC-0089 2026-10-12).
 *
 * The macro yields the address of the generated descriptor:
 *
 *     ROSIDL_GET_ACTION_TYPE_SUPPORT(pkg, action, Name)  ->  (&pkg__action__Name__nros_type)
 *
 * a `const rclc_action_type_t *`, which is what every nano-ros init call takes. So
 * the line a ported file writes, passing the macro straight to an init call,
 * compiles and behaves identically. Upstream's macro is a function call; this
 * one is an address constant, so it also works in a static initialiser — a
 * superset no upstream file can depend on.
 *
 * `rosidl_action_type_support_t` is deliberately NOT declared. A rosidl
 * typesupport's MEMBERS (`typesupport_identifier`, `data`, the `func`
 * dispatcher) are a contract this descriptor does not have, so declaring a
 * `const rosidl_action_type_support_t *` or reading one of those members fails
 * to compile here, by construction.
 *
 * The guard is ours, not upstream's: this file defines a different thing under
 * upstream's path, and must not suppress (or be suppressed by) the real one.
 */

#ifndef NROS_ROSIDL_RUNTIME_C__ACTION_TYPE_SUPPORT_STRUCT_H_
#define NROS_ROSIDL_RUNTIME_C__ACTION_TYPE_SUPPORT_STRUCT_H_

#define ROSIDL_GET_ACTION_TYPE_SUPPORT(P, S, N) (&P##__##S##__##N##__nros_type)

#endif /* NROS_ROSIDL_RUNTIME_C__ACTION_TYPE_SUPPORT_STRUCT_H_ */
