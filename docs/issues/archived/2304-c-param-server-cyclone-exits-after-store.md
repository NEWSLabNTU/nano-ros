---
id: 2304
title: "A C talker with `rclc_parameter_server` and `features = [\"param_services\"]` exits 1 on Linux with Cyclone DDS, right after its parameter-store log line"
status: resolved
type: bug
area: rmw, cyclonedds, c-api, params
related: [phase-537, 1268, 1269, 1293]
found: 2026-11-06
resolved: 2026-11-10
resolved_in: "the cmake road generates the runtime's own interface types (nros_runtime_interface_types); C/C++ entity refusals log what and why; row c-parameters-cyclonedds"
---

**Home: [phase-537](../../roadmap/phase-537-book-user-guide-organization.md).** Found by the phase-537 W5 book session (PR #3187),
writing the parameters chapter.

## Symptom

A Linux-host C talker that serves parameters through `rclc_parameter_server`
with `features = ["param_services"]` and `rmw = "cyclonedds"` prints its
parameter-store log line, then exits with status 1 and no further output.
The book session reports the same program as working on zenoh. That claim
was not re-measured here.

## Reproduction (not run here)

It was not reproduced in this session: a Cyclone C build on this host costs
a Cyclone configure plus a fixture build, more than the triage budget. The
recipe:

1. Start from `examples/linux/c/parameters/` (or the
   `c_param_talker_pkg` node in `examples/workspaces/features`).
2. Set `rmw = "cyclonedds"` and `features = ["param_services"]` on the image.
3. `nros build`, run it, and read the exit status and the last log lines.
   Run it under `NROS_LOG_LEVEL=debug` (or the C log knob) so the failing
   `rcl_ret_t` is printed.

## Coverage gap

No fixture runs this combination. `examples/fixtures.toml` has
`param-two-node-talker-cyclone`, which is RUST. Every C parameter row
(`workspace-features-c-params`, the `c_parameters` leaf) is `rmw = "zenoh"`.
So the C half of issue 1268's fix was never exercised on Cyclone.

## Suspects, in order

1. **The C path makes every parameter-service failure fatal.**
   `rclc_executor_add_parameter_server_with_context`
   (`packages/api/nros-c/src/parameter.rs`) calls
   `exec.register_parameter_services()` and maps ANY error to
   `RCL_RET_ERROR`, so the example's `if (rc != RCL_RET_OK) return 1;`
   exits 1 with no message of its own. The Rust road reaches the same
   services through the spin's reconcile, which logs a failure once and
   continues (issue 1268). That explains "exits right after the store
   line, on Cyclone only": the call does go through `create_param_srv`,
   so the six `rcl_interfaces` descriptors are registered (1268's fix),
   but whatever Cyclone then refuses becomes a silent exit 1 in C and a
   logged degradation in Rust. What Cyclone refuses is the unknown, and
   suspects 2 and 3 are the candidates.
2. **The service/queryable budget.** The six parameter services claim six
   service slots before the app declares anything. On Cyclone the cap is
   `NROS_CYCLONEDDS_MAX_TYPES` / the service table, derived from the
   SystemModel. A C leaf with no contract `params:` may derive a budget
   that leaves no room, and the sixth create fails.
3. **Cyclone's service-create path itself**, for these nested-sequence
   types, on the C road's descriptor build.

## Fix direction

- Reproduce with the recipe above and record the failing `rcl_ret_t` and
  which of the six services failed (1268 made the Rust path log this once,
  naming node and service; the C path should too).
- Make the C refusal say what failed. Map the `register_parameter_services`
  error to a specific `rcl_ret_t` and log the node and service, as the Rust
  road does, instead of a bare `RCL_RET_ERROR`.
- Fix the underlying Cyclone refusal (suspect 2 or 3) once it is named.
- Add a `c` + `cyclonedds` parameter fixture row and an e2e cell beside
  `param-two-node-talker-cyclone`, so the combination has a verdict.

## Resolution — 2026-11-10

### Reproduced

`examples/linux/c/parameters` copied to a scratch dir with `rmw = "cyclonedds"`
(the leaf already declares `param_services` through `NROS_FEATURES`), configured
and run on Linux:

```
[nros] .../src/main.c:102 rclc_parameter_server_init_with_option(&app.params, &app.node, &opts) -> -1
exit=1
```

`-1` is nros-c's `RCL_RET_ERROR`. A temporary diagnostic put the discarded error
on the console: `Executor::enable_parameter_events_on` returned
`Transport(PublisherCreationFailed)`, i.e. the `/parameter_events` publisher
(`rcl_interfaces/msg/ParameterEvent`), which `notify_changed_over_dds = true`
creates inside the server init — so the failure was one call EARLIER than
suspect 1 named. With the events opted out (`notify_changed_over_dds = false`),
the next call, `rclc_executor_add_parameter_server`, failed the same way on the
first of the six services.

### Cause — suspect 3's neighbourhood, not suspect 2

Neither the budget nor Cyclone's handling of nested sequences. On the CMAKE road
nothing generated a descriptor for any `rcl_interfaces` type at all. Cyclone
creates an entity only for a type whose descriptor a linked TU registered; on
that road the only producer is `nros_generate_interfaces()` through the
backend's `[rmw.codegen].per_message` hook, run for the packages the
APPLICATION names. The runtime's own entities (`/parameter_events`, the six
parameter services, and — measured the same way — the five lifecycle services
and `TransitionEvent`) are typed from packages the application never names.
Issue 1268 fixed the CARGO road (nros-node registers each type from its schema
at create time through `rmw_type_registry::register_type`), a seam the C++
backend does not install on the cmake road, so the C half of 1268 was never
fixed, only never exercised. The configure confirmed it: "1 distinct type(s)
generated in this configure" for an image serving six services.

The descriptor table was never the limit (256 rows by default); with the fix the
configure generates 26 (38 with `lifecycle`).

Suspect 2 did hold a smaller, separate defect on the cargo road:
`PARAM_SERVICE_TYPES` counted the twelve service types and not ParameterEvent,
which issue 2023 added as a registration and not as a count. Now 13.

### Fix

- `cmake/NanoRosRuntimeInterfaceTypes.cmake` — `nros_runtime_interface_types()`,
  called from the root after the backend is added: for `param_services` it runs
  the selected backend's per-message hook over `builtin_interfaces` +
  `rcl_interfaces`, for `lifecycle` over `lifecycle_msgs`, and links the result
  into both umbrellas. A backend without a hook (zenoh, xrce) pays nothing. It
  goes through the same hook a user package does, so a package both need has one
  typesupport library. A missing package is a configure FATAL naming it.
- nros-c / nros-cpp — every entity-creating entry point that answered a refusal
  with a bare `RCL_RET_ERROR` / `NROS_CPP_RET_ERROR` / `TRANSPORT_ERROR` and no
  message now logs the call, the entity (topic, service or node, and type) and
  the error, and maps it through an exhaustive `node_error_to_ret`. An explicit
  `register_parameter_services()` now reports through the reconcile's once-only
  line, which names the node FQN and the service (1268's), so C and C++ callers
  get it. The remaining `Err(_) => *_ERROR` sites (44 in nros-c, 46 in nros-cpp)
  are runtime operations (send, take, publish, cancel, graph queries, ping,
  timers, custom transport, wake source), listed in the commit, not entity
  creation.

### Verified

- The scratch leaf on Cyclone: exit 0, and from stock ROS 2 Humble over
  `rmw_cyclonedds_cpp`, `ros2 service list` shows the six
  `/c_parameters/*_parameters` services, `ros2 param list` the four declared
  parameters, `ros2 param get publish_rate_hz` → `Integer value is: 10`.
- The C++ parameters example on Cyclone (scratch copy): exit 0. Its state before
  the fix was not measured.
- Lifecycle (scratch copy with `lifecycle` added and
  `rclc_executor_register_lifecycle_services` called): before, the new message
  `rclc_executor_register_lifecycle_services: the lifecycle services could not be
  created: Transport(Unsupported)` and `-12`; after, `0`, and the five lifecycle
  services appear in `ros2 service list`.
- New fixture row `c-parameters-cyclonedds`, matrix cell
  `(Linux, C, Cyclonedds, Params, Example)`, test
  `parameters_roundtrip::c_parameters_roundtrip_on_cyclonedds`. Before (the hook
  disabled, fixture rebuilt): FAIL, exit 1, with
  `rclc_parameter_server_init_with_option: /parameter_events for node
  /c_parameters could not be created: Transport(PublisherCreationFailed)`.
  After: PASS; the zenoh C and C++ cases of the same file still pass.

### Not done here

- No live `ros2 param` interop CELL on Cyclone for C (the zenoh
  `native-params-c-zenoh` has one); the ros2 checks above were run by hand.
- The C executor-scoped lifecycle services came up as `//get_state` etc. in the
  lifecycle scratch run (no node name in the path) and `ros2 lifecycle get
  /c_parameters` answered `Node not found`. That is naming, not descriptors, and
  was not checked against zenoh.
