---
id: 1268
title: "Parameter services never start on Cyclone: the rcl_interfaces service
  types have no type descriptor, and the failure is retried and dropped on
  every spin"
status: resolved
type: bug
area: rmw, core
severity: high
resolved: 2026-09-12
related: [issue-0745, issue-1269, issue-1270, issue-1293, phase-444]
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

## Status — 2026-09-11 (phase-444 W6): fixed in the tree, UNVERIFIED LIVE (superseded below)

All three items of the Fix shape landed. The first acceptance above is NOT
checked off: it needs a ROS 2 peer, this host has none, and the agent that made
the change did not run the distrobox. The issue stays OPEN for that reason
alone. What a live run has to confirm is `ros2 param list/get/set` against a
multi-node Cyclone image; the cell that would do it in the live-peer lane is
named at the bottom of this section.

- **Registered.** `create_param_srv` (`nros-node/src/executor/spin.rs`) calls
  `register_type::<Svc::Request>()` / `::<Svc::Reply>()` before
  `create_service`, exactly as a typed user service does
  (`register_service_sized`). Chosen over baking the descriptors into the
  backend the way `ParticipantEntitiesInfo` is: the generic seam already
  existed, it is what every other service uses, and it keeps zenoh and xrce
  no-ops (which is why zenoh served this image all along).
- **The types do build.** `nros-rmw-cyclonedds/tests/infra_service_descriptors.rs`
  builds a descriptor for each of the twelve. That was the open question —
  `Parameter` / `ParameterValue` / `ParameterDescriptor` carry sequences of
  nested structs, sequences of strings and a bounded sequence, the class that
  was rejected outright before phase-212 K.7.4.c.
- **Reported once, naming both.** The log line now names the node FQN and the
  service suffix (`/mrm_handler` at `get_parameters`), from a
  `ParamServiceReconcileFailure` recorded where the failure happens — the only
  place that knows both.
- **Stopped retrying.** A `Transport(Unsupported)` is marked permanent and
  `parameter_services_pending()` returns false, so the spin no longer runs six
  `create_service` calls per spin forever. An explicit
  `register_parameter_services()` still tries, because that is a caller asking
  rather than the spin guessing. Test:
  `a_permanent_parameter_service_failure_is_asked_once_and_names_what_failed`
  (nros-node). Negative control: with the stop-retrying clause reverted it
  fails — `five spins made 5 create_service call(s)`.
- **Sized.** `NROS_CYCLONEDDS_MAX_TYPES` is derived from the SystemModel, which
  names only what an entry WIRES, so the twelve were invisible to it;
  `cyclonedds_type_sizing::infra_types` counts them from the same
  `InfraServices::from_model` predicate the queryable counts use. Before the
  registration landed they cost no slots, because nothing registered them.
- **0745 comment corrected** in `nros-cli-core/src/codegen/entry/emit_c.rs`: the
  cause was never "an RMW without service-server support".

**The class, swept.** Of the entities the executor creates on its own:
`/clock` goes through the subscription path, which already registers its type;
action protocol types are registered by `A::register_protocol_types`;
`/parameter_events`, `/rosout` and the type_description services are not created
by nros-node at all. The one remaining gap is the **lifecycle** services, and
they are NOT fixed here: three of their request types are empty and the
descriptor builder refuses an empty schema. That is **issue 1293**, filed with
the serializer half it also needs;
`infra_service_descriptors::empty_request_types_have_no_descriptor_yet` pins the
boundary so it fails when 1293 lands.

**What would verify it live:** a Cyclone parameter cell in `interop::CELLS`
alongside the existing `native-params-rust-zenoh` /
`native-params-per-node-rust-zenoh` (`packages/testing/nros-tests/src/interop.rs`),
with its verdict row in `.config/interop-verdicts.toml` and the lane in
`.github/workflows/live-peer.yml`. No Cyclone params cell exists today, on any
RMW but zenoh.

## Resolution — 2026-09-12 (phase-444 W6): verified live, on a ROS 2 peer

That cell exists and it ran. `native-params-per-node-rust-cyclone` in
`interop::CELLS`, driving `ros2_param_cli_addresses_each_node_on_cyclonedds`
(`params_per_node_interop.rs`) against this host's ROS 2 Humble
`rmw_cyclonedds_cpp`, on a two-node image:

    === ros2 service list (cyclonedds) ===
    /alpha/describe_parameters   /beta/describe_parameters
    /alpha/get_parameter_types   /beta/get_parameter_types
    /alpha/get_parameters        /beta/get_parameters
    /alpha/list_parameters       /beta/list_parameters
    /alpha/set_parameters        /beta/set_parameters
    /alpha/set_parameters_atomically  /beta/set_parameters_atomically

    === ros2 param list /alpha ===   rate, stepped
    === ros2 param get  /alpha rate === Integer value is: 10
    === ros2 param set  /alpha rate 42 === Set parameter successful
    === ros2 param get  /alpha rate === Integer value is: 42
    (and the same four on /beta, 20 -> 42)

Both Acceptance items are met. **All three verbs on BOTH nodes**, not `get`
alone: `set` travels in the other direction and its reply comes back through
`SetParametersResult`, a type nothing but the set path needs — and the value is
read back afterwards, because a `set` that reports success and changes nothing
is the same shape as the create this issue is about. The second item (logged
once, naming node and service) landed with `5572267bb`.

Two things the live run settled that the tree-only fix could not:

- **All twelve services appear, not two.** A parallel branch had built the other
  candidate mechanism — baking the six service descriptors into the Cyclone
  backend beside `ParticipantEntitiesInfo` — and reached only
  `get_parameters` / `list_parameters` per node. The generic `register_type`
  seam reaches all six. That branch's baking is **dropped**, not merged
  alongside: two mechanisms registering the same twelve would double-count them
  against the `NROS_CYCLONEDDS_MAX_TYPES` budget `cyclonedds_type_sizing::infra_types`
  sizes, and a second spelling of one fixed set is the issue-0135 class.
- **The zenoh cell was green throughout.** `native-params-per-node-rust-zenoh`
  drives these same three verbs and passed for the whole time every Cyclone
  parameter service failed to create. That is issue 1269's argument for a
  per-RMW cell rather than a wider one, with a second witness.

The 0745 comment's stated cause is corrected in both `service_trailer.jinja`
packs as well as `emit_c.rs` — the jinja packs are what a generated C or C++
entry actually carries into a downstream tree.

Still out of scope and still open: the **lifecycle** services (issue 1293), for
the empty-request-type reason above.
