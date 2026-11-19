---
id: 2737
title: "A Zephyr west image with an owned entry and no model was refused at its message-bound seed under NROS_PROBE_FAILURE=refuse"
status: resolved
type: bug
severity: medium
area: [cli, orchestration, zephyr]
found: 2026-11-19
resolved: 2026-11-19
resolved_in: "fix(cli, issue 2737): one rule for an owned entry with no model, in every reader that derives from it"
related: [2482, 2706, 2735, 2736]
---

## What happened

An image that boots an OWNED entry (`[image.<id>] entry = ...`) from a
bringup with no `[[component]]` row and no launch has no SystemModel
(`owned_entry_without_model`, RFC-0071 D19). Issue 2482 taught the entity
inventory to read a failed derivation of such an image as "nothing
declared". The two Zephyr west message-bound seeds did not:
- `resolve_rust_west_image`;
- `seed_hand_written_west_bounds`.

Each read the model through `image_launch_node_dirs`, got "cannot resolve the
SystemModel: its launch file ... does not exist", and reported it as
`NotDerived::Failed`. The fixture lane runs with
`NROS_PROBE_FAILURE=refuse`, so the configure stopped:

```
Error: configure: image `isr_wake_zephyr_bringup:zephyr_zenoh`
  `isr_wake_zephyr_bringup:zephyr_zenoh`: message bounds not seeded, because what it
  declares could not be derived: cannot resolve the SystemModel: ...
```

The `no_inventory` statement could not help, by design: it excuses "nothing
declared", not a failure. Met building issue 2706's Zephyr interrupt-wake
image, the first Rust west image with an owned entry and no component.

## Fix

The rule has one home: `entity_census::owned_entry_declares_nothing`. It maps a
`Failed` derivation of an owned entry with no model to `NothingDeclared`; any
other image's failure stays a failure. It is used by:
- the entity inventory (stage 3.5), replacing its inline match;
- both west message-bound seeds.

Sweep: `git grep -n 'NotDerived::Failed(why)' -- packages/cli` now matches only
the helper. The other seed sites read a generated entry's node closure or a
leaf, never an owned entry's model.

Test: `entity_census::tests::a_failed_derivation_of_an_owned_entry_without_a_model_declares_nothing`.
The end-to-end check is the `zephyr-rust-isr-wake-zenoh` fixture row, which
configures under `refuse`.
