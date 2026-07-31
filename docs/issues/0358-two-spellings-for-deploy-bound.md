---
id: 358
title: "`[package.metadata.nros.entry]` and `[package.metadata.nros.deploy.<target>]` both mean \"bound to a deploy target\", and every consumer must remember both"
status: open
type: tech-debt
area: build
related: [issue-0288, issue-0318, issue-0316, issue-0353, issue-0311]
---

## Finding (2026-07-31, while scoping issue 0288)

Two manifest spellings say the same thing:

```toml
[package.metadata.nros.entry]        # 24 standalone examples
deploy = "native"

[package.metadata.nros.deploy.zephyr]  # 27 standalone examples
board = "native_sim/native/64"
```

Both mean *this package is bound to a deploy target, so it cannot be
host-compiled*. Nothing enforces that a consumer checks both, and the first one
that did not was the source-metadata probe:

```rust
let deploy_bound = nros.entry.is_some();
```

27 packages fell through to the host probe and hit a hard build failure instead
of the graceful degrade the code intended — issue 0318 is one instance, which
surfaced as `DOTCONFIG must be set by wrapper` on a Zephyr leaf and cost a
debugging session to trace back to a predicate.

0318 fixed that call site by accepting both spellings. **The shape is
unfixed**: the next consumer of "is this deploy-bound?" starts from the same
coin-flip, and picking the wrong half fails somewhere unrelated to the
predicate.

## Why this is worth its own issue

It is the drift class this repo keeps paying down, and the instances are
starting to rhyme:

* two `system.toml` parsers (the macro's and the envelope's) — a `[deploy]`
  key the macro read and the envelope rejected, so every board example with a
  static IP hard-failed `Workspace::discover`;
* two capability sources — the declaration and the posix always-on (issue 0353),
  where hosted could not fail when a declaration was missing;
* two knob spellings (issue 0316);
* two child-indexing rules (issue 0319);
* two capability accessors — `SystemToml::capability_enabled` honours both
  declaration forms, `NrosPlan::capability_enabled` silently omits `lifecycle`.

Each was found by something breaking, never by the second copy being noticed.

## Options

1. **One accessor, no schema change.** Add
   `PackageMetadataNros::deploy_bound()` (or similar) and make every consumer
   call it. Cheap, and it converts "remember both spellings" into "call the
   function". Does not stop a THIRD spelling being added.
2. **Collapse to one spelling.** Pick `[deploy.<target>]` (it carries the target
   name, which `[entry] deploy = "..."` also does, so it is a superset) and
   migrate the 24 `[entry]` packages. Removes the ambiguity at the source; costs
   a migration and a deprecation window.
3. **Keep both, gate the consumers.** A check that greps for `\.entry\.is_some`
   / `\.deploy\.is_empty` style predicates outside the one accessor. Weakest —
   it polices spelling rather than removing the second source.

(1) then (2) is the honest order: the accessor stops the bleeding immediately
and is a prerequisite for the migration anyway.

## Acceptance

* Exactly one place answers "is this package deploy-bound?".
* A package using either spelling is treated identically by every consumer,
  demonstrated by a test that asserts both forms produce the same answer.
