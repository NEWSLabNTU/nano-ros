---
id: 358
title: "`[package.metadata.nros.entry]` and `[package.metadata.nros.deploy.<target>]` both mean \"bound to a deploy target\", and every consumer must remember both"
status: resolved
resolved_in: issue-0358-fix
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

## Fix (2026-07-31)

Option (1). `PackageMetadataNros::deploy_bound()` lives on the struct that owns
both fields; the one real call site (`workspace.rs`, the source-metadata probe's
predicate) goes through it. A third spelling is now one edit there rather than a
hunt through consumers.

The test asserts the two spellings **agree** rather than checking each arm.
A per-arm test passes happily while the other half is broken — which is exactly
how this survived until 27 packages hard-failed. Verified as a negative control:
reverting the accessor to `self.entry.is_some()` fails the test with the
`[deploy.<target>]` diagnostic.

`check-feature-set-ssot.sh` rule 8 forbids re-deriving the predicate at a call
site, verified to fire on the exact expression issue 0318 had to write by hand.

Option (2) — collapsing to one spelling — is NOT done. The ambiguity in the
manifest format remains; what is fixed is that no consumer has to know about it.
That is the honest scope: the accessor stops the bleeding and is a prerequisite
for any later migration.

## Acceptance

- [x] Exactly one place answers "is this package deploy-bound?" —
      `PackageMetadataNros::deploy_bound()`, with rule 8 gating re-derivation.
- [x] A package using either spelling is treated identically by every consumer,
      demonstrated by `both_deploy_bound_spellings_agree`, which asserts the
      equivalence and fails when either half is dropped.
- [ ] (option 2, not done) One spelling in the manifest format.
