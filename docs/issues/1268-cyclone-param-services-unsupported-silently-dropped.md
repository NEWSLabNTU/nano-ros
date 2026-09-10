---
id: 1268
title: "Parameter services never start on Cyclone: the rcl_interfaces service
  types have no type descriptor, and the failure is retried and dropped on
  every spin"
status: open
type: bug
area: rmw, core
severity: high
related: [issue-0745, issue-1269, issue-1270]
---

## Symptom

A downstream image (Autoware Safety Island: four C++ component nodes on one
executor, Cyclone, `param_services` declared) declares all 21 of its
parameters and boots. From ROS 2 on the image's own domain:

- `ros2 service list` shows the image's two application services
  (`/system/mrm/comfortable_stop/operate`, `/system/mrm/emergency_stop/operate`)
  and none of the `<node>/list_parameters`, `get_parameters`, ... services;
- `ros2 service call /mrm_handler/list_parameters ...` waits and never finds
  the service;
- the same listing on domain 0 shows nothing from the image either, so this is
  not the domain split of issues 0801/0824.

A temporary diagnostic that printed the result the executor discards gave:

```
reconcile_parameter_services failed: Transport(Unsupported) (nodes=4)
```

## Cause

Cyclone creates a service only if its request and reply types have a
registered descriptor; `nros-rmw-cyclonedds/src/descriptors.cpp` documents that
an unregistered type makes the create fail `UNSUPPORTED`. The backend bakes in
exactly one descriptor of its own, `ParticipantEntitiesInfo` for the graph
(`nros-rmw-cyclonedds/CMakeLists.txt`). The downstream's generated Cyclone
typesupport covers its own message packages (`autoware_*`, `std_msgs`,
`geometry_msgs`, ...). Nothing registers the `rcl_interfaces` service types the
six parameter services use -- ListParameters, GetParameters, SetParameters,
SetParametersAtomically, DescribeParameters, GetParameterTypes -- so
`build_parameter_service_set` fails on the first of them.

`reconcile_parameter_services` is called on every spin with its result
discarded (`nros-node/src/executor/spin.rs`, `let _ = self.reconcile_parameter_services();`),
so the executor retries a permanent failure forever and never says so.

It was half known. A test comment in `nros-cli-core/src/codegen/entry/emit_c.rs`
(issue 0745) says registration "fails outright on an RMW without
service-server support (cyclonedds today)", and 0745 made the failure
non-fatal. The stated cause is wrong -- Cyclone serves the image's own
services -- and non-fatal plus discarded made the loss invisible.

## Cost meanwhile

The image still allocates the parameter store: 285,440 B at the default
limits (32 slots of 8,920 B), for services no ROS 2 tool can reach.

## Fix shape

- Register the six `rcl_interfaces` service descriptors with the Cyclone
  backend whenever `param-services` is on, the way `ParticipantEntitiesInfo`
  is baked in.
- Report a failed registration once through `nros_log`, and stop retrying a
  failure that cannot change (`Unsupported`) on every spin.
- Correct the 0745 test comment's stated cause.

## Acceptance

- `ros2 param list`, `get` and `set` reach every node of a multi-node Cyclone
  image (this also needs issue 1269's node naming).
- A registration failure is logged exactly once, naming the node and service.
