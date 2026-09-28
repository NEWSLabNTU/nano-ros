---
id: 1539
title: "Five zenoh integration tests `return` from a `let … else` when zenohd is absent, so they report PASS having run nothing"
status: resolved
type: bug
area: [testing, rmw]
severity: high
found: 2026-09-28
resolved: 2026-09-28
related: [phase-472, 0445, 1284, 1552]
---

## What happens

`packages/rmw/zenoh/nros-rmw-zenoh/tests/zenoh_integration.rs`:

```rust
fn router() -> Option<ZenohRouter> {
    if let Some(why) = nros_tests::process::zenohd_unavailable_reason() {
        eprintln!("[SKIP] {why}");
        return None;
    }
    Some(ZenohRouter::start_unique().expect("failed to start zenohd"))
}
```

and five callers, at lines 124, 241, 340, 376 and 524:

```rust
let Some(_router) = router() else { return };
```

On a host without zenohd each of those tests prints `[SKIP]` and returns. A
return from a test is a PASS. And because nothing panics, `test-all`'s junit
rewrite — which turns `nros_tests::skip!` panics into skips — never sees it.

CLAUDE.md states the rule this breaks, verbatim: *"Tests must fail on unmet
preconditions (`assert!`/`bail!`/`nros_tests::skip!`). Bare `eprintln!`+`return`
reports PASS — never."* The file's own comment at line 812 says the same.

VERIFIED by reading, by the phase-472 coordinator.

## Why the gate missed it

`check-test-precondition-guards` exempts a helper that returns a real value, on
the grounds that "`let Some(x) = f() else { skip!(..) }` is the correct spelling"
— and never checks what the caller's `else` arm actually does. Renaming `router()`
to `require_router()` to put it squarely in the gate's population still passes.
Phase-472 W8.

## Fix

Each `else { return }` becomes `else { nros_tests::skip!(…) }`, and the gate checks
the caller's `else` arm for `skip!`/`panic!` when a test-file helper prints and
returns `None`.

## Resolution

`router()` in `zenoh_integration.rs` now owns its verdict: it returns the
router, `skip_class!(capability, …)`s when zenohd is absent, and goes through
`fixtures::or_skip`, so a router that is present and will not start FAILS. The
five callers are `let _router = router();` — there is no `None` left to drop.
Measured with the ROS environment unset: the origin/main binary reported
`5 passed`; the fixed one panics `[SKIPPED:capability]` for all five (a declared
capability reason, which `test-all` rewrites to a skip). With ROS sourced all
five run and pass.

`check-test-precondition-guards` gained a rule 2 on the caller: in every test fn
of every tracked `.rs`, a `let … else` arm may not exit by a bare `return` /
`return Ok(())` unless it also skips or panics; and a same-file helper that
prints and returns `Option`/`bool` (or propagates one with `?`) may only be
called from a test in a form whose failure path diverges. Mutation-tested: the
origin/main file fails the new gate at all five sites (old gate: rc 0), and one
restored `else { return }` fails it at that line.

The rule found 19 more sites: 5 in `nros-tests`, converted to skips in the same
change, and 14 in `packages/cli`, held by a shrink-only baseline and filed as
issue 1552 (they need a decision on how the CLI workspace spells a skip).
