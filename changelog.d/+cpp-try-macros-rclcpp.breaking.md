C++ API: the error-propagation macros take `rclcpp`'s prefix (RFC-0089
amendment 2026-10-13, phase-499 W8): `NROS_TRY` -> `RCLCPP_TRY`,
`NROS_TRY_RET` -> `RCLCPP_TRY_RET`, and the user-overridable hook
`NROS_TRY_LOG` -> `RCLCPP_TRY_LOG`. The void-returning form, which phase-492
had mistakenly spelled `RCLC_CHECK` (colliding with the C API's `RCLC_CHECK`),
is `RCLCPP_CHECK`. No forwarders.
