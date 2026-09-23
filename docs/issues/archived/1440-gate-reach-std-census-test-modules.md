---
id: 1440
title: "`check-std-census` excluded `#[cfg(test)]` modules by FILENAME; the fix that
  replaced the filename then read `any(has_rmw, test)` as a test gate and exempted
  most of `nros-node`"
status: resolved
type: tech-debt
area: [tooling, docs]
found: 2026-09-21
related: [0196, 0701, phase-359, phase-444]
---

# The rule and the reach, twice

`scripts/check-std-census.py` states its own policy in the module docstring:

> ## `#[cfg(test)]` code is excluded, and that is a CORRECTION not a win
>
> Host unit tests link `std` even in a `no_std` crate, so their `std::` use can
> never block a target build.

The counting walk implemented that rule as a literal filename test:

```python
# `executor/tests.rs` is declared `#[cfg(all(test, …))] mod tests;`
# — the whole FILE is host-test code, and the gate lives in the
# parent module where this per-file scan cannot see it.
if rs.name == "tests.rs":
    continue
```

The comment describes the RULE correctly and the code implements a NAME. A
module declared exactly the same way in a file called anything else was
counted. This is the issue-0196 shape — a gate whose reach is narrower than the
rule it enforces — and the 2026-07-28 audit found four more like it.

**The machinery to do it properly was already in the same file**, and the
`--check-guards` walk at the bottom already asked the right question:

```python
if feats & FLAVOURS or "test" in feats:
    continue  # gated on the flavour itself, or test-only
```

## Why the machinery could not work

`"test" in feats` had **never once fired**. `feats` comes from
`required_features()`, whose only source is

```python
FEATURE_RE = re.compile(r'feature\s*=\s*"([A-Za-z0-9_-]+)"')
```

`test` in `cfg(all(test, feature = "alloc", …))` is a bare cfg PREDICATE, not a
`feature = "…"`, so `FEATURE_RE` cannot see it and `required_features` could
never return it. The condition has been dead since issue 0701 introduced it.
Re-verified against `main` on 2026-09-23, after the first fix had landed:
`required_features('all(test, feature = "alloc")')` returns `{'alloc'}`.

So the filename test was not merely narrow — it was **load-bearing**, the only
thing standing in for a predicate that did not work. With no exemption at all
the census reads **59 cfg / 142 path** against its steady **17 / 18** (measured
2026-09-23 on `main`; the same measurement taken on the phase-444 branch read
59 / 127, the difference being which files were in the tree at the time). The
two halves have to move together.

## How it surfaced

phase-444 added `packages/core/nros-node/src/executor/graph_wait_tests.rs`,
declared

```rust
#[cfg(all(test, feature = "alloc", not(feature = "rmw-cffi")))]
mod graph_wait_tests;
```

— identical in kind to `executor/tests.rs` beside it — holding two
`std::time::Instant::now()` calls in host test code. The gate reported

```
[FAIL] 1 count(s) went UP:
    nros-node: path 0 -> 2
```

for code that ships nowhere, which is exactly the inflation the docstring says
the exclusion exists to prevent.

# The second half: the first fix overshot

Commit `06f20cc28` replaced the filename with `test_gated_module_files()`,
which reads each `mod x;` declaration and exempts the file when the attribute
above it is a test gate. The rule was right. The PREDICATE was

```python
def is_test_gate(stripped: str) -> bool:
    """A `#[cfg(test)]` / `#[cfg(all(test, ...))]` attribute."""
    return stripped.startswith("#[cfg(") and re.search(r"\btest\b", stripped) is not None
