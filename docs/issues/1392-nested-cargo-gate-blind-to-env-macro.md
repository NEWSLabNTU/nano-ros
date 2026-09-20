---
id: 1392
title: "`check-nested-cargo-lock-discipline` matches `env::var(\"CARGO\")` and not
  `env!(\"CARGO\")`, so the four nested cargos that actually exist are invisible
  — and one of them rewrote the root `Cargo.lock`"
status: open
type: bug
area: ci, tooling, build
severity: medium
found: 2026-09-20
related: [issue-0359, issue-0378, issue-0196, phase-454]
---

## The symptom that started it

During phase-454, a `check::build` lane **rewrote the root `Cargo.lock`**
(`heapless 0.8.0` → `heapless`) once the `cyclonedds` submodule was provisioned.
The agent reverted it and did not commit it. `Cargo.lock` is a promise that
someone else's build resolves what yours did (issues 0359/0378), and the
`--locked` PATH shim exists so a mismatch fails instead of silently rewriting.

Something escaped the shim.

## The escape route

`scripts/bin/cargo` injects `--locked` by being first on `PATH`. **Cargo sets
`CARGO` to the REAL binary for anything it spawns**, so a test that runs
`Command::new(env!("CARGO"))` bypasses `PATH` entirely — the shim never sees it.

That class is known and gated: `scripts/check/check-nested-cargo-lock-discipline.py`,
whose docstring says a nested cargo bypassing the shim must declare what it does
to the lockfile.

**The gate does not catch it.** Its detector is:

```python
BYPASS_SOURCE = re.compile(r"""env::var(?:_os)?\(\s*"CARGO"\s*\)|\bvar(?:_os)?\(\s*"CARGO"\s*\)""")
```

That matches the **runtime** spelling `env::var("CARGO")`. It does not match the
**compile-time macro** `env!("CARGO")` — which is the idiomatic spelling in a
test, and therefore the spelling the sites that exist actually use.

A site that fails `bypasses` is `continue`d **before `sites += 1`**, so it is not
merely unchecked: it is not even counted in the total the gate reports.

## Measured

| spelling | sites | gate sees it? |
| --- | --- | --- |
| `env::var("CARGO")` | 2 | yes |
| `env!("CARGO")` | **4** | **no** |

The gate reports `OK (6 shim-bypassing cargo invocation(s) … each is
non-resolving or carries lock discipline)` — those 6 are the visible ones. The
four blind sites, **none of which carries any of
`--locked` / `--frozen` / `--lockfile-path` / `resolver.lockfile-path` /
`apply_nested_lock_discipline`**:

* `packages/cli/rosidl-codegen/tests/cpp_heap_compile_check.rs`
* `packages/cli/rosidl-codegen/tests/heap_compile_check.rs`
* `packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/bare_metal_link.rs`
* `packages/tooling/nros-sizes-build/tests/bitcode_probe.rs`

The cyclonedds one matches the observed symptom exactly —
`bare_metal_link.rs:92`:

```rust
let out = Command::new(env!("CARGO"))
    .current_dir(&root)
    .args(["build", "-p", "nros-rmw-cyclonedds", "--no-default-features", "--target", TARGET])
```

`cargo build` from the workspace root, resolving, with no lock discipline. It is
`#[ignore]`d as heavy, which is why it surfaces only in a lane that runs ignored
tests — and why the rewrite looked like it came from nowhere.

## A second, narrower hole in the same detector

```python
ARG_LITERAL = re.compile(r"""\.arg\(\s*"([^"]+)"\s*\)""")
```

Only the single-argument `.arg("…")` form. Every site above uses `.args([…])`,
so even a site that clears `bypasses` yields an empty argument set and is
described as `['(unknown)']` rather than by its real subcommand. That does not
hide a finding on its own — an empty `subs` still fails the
`NON_RESOLVING`/`metadata --no-deps` exits and reaches the check — but it makes
the diagnosis wrong at exactly the moment someone needs it right.

## Why this is the issue-0196 shape

The gate's rule is correct and its reasoning is written out: a nested cargo that
bypasses the shim must say what it does to the lockfile. Its **reach** is
narrower than that rule by one spelling, and the missing spelling is the common
one. It has been reporting a green over a set that excludes every site anyone
would actually write.

Same family as `check-c-array-pool-floors` requiring adjacent `#ifndef`/`#define`
lines, and `check-knob-ends` crediting a knob with a reader that was its own name
inside an error string.

## What a fix has to decide

* **Widen `BYPASS_SOURCE`** to the macro spelling. Both `env!("CARGO")` and
  `option_env!("CARGO")` resolve to the real binary; a regex over the three
  forms is the minimum.
* **Widen `ARG_LITERAL`** to `.args([…])`, so the reported reason names the real
  subcommand.
* **Then rule the four sites**, which is the substantive half: each either gains
  `--locked`, or is redirected with
  `--config resolver.lockfile-path="<probe dir>/Cargo.lock"` where it injects a
  `[patch]` the workspace lock cannot record, or is argued non-resolving. The
  gate's own `FIX` text already states these three options.
* **A negative control**: the gate must be shown to go red on an
  `env!("CARGO")` site with no discipline, or this recurs under a third
  spelling. A gate that cannot fail proves nothing.

## Reproduction

`python3 scripts/check/check-nested-cargo-lock-discipline.py` exits 0 today.
Add `--locked` to none of the four files and it still exits 0. Change one of
them from `env!("CARGO")` to `env::var("CARGO").unwrap()` and the same site is
reported.

Found while tracing a root-lock rewrite observed during phase-454.
