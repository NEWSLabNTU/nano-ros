---
id: 2650
title: "Every parameter store slot carries a 256 B description buffer, about 15 KB at 32 slots when no value bound is paid"
status: resolved
type: enhancement
area: [memory, params]
severity: low
found: 2026-11-16
related: [2647, 2649, phase-478]
---

**Home:** [phase-478](../../roadmap/phase-478-island-developer-experience-findings.md)
— follow-up to I28. This is the next largest part of a slot once issue 2647 has
removed the string-array product.

## Problem

A store slot is `Option<ParameterEntry>`, and the entry carries an
`Option<ParameterDescriptor>` inline. The descriptor holds a second copy of the
name (`MAX_PARAM_NAME_LEN`, 64 B) and a description of
`MAX_PARAM_DESCRIPTION_LEN` bytes, default 256. The description is code-supplied
text, so nothing derives its bound.

Measured on host x86_64 with every value bound at 0: `ParameterValue` is 16 B
and a slot is 480 B. At the default 32 slots that is 15,360 B, almost all of it
descriptor.

## Possible shapes

- Default `MAX_PARAM_DESCRIPTION_LEN` to 0 on embedded boards. This is a board
  fact already (`[knobs.params]`), so it is cheap, but it drops the text that
  `ros2 param describe` shows.
- Hold descriptions out of line, sized by what the code actually passes. That
  needs the same arena machinery as [issue 2649](../2649-param-string-array-shared-arena.md).
- Drop the descriptor's copy of the name, since the entry's `Parameter` already
  holds it, and build the name only when the describe reply is written.

## Ruling and progress

Ruled 2026-11-17: size the description space from what the code declares, so
users see no change, and never default it to 0. The work is in three parts:

1. **Name copy (PR #4167).** The descriptor no longer holds the parameter's
   name: 64 B less per slot.
2. **Census capture (this change).** The contract's `params:` rows state a type
   and no description, so only running the code can say how long the prose is.
   A descriptor's `description` (`NROS_MAX_PARAM_DESCRIPTION_LEN`) and its
   `additional_constraints` (`NROS_MAX_PARAM_CONSTRAINTS_LEN`, default 0) are
   set only through `ParameterDescriptor::set_description` and
   `set_additional_constraints`. Every route reaches those two: the typed
   builder, a descriptor the code builds, and the C and C++
   `add_parameter_description` calls. Under `metadata-mode` each reports the
   requested length to the probe, before truncation. The sidecar (schema v6)
   states the longest of each as top-level `parameter_description_max_len` and
   `parameter_constraints_max_len`. The keys are ABSENT when nothing watched,
   and `0` when the probe watched and saw no such text.
3. **Derivation.** `[params] max_param_description_len` in the sizing
   descriptor. `nros-params` reads it below every stated rung (env, Kconfig,
   board). The model road reads each launched node's sidecar key
   (`ProbeRow::description_max_len`, folded into
   `EntityInventory::param_prose`), with one test per case:
   - Every launched node has one fresh sidecar that watched: the longest
     description, floored at 16 and capped at 256.
   - A node with no sidecar, or one written before the watch existed: refused,
     naming the node. The store keeps 256.
   - Nothing consulted the probe (a contract-only image, the cargo-leaf and
     standalone roads): absent, so the store keeps 256 and no descriptor gains
     a refusal.

   A node the census cannot see as a NODE (issue 2681) does not hide prose: the
   watch is process-wide, so its descriptions are counted under the component
   whose code ran.

## Still open

- The cargo-leaf and standalone roads read their own probe JSON
  (`leaf_entity_env::declaration_from_probe`) and do not carry the key yet, so
  they keep 256.
- A shared runtime (several images, one runtime) states the length only when
  every image was watched. An unprobed image beside probed ones refuses, and
  all-unprobed stays absent.
- `NROS_MAX_PARAM_CONSTRAINTS_LEN` is captured (`parameter_constraints_max_len`)
  and not derived. Its default is already 0, and its truncation report names
  the wrong knob (issue 2686).

## Resolved (2026-11-23)

The derivation in part 3 never reached a Rust image, and did not reach the
leaf roads at all. Both are fixed here.

**No Rust sidecar stated the length.** The Rust source-metadata harness (the
CLI's generated `render_harness_main`, used by the workspace probe refresh
and the leaf probe alike) records into its OWN `MetadataRecorder`. The only
prose observer was `metadata_mode`'s, which writes to that module's global
recorder, and the harness is not built with `metadata-mode` anyway. So every
Rust sidecar left `parameter_description_max_len` out, and every Rust image
refused the derivation and kept 256.
- **The new entry.** `nros::__macro::record_node_metadata_watching_prose`
  (feature `probe-descriptor-text`, which compiles `nros-params`' observer and
  nothing else of a probe) installs an observer, runs `register()`, then arms
  the harness's recorder and records the longest description and constraints
  text.
- **With `metadata-mode` also compiled in,** opening a node re-installs that
  module's observer over this one, so the entry also folds in what
  `metadata_mode::descriptor_text_seen()` heard.
- **The harness** builds with the feature and calls the entry.
- **Test:** `the_harness_entry_states_the_prose_its_component_set`. A
  component sets a 42-byte description and an 8-byte constraint; the sidecar
  states 42 and 8, and the plain entry, the control, states nothing.

**The leaf roads.**
- **The cargo leaf** (`leaf_entity_env::inventory_for_leaf`) reads each
  sidecar's key, by the model road's rule:
  - watched only when every sidecar watched;
  - a sidecar from before the watch, or an unprobeable component, makes it
    unknown and refuses, so the store keeps 256;
  - no sidecar at all leaves it not consulted.
  - Test: `a_leaf_states_the_description_length_its_sidecars_watched`.
- **The standalone (census) leaf** (`cmd/sizing_descriptor.rs`) takes the
  census document's key (`census_description_prose`): the census runs the
  leaf's whole program.

**Measured.** `max_param_description_len` went from refused to 16 (no
description is set, so the floor applies). Before is the same build with
`NROS_PARAMS_MAX_DESCRIPTION_BYTES=256`, the room the store kept; mem-report
`--baseline` gives the diff.

| image | road | static RAM |
|---|---|---:|
| `examples/mps2-an385-baremetal/rust/listener` | cargo leaf | -1,936 B (`EXECUTOR_BACKING` -1,920) |
| `examples/templates/cpp-port-minimal-publisher/mps2-an385-freertos` | standalone census | -1,920 B (`GlobalStorageHolder<0>::storage` -1,920) |

1,920 B is 8 slots x (256 - 16) B.

**Not done, and not this issue's:** `NROS_PARAMS_MAX_CONSTRAINTS_BYTES` is
still captured and not derived. Its default is 0, so deriving it saves
nothing, and would only ADD room where code sets constraints text (which is
truncated to 0 today). The shared-runtime rule in "Still open" above is how
that road behaves (`shared_runtime_prose`), not a gap.
