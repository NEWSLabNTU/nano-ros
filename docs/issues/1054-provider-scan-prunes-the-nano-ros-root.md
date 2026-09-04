---
id: 1054
title: "`provider_scan` reads `.nros-ignore` on the root it was handed, so scanning the nano-ros tree finds ZERO providers of any family — the marker's own header says it must not"
status: open
type: bug
area: cli, tooling
severity: high
found: 2026-09-04
related: [0621, 0809, phase-348, phase-420, phase-421]
---

# The marker prunes the tree it was written to protect

`walk_packages` checks the ignore markers on every directory it pops, starting
with the root it was given:

```rust
let mut stack = vec![root.to_path_buf()];
while let Some(dir) = stack.pop() {
    if IGNORE_MARKERS.iter().any(|m| dir.join(m).exists()) {
        continue;
    }
```

The repository root carries `.nros-ignore`. So `scan_roots(&[<nano-ros root>])`
returns **zero packages**, and therefore zero providers — not just serdes:
`rmw`, `board` and `platform` are pruned identically.

## The contract says the opposite, in the marker file itself

`.nros-ignore`'s header (issue 0621):

> This marker prunes the whole tree from any walk that starts ABOVE it. It does
> NOT affect nano-ros's own discovery: `build_pkg_index` exempts the root it was
> given (`entry.depth() == 0` returns true before any marker is read), so when
> nano-ros IS the workspace root this file is never consulted, and when it is
> nested it prunes at depth 1.

`nros-pkg-index` honours that. `provider_scan` does not.

## How it got here

Issue 0809 taught `provider_scan` the `.nros-ignore` spelling — the repo root's
own marker had been written for `nros-pkg-index`, and `provider_scan` did not
recognise it. The spelling was carried across; the **depth-0 exemption that
makes the spelling safe was not**. Two walks, one marker vocabulary, two
different meanings for the root.

## Why nothing has failed yet

`default_search_path` returns `[nano-ros root, user workspace]`, and every
consumer today resolves providers through the generated compile-time tables
(`rmw_table.rs`, and now `serdes_table.rs`), which `build.rs` produces by
globbing descriptor paths directly rather than by scanning. The scan's answer is
consumed for workspace discovery, where the caller passes the *workspace* root,
not the nano-ros root. So root 0 of the search path has been contributing
nothing, silently, since 0809.

phase-421 W4's `resolve_serdes_in` is the first caller to ask the search path
for an in-tree provider by name, which is how this surfaced.

## Reproduction

`cargo test -p cargo-nano-ros --lib
serdes_resolver::tests::the_nano_ros_root_is_pruned_by_its_own_nros_ignore`
pins the behaviour as it is today, and says in its body what changes when this
is fixed. It passes *because* the bug exists; it is a characterization test, not
an endorsement.

## Fix

Exempt depth 0 in `walk_packages`, the way `build_pkg_index` does — check the
markers on descendants only. That is the smallest change that makes the two
walks agree with the marker's stated contract.

It changes discovery for **every** provider family, so it wants its own change
and its own verification rather than riding a serdes wave: after the fix, a scan
of the nano-ros root starts returning ~272 packages where it returned none, and
every caller that passes that root needs to be checked for whether it wanted
them. The characterization test above becomes the assertion that it does.