```

A bare `\btest\b` search over the whole attribute, which reads

```rust
#[cfg(any(has_rmw, test))]
mod handles;
```

— "this module is in the PRODUCTION build, and also compiled under test" — as
a test gate. `nros-node/src/executor/mod.rs` and `lib.rs` declare almost
everything that way. Measured 2026-09-23: the loose predicate exempts **23 of
`nros-node`'s 26 declared modules** — `action`, `action_core`, `activator`,
`arena`, `backing`, `callback_trace`, `dispatcher`, `format_check`, `handles`,
`lifecycle_services`, `monitor`, `node`, `node_record`, `node_wake`,
`parameter_services`, `ready_set`, `session`, `spin`, `spsc_ring`, `storage`,
`time_source`, `triple_buffer`, `wake_probe` — i.e. nearly the whole crate sat
outside the census it exists to ratchet.

Fixing a too-narrow reach by widening it past the rule is the same defect
facing the other way, and it hides for the same reason: the number it produces
looks like progress.

## What that cost, and what the commit message claimed

`06f20cc28`'s message records

> The `nros-node` cfg baseline falls 3 -> 1 as a consequence, deliberately and
> in this diff: two cfg sites that were only ever host test code stop being
> counted as production `std` use.

**That is not what happened.** Measured 2026-09-23 by swapping only the
predicate and re-running the census: with `test` required as a real conjunct,
`nros-node` is **cfg 3** again and every other crate is unchanged. The two cfg
sites are in modules gated `#[cfg(any(has_rmw, test))]` — production code, not
host test code.

## Fix

1. `required_features()` returns the bare `test` predicate alongside the
   features, searching for it only OUTSIDE the `feature = "…"` strings so a
   feature named `test-util` is not mistaken for it (`\btest\b` matches
   `test-util`, because `-` is a word boundary).
2. `is_test_gate()` asks `required_features` instead of searching the raw
   attribute. That is the one-helper answer: `required_features` already
   encodes this file's conservatism — `any(...)` contributes nothing, because
   a site reachable through either of two cfgs does not let the tool say which
   — so `any(has_rmw, test)` yields the empty set and is not a test gate.
   `test_gated_module_files()` is kept exactly as it merged; only its predicate
   changes.
3. `guard_check()` carried the same `rs.name in ("generated.rs", "tests.rs")`
   filename skip. Removed: with the predicate working, its own
   `"test" in feats` arm excludes test modules BY THE RULE. `generated.rs`
   keeps its own exclusion — that one really is about the file, not about a
   cfg.
4. The `nros-node` baseline goes back to `cfg 3`, with the comment saying why
   it moved in both directions.

## Measured effect

**No baseline moves.** `nros-node` is cfg 3 / path 0 before the first fix and
after this one, and the total is 17 cfg / 18 path either way. Files newly
excluded by the correct predicate, against the ORIGINAL filename test:

| file | raw `std::` hits | effect on a published count |
| --- | --- | --- |
| `nros-macros/src/entry_parity.rs` | 6 | none — `nros-macros` is in `EXCLUDE` (proc-macro, host crate) |
| `nros-node/src/executor/graph_wait_tests.rs` | 2 | the false `path 0 -> 2` this issue opened on |
| `nros-node/src/mock.rs` | 0 | none |
| `nros-serdes/src/compat_tests.rs` | 0 | none |

Files the original filename test excluded that the predicate does not: **zero**.
So dropping the name special case re-includes nothing.

The correction is therefore a **prevented inflation, not a removal** — no
`std::` site was deleted and no number came down. Two of the four files were
already at zero, which is why the defect stayed latent: it needed a test module
that both used `std::` and was not called `tests.rs`.

## Negative control

The predicate had no test, which is how it was wrong twice. `--self-test` now
covers it (11 cases, up from 3): the `required_features` table including the
`test-util` trap, `not(test)`, and **both `any(...)` shapes that caused the
over-exemption**; plus a synthetic `lib.rs` → `executor/mod.rs` chain that
declares one module under `#[cfg(any(has_rmw, test))]` and one under
`#[cfg(all(test, …))]`, asserting `test_gated_module_files` returns exactly the
second.

Verified on reversion in both directions: restoring the old `required_features`
body turns the self-test red naming the exact expression, and restoring the
bare `\btest\b` predicate turns it red on `any(has_rmw, test)`.
