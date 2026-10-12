C API: the C user API takes rcl/rclc names and contracts (RFC-0089,
phase-492). There are no forwarders; port C code with the rename map
`docs/reference/c-api-rename-map.tsv` (old name, new name, class, why).

- A name upstream has takes upstream's spelling and contract
  (`rclc_executor_add_subscription`, `rcl_publish`, `rclc_executor_add_client`, …).
- A name only nano-ros has moves from `nros_`/`NROS_` to `rclc_`/`RCLC_`
  (`rclc_monitors_arm`, `RCLC_QOS_DURABILITY_TRANSIENT_LOCAL`, …). Return codes
  are `RCLC_RET_*`.
- Kept as `nros_`: the port seams, the entry and boot hooks (`nros_app_main`,
  `NROS_APP_*`), the build facts, and the C++ binding's `nros_cpp_*`.
- Headers are reachable at upstream's paths (`<rcl/rcl.h>`, `<rclc/executor.h>`,
  `<rcl_action/*.h>`, `<rcl_lifecycle/*.h>`); `<nros/*.h>` still works.
- A C component (`RCLC_C_COMPONENT`) is configured with `(rcl_node_t *,
  rclc_executor_t *, StructT *)` and uses the C API; the C and C++ bindings
  share one executor object.
- Codegen version 14: regenerate C message trees (`nros sync`).
