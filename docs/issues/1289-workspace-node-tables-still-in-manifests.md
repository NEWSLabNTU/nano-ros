---
id: 1289
title: "45 workspace node packages still declare their class in `[package.metadata.nros.node]` — phase-445's acceptance `rg` cannot return nothing"
status: open
type: tech-debt
area: tooling, examples
severity: low
found: 2026-09-11
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
