---
id: 2482
title: "In a two-node zenoh image the second node serves 5 of the 6 parameter services"
status: resolved
type: bug
severity: high
area: [params, rmw-zenoh, rmw-xrce, sizing, interop]
found: 2026-11-13
resolved: 2026-11-13
related: [phase-556, issue-1713, issue-1743, issue-2023]
---

## What

Measured on 2026-11-13 (phase-556 first measurement, Humble,
`rmw_zenoh_cpp`, `ros2 ... --no-daemon`) against the workspace fixture
`param-two-node-talker`
(`packages/testing/nros-tests/bins/param-two-node-talker`), an image with two
nodes, `/alpha` and `/beta`, both with parameter services on.

`ros2 node info /alpha` lists all six parameter services. `/beta` lists five:

```
$ ros2 node info --no-daemon /beta
/beta
  Subscribers:

  Publishers:
    /beta_chatter: std_msgs/msg/Int32
  Service Servers:
    /beta/describe_parameters: rcl_interfaces/srv/DescribeParameters
    /beta/get_parameters: rcl_interfaces/srv/GetParameters
    /beta/list_parameters: rcl_interfaces/srv/ListParameters
    /beta/set_parameters: rcl_interfaces/srv/SetParameters
    /beta/set_parameters_atomically: rcl_interfaces/srv/SetParametersAtomically
```

`/beta/get_parameter_types` is missing, and
`ros2 service call /beta/get_parameter_types ...` times out. Reproduced in 3
runs. A stock node always serves all six, so `ros2 param list --param-type`
against `/beta` cannot work.

`/beta` also publishes no `/parameter_events`, where every stock node
publishes its own. That is a different matter and is tracked by a separate
design-gap issue.

## Why nothing caught it

`params_per_node_interop.rs` drives both nodes with `param list`, `get` and
`set` only, which use the five services that ARE served. No test lists a
node's services against stock (phase-556 W0).

## Cause (measured)

Not the queryable table: it was the hosted undeclared default, 32 slots, and
every queryable was declared. The **liveliness pool** was 16 and the image
declares 17 tokens: the session's primary node, two node tokens, two chatter
publishers, twelve parameter servers, one `/parameter_events` publisher. The
seventeenth, `/beta/get_parameter_types`, had a working queryable and no graph
token, so `ros2 node info` omitted it and `ros2 service call` waited forever.
Built with `NROS_ZENOH_MAX_LIVELINESS=24`, the defect disappeared.

Why 16: the fixture is an OWNED entry (its own `main`, no launch file), so it
has no SystemModel. `nros build`'s resolve stage refused
(`resolved.toml` `status = "refused"`), wrote no sizing descriptor, and the
bringup's `features = ["param_services"]` never reached any sizing build
script. `nros-zpico-build`'s undeclared liveliness default was a flat 16 with
no runtime-services term (archived issue 1713 kept it that way), while the
undeclared queryable default on a hosted target already assumed the runtime
families present.

The same road starved XRCE, measured on the same image with an XRCE image
added: `XRCE_MAX_SERVICE_SERVERS` stayed at the header's 4, so the FIRST node
already failed its fifth parameter service
(`no free service server slot for '/alpha/describe_parameters'`), `/alpha`
listed no service servers and `ros2 param list /alpha` answered
`Node not found`. One node's parameter family alone needs 6, so every XRCE
image with no descriptor and a parameter or lifecycle family failed this way.
Even with the families stated, `derive_service_servers` kept the header's 4
whenever the application's server count was unknown.

## Fix

One class: a road that cannot count something priced it in the small
direction.

- **The owned-entry road gets a descriptor** (`nros-cli-core`
  `cmd/build.rs::resolve_image`). When the model resolve refuses because the
  image boots an owned entry with no launch
  (`entity_census::owned_entry_without_model`), the families come from
  `system.toml` (`[system] features` plus `[image.<id>] features`) through
  `InfraServices::from_features`, the same reader the leaf road uses.
  `compose_unwired` now states `[image] infra_families` whenever a family is
  ON even with no node count, and REFUSES `model_nodes` with the reason (the
  program creates its nodes in its own code). A family OFF is not stated: an
  owned program may register one itself. The standalone-leaf road with no
  entities takes the same branch.
- **An unknown node count is never the small direction.** New shared
  predicate `nros_sizing_descriptor::infra::per_node_term_is_unknown`
  (parameter family stated ON, node count not stated). zenoh's queryable and
  liveliness defaults, and XRCE's service-server slots, then never size below
  their own undeclared budget.
- **zenoh hosted undeclared liveliness** prices the runtime families present,
  through the same terms as the stated branch (32 on a hosted target, the
  queryable table's number). Embedded keeps the flat 16: its undeclared
  queryable default carries no runtime term either, and RAM there is the
  reason 1713 kept it.
- **XRCE**: with the families stated and the application's count unknown,
  the slots are the runtime's servers plus the header's 4 (was: the header's
  4, a certain boot failure for either family); with the node count unknown
  too, a hosted build never goes below every family on one node plus the
  header's 4 (15), and makes no builtin split over a guessed share. A build
  with NO fact at all (a bare `cargo build` with no descriptor) keeps the
  header's 4, unlike zenoh: `just check rmw-xrce`'s CMake harness compiles
  the backend at the header's values and links the bare-build archive, so any
  other bare-build value splits one struct across two layouts (measured: the
  harness failed its service and reply checks when this fallback was tried).
  Every `nros build` road that knows a family states it now, so this leaves
  only images built outside `nros build`.

Measured after the fix: the XRCE image lists all six services on both nodes
and `ros2 param list` answers for both; the zenoh image passes
`each_node_of_a_two_node_image_serves_the_parameter_services_as_stock_does`.

## Acceptance

`ros2_cli_parity_interop::each_node_of_a_two_node_image_serves_the_parameter_services_as_stock_does`
asks `ros2 node info` for each node of the two-node image and finds all six
parameter services, as stock does (plain check). Unit tests:
`liveliness_default_tests::a_hosted_undeclared_image_prices_the_runtime_families`,
`a_stated_family_with_an_unknown_node_count_is_never_below_undeclared`,
`queryable_default_tests::a_stated_param_family_with_an_unknown_node_count_keeps_the_undeclared_budget`
(`nros-zpico-build`), the XRCE build script's self-test 13a, and
`an_owned_entry_states_its_families_and_refuses_its_node_count`
(`nros-cli-core`).
