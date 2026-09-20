---
id: 1393
title: "A workspace image and a cmake/Zephyr/NuttX entry can never get
  payload-class sizing: they have no bound inventory and no schema walk, so the
  fields carrying most of phase-454's measured savings are unsourceable there"
status: open
type: enhancement
area: build, cli, rmw
severity: medium
found: 2026-09-20
related: [rfc-0100, phase-454, issue-1340]
---

## What is true after phase-454

The sizing descriptor reaches **one road of three**. A single-package cargo leaf
has everything `write_for_leaf` needs — a `LeafImage`, the leaf's `metadata/`
probe, and its `generated/` bound tables — and since W12 it carries the
contract's facts, which is where the measured **−37 %** on
`examples/native/rust/listener` comes from.

A **workspace cargo image** and a **cmake / Zephyr west / NuttX entry** have none
of those three. There is nothing to write a descriptor from, and naming
`NROS_SIZING_DESCRIPTOR` at a file nobody writes is a hard build error by design,
so it would break every C/C++ and workspace image rather than size one.

## What a model-only producer can and cannot answer

The owner's ruling is to take **option A now** — a second producer that emits
what the SystemModel knows and **refuses** every field it cannot source. This
issue is the follow-up that ends the refusals.

| field | model-only producer | why |
| --- | --- | --- |
| entity counts, per-endpoint QoS, topics, types | **Stated** | the SystemModel carries the declaration |
| `wire_bound_bytes` | **Refused** | needs the bound inventory |
| `storage_bytes` | **Refused** | target-ABI dependent; needs the board descriptor resolved for that image |
| `[types]` `max_fields` / `max_kinds` / `max_nested_depth` | **Refused** | needs the schema walk codegen already performs |
| `registration_path` | **Refused** | needs to know which subscribe spelling the image writes |

## Why this matters more than the field count suggests

**The refused fields carry most of the savings.** The count-derived knobs are
real but small — uORB −1,512 B, cffi −280 B. The large measured numbers are
payload-class:

| wave | saving | class |
| --- | --- | --- |
| W6.b (XRCE) | −355,008 B (83 %) | payload + reliability |
| W6.a (zenoh) | −124,032 B (22 %) | payload |
| W12 (listener, end to end) | −101,504 B (37 %) | payload, via declared depth |

So option A ships counts to two thirds of the tree and leaves **the majority of
the benefit on the cargo-leaf road only**. That is an acceptable staging point
and a poor terminal state, which is why this is filed rather than left implied.

## An extra obstacle, already measured

Even on the road that HAS a bound inventory, **no service or action endpoint can
get a `wire_bound_bytes` today**: `BoundInventory::record_message` runs for
`.msg` files only, so `pkg/srv/Name_Request` has no bound row. W6.a found this
and correctly declined to fix it inside a backend wave.

So "give the cmake road a bound inventory" is really two things:

1. record bounds for service and action member messages, on **every** road; and
2. produce a per-image bound inventory and schema shape where today only a cargo
   leaf has one.

## What a fix has to decide

* **Where the second producer's inputs come from.** A cmake entry knows its
  interface closure at configure time (`nros_generate_interfaces`), which is the
  same information codegen walks. Whether that is re-derived or exported from
  codegen is the design question — re-deriving it is a second opinion about the
  bound, which is the class issue 0196 keeps finding.
* **Whether `registration_path` is answerable at all** for a C/C++ entry. W4
  credits one `c_typed_hint`, but whether a given call site passes
  `rx_size_bound<M>` is per-call-site. Issue **1340** is blocked on the same
  missing fact from the Rust side, and the two should be settled together rather
  than twice.
* **Whether the board descriptor can be resolved per image** on the cmake road,
  which is what `storage_bytes` needs and what `[target]` already does for a
  leaf.

## Acceptance

`just check` has no natural home for this. The measurable form is: a cmake or
Zephyr image with a contract derives the same payload-class knobs a cargo leaf
with the same contract derives, and `mem-report --baseline` shows a comparable
delta on a named image. Until then, every refusal this issue ends should name
this id, so the descriptor itself says what is missing and why.

## Not a regression

Nothing is worse than before phase-454 on these roads: they keep the
`NROS_DECLARED_*` / `NROS_DERIVED_*` carriers and behave exactly as they did.
This issue is about the ceiling, not a fault.

**It also gates phase-454 W9.** Retirement removes those carriers, and while two
of three roads have no replacement they must stay — `check-knob-single-reader`'s
rule inverted.
