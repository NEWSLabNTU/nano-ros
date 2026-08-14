---
id: 578
title: "A `0` size probe emits the most under-sized opaque macro there is, and only a build-script warning says not to link it"
status: open
type: bug
area: build
related: [issue-0472, issue-0464, issue-0360]
---

## The gap

When the size probe returns `0` — no rlib to read — `nros-build-helpers` emits
the smallest possible opaque width and warns:

> `EXECUTOR_SIZE probe returned 0 — likely a cargo check --no-default-features
> run. The emitted CPP_EXECUTOR_OPAQUE_U64S will be 1; do not link the resulting
> rlib.`

The accommodation is legitimate and should stay: a `cargo check
--no-default-features` run genuinely has no compiled runtime to probe, and
hard-failing it would break `just check`.

What is missing is enforcement. **"Do not link" lives only in a build-script
warning**, and `1` is the most under-sized value the macro can take — a C caller
allocating `uint64_t _opaque[1]` for a type that needs hundreds. Nothing stops
that artifact reaching a link.

## Why the 0472 guards do not cover it

Issue 0472 gave every opaque macro a compile-time guard comparing the header's
probe-derived width against `size_of::<T>()`. Those guards deliberately **skip**
the `stated == 0` case, for the reason above: firing there would turn the
legitimate check run into a build failure.

So the guards catch a probe that is *wrong*, and by construction cannot catch a
probe that is *absent*. That is the remaining half, and it was 0472's item 2.

## Direction

Issue 0360 already established the mechanism: a symbol whose name encodes the
variant, so a header/archive mismatch surfaces as an undefined reference
**naming what it wanted** rather than as a silent `_opaque` overflow. The same
shape fits here — emit a poison symbol (e.g. `nros_opaque_sizes_unprobed`) into
the artifact when the probe returned zero, and reference it from the linked
path, so linking a check-only rlib fails at the link with a name that says why.

The warning stays as the friendly first signal; the symbol is what makes it
enforceable.

## Provenance

Split out of issue 0472 on 2026-08-15, when items 1 and 3 (the guards and their
gate) landed. Recorded separately rather than left inside a resolved issue,
because the guards make the *wrong-size* path safe and leave this one exactly as
it was.
