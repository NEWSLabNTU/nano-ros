---
id: 615
title: "`check-feature-contract` clause (d) counts only DEP-SITES, so it calls a default unreachable when the crate's OWN artifact needs it"
status: resolved
type: bug
area: build
related: [issue-0593, issue-0613, issue-0591, phase-360, phase-361]
---

## Symptom

`just ci` fails at `check-feature-contract`:

```
[FAIL] clause (d) — no `default` feature is unreachable
  packages/api/nros-cpp/Cargo.toml: `default` names ['panic-spin'], and all 2
      in-workspace dep-sites on `nros-cpp` pass `default-features = false`
      without naming them.
      Request it at the dep-sites that need it.
```

## The remedy it names is WRONG here, and would break the build

Clause (d) reasons entirely about DEP-SITES. That is right for the class issue
0593 identified and issue 0613 fixed, where a default was dead because every
consumer disabled it. It is wrong for `nros-cpp`, whose default serves the
crate's OWN build.

`nros-cpp` produces a `staticlib`/`cdylib` — a FINAL artifact. A `no_std` final
artifact with no panic provider does not link, so the crate names its own spin
handler in `default` to keep a standalone `cargo check -p nros-cpp` meaningful.
Its manifest says so, from phase-361 W3 / issue 0591:

> But a bare `cargo check -p nros-cpp` builds this crate's `staticlib`/`cdylib`,
> which is a FINAL artifact: `no_std` + no panic provider is `#[panic_handler]
> function required, but not found`. … every real consumer takes this crate with
> `default-features = false` and picks its own provider, so nothing downstream
> is affected.

**Verified, not assumed.** Applying the remedy — `default = []` — and running the
bare check:

```
$ cargo check -p nros-cpp
error: `#[panic_handler]` function required, but not found
error: could not compile `nros-c` (lib) due to 1 previous error
```

So the gate is asking for a change that breaks the very build the default exists
to serve. Following it mechanically, as one would after issue 0613, converts a
green standalone check into a hard failure.

## The gap

The rule is stated as "no `default` feature is unreachable", but it is
IMPLEMENTED as "no `default` feature is unreachable *from a dep-site*". A crate
whose own artifact is final has a third consumer the rule cannot see: itself.

That distinction is exactly what separates this from issue 0613. There,
`nros-board-nuttx` is only ever a dependency — the overlay forwards the feature
explicitly, so the default really was dead, and emptying it was verified inert.
Here the default is live and load-bearing.

## Fix shapes (none implemented)

1. **Exempt a crate whose own build is final.** If `[lib] crate-type` includes
   `staticlib`/`cdylib`/`bin`, the crate is a consumer of its own default and
   the clause should not fire. This is the general rule and matches the reason
   the default exists.
2. **Named exemption with a reason**, the idiom
   `check-zenohd-spawn-sites` and `check-opaque-macro-guards` already use — a
   short list where each entry states why, so a future addition needs an
   argument rather than a silent pass.
3. **Assert the property instead of the proxy**: a default is "reachable" if
   ANY build in the workspace activates it, including `cargo check -p <crate>`.
   Closest to the rule as written, and the most work.

(1) is the smallest change that is also correct in general; (2) is the safest if
`crate-type` turns out to be a poor proxy on some crate.

## Why this was filed before being fixed

`check-feature-contract` is phase-360 W4's gate and `nros-cpp`'s default is
phase-361 W3's, both landed within days. Changing either mid-flight is how the
duplicate-work collisions earlier in this session happened. The evidence above
is what the owner needs; the choice between the three shapes is theirs.

It is currently RED on the `just ci` line.

## Provenance

Found 2026-08-16 immediately after issue 0613 fixed the genuine instance of this
class in `nros-board-nuttx`. Clause (d) then fired on `nros-cpp` — a different
situation with the same symptom, which is what makes the dep-site-only
implementation worth correcting rather than papering over.

## Resolved 2026-08-16 — fix shape (1)

`clause_d` now skips a crate whose OWN build is final — `crate-type` containing
`staticlib`/`cdylib`/`bin`, or any `[[bin]]` target. That crate is a consumer of
its own default, which the dep-site view cannot see.

The exemption is REPORTED, not silent:

```
note (d) exempt, own build is final: nros-cpp (own build is final: ['lib', 'staticlib'])
ok  (d) no `default` feature is unreachable
```

A gate that exempts without saying so is how issue 0442 hid.

### Kept narrow, and self-tested in both directions

Two cases added to `--self-test` (now 19):

* `final-artifact-default-exempt` — a `staticlib` crate with an otherwise
  unreachable default must PASS.
* `rlib-only-default-still-fails` — the same shape with `crate-type = ["rlib"]`
  must still FAIL.

The second is the one that matters: an exemption that swallowed the rlib case
would have silently retired the clause.

## Postscript — issue 0613 was a FALSE POSITIVE, and is reverted

Filing this exposed that the instance 0613 "fixed" was not real either.

Upstream's `a32196ab2` (2026-08-15 13:49) had already fixed clause (d)'s blind
spot: it now follows a feature reached by FORWARDING
(`image-runtime = ["nros-board-nuttx/image-runtime"]` in the consumer's own
feature table) and not only by `features = [...]` on the dep line. That is
exactly how `nros-board-nuttx-qemu` reaches the feature.

My 0613 diagnosis ran against a checkout predating that commit by ~11 hours, so
I saw a stale failure and emptied `nros-board-nuttx`'s `default` to satisfy it.
With the current gate, restoring `default = ["image-runtime"]` is green — the
clause does not fire — so the manifest change was unnecessary, and it removed a
default the crate's comment documents as intentional for out-of-tree pure-Rust
consumers. **Reverted.**

Verified both ways before reverting: with the current gate and the default
restored, clause (d) reports `ok`.
