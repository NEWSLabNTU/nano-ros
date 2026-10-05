---
id: 1680
title: "A census taken under one `[system] features` set reads FRESH after the features change, though the features change what the census records"
status: open
type: bug
area: [cli]
severity: low
found: 2026-10-05
related: [1679, 1556, 1649, phase-463]
---

## What

`cmd/entity_census.rs::is_freshness_input` decides freshness from four
recorded roles: `binary`, `source_tree`, `entry_tu`, `build_file`. The image's
`[system] features` (and `capabilities`) in `system.toml` are none of them, and
the doc comment there explains why the MODEL is deliberately not a freshness
input (a contract edit must not invalidate the evidence it is compared
against). That argument is about the contract. It does not cover the feature
selection, which is a BUILD input: it decides which code the native census
binary compiles.

Measured 2026-10-04 on `examples/workspaces/cpp` (issue 1649's parameter
measurement): a census taken before `features = ["param_services"]` was added
recorded no parameters (issue 1679 is why); after the edit, `nros build`
read that census as fresh and compared the contract against it, reporting
`param-phantom` for parameters the code does declare. Deleting the census file
and re-taking it fixed it. The `binary` digest did not catch it because the
check ran against the binary the census itself had recorded — nothing had
rebuilt it yet.

## Scope

The only feature-dependent recording known is issue 1679's (the C++ parameter
hook compiled only under `param-store`). If 1679 is fixed by making the hook
unconditional, the census may no longer depend on any feature, and this issue
closes as a RULING: say so in `is_freshness_input`'s comment, with the
measurement. If some feature still changes what the census records, the
feature set (or the cargo feature list the native sibling is built with) is a
freshness input and belongs in `inputs` with its own role.

## Acceptance

A test: take a census, change `[system] features`, and assert the verdict —
STALE if the features can change the record, FRESH with the ruling cited if
they cannot.
