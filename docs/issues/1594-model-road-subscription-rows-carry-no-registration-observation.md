---
id: 1594
title: "The model-road sizing descriptor never sees a registration OBSERVATION —
  it composes the SystemModel with `nros-metadata.json`, while `in_place` lives in
  the probe's per-component sidecars — so every subscription on zenoh / XRCE
  refuses its `registration_path` and keeps a receive region it may never claim"
status: open
type: tech-debt
area: [build, cli]
severity: medium
found: 2026-10-01
related: [1340, 1393, 1522, 1407, 1419]
---

## What is open

phase-457 W3 made `registration_path` a per-endpoint OBSERVATION:
`Executor::open_subscription` reports whether the call site can dispatch in place,
the metadata probe writes it into the component's sidecar as `in_place`
(schema v3), and the composer states `in_place` only for a row that carries
`EntityDecl::in_place_capable = Some(true)`. W3 also wrote that "the MODEL road
can now state the in-place row".

It cannot, because no row on that road ever carries the observation. The model
road (`write_for_model`, reached from `nano_ros_entry()` and from `nros build`'s
workspace cargo road) composes two inputs:

| input | what it carries | `in_place`? |
| --- | --- | --- |
| the resolved SystemModel → `EntityInventory::from_model` | endpoints from the launch tree + contract | never — `EntityDecl::bare` leaves it `None` |
| `nros-metadata.json` → `inventory_from_metadata` | the REGISTERED component population | never — `ComponentMeta` has no entity rows in production (issue 1555) |

The observation exists — on the leaf road `leaf_entity_env::declaration_from_probe`
reads `ProbeEntity::in_place`, and `nros sync` over a C++ workspace writes
`"in_place": true` into the sidecar — but nothing joins it onto a model row.

## Measured

`examples/workspaces/cpp`'s `native_entry` (zenoh), descriptor written by the
in-tree CLI from that build's own model, metadata and the five bound tables its
closure registered:

```
[[endpoint]]
kind = "subscription"
type = "std_msgs/msg/Int32"
topic = "/chatter"
wire_bound_bytes = 12

[endpoint.refused]
registration_path = "... does not say how each endpoint REGISTERS -- the inventory
  composed here carries no per-endpoint observation ... Tracked by issue 1594"
```

## What it costs

A refused path is priced at the buffered region, which is the safe direction
(`claims_no_receive_region` is false for it). On a zenoh or XRCE image whose
subscriptions DO register in place, that is issue 1340's saving left on the
table on exactly the road where the arena's per-endpoint model runs from the
descriptor (issue 1577): measured on the leaf road, 10,840 bytes per
`KEEP_LAST(10)` subscription once the image clears `ARENA_FLOOR`.

## What closing it looks like

Join the sidecar's per-subscription `in_place` onto the model's subscription
rows. The join key is the hard part and it is not new: the sidecar keys a
subscription by its CALLBACK id with the WRITTEN topic beside it
(`source_topic`), while the model names the RESOLVED topic. `contract_join`
already solved the same attribution problem for QoS (attribute only when the
node declares no remaps, the written name is absolute, and `(kind, type, name)`
picks out one row on each side; otherwise refuse per row) — reuse that rule,
never a second one. A row the join cannot attribute keeps refusing, which is
today's price.

The probe also knows the component's LANGUAGE (`SourceMetadata::language`), so
the same join could supply the language half for an observed-`false` row on a
schemaless backend, which the model road refuses for want of one entry
language.

## Do not

Do not state `in_place` from the backend alone, and do not infer it from the
language. Nine of the executor's eleven subscription entry points cannot use an
in-place dispatch; crediting an unobserved endpoint with it prices it at NO
receive region, which is `NodeError::BufferTooSmall` at a registration the
oracle passed (phase-457 W3's reproduction).
