---
id: 1539
title: "Five zenoh integration tests `return` from a `let … else` when zenohd is absent, so they report PASS having run nothing"
status: open
type: bug
area: [testing, rmw]
severity: high
found: 2026-09-28
related: [phase-472, 0445, 1284]
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
