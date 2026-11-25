---
id: 2964
title: "An image observed to declare no parameters keeps the default 32-parameter store, about 62 KB of executor backing"
status: open
type: bug
severity: high
area: [cli, params, sizing]
related: [issue-2772, issue-2768, rfc-0100]
found: 2026-11-25
---

## What happens

An image that serves the stock parameter services, and whose code declares
no parameters, gets the crate-default parameter store: 32 parameters,
64-byte names, 256-byte strings, 8 store slots. The store is carved from the
executor backing, and on the Raspberry Pi Pico W (264 KiB) that is the
difference between fitting and overflowing.

Measured by nros 118 on 2026-11-25 (issue 2772), Rust templates on
`rpi-pico-w-zephyr`:

| Template | Default store | `max_parameters = 1` stated | Executor backing |
| --- | --- | --- | --- |
| listener | RAM over by 16,344 B | 224,632 B (83.1 %) | 102,296 to 40,248 B |
| server | over by 18,216 B | 226,504 B (83.8 %) | 104,344 to 42,296 B |
| action-server | over by 33,152 B | 241,440 B (89.3 %) | 110,232 to 48,184 B |

The talker fitted (196,120 B, 72.6 %) only because `nros new` wrote it a
contract with `params: {}`. PR #4843 makes every template write one, which
fixes the templates. This issue is the class: any image with no `params:`
in its contract, whose code the build watched declare nothing.

## Why

`ParamDeclarations::from_model_with` (`nros-cli-core`
`entity_inventory.rs`) returns `Absent` when the contract states no
`params:` and every recorded declaration list is empty:

```rust
if model.contracts.node_params.is_empty() && declared.values().all(|ps| ps.is_empty()) {
    return ParamDeclarations::Absent;
}
```

`Absent` keeps every store bound on its default, by design: it means
"unknown", and a node may declare parameters the build never saw. But the
condition folds two different facts into it: no record at all
(`recorded = None`), and a record that observed every launched node and
found nothing (`recorded = Some`, all lists empty). The second is a
declaration of zero. `for_shared_runtime` already treats it as one for a
multi-entry configure; the single-image roads do not.

## Ruling (user, 2026-11-25)

Derive the parameter store for every image. An image that serves the
stock parameter services but has no declared parameters sizes its store
for what the runtime seeds (`use_sim_time`, one parameter), not the crate
default. Precondition: on each road, the build must OBSERVE a node's
runtime declarations before reading "none" as zero. Where it does not, the
road keeps the default or refuses, and says why.

## Does each road observe the code's declarations?

The metadata probe records every `declare_parameter`, in all three
languages, into the sidecar's `parameters` list
(`orchestration/source_metadata.rs`). Rust records through
`census_hooks::on_param_declare` and the `nros-node` declare observer, C
through `nros-c` `parameter.rs` `declare()` (which `rclc_add_parameter`
uses), C++ through `nros-cpp` `params_shim.rs`. A read is recorded as a read
and not counted. A fresh sidecar with `parameters: []` is a positive zero.
Its freshness key covers the package source, the CLI planning stamp and the
`probe-source-dirs.txt` closure. On the workspace roads a missing sidecar
makes the image's record `None` and a stale one is an error, so neither can
read as zero.

| Road | Observes the code's declarations | Status source today | "None declared" may read as zero |
| --- | --- | --- | --- |
| workspace cargo image (`nros build`) | yes | `contract_inventory`, then `from_model_with(recorded)` | yes, when the record is `Some` |
| cmake workspace (model road) | yes | `nros dev image entity-inventory --model`, then `from_model_with`; `NROS_PARAM_DECLARATION_STATUS` in the fragment | yes, when the record is `Some` |
| Zephyr west entry (C/C++) | yes | `nros build`'s composed inventory, then `from_model_with` | yes, when the record is `Some` |
| multi-entry shared runtime, cmake fragment | yes | `for_shared_runtime`, then `union_over_images` | yes, when the record is `Some` and watched whole (it read any silent record as zero before) |
| `--from-model` sizing descriptor | yes | `from_model_with(recorded)` | yes, when the record is `Some` |
| cargo single-package leaf | yes, but unguarded | `from_model_with(model, recorded_params_for_leaf)` | **no, until its reader checks freshness**: `recorded_params_for_leaf` parses every `metadata/*.json` with no provenance check, so a stale `[]` reads as zero |
| standalone cmake leaf (`--from-leaf`) | no | always `Absent` | no: keeps the default |
| unwired image (store only) | no | `Absent` (`from_model`, contract only) | no: keeps the default |
| multi-entry shared runtime, unwired descriptor | no | `union_over_images` over contract-only inventories | no: keeps the default |

## Declarations a record cannot rule out

Reading a record as zero is safe only where none of these applies:

1. **Undeclared-parameter modes.** A remote `set_parameters` can create
   store entries at runtime. The modes are Rust `Node::use_undeclared_parameters`
   and `allow_undeclared_parameters`, and C
   `rclc_parameter_options.allow_undeclared_parameters`. C++ refuses both
   at compile time. The sidecar does not record that a node opted in.
