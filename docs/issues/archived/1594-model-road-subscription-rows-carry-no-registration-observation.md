---
id: 1594
title: "The model-road sizing descriptor never sees a registration OBSERVATION —
  it composes the SystemModel with `nros-metadata.json`, while `in_place` lives in
  the probe's per-component sidecars — so every subscription on zenoh / XRCE
  refuses its `registration_path` and keeps a receive region it may never claim"
status: resolved
type: tech-debt
area: [build, cli]
severity: medium
found: 2026-10-01
resolved: 2026-10-03
related: [1340, 1393, 1522, 1407, 1419, 1648]
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

## Resolution

The join is `contract_join::observe_registrations` — and it IS the contract
join, not a second rule. It runs `join(probe, model)` over the workspace's probe
sidecars exactly as the leaf road does (no remaps on the node, an absolute
written name, `(kind, type, name)` unique on BOTH sides), and only a row the
join ATTRIBUTED lends its `in_place` to the model row with the same component
and key — which must itself be unique in the composed (metadata ⨯ model)
inventory, or nothing is written. Every other row stays `None` and keeps
refusing: today's price, the safe direction.

* **Only CURRENT sidecars count.** `metadata_refresh::fresh_probe_inventory`
  uses the key `refresh_stale_sidecars` itself trusts (`probe_inputs_key` —
  sources, CLI stamp, probe closure; issue 1578). A stale sidecar is skipped with
  a note, because a reused `in_place: true` against code that moved to a
  buffered entry point would price that endpoint at no receive region. Two
  sidecars naming one component are both dropped.
* **Both model-road producers call one composition**,
  `contract_join::observe_workspace_registrations`: `nros ws sizing-descriptor
  --from-model ... --workspace <ws>` (the cmake entry road —
  `nano_ros_entry()` passes its `_ws_root`) and `nros build`'s workspace cargo
  image (`ResolvedImage` now carries the model beside the inventory). Neither
  is fatal on a workspace that does not discover.
* **The sidecars are configure inputs** (issue 1018): the verb prints each one
  it read as an `input <path>` line after the descriptor path, and
  `nros_sizing_descriptor_from_model` registers them in
  `CMAKE_CONFIGURE_DEPENDS`. `tests/cmake-sizing-descriptor-tests.sh` G4 checks
  both halves, with a negative control for each (dropping the line split, and
  dropping the registration, each fail it).

### Measured

`examples/workspaces/cpp` after `nros sync` (six sidecars written, the
listener's subscription `in_place: true`), descriptor for `native_entry` written
by the in-tree CLI from `system_model.yaml`, `--rmw zenoh --host-build`:

| | `/chatter` subscription `registration_path` |
| --- | --- |
| no `--workspace` (before) | refused, naming this issue |
| `--workspace examples/workspaces/cpp` | `"in_place"` — "registration observed for 1 of 1 subscription row(s), from 6 probe sidecar(s)" |
| same, after appending a comment to `listener_pkg/src/Listener.cpp` | refused — the sidecar is reported stale and the descriptor is byte-identical to the no-workspace one |

The image-level byte saving is issue 1340's (10,840 bytes per `KEEP_LAST(10)`
subscription once the image clears `ARENA_FLOOR`, measured on the leaf road);
it was not re-measured on a built cmake image here.

### Tests

`contract_join`: `a_model_row_takes_the_probes_observation_through_the_contract_join`
(both `true` and `false` carried), `an_unattributable_probe_row_lends_the_model_row_nothing`
(remapped node, relative name, other topic, unobserved, no written topic),
`two_registrations_on_one_topic_observe_nothing`,
`another_components_sidecar_observes_nothing_here`.
`sizing_descriptor`: `the_model_road_states_in_place_once_the_probe_observation_is_joined`.

### Not done here

The LANGUAGE half — an observed `false` row on a schemaless backend still
refuses, because the model road has no one entry language and the join does not
carry the sidecar's — is issue 1648.
