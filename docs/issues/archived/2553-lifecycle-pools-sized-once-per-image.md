---
id: 2553
title: "The derived pools price the lifecycle family once per image, so an image with several launch-marked managed nodes is sized for one machine and runs several"
status: resolved
resolved_in: "the change that archives this issue"
type: bug
severity: medium
area: [sizing, lifecycle, zenoh, cli]
related: [phase-554, issue-2428]
found: 2026-11-14
---

**Home: [phase-554](../../roadmap/phase-554-per-node-lifecycle.md)** W5 (the derived pools).

## What

RFC-0128 D1 gives every managed node its own state machine, and the runtime
registers one per node: each machine serves its own five REP-2002 services
and its own `transition_event` (`Executor::register_lifecycle_services_on`,
`packages/core/nros-node/src/executor/spin.rs`). The build still prices the
family as one per executor:

- `InfraServices::lifecycle` is a single switch: "five servers, once per
  executor" (`packages/cli/nros-cli-core/src/entity_inventory.rs`, the field
  doc and `InfraServices::queryables`, which adds
  `LIFECYCLE_SERVICE_QUERYABLES` once);
- the `transition_event` publisher is added once beside it;
- `nros_sizing_descriptor::infra::infra_service_servers` adds 5 for the
  family, not 5 per managed node
  (`packages/tooling/nros-sizing-descriptor/src/infra.rs`).

So an image whose launch file marks N nodes `<lifecycle_node>` claims 5N
queryables and N `transition_event` publishers at boot and was sized for 5
and 1.

## Which images it reaches

The machines that come from the LAUNCH file are the ones missing. A node
whose own code registers its machine is recorded by the metadata probe
(phase-554 W2.b), and its five services and its publisher reach the
inventory as ordinary probe rows, so they are counted per node already. A
launch-marked node with plain code has no such rows: the generated entry
registers its machine after the probe has run, so nothing but the family
term covers it.

The opposite also holds and is safe: an image whose code-managed nodes are in
the probe rows AND which states the lifecycle family counts those nodes
twice (once as rows, once as the family). That is an over-size, recorded here
because W5's per-node pricing is where both are fixed.

Derived from reading the code (sizing composition and the runtime
registration). No image was booted to observe the shortfall.

## Fix direction

W5's "every count of the lifecycle family becomes per managed node": price
the family per managed node from the model's managed set, and do not add
it again for a node whose rows the probe already carries.

## Fix (2026-11-19)

The family is priced per STATE MACHINE, from the registration's own list.

- `InfraServices::managed_nodes` is `model.managed_nodes().len()`, the same
  `LoadedModel::managed_nodes` that `codegen::entry` builds
  `Plan::managed_nodes` from, so the price and the registration cannot count
  different nodes. A marked node turns the family on.
- `InfraServices::lifecycle_machines()`: one per managed node, and one when
  the family is on with no node marked (a feature switch, an owned entry: the
  pre-2553 price). `queryables` adds 5 per machine and `publishers` adds one
  `transition_event` per machine.
- The sizing descriptor gains `[image] lifecycle_nodes` (additive; the schema
  version stays). Both producers write it: `image_facts` and
  `compose_unwired`.
- `nros_sizing_descriptor::infra::lifecycle_machines` and
  `infra_service_servers_with` are the build scripts' copy of the rule.
  - The zenoh runner prices its queryable table and its liveliness pool's
    `transition_event` term per machine.
  - XRCE prices its service-server slots per machine (self-test 13a).

Test `each_launch_marked_managed_node_is_priced_its_own_machine`: two marked
nodes cost 2 x 5 queryables and 2 publishers over an unmarked model. It fails
with the old once-per-image price ("two managed nodes: two machines, not one
per image") and passes with this fix. The shared crate's
`the_lifecycle_family_is_priced_per_managed_node` holds the same rule for
the build scripts.

Not changed: the OVER-size noted above. An image whose code-managed nodes are
probe rows AND which states the family still counts those nodes twice. That
is the safe direction, so it is left for W5's de-duplication.

Sweep: `git grep -nE 'LIFECYCLE_SERVICE_(QUERYABLES|PUBLISHERS)|lifecycle_machines|lifecycle_nodes' -- packages`
