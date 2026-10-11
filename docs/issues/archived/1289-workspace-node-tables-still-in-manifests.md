---
id: 1289
title: "45 workspace node packages still declare their class in `[package.metadata.nros.node]` — phase-445's acceptance `rg` cannot return nothing"
status: resolved
type: tech-debt
area: tooling, examples
severity: low
found: 2026-09-11
resolved: 2026-10-12
related: [1288, rfc-0098, phase-445]
---

# The half of D5 the deployment reader does not own

RFC-0098 D5 retires `[package.metadata.nros.{entry,deploy.*,node,component}]`
from the manifest. phase-445 W3b did that for every SINGLE-PACKAGE leaf (the node
moved to its `system.toml` `[[component]]`), and W5 removed every DEPLOYMENT key
left anywhere — `[package.metadata.nros.entry] deploy` and every
`[package.metadata.nros.deploy.*]` table — and deleted the manifest fallback
(`leaf_system::from_manifest`) that read them.

What remains, counted by `check-leaf-deployment-spelling` (it now scans workspace
manifests and refuses their deployment keys, and reports these instead):

- **45 workspace NODE packages** (`examples/workspaces/**`, `examples/templates/**`,
  `packages/testing/nros-tests/fixtures/**`) carrying
  `[package.metadata.nros.node]` or `[package.metadata.nros.component]`. These
  are not a deployment: they are the metadata pipeline's declaration of a class
  (`orchestration::workspace`'s serde read, `nros sync`'s source metadata, the
  `nros check` dispatch lint). The bringup's `[[component]]` rows already name
  `pkg`/`class`/`name`, but not everything a node table states — `dispatch`,
  `default_namespace`, `entities` (issue 1265's D8 workaround) — and a workspace
  node package can be launched by several bringups, so "its" `[[component]]` is
  not one row.
- **The empty `[package.metadata.nros.entry]` marker** on every workspace entry
  (and the RTIC leaves' `node_pkgs`). It is not a deployment either: the
  selection facade (`orchestration::facade::write_facade`) keys on the table's
  presence to decide a package is an entry, and so does `deploy_bound()`.

So phase-445's acceptance line

    rg '^\[package\.metadata\.nros\.(deploy|entry|node|component)' examples

cannot return nothing as written. `deploy` returns nothing since W5; `entry`,
`node` and `component` need the decisions above first.

## Fix

Decide the home of a workspace node's per-class facts (the bringup
`[[component]]` row, or a node-package `nros.toml`), move `dispatch` /
`default_namespace` / `entities` there, have the metadata pipeline read it, and
replace the facade's "has an entry table" predicate with "is claimed by an
image" (`leaf_system::for_entry`). Then the acceptance `rg` can be the gate.

## Resolution (2026-10-12)

The Fix line above proposed MOVING the node tables. The decision taken instead
splits the issue along the line RFC-0098 D5 actually draws — deployment versus
everything else — and measures each half.

**1. The node / component tables STAY: they are registration, not deployment.**
`[package.metadata.nros.node]` / `component` (class, name, callback groups,
publishes, subscribes, parameters, remaps) is the Rust spelling of node
REGISTRATION: the counterpart of the 82 C/C++ packages'
`nros_components_register_node(<target> PLUGIN pkg::Class EXECUTABLE …
CALLBACK_GROUPS …)` in their `CMakeLists.txt`, itself the mirror of ROS's
`rclcpp_components_register_node`. A package registers its class once, in its
own build file, however many bringups launch it; a bringup's `[[component]]` row
is that bringup's choice to RUN it. D5 retired deployment keys from manifests,
and this is not one. `check-leaf-deployment-spelling` now reports these as
registrations (43 workspace node packages), not as debt, and still refuses them
on a single-package leaf, whose `system.toml` `[[component]]` is its only
declaration.

**2. `default_namespace = "/"` is gone from every manifest.** All 41 tracked
tables carried exactly `"/"`, and the `cargo-nano-ros` component scaffold wrote
it. Absent and `"/"` were checked to plan identically: the value reaches only
`CargoComponentSummary::default_namespace`, which feeds the synthetic metadata
JSON's `default_namespace` key and `ComponentOverrides::default_namespace`, and
no planner, codegen, census or resolver path reads either (`git grep` over
`packages/` and `play_launch`); a launch node's namespace already defaults to
`"/"` (`planner.rs`). A namespace is the launch file's to give, as for a C/C++
node. The schema still ACCEPTS the key, so an out-of-tree manifest keeps parsing
under `deny_unknown_fields`; nothing in-tree writes it.

**3. "Is an entry" is "an image claims it", in every reader.** One predicate,
`nros_orchestration_ir::leaf_system::claiming_image` — the package's own
`system.toml`, else a bringup `[image.<id>]` naming it (`entry = "<pkg>"`, or the
`<id>_entry` name a generated entry gets) through the existing `for_entry` —
replaced the presence of `[package.metadata.nros.entry]` in:

- `orchestration::facade::write_facade` (now takes the bringup dir);
- `nros check --workspace`'s dispatch-matrix lint (`check_workspace.rs`);
- the metadata probe's `deploy_bound` — `PackageMetadataNros::deploy_bound()`
  is deleted; `Workspace::discover` settles it over the discovered bringups;
- `cmd::ws::entry_package_name_of` → `cargo_package_name_of`, whose three
  callers (`leaf_projection_board`, `board_facts`, `build.rs`
  `rust_entry_system`) already asked `workspace_entry_system`, which now
  delegates to `claiming_image` instead of its own copy of the loop;
- the `example_shape` test's `is_entry`.

The two generators that wrote the marker (`builder/entry.rs`,
`cmd/new_entry.rs`) no longer do, and the 10 tracked empty markers plus the
`nros check` fixture's were deleted. Every one was already claimed by an image
(checked per package before deleting), so no facade, lint or probe outcome
moved.

**`node_pkgs`: the table keeps a home, the in-tree uses do not.** The 9 RTIC
manifests' `[package.metadata.nros.entry] node_pkgs` (8 under
`examples/mps2-an385-baremetal/rust/*-rtic*`, plus
`nros-tests/bins/rtic-run-plan-e2e`) each named only the package itself, which
is exactly `nros::main!`'s fallback when the key is absent (the package name,
`_entry` stripped) — so all 9 were deleted with no change in what the macro
emits. The reader (`main_macro.rs::read_entry_node_pkgs`) and the schema keep
the key, with `max_callbacks` / `max_sched_contexts`, for an out-of-tree entry
that registers a package OTHER than itself; that is a build fact, not a
deployment and not a marker. `check-leaf-deployment-spelling` R3 refuses an
entry table stating no such fact (empty, or a self-referencing `node_pkgs`)
anywhere in the tree.

**4. phase-445 acceptance bullet 5 is MET and gated.** Narrowed to
`rg '^\[package\.metadata\.nros\.(deploy|entry)' examples` — 0 hits (was 35,
and 45 when this was filed); R3 of `check-leaf-deployment-spelling` runs it.

Mutation evidence (each predicate reverted to the old marker test, then
restored): facade `a_facade_follows_the_image_claim_not_an_entry_table`,
`check_workspace::the_matrix_keys_on_the_image_claim_not_a_manifest_table`,
`workspace::deploy_bound_is_the_image_claim_not_an_entry_table` and
`leaf_system::an_entry_is_what_an_image_claims_not_what_a_manifest_marks` each
went rc=101 mutated and rc=0 restored; the gate went rc=1 with a re-added RTIC
`node_pkgs` marker, rc=1 with an empty marker on a workspace node package, rc=1
with its acceptance check removed (self-test), and rc=0 restored.
