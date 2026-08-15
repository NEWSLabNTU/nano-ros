---
id: 624
title: "`check-examples` re-creates the per-leaf `target/` population that 0488 declared empty — and its own gate cannot see it"
status: open
type: tech-debt
area: build
related: [issue-0488, issue-0393, phase-340, rfc-0070]
---

## Symptom

`just ci` fails at `check-example-leaf-target-dirs`:

```
check-example-leaf-target-dirs: no build writes a leaf `target/`, but some EXIST.
  examples/native/rust/talker/target
  examples/native/rust/custom-msg/target
  … 37 total
```

Deleting them does not help. They are back after the next run.

## The writer

`just/native.just` ~495, the `check-examples` unit emitter:

```sh
printf 'cd %s && cargo +%s fmt --check -p %s && %s cargo clippy --quiet %s\n' \
    "$d" "{{NIGHTLY}}" "$pkg" "$e" "$f"
```

`cd <leaf> && cargo clippy` with no `--target-dir`, which is precisely the shape
CLAUDE.md names: *"a bare `cd <leaf> && cargo build` … kept re-creating
`examples/**/target/`"*. A leaf with no `[[fixture]]` row has no coordinate, so
it gets no shared cargo group and cargo falls back to the leaf's own `target/`.

`$f` is `$NROS_EXAMPLE_PROFILE_FLAGS` for non-native platforms and **empty for
native** — which is why the residue skews native, with the baremetal leaves
arriving through whatever `NROS_EXAMPLE_PROFILE_FLAGS` does not pin.

The serial branch a few lines up (`check_one`) has the same `( cd "$dir" && …
cargo clippy … )`, so fixing only the pool branch would leave `SERIAL=1` writing
them.

## Why it survived 0488

Issue 0488 is `status: resolved`, and says of this exact population:

> That population is now empty and gated by
> `scripts/check-example-leaf-target-dirs.py`.

It was empty *at the time* — 0488 swept `build-examples`, and its residue table
lists four more sites, all under `packages/testing/**`. `check-examples` is a
different recipe in the same file family and was not in the sweep. So this is
not 0488 regressing; it is a site 0488's sweep did not reach, in the population
0488 declared closed. Worth noting for the class rule in CLAUDE.md: the sweep
found the writers it looked for, and the gate then asserted a stronger claim
than the sweep had established.

## Why the gate never catches its own writer

The ordering makes it self-perpetuating and confusing to diagnose:

1. `check-example-leaf-target-dirs` runs **before** the examples lane — clean, passes.
2. `check-examples` then creates 37 leaf `target/` dirs.
3. The **next** run fails on dirs that the **previous** run made.

So the failure is always attributed to whatever else changed in between, and
clearing the dirs "fixes" it for exactly one run. Measured: delete all 37, run a
fully green `just check`, and all 37 are back.

The gate's own remedy text — *"rm -rf the directories, then re-run a build. If
one comes back, it is the second case and the writer needs finding"* — is
correct, and this issue is that second case.

## Fix

Per CLAUDE.md's rule for this class: give the build a `[[fixture]]` row
(preferred), or derive the dir from `nros_fixture_target_dir_flag` +
`nros_fixture_row_artifact_dir` — never a literal — and **move the test-side
locator in the same commit** (#393). Both branches of `check-examples` (pool and
`SERIAL=1`) must move together.

Worth considering as part of the fix: run the gate **after** the examples lane
as well, or the next writer added to this recipe gets the same one-run-delayed,
misattributed failure.

## A constraint the fix has to reckon with first

The obvious fix — point the lane at one shared `--target-dir` — is not obviously
safe here, and the reason is worth having before someone writes it:

**Every example leaf is its own workspace root.** `examples/native/rust/talker/
Cargo.toml` and its siblings each declare `[workspace]`, because examples are
standalone copy-out projects with no workspace walk-up (RFC-0026). Issue 0616's
rule is that *"a cargo `--target-dir` serves exactly ONE workspace root"* — a
crate reached by two different path spellings gets two units, identical but for
the `path` fingerprint field, and `nros-platform` holds the tree's one
`#[global_allocator]`. So a single dir shared by 37 roots is precisely the shape
0616 describes.

Note this is NOT a claim that the existing fixture groups are broken. 0616 is
resolved and its population was the cmake/`mixed` entry, not these leaves; and
`nros_fixture_group_slug` keys on `platform` (+ a variant hash of args/env) with
no workspace-root component, so per-platform sharing across leaf roots is
already the landed, working design from phase-340. The point is only that a fix
here should follow that precedent — and be measured, since a lint lane's cache
mixed into a fixture group dir would also thrash fingerprints between `cargo
clippy` and `cargo build`.

Which suggests the ordering for whoever takes this: decide *where* the lane's
artifacts belong (its own kind, or an existing group) before touching the
recipe, and confirm 0616's duplicate-unit hazard does not apply to a lint-only
lane that produces no linked artifact. `check-examples` consumes nothing
downstream, so unlike #393's cases there is no test-side locator to move with
it — that part, at least, is easier than the class usually is.
