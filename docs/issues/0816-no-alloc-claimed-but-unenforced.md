---
id: 816
title: "The book promises no-alloc integrations and nothing checks the linked
  image, so it is a claim rather than a property"
status: open
type: test-gap
area: tooling
related: [issue-0817, phase-391]
---

## Problem

The tree is genuinely built for heap-free operation. `alloc` is a Cargo feature
and every core crate gates on it:

```rust
#[cfg(feature = "alloc")]
extern crate alloc;
```

(`nros-core`, `nros-rmw`, `nros-node`, `nros-serdes`, `nros` — all the same
shape.) The book then makes the property a promise:

- `book/src/user-guide/embassy-integration.md:81` — *"The no-alloc contract."*
- `book/src/user-guide/embassy-integration.md:336` — *"fully no-alloc."*
- `book/src/internals/dispatch-strategy.md:171` — *"the no-alloc +
  framework-task-routed"* constraint
- `book/src/concepts/no-std.md:146` — the parameter API *"works without alloc"*

**Nothing verifies any of it.** No lane `nm`s a built image and asserts the
absence of allocation symbols. The feature gates are necessary and are not
sufficient: a vendored C dependency, a weak-symbol fallback, or one
`extern crate alloc` added under a default-on feature reintroduces the heap
with no diagnostic, and the book keeps promising otherwise.

## Evidence that this class of drift is real

[Issue 0817](archived/0817-platform-funnel-bypassed-in-zephyr-port.md) found
sixteen allocation sites in the Zephyr platform port that bypassed
`nros_platform_alloc` and called `k_malloc` directly. They compiled, linked,
ran, and passed every lane for as long as they existed. A source grep is what
eventually found them, and a source grep cannot see vendored C — zenoh-pico
alone reaches the allocator from 42 call sites.

## Shape of the fix

A link-time symbol gate, not a source check: `nm` the image and deny
`malloc`/`calloc`/`realloc`/`free`/`k_malloc`/`k_free`/`pvPortMalloc`/... with
a per-tier strictness:

| tier | rule |
| --- | --- |
| `heap-free` | deny every allocation symbol, no exceptions |
| `unified` | allow them only inside the `nros_platform_*` backend objects |

That turns both the book's promise and RFC-0034 D6's single-funnel rule into
things a build can fail on. Owned by
[phase 391](../roadmap/phase-391-allocation-unification-and-tier-model.md).
