---
id: 2085
title: "24 example workspace packages path-depend on `nros` by five `..`, so no in-tree workspace survives being copied out"
status: resolved
type: tech-debt
area: [examples, build, workspace]
severity: medium
found: 2026-10-26
resolved: 2026-10-27
related: [rfc-0085, rfc-0026, rfc-0067, phase-452, issue-0905, issue-2046]
---

# Example workspace packages reach `nros` by repository layout

**Home:** [phase-452](../../roadmap/phase-452-copy-out-front-door.md) W8. Needs a
maintainer ruling first: it reverses an RFC-0085 finding (below).

## Problem

The rule the audit now checks (checklist G2, colcon experience): a package is
self-contained. It names what it depends on, and the toolchain supplies where
that lives; it never reaches another package by a path that assumes the tree
around it.

Every example workspace package that depends on `nros` does it by path:

```toml
nros = { path = "../../../../../packages/api/nros", default-features = false, features = [ … ] }
```

`git grep -nE '^nros *=' -- 'examples/workspaces/**/Cargo.toml'` → 24 files, all
of this one spelling, none by version:

| workspace | packages |
| --- | --- |
| `features` | 8 |
| `rust` | 6 |
| `launch`, `realtime-rust`, `safety` | 2 each |
| `bridge-cyclonedds`, `bridge-xrce`, `mixed`, `sizing` | 1 each |

It is the only in-repo crate a workspace reaches by path; entries and C/C++
packages name nothing that way.

## Evidence

A copy of `examples/workspaces/launch` outside the checkout (main as of 2026-10-26,
`git archive` so no build output came along), with the checkout's `nros` on
PATH:

```
$ nros sync
Error: refresh source metadata for `listener_pkg`
    metadata-mode harness failed (exit 101) for component 'listener':
    error: no matching package named `nros` found
```

The single-package examples (`examples/<platform>/<lang>/<case>`) already do the
right thing: `nros = { version = "*" }`, with the path supplied by the generated
`[patch.crates-io]` (RFC-0098 D1). Workspaces are the one family left.

## Why it is still this way

[RFC-0085](../../design/0085-zephyr-workspace-and-west-handoff.md) (§ "What it found
on its first run", line ~765) met exactly this while testing two-tree builds
(issue 0905) and ruled that copying an in-tree workspace out "was the wrong
reproducer, not a product bug. Such a workspace is not portable by
construction". Its answer was to SCAFFOLD the test workspace with `nros new`,
whose output names `nros` by version, and leave the in-tree ones as they were.

That keeps the tested path honest and leaves the tree's own examples teaching
the opposite of it: a user who copies `examples/workspaces/rust` to start a
project gets a workspace that cannot build. The copy-out contract
(RFC-0026, phase-277 W6) covers single-package examples and not workspaces,
which is the gap.

## Fix direction

1. **Ruling.** Amend RFC-0085 (and RFC-0026's contract) so an in-tree workspace
   is copy-out like a single-package example, or record why workspaces stay
   in-tree-only. Everything below assumes the former.
2. Name it: `nros = { version = "*", default-features = false, features = […] }`
   in all 24, and confirm `nros sync` writes the patch for a workspace member
   the way it does for a leaf (it does for a scaffolded workspace, per
   RFC-0085's two-tree check).
3. Gate it: no tracked `examples/**/Cargo.toml` names an in-repo crate by a
   path outside its own package. Today that is the checklist G2 net
   (`git grep -nE 'path *= *"(\.\./){4,}packages/' -- 'examples/**/Cargo.toml'`),
   which a reader runs; it should be a fast-line gate so the 25th package
   cannot be written this way.
4. Acceptance: the copied `launch` workspace above syncs and builds, and so does
   `rust` (the largest).

## Resolution (2026-10-27)

**Ruling (maintainer, 2026-10-27): option A — in-tree workspaces are copy-out.**
RFC-0085's "not portable by construction" paragraph carries the amendment;
RFC-0026's copy-out contract and `examples/README.md` now cover
`examples/workspaces/` and show the `cp -r … && nros sync && nros build` flow.
The options weighed were A (name `nros` by version), B (keep the paths,
document workspaces as in-tree only) and C (an `nros ws export` verb that
rewrites them); A was the only one that removed the second spelling rather
than documenting or translating it.

- All 24 manifests: `nros = { path = "../../../../../packages/api/nros", … }`
  → `nros = { version = "*", … }`, nothing else. They were the only path
  dependencies out of `examples/` in any tracked example manifest.
- New fast-line gate `just check example-path-deps`
  (`scripts/check-example-path-deps.py`): no `path` in any dependency table
  (`target.*`, `dev-`, `build-`, sub-tables) or `[patch.*]` row of a tracked
  example manifest may resolve outside `examples/`. It runs its 9-case
  negative control on every invocation; against main's tree it reports
  exactly the 24 rows, against this one 0.

Measured on copies made from the tracked file set outside the checkout, each
`nros sync` + `nros build <image>`:

| workspace | packages changed | result |
| --- | --- | --- |
| `launch` | 2 | builds `linux_entry` |
| `rust` | 6 | `demo_bringup:linux` builds `linux_entry` |
| `features` | 8 | all 6 Rust linux images build (`params`, `qos`, `lifecycle`, `remap`, `before_boot`, `custom_msg`) |

`nros build --all` on the `features` copy stops at
`demo_bringup:zephyr_rust_qos` with issue 1419's census/contract refusal —
a Zephyr image this change does not touch.
