---
id: 1515
title: "Four template workspaces carry NEITHER workspace-root marker — the
  resolver has two spellings and these have both missing"
status: open
type: bug
area: examples, cli
severity: medium
found: 2026-09-27
related: [rfc-0065, rfc-0098, issue-1453]
---

## What this is

A workspace root has **two** tracked spellings, and `detect_workspace_root`
resolves them in this order (documented at
`packages/testing/nros-tests/tests/example_shape.rs`, the
`belongs_to_enclosing_workspace` helper):

1. a `.colcon_workspace` file — the tracked marker a MIGRATED workspace carries
   (RFC-0065 D3 / phase-383 W10.a), because `nros build` GENERATES the root
   manifest into the working tree, so the manifest is gitignored and absent from
   a fresh clone;
2. otherwise a root `Cargo.toml` declaring `[workspace]` — the original
   spelling.

Asking for both is deliberate: reading only the manifest made the answer depend
on whether the workspace had ever been BUILT — green on a machine that had, red
on a fresh clone.

**Four trees under `examples/templates/` answer neither question.** Measured:

| tree | `.colcon_workspace` | root `Cargo.toml` `[workspace]` | has a bringup |
| --- | --- | --- | --- |
| `c-and-cpp-mixed-workspace` | no | no | yes |
| `multi-node-workspace-cpp` | no | no | yes |
| `pure-c-workspace` | no | no | yes |
| `multi-package-workspace` | no | no | no (`build-all.sh`) |

All four are C/C++-led, so they have no root `Cargo.toml` at all and the second
rung cannot fire for them even in principle. Every one of the 15 trees under
`examples/workspaces/` carries the marker; so do
`templates/multi-node-workspace` and the scaffold `cargo-nano-ros` writes
(`workspace_scaffold.rs` copies
`examples/templates/multi-node-workspace/.colcon_workspace` verbatim and asserts
it back as "the root marker").

## Why it matters

The marker is not decoration. It is how a consumer answers "is this directory a
workspace root" without building:

- `nros::main!` finds the bringup through it first
  (`packages/cli/nros-cli-core/src/builder/entry.rs`);
- the book teaches it as "the tracked marker: this directory IS a workspace
  root" in five places under `book/src/`;
- `nros-build`'s own tests write it into every fixture root they construct
  (`tests/pkg_index.rs`, `tests/launch_parser.rs`).

So a template that lacks it is teaching a shape the tooling does not recognise,
and `pure-c-workspace` is additionally the tree issue 1453 is about — it is
built somewhere, which is how this stays interesting rather than theoretical.

## What is NOT yet measured

**Whether it currently breaks a build.** The four were surveyed statically; no
build was run against them for this issue. Three have a bringup, so the
resolver is reached, but which rung each consumer needs — and whether some other
path (an explicit `--workspace-root`, a `NROS_WORKSPACE_ROOT` from
`builder::cargo_config`, a colcon invocation that never asks) supplies the
answer anyway — is open. **Do not fix this by adding four files until a build
says what the absence costs**: if a consumer resolves these today, the finding
is a gate ("every tree with a bringup declares a root") rather than four
touched files, and if it does not, the failure text is what the gate should
name.

The same measurement decides `multi-package-workspace`, which has no bringup
either and builds through `build-all.sh`. It may legitimately not be a
workspace root at all — in which case the answer is a note saying so, not a
marker.

## Acceptance

- A build (or a consumer invocation) per tree, recorded, saying what the missing
  marker costs — the failure text if it fails, the rung that rescued it if it
  does not.
- Each of the four resolved on that evidence: a marker, or a documented reason
  it is not a workspace root.
- A gate that keeps the class closed, scoped to the rule and not to the four
  sites (`docs/development/codebase-audit-checklist.md`'s issue-0196 rule): the
  predicate is "a tree with a bringup declares a root by one of the two
  spellings", over `examples/` as a whole.
