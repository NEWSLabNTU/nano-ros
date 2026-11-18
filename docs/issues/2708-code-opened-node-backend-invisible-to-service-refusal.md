---
id: 2708
title: "A node a component opens on another RMW in its own code is invisible to the configure-time service refusal"
status: open
type: limitation
severity: low
area: [orchestration, runtime, rmw]
related: [phase-556, 2704, 2487]
found: 2026-11-18
---

**Found by** phase-556 W13, which made the stock node interfaces (the six
parameter services, `/rosout`) the default for every image and added a
configure-time refusal for a node whose backend cannot create services
(`backend_link::unservable_services_refusal`, with its cmake twin in
`nros_capability_features`).

## What the refusal sees, and what it cannot

The refusal is per node: each component an image declares in `system.toml`
runs on the image's RMW, and each one is checked against that backend's
`[rmw.facts] services`. A backend that MEASURED it cannot create services
(uORB, held by `packages/rmw/uorb/nros-rmw-uorb/tests/register_smoke.cpp`)
refuses a node that would serve the parameter services or lifecycle.

The model carries no per-node RMW. A component may open a SECOND node on
another backend in its own code (`NodeBuilder::rmw`): the PX4 uORB bridge does
exactly this for its outward (zenoh) side. That node, and the backend it runs
on, never reach the system model, so the refusal neither checks it nor guesses
at it. Its services are created at runtime on whatever backend the code
chose, and a failure there is still the one-line runtime error
(`parameter services could not be created ...`) with boot continuing.

Today this costs nothing in-tree: the only backend stating `services = false`
is uORB, the bridge's code-opened node is on zenoh (which serves services), and
both uORB images state `node_interfaces = "none"`.

## What closing it needs

A per-node backend the model can see: either the component declares the
backend of each node it opens (a `system.toml` row the codegen holds the code
to), or the metadata probe reports each node's backend the way it reports
`in_place` subscription capability, and the refusal reads that. Until then,
the PR that added the refusal and this issue name the gap.
