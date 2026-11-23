---
id: 2873
title: "The interop gates' Rust source scanner read the char literal '\"' as the start of a string, so a test holding it reported no cases"
status: resolved
type: bug
severity: low
area: [testing, ci]
related: [issue-1191]
found: 2026-11-23
---

## What happens

`categorize` in `scripts/check-interop-cell-runners.py` marks each character
of a Rust test file as code, string or comment. Both interop gates read a test
through it: `check-interop-cell-runners` and `check-interop-verdicts` (its
`source_cases`, which derives the cases a test binary runs for
`interop::CASE_CELLS`).

It had no char-literal case. In `'"'` the `"` opened a string, which ran to the
next `"` anywhere in the file and turned the code between into "string". Every
`#[test]` and `#[rstest]` after it went missing. Found on phase-554 W9's
`tests/lifecycle_pair_interop.rs`, whose `.trim_matches('"')` made
`source_cases` return `[]`, so `interop-verdict-ledger` failed with
"`lifecycle_pair_interop.rs` does not run any such case" for both of the file's
real cases. The test worked around it by spelling the quote `'\x22'`.

## Fix

A char literal is one token: `'x'`, or an escape (`'\''`, `'\x22'`,
`'\u{27}'`) whose closing quote is a few characters on. A lifetime (`'a`,
`'static`) has no closing quote two places on, so it stays code.

The self-test now covers the four literal shapes and a function with
lifetimes. Negative control: with the char-literal branch removed, the
self-test fails on `'"'` ("hid the code after its char literal"). Checked on
the W9 test with the original `'"'` restored: `source_cases` finds its three
cases.

## Resolution

Fixed in `categorize` (above). Raw strings (`r#"…"#`) are still read as a
plain string, so an unbalanced `"` inside one is not modelled; no test holds
one today.