2. **Launch seeds** (`parameters=`, `params_files`) take store slots at
   boot. `from_model_with` already counts them when the record is `Some`,
   so a zero read stays a zero plus seeds.
3. **Declarations on a branch the probe's registration pass does not
   reach**: in a callback, after spin, or behind a condition that is false
   in metadata mode.
4. **A probe built without `param-store`**: the Rust imperative routes'
   declare observer is behind that feature, so such a probe reports
   nothing for them.
5. **A full recorder** drops a row. That is fatal only inside a census run.

## Plan

1. A fourth state, or a split of `Absent`: **observed none**. It means the
   record is `Some`, every launched node is covered, every list is empty,
   and the contract is silent. It sizes the store for the seeded parameter
   plus launch seeds, the bound `params: {}` already produces.
2. Item 1 of the list above becomes part of the record: the sidecar states
   whether a node opted into undeclared parameters, and such a node keeps
   the default.
3. Items 3 to 5 are probe properties. Item 4 needs its own answer: a
   record from a probe without `param-store` is no evidence of zero, so it
   must not reach "observed none". Items 3 and 5 are a residual risk,
   bounded by how a declaration past the store fails at runtime, which is
   named in every language (checked 2026-11-25):
   - the runtime logs the node, the parameter, the store's capacity and the
     knob that sizes it (`Executor::report_parameter_refused`, nros-node);
   - Rust's declare returns `NodeError::ParameterRejected`, whose text names
     `max_parameters` and the contract's `params:`;
   - C returns `RCLC_RET_FULL` (`nros-c` `parameter.rs`);
   - C++ returns `NROS_CPP_RET_FULL` and the node's error becomes
     `PARAM_STORE_FULL_WHAT` (`params_shim.rs`, `rclcpp.hpp`).
   Tests: nros-node's store-full cases (`store_full_probe_4018`) and the
   C++ `param_store_full_runtime.cpp` fixture. So a smaller store turns an
   unobserved declaration into a named, logged refusal at boot, not a
   silent one. The code adds a case for the derived seed-only store: a
   second declaration is refused with the same report.
4. The cargo leaf reader checks sidecar freshness the way the workspace
   reader does. Until it does, that road keeps the default.
5. The roads that observe nothing keep `Absent`, and `--explain-sizing`
   names it: "the parameter store keeps its default bounds because nothing
   observed this image's declarations; state `params:` in its contract".

Acceptance: the Pico listener, server and action-server templates with
NO contract (or with a contract that omits `params:`) fit the board, at the
same backing as with `params: {}`; a node that sets
`allow_undeclared_parameters` keeps the default; a stale cargo-leaf
sidecar keeps the default.

## Progress (2026-11-25): the fix, on PR #4849

- **The record says when it is whole.** The probe sidecar (schema 7) gains
  `parameter_declarations_watched`. `metadata_mode` arms it when it installs
  its parameter-declaration listeners (a build with `param-store`). It writes
  `false` when a row is refused, a declaration names no node, or a node
  opts into undeclared parameters: nros-node now reports
  `allow_undeclared_parameters(true)` and `use_undeclared_parameters()` to
  the probe through the registration observer. An absent key, from an older
  sidecar or a producer that does not watch, is no evidence. This answers
  items 1, 4 and 5 above.
- **The inventory reads it.** `recorded_for_launch` keeps the record WHOLE
  only when every launched node's sidecar says `true`, and
  `ParamDeclarations::from_model_observed` reads a whole, silent record as
  `Declared` with no parameters. That is the bound `params: {}` gives. The
  workspace cargo image, the cmake model road, the Zephyr west entry and
  `--from-model` take it.
- **Unchanged, so they keep the default:**
  - the cargo single-package leaf, whose reader still checks no freshness;
  - `--from-leaf`;
  - the unwired roads;
  - the Rust source-metadata harness, which records into its own recorder
    and does not arm the watch yet, so a Rust component's sidecar states no
    watch.
  The multi-entry fragment's `for_shared_runtime` now asks the same
  question through the same check: it read any silent record as zero, which
  is the unsafe case for a node that allows undeclared parameters. A shared
  runtime with such an image beside a declaring one refuses the union, so
  the store keeps its default.
- **Say why.** `--explain-sizing` (and the one-line summary) now names a
  built store whose parameter count nothing stated, with the reason and the
  `params: {}` remedy.

Tests:
- `an_image_watched_whole_that_declares_nothing_declares_zero` and
  `a_declaration_or_a_contract_answers_as_before_when_watched_whole`
  (nros-cli-core);
- the recorder's three watch states (nros `node_metadata`);
- `imperative_param_census`: every route reported gives `true`, and
  `allow_undeclared_parameters(true)` turns it `false`;
- `a_built_store_with_no_stated_count_is_named` (nros-sizing-descriptor);
- `a_shared_runtime_image_whose_watch_broke_keeps_the_default`
  (nros-cli-core).

Not measured yet: a Pico image with no contract, built on this branch.
