---
id: 1209
title: "The `std` census counts `feature = \"std\"` only on lines containing the
  literal `cfg`, so rustfmt wrapping an attribute hides a site from the ratchet"
status: open
type: bug
area: [tooling, ci, core]
related: [0597, 0196, phase-359, phase-361]
---

## What

`scripts/check-std-census.py` is the one-way ratchet for phase-359: it fails
when a crate's `std` count goes **up**, and its target is zero. The counting
line is `scripts/check-std-census.py:480-483`:

```python
if test_depth is None and code and not is_test_gate(stripped):
    if "cfg" in code:
        cfg += len(CFG_FEATURE_RE.findall(code))
    path += len(PATH_RE.findall(code))
```

The `cfg` metric is gated on the literal substring `cfg` appearing **on the same
line**. A multi-line attribute puts `#[cfg(all(` on one line and the feature
predicates on the following ones, so every predicate after the first line is
invisible to the metric.

## Measured

One site in the tree today —
`packages/core/nros-node/src/executor/spin.rs:6564-6573`:

```rust
        #[cfg(all(feature = "std", not(feature = "rmw-cffi")))]
        let primary_drive_timeout_ms = 0;

        #[cfg(all(
            not(feature = "std"),
            not(all(feature = "alloc", feature = "rmw-cffi"))
        ))]
        let primary_drive_timeout_ms = timeout_ms;
```

Line 6564 is counted. Line 6570 (`not(feature = "std"),`) is not — it carries no
`cfg`. `nros-node`'s shipped cfg sites are 4; the gate reports 3, and the
baseline at `scripts/check-std-census.py` records 3.

Reproduce:

```
python3 - <<'PY'
import pathlib
for scope in ["packages/core","packages/api"]:
    for f in pathlib.Path(scope).rglob("*.rs"):
        if "/target/" in str(f): continue
        in_attr=False
        for i,l in enumerate(f.read_text(errors="replace").splitlines(),1):
            s=l.strip()
            if s.startswith("#[cfg") or s.startswith("#![cfg"):
                in_attr = l.count("(")>l.count(")"); continue
            if in_attr:
                if 'feature = "std"' in l and "cfg" not in l: print(f"{f}:{i}: {s}")
                if l.count(")")>=l.count("("): in_attr=False
PY
```

## Why it matters more than one site suggests

**The formatter decides whether a site is counted.** `rustfmt` wraps an
attribute once it passes the 100-column limit, and `just format` is run before
every broad change. So a one-line `#[cfg(all(feature = "std", …))]` that the gate
sees becomes a wrapped attribute the gate does not, with no source change a
reviewer would read as touching `std` — and the ratchet reports no movement,
which reads as "nothing was added".

That is the failure this file's own docstring says it exists to prevent, and it
is the second occurrence of the class. Archived issue 0597 is the first: the
original regex anchored on `cfg(` / `cfg(not(` immediately followed by the
feature and could not see `cfg(all(feature = "std", ...))` — 26 sites in
`spin.rs` alone. The docstring's verdict then applies verbatim now:

> A ruler that cannot see the most common form of the thing it measures is worse
> than no ruler, because it reads as progress.

The fix that landed for 0597 widened the *regex*; it did not remove the
*line-at-a-time* assumption underneath it, which is what this recurrence rides
on.

## Fix sketch (not applied)

Attribute-aware rather than line-aware: track attribute nesting (the walk already
tracks brace depth for `#[cfg(test)]` exclusion, so the machinery is there) and
attribute the whole `#[cfg(...)]` span to the line it opens on. Then reformatting
cannot change a count.

A cheaper interim: drop the `if "cfg" in code` guard and count
`feature = "std"` anywhere in live code — it would over-count a bare
`feature = "std"` in a `cfg!()` expression or a `[features]`-shaped string, but
those are rare here and over-counting fails **closed** on a ratchet. Verify with
`--self-test`, which currently has no wrapped-attribute case.

Whichever shape lands, raise the `nros-node` baseline from 3 to 4 in the same
commit so the correction is in the diff, and add a wrapped-attribute case to
`self_test()` — the gate must be shown able to fail on the form it missed.
