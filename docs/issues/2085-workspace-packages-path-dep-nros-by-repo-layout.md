---
id: 2085
title: "24 example workspace packages path-depend on `nros` by five `..`, so no in-tree workspace survives being copied out"
status: open
type: tech-debt
area: [examples, build, workspace]
severity: medium
found: 2026-10-26
related: [rfc-0085, rfc-0026, rfc-0067, phase-452, issue-0905, issue-2046]
---

# Example workspace packages reach `nros` by repository layout

**Home:** [phase-452](../roadmap/phase-452-copy-out-front-door.md) W8. Needs a
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

[RFC-0085](../design/0085-zephyr-workspace-and-west-handoff.md) (§ "What it found
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
