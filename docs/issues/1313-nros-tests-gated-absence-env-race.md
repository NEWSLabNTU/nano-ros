---
id: 1313
title: "`gated_absence_is_a_hard_failure` races a sibling test on process env under
  plain `cargo test`"
status: open
type: bug
area: testing
severity: low
related: []
---

## What happens

`packages/testing/nros-tests/src/fixtures/binaries/mod.rs`,
`fn gated_absence_is_a_hard_failure` (line ~5849), reads an environment
variable that a sibling test in the same module clears. Under nextest each test
is its own process, so it passes. Under plain `cargo test -p nros-tests --lib`
the tests share one process and run on threads, and it fails intermittently:
observed 212/213 on 2026-09-11 (branch `test/rv-virt-threadx-c-workspace`,
PR #936). It passes when run alone. The test's own comment says it relies on
nextest's process-per-test.

## Why it matters

A red that goes away on retry teaches people to retry. And
`std::env::set_var` / `remove_var` are `unsafe` in edition 2024 precisely
because concurrent access is undefined behaviour, so this is a latent UB site,
not only a flake.

## Fix

Pick one:
- **Stop reading process env in the code under test for this path.** Inject the
  value, which is the better fix.
- **Serialise every test in the module that touches that variable** behind one
  `static` mutex, and document it.

Grep for siblings with the same shape: `rg -n 'set_var|remove_var'
packages/testing/nros-tests/src`.

## Acceptance

`cargo test -p nros-tests --lib` passes 20 times in a row with the default
thread count.
